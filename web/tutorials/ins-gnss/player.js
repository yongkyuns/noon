// UI orchestration only. Noon owns the scene, animation state, and sampling.
import { ProvenancedPythonAuthoringClient } from '../../provenanced-authoring-client.js';
import { AuthoringExecutionClient } from '../../authoring-execution-client.js';

const $ = id => document.getElementById(id);
const metadata = await (await fetch('./chapters.json')).json();
const source = await (await fetch('./scene.py')).text();
let authoring, execution, generation=0, chapter=1, time=0, playing=false, completed=false, busy=false;
// Only animation-frame timestamps establish the playback clock. Mixing them
// with performance.now() can make the first requested sample move backwards.
const MAX_FRAME_STEP_SECONDS = 1 / 15;
let lastClock=null, animationFrame=null, playbackEpoch=0;
const errors=[];

function stop(){
  playing=false;
  playbackEpoch++;
  cancelAnimationFrame(animationFrame);
  animationFrame=null;
  lastClock=null;
  $('play').textContent='Play';
}
function fail(error){errors.push(String(error));stop();$('status').textContent=String(error);}

async function open(number){
  if(!Number.isInteger(number)||number<1||number>metadata.chapters.length)
    throw new RangeError('Unknown tutorial chapter');
  const token=++generation;
  stop(); execution?.terminate(); authoring?.terminate();
  chapter=number;time=0;completed=false;busy=true;
  $('play').disabled=true;$('status').textContent='Loading Python and Noon…';
  $('chapter').value=String(chapter);
  const item=metadata.chapters[chapter-1];
  $('title').textContent=item.title;
  $('reading').replaceChildren(...item.notes.map(words=>{
    const p=document.createElement('p');p.textContent=words;return p;
  }));
  // A canvas is exclusively owned by its render worker. Never transfer it twice.
  const canvas=document.createElement('canvas');canvas.id='scene';
  canvas.width=1280;canvas.height=720;canvas.setAttribute('aria-label',item.title+' animation');
  $('stage').replaceChildren(canvas);
  const a=new ProvenancedPythonAuthoringClient();authoring=a;
  const e=new AuthoringExecutionClient(canvas,{onError:error=>{if(token===generation)fail(error);}});execution=e;
  try{
    await a.ready();if(token!==generation)return;
    let attached;
    const ready=new Promise(resolve=>{attached=resolve;});
    const run=a.run(source,{chapter},{onSemanticContinuation:async registration=>{
      if(token!==generation)throw new Error('Chapter replaced');
      await e.prepare({transportMode:'transferable'});
      await e.startSemanticExecution(registration.semanticExecution,{
        authoringClient:a,transportMode:'transferable',pacing:'external_samples',loopDurationSeconds:600,
      });
      attached();
    }});
    run.catch(error=>{if(token===generation)fail(error);});
    await Promise.race([ready,run.then(()=>{throw new Error('No semantic continuation');})]);
    if(token!==generation)return;
    await e.sampleToAuthoredTime(0,{stopAtSourceCompletion:true});
    $('status').textContent='Ready · forward playback · chapter restart';
    $('play').disabled=false;
    history.replaceState(null,'',`?chapter=${chapter}`);
    $('clock').textContent='0.0 s';
  }catch(error){if(token===generation)fail(error);}
  finally{if(token===generation)busy=false;}
}

async function sample(target){
  if(busy||!execution)throw new Error('Chapter is busy');
  if(!Number.isFinite(target))throw new TypeError('Sample time must be finite');
  if(target<time)throw new Error('Restart the chapter before moving backwards');
  busy=true;
  const token=generation;
  try{
    const result=await execution.sampleToAuthoredTime(target,{stopAtSourceCompletion:true});
    if(token!==generation)return result;
    time=result.time;completed=Boolean(result.sourceCompleted);
    $('clock').textContent=`${time.toFixed(1)} s`;
    if(completed){stop();$('status').textContent='Chapter complete · replay or choose the next chapter';}
    return result;
  }finally{if(token===generation)busy=false;}
}

function play(){
  if(playing||completed||$('play').disabled)return;
  playing=true;
  lastClock=null;
  $('play').textContent='Pause';
  requestFrame(++playbackEpoch);
}
function requestFrame(epoch){
  animationFrame=requestAnimationFrame(now=>frame(now,epoch));
}
async function frame(now,epoch){
  if(!playing||epoch!==playbackEpoch)return;
  animationFrame=null;
  if(lastClock===null)lastClock=now;
  // Keep lesson time monotonic. Slow hardware slows the lesson rather than
  // skipping explanations; the runtime still owns the published authored time.
  const delta=Math.max(0,Math.min((now-lastClock)/1000,MAX_FRAME_STEP_SECONDS));
  lastClock=Math.max(lastClock,now);
  if(!busy&&delta>0){
    try{await sample(time+delta);}
    catch(error){if(epoch===playbackEpoch)fail(error);}
  }
  // A paused or replaced session must not resurrect an old frame loop after
  // an in-flight sample resolves. A new Play has its own epoch and clock.
  if(playing&&epoch===playbackEpoch)requestFrame(epoch);
}
for(const [index,item] of metadata.chapters.entries()){
  const option=document.createElement('option');option.value=String(index+1);
  option.textContent=`${String(index+1).padStart(2,'0')} · ${item.title}`;$('chapter').append(option);
}
$('chapter').addEventListener('change',()=>open(Number($('chapter').value)));
$('play').addEventListener('click',()=>playing?stop():play());
$('restart').addEventListener('click',()=>open(chapter));
$('previous').addEventListener('click',()=>open(Math.max(1,chapter-1)));
$('next').addEventListener('click',()=>open(Math.min(metadata.chapters.length,chapter+1)));
window.addEventListener('pagehide',()=>{stop();execution?.terminate();authoring?.terminate();});
window.tutorial={open,sample,play,pause:stop,status:()=>({chapter,time,playing,completed,busy,errors:[...errors]}),
                 inspect:()=>execution.debugFrame(),metrics:()=>execution.metrics()};
const requested=Number(new URL(location.href).searchParams.get('chapter')??1);
await open(Number.isInteger(requested)&&requested>=1&&requested<=metadata.chapters.length?requested:1);

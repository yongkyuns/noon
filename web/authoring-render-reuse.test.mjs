import assert from "node:assert/strict";
import {readFile} from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

// Exercise the real controller through its host/port boundary, replacing only
// browser GPU objects. A fake renderer exposes replaceResourceBundle: without
// that API existing compatibility harnesses keep testing cold initialization.
const controllerSource = await readFile(new URL("./authoring-render-controller.js",import.meta.url),"utf8");
const metricsSource = await readFile(new URL("./frame-metrics.js",import.meta.url),"utf8");
const source = (metricsSource+"\n"+controllerSource)
  .replace(/^import\s+[\s\S]*?;\n/gm,"").replace(/^export\s+/gm,"");
const spin = () => new Promise(resolve => setImmediate(resolve));
function deferred() { let resolve; const promise = new Promise(r => {resolve=r;}); return {promise,resolve}; }

function harness({backend="WebGL2",failReset=false}={}) {
  const messages=[], frames=[], receivers=[], bundles=[], events=[];
  let creates=0, freed=0, renderResults=[true], gpu=deferred();
  class Canvas { width=320; height=240; addEventListener(){} removeEventListener(){} }
  class Port {
    messages=[]; listeners=[]; closed=false;
    addEventListener(type,callback){if(type==="message")this.listeners.push(callback);}
    start(){} close(){this.closed=true;} postMessage(m){this.messages.push(m);}
    emit(m){for(const l of this.listeners)l({data:m});}
  }
  const renderer={
    replaceResourceBundle(bytes){events.push("reset");if(failReset)throw Error("invalid scene bundle");bundles.push([...bytes]);},
    applyDeltaTransportJson(packet){events.push("apply:"+packet);return true;},
    rendererBackend:()=>backend,resize(){},render(){events.push("render");return renderResults.shift()??true;},
    waitForSubmittedWork:()=>gpu.promise,gpuGeneration:()=>1,time:()=>0.5,objectCount:()=>1,
    preloadedGeometryCount:()=>1,preloadBytesUploaded:()=>3,free(){freed++;},
    setRenderSubstageProfiling(){},setGpuCompletionProfiling(){},
  };
  const ctx=vm.createContext({
    performance,Promise,Uint8Array,setTimeout,clearTimeout,
    OffscreenCanvas:Canvas,MessagePort:Port,init:async()=>{},
    RetainedExecutionCanvasRenderer:{create:async()=>{creates++;events.push("create");return renderer;}},
    TransferableExecutionDeltaReceiver:class {
      constructor(_port,consume){receivers.push(consume);}
      drain(){} pendingCount(){return 0;}
    },
    EXECUTION_TRANSPORT_SHARED:"shared",EXECUTION_TRANSPORT_TRANSFERABLE:"transferable",
    drainRendererGpuDiagnostics:()=>true,formatGpuDiagnostic:String,
    host:{postMessage:m=>messages.push(m),
      requestAnimationFrame(cb){frames.push(cb);return frames.length;},cancelAnimationFrame(){}},
  });
  vm.runInContext(source,ctx);
  const controller=vm.runInContext("createAuthoringRenderController(host)",ctx);
  const dispatch=msg=>controller.dispatch({channel:"noon.render",protocolVersion:1,...msg});
  const canvas=new Canvas();
  const port1=new Port(),port2=new Port(),port3=new Port();
  return {messages,frames,receivers,bundles,events,port1,port2,port3,renderer,dispatch,
    createCount:()=>creates,freeCount:()=>freed,
    setRenders:(...xs)=>{renderResults=xs;},
    holdGpu:()=>{gpu=deferred();return gpu;},
    async setup(){
      await dispatch({type:"prepare",canvas,requestId:0,transportMode:"transferable"});
      await dispatch({type:"start_engine",port:port1,requestId:1,mode:"retained",transportMode:"transferable"});
      port1.emit({type:"retained_resources",bytes:new Uint8Array([1])});
      assert.equal(receivers[0](null,{session:1,sequence:0},"initial"),true);
      await spin();
      if(backend==="WebGPU"){gpu.resolve();await spin();}
      assert.ok(messages.some(m=>m.type==="engine_started"),"the first frame admits source");
    },
    async rebuild(port,bytes,requestId,session){
      await dispatch({type:"rebuild_engine",port,requestId,mode:"retained",transportMode:"transferable"});
      port.emit({type:"retained_resources",bytes:new Uint8Array(bytes)});
      assert.equal(receivers.at(-1)(null,{session,sequence:0},"next"),true);
      await spin();
    },
    stop(){controller.shutdown();},
  };
}

test("WebGL consecutive authoring edits reuse one GPU renderer and wait for actual presentation",async()=>{
  const h=harness();await h.setup();assert.equal(h.createCount(),1);
  h.setRenders(false,true);
  await h.rebuild(h.port2,[4,5],9,2);
  assert.equal(h.createCount(),1,"WebGL adapter/device must not be recreated");
  assert.equal(h.freeCount(),0,"existing device must remain owned");
  assert.deepEqual(h.bundles,[[4,5]]);
  assert.equal(h.messages.some(m=>m.type==="renderer_rebuilt"),false,"failed present cannot acknowledge");
  assert.equal(h.port1.closed,true);
  for(const cb of h.frames.splice(0))cb(20);
  await spin();
  assert.equal(h.messages.filter(m=>m.type==="renderer_rebuilt"&&m.requestId===9).length,1);
  assert.ok(h.events.indexOf("reset")<h.events.lastIndexOf("apply:next"));
  await h.rebuild(h.port3,[7],10,3);
  assert.equal(h.messages.filter(m=>m.type==="renderer_rebuilt").length,2);
  assert.equal(h.createCount(),1,"consecutive edits must not reselect the GPU");
  assert.deepEqual(h.bundles,[[4,5],[7]]);
  h.stop();assert.equal(h.freeCount(),1,"device freed once at worker shutdown");
});

test("WebGPU reused device retains the submitted-work fence before source admission",async()=>{
  const h=harness({backend:"WebGPU"});await h.setup();
  const fence=h.holdGpu();
  await h.rebuild(h.port2,[8],15,3);
  assert.equal(h.createCount(),1);
  assert.equal(h.messages.filter(m=>m.type==="renderer_rebuilt").length,0);
  fence.resolve();await spin();
  assert.equal(h.messages.filter(m=>m.type==="renderer_rebuilt").length,1);
  h.stop();
});

test("failed bundle validation never acknowledges a replacement or creates another device",async()=>{
  const h=harness({failReset:true});await h.setup();
  await h.rebuild(h.port2,[0],17,4);
  assert.equal(h.createCount(),1);
  assert.equal(h.freeCount(),0);
  assert.equal(h.messages.some(m=>m.type==="renderer_rebuilt"),false);
  assert.ok(h.messages.some(m=>m.type==="error"&&m.requestId===17&&/invalid scene bundle/.test(m.message)));
  h.stop();assert.equal(h.freeCount(),1);
});

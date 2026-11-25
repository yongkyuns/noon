// Independent scalar specification. No generated shader/uniform/runtime data is
// read here: inputs are authored removal progress, style and capture dimensions.
export function radialHash(seed, x, y) {
  let h=(Math.imul(x,0x9e3779b9)^Math.imul(y,0x85ebca6b)^seed)>>>0;
  h=Math.imul(h^(h>>>16),0x7feb352d)>>>0;
  h=Math.imul(h^(h>>>15),0x846ca68b)>>>0;
  return (h^(h>>>16))>>>0;
}
function noise(seed,x,y) {
  const ix=Math.floor(x),iy=Math.floor(y),fx=x-ix,fy=y-iy;
  const tx=fx*fx*(3-2*fx),ty=fy*fy*(3-2*fy);
  let result=0;
  for(let j=0;j<2;j++) for(let i=0;i<2;i++) {
    result+=(radialHash(seed,ix+i,iy+j)>>>8)/16777216*(i?tx:1-tx)*(j?ty:1-ty);
  }
  return result;
}
const clamp=x=>Math.max(0,Math.min(1,x));
function smooth(a,b,x) {const t=clamp((x-a)/(b-a));return t*t*(3-2*t);}
export const cleanRadial=Object.freeze({center:[.5,.5],axes:[.5,.5],softness:.02,seed:0,
  roughness:0,frequency:12,rimWidth:0,innerColor:[1,.85,.24,1],outerColor:[.24,.035,.01,1],hole:false});
export const burnRadial=Object.freeze({...cleanRadial,softness:.025,roughness:.1,rimWidth:.12});

// Source is encoded-premultiplied RGBA in [0,1]; uv is upright normalized
// capture-bounds position. Tint alpha is strength and cannot create coverage.
export function radialSample(source,uv,progress,style={},dimensions=[180,90]) {
  if(progress<=0) return source.slice();
  if(progress>=1) return [0,0,0,0];
  const p={...cleanRadial,...style};
  const q=uv.map((u,i)=>(u-p.center[i])/p.axes[i]);
  const maximum=Math.hypot(...p.center.map((c,i)=>Math.max(c,1-c)/p.axes[i]));
  const footprint=Math.hypot(...dimensions.map((n,i)=>1/n/p.axes[i]));
  const unresolved=Math.max(...dimensions.map(n=>1/n))*p.frequency;
  const roughness=p.roughness*(1-smooth(.5,1,unresolved));
  const distance=Math.hypot(...q)+roughness*(2*noise(p.seed,uv[0]*p.frequency,uv[1]*p.frequency)-1);
  const width=Math.max(p.softness,footprint),margin=p.roughness+width+p.rimWidth;
  const opening=p.hole?progress:1-progress;
  const radius=-margin+(maximum+2*margin)*opening;
  const signed=p.hole?distance-radius:radius-distance;
  const coverage=smooth(-width/2,width/2,signed);
  const color=source.slice();
  if(p.rimWidth>0) {
    const phase=clamp(signed/p.rimWidth);
    const alpha=p.innerColor[3]*(1-phase)+p.outerColor[3]*phase;
    const band=1-smooth(Math.max(0,p.rimWidth-footprint),p.rimWidth+footprint,signed);
    for(let c=0;c<3;c++) {
      const rim=p.innerColor[c]*p.innerColor[3]*(1-phase)+p.outerColor[c]*p.outerColor[3]*phase;
      color[c]=source[c]*(1-band*alpha)+rim*source[3]*band;
    }
  }
  return color.map(c=>c*coverage);
}

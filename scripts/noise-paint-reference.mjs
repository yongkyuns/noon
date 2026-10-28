// Independent integer/point-polynomial + piecewise Gauss quadrature reference.
export function lattice(seed, x, y) {
  const cell = n => BigInt(((n % 65536) + 65536) % 65536);
  const mask = 0xffffffffn;
  let n = BigInt(seed) ^ ((cell(x)*0x9e3779b9n)&mask) ^ ((cell(y)*0x85ebca6bn)&mask);
  for (const [shift,multiplier] of [[16n,0x7feb352dn],[15n,0x846ca68bn]])
    n = ((n ^ (n >> shift))*multiplier)&mask;
  n ^= n >> 16n;
  return Number(n >> 16n)/65535;
}
function point(seed,x,y) {
  const ix=Math.floor(x), iy=Math.floor(y);
  const curve=t=>10*t**3-15*t**4+6*t**5;
  const u=curve(x-ix), v=curve(y-iy);
  return (1-u)*(1-v)*lattice(seed,ix,iy)+u*(1-v)*lattice(seed,ix+1,iy)
    +(1-u)*v*lattice(seed,ix,iy+1)+u*v*lattice(seed,ix+1,iy+1);
}
function quadrature(center,width) {
  if(width<1/256) return [[center,1]];
  const a=center-width/2,b=center+width/2,cuts=[a], split=Math.floor(a)+1;
  if(split<b)cuts.push(split);
  cuts.push(b);
  const nodes=[];
  for(let i=1;i<cuts.length;i++) {
    const half=(cuts[i]-cuts[i-1])/2,mid=(cuts[i]+cuts[i-1])/2;
    for(const [node,weight] of [[-Math.sqrt(3/5),5/9],[0,8/9],[Math.sqrt(3/5),5/9]])
      nodes.push([mid+node*half,weight*half/width]);
  }
  return nodes;
}
export function filtered(seed,uv,frequency,phase,widths) {
  const coarse=Math.max(...widths);
  if(coarse>=2)return 0.5;
  const q=uv.map((v,i)=>Math.max(0,Math.min(1,v))*frequency[i]+((phase[i]%65536)+65536)%65536);
  let value=0;
  for(const [x,wx] of quadrature(q[0],Math.min(1,widths[0])))
    for(const [y,wy] of quadrature(q[1],Math.min(1,widths[1]))) value+=wx*wy*point(seed,x,y);
  const a=Math.max(0,Math.min(1,coarse-1)),fade=3*a*a-2*a*a*a;
  return value*(1-fade)+0.5*fade;
}

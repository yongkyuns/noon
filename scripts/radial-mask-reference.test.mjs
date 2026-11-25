import assert from 'node:assert/strict';
import test from 'node:test';
import {radialHash,radialSample,burnRadial} from './radial-mask-reference.mjs';

test('seed field has fixed unsigned and negative-coordinate vectors',()=>{
  for(const [s,x,y,expected] of [[0,0,0,0],[7,1,2,0xb02adea4],[0xffffffff,-1,3,0x56d60568]])
    assert.equal(radialHash(s,x,y),expected);
});
test('removal endpoints preserve premultiplication and hidden image holes',()=>{
  for(const hole of [false,true]) for(const source of [[0,0,0,0],[.1,.2,.3,.4],[.2,.4,.9,1]]) {
    assert.deepEqual(radialSample(source,[.3,.7],0,{...burnRadial,hole}),source);
    assert.deepEqual(radialSample(source,[.3,.7],1,{...burnRadial,hole}),[0,0,0,0]);
    for(const p of [.1,.5,.9]) {
      const value=radialSample(source,[.3,.7],p,{...burnRadial,hole});
      assert.ok(value.every(c=>c>=0&&c<=source[3]+1e-14));
      assert.ok(value[3]<=source[3]);
    }
  }
});
test('both field orientations remove coverage monotonically in nonsquare bounds',()=>{
  for(const hole of [false,true]) for(const uv of [[.5,.5],[.1,.5],[.9,.2],[.22,.78]]) {
    let last=1;
    for(let i=0;i<=100;i++) {
      const alpha=radialSample([1,1,1,1],uv,i/100,{...burnRadial,hole,seed:7,center:[.35,.6],axes:[.4,.8]},[211,53])[3];
      assert.ok(alpha<=last+1e-14);last=alpha;
    }
  }
});
test('iris retains center and hole retains outside at halfway; footprint stays finite',()=>{
  assert.equal(radialSample([1,1,1,1],[.5,.5],.5)[3],1);
  assert.equal(radialSample([1,1,1,1],[.5,.5],.5,{hole:true})[3],0);
  for(const dimensions of [[180,90],[4,2],[1,1]]) {
    const out=radialSample([.3,.3,.3,.5],[.25,.5],.5,{...burnRadial,frequency:64},dimensions);
    assert.ok(out.every(Number.isFinite));
  }
});

test('under-resolved roughness loses seed variation at its zero-mean limit',()=>{
  for(const uv of [[.1,.2],[.5,.5],[.8,.7]]) {
    const base={...burnRadial,roughness:1,frequency:64};
    assert.deepEqual(radialSample([.2,.4,.9,1],uv,.5,{...base,seed:7},[4,2]),
      radialSample([.2,.4,.9,1],uv,.5,{...base,seed:0xffffffff},[4,2]));
  }
});

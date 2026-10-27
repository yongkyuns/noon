import test from "node:test";
import assert from "node:assert/strict";
import { lattice, filtered } from "./noise-paint-reference.mjs";
test("literal seed vectors and rational point/box means",()=> {
  for(const [seed,x,y,n] of [[0,0,0,0],[7,0,0,38027],[7,1,0,42773],[7,0,1,19981],[7,1,1,46635],[4294967295,65535,65535,48434]])
    assert.equal(lattice(seed,x,y),n/65535);
  for(const widths of [[0,0],[1,1]])
    assert.ok(Math.abs(filtered(7,[0.5,0.5],[1,1],[0,0],widths)-36854/65535)<1e-12);
  assert.equal(filtered(7,[0.25,0.75],[1,1],[0,0],[2,0.1]),0.5);
});

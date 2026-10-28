import assert from "node:assert/strict";
import test from "node:test";
import { releaseRetirementWitness as witness } from "./audio-retirement-reference.mjs";

function fixture() {
  const source = { node:4, context:0, source:1, busNode:2, busParameter:1,
    loop:false, args:[0.08126984126984127,0,30] };
  const deadline = 0.27188208616780046;
  const snapshot = {
    stops:[{node:4,context:0,args:[deadline],observedAt:0.21188208616780047,order:10}],
    ended:[{node:4,context:0,trusted:true,observedAt:0.2699319727891156,order:11}],
    disconnects:[{node:2,context:0,observedAt:0.2699319727891156,order:13}],
    parameters:[{}, {events:[{method:"value",args:[0],observedAt:0.2699319727891156,order:12}]}],
  };
  return {source,deadline,snapshot};
}
test("trusted completion permits same-block clock observations with exact scheduled stop", () => {
  const {snapshot,source,deadline}=fixture();
  const result=witness(snapshot,source,deadline);
  assert.equal(result.kind,"native-ended");
  assert.equal(result.stop.args[0],deadline);
  assert.ok(result.disconnect.observedAt<deadline);
});
test("later device-clock retirement remains qualified", () => {
  const {snapshot,source,deadline}=fixture();
  snapshot.disconnects[0].observedAt=deadline;
  assert.equal(witness(snapshot,source,deadline).kind,"device-time");
});
test("device-clock cleanup may precede dispatch of an already finished source event", () => {
  const {snapshot,source,deadline}=fixture();
  snapshot.disconnects[0].observedAt=deadline;
  snapshot.ended[0].order=15;
  snapshot.parameters[1].events[0].observedAt=deadline;
  assert.equal(witness(snapshot,source,deadline).kind,"device-time");
});
for (const [name,mutate] of [
  ["early disconnection before native completion", s=>{s.disconnects[0].order=10.5;}],
  ["early ordered disconnection", s=>{s.ended[0].order=20;s.parameters[1].events=[];}],
  ["untrusted synthetic event", s=>{s.ended[0].trusted=false;}],
  ["another source's event", s=>{s.ended[0].node=99;}],
  ["another context's event", s=>{s.ended[0].context=1;}],
  ["missing event", s=>{s.ended=[];}],
  ["missing causal order", s=>{delete s.disconnects[0].order;}],
  ["superseding immediate stop", s=>{s.stops.push({node:4,context:0,args:[],observedAt:.25,order:11});s.ended[0].order=12;}],
  ["shortened stop", s=>{s.stops[0].args[0]=.25;}],
  ["premature immediate bus silence", s=>{s.parameters[1].events[0].order=11;s.ended[0].order=12;}],
  ["later disconnect hiding an early one", s=>{s.disconnects.unshift({node:2,context:0,observedAt:.23,order:11});s.ended[0].order=12;}],
]) test("rejects "+name, () => {
  const {snapshot,source,deadline}=fixture();mutate(snapshot);
  assert.equal(witness(snapshot,source,deadline),null);
});
test("natural completion cannot excuse a stop later than the finite source", () => {
  const {snapshot,source,deadline}=fixture();source.args[2]=.1;
  assert.equal(witness(snapshot,source,deadline),null);
});

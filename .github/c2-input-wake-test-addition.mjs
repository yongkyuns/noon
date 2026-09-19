
// Input must observe the active Rust wake owner, not the independently paused
// ordinary playback clock. Cover animation and pure-wait leases on every lane.
const continuationWakeInputCases = [
  ["native_state_input", {
    source: { kind: "control", name: "opacity" },
    value: { kind: "scalar", value: 0.75 },
  }],
  ["native_event", { source: { kind: "wheel" } }],
  ["browser_pointer_input", {
    kind: "move", surface_x: 200, surface_y: 100,
    viewport_width: 800, viewport_height: 400, view_revision: 0,
  }],
];

for (const [inputType, fields] of continuationWakeInputCases) {
  for (const cadence of ["animation_frame", "timer"]) {
    test(`native input preserves active continuation wake: ${inputType}/${cadence}`, { timeout: 5000 }, async () => {
      let completed;
      let failed;
      const completion = new Promise((resolve, reject) => { completed = resolve; failed = reject; });
      const f = fixture("transferable", null, {
        generation: 71,
        onComplete: completed,
        onError: (_generation, error) => failed(error),
      });
      let endpoint;
      let ordinaryReads = 0;
      let segmentReads = 0;
      let delay = 250;
      try {
        // A real live drive pauses the ordinary clock while its segment remains
        // active. A generic executionWake would therefore report idle here.
        f.player.pause();
        f.player.executionWake = () => {
          ordinaryReads += 1;
          return { cadence: "idle", timerAfterMilliseconds: undefined };
        };
        f.player.liveSegmentWake = () => {
          segmentReads += 1;
          return { cadence, timerAfterMilliseconds: cadence === "timer" ? delay : undefined };
        };
        const ready = next(f.control.port2);
        const initial = nextMatching(f.render.port2, (message) => message.type === "execution_delta");
        const initialWake = nextMatching(f.render.port2, (message) => message.type === "execution_wake");
        endpoint = await f.attach();
        await ready;
        assert.equal((await initialWake).cadence, cadence);
        const publication = await initial;
        f.render.port2.postMessage({ type: "execution_ack", session: publication.session, sequence: publication.sequence });
        f.render.port2.postMessage({ type: "execution_presented", session: publication.session, sequence: publication.sequence });
        await turn();

        const inputWake = nextMatching(f.render.port2, (message) => message.type === "execution_wake");
        const reply = await request(f.control.port2, inputType, 170, fields);
        assert.equal(reply.type, inputType, reply.message);
        const wake = await inputWake;
        assert.equal(wake.cadence, cadence, "input must not replace an active segment wake with idle");
        assert.equal(wake.timerAfterMilliseconds, cadence === "timer" ? delay : null);
        assert.equal(ordinaryReads, 0, "input cannot observe the ordinary playback clock during a lease");
        assert.equal(segmentReads, 2);
        assert.equal(f.stats().nativeInputs.length, 1);
        assert.equal(f.player.time(), 0, "input never advances authored time");
        assert.equal(f.stats().completedSegments, 0);
        assert.equal(f.stats().returned, 0);

        // The next due platform wake still reaches shared segment completion,
        // instead of requiring another input or an unrelated timer to unstick it.
        delay = 0;
        f.render.port2.postMessage({ type: "tick", timestamp: 1 });
        assert.equal(await completion, 71);
        assert.equal(f.stats().completedSegments, 1);
        assert.equal(f.stats().returned, 1);
        assert.equal(f.player.time(), 1);
      } finally { endpoint?.stop(); f.close(); }
    });
  }
}

test("external sample input never arms a realtime continuation wake", { timeout: 5000 }, async () => {
  const f = fixture("transferable", null, {
    generation: 72, onComplete: () => {}, onError: () => {},
  }, { pacing: "external_samples" });
  let endpoint;
  let wakeReads = 0;
  const wakes = [];
  try {
    f.player.executionWake = f.player.liveSegmentWake = () => {
      wakeReads += 1;
      return { cadence: "animation_frame" };
    };
    f.render.port2.on("message", (message) => {
      if (message.type === "execution_wake") wakes.push(message.cadence);
    });
    const ready = next(f.control.port2);
    endpoint = await f.attach();
    await ready;
    let requestId = 180;
    for (const [inputType, fields] of continuationWakeInputCases) {
      const reply = await request(f.control.port2, inputType, requestId++, fields);
      assert.equal(reply.type, inputType, reply.message);
    }
    await turn();
    assert.deepEqual(wakes, ["idle"]);
    assert.equal(wakeReads, 0);
    assert.equal(f.player.time(), 0);
    assert.equal(f.stats().completedSegments, 0);
  } finally { endpoint?.stop(); f.close(); }
});

test("rejected continuation pointer input preserves its existing wake", { timeout: 5000 }, async () => {
  const f = fixture("transferable", null, {
    generation: 73, onComplete: () => {}, onError: () => {},
  });
  let endpoint;
  let ordinaryReads = 0;
  let segmentReads = 0;
  const wakes = [];
  try {
    f.player.executionWake = () => { ordinaryReads += 1; return { cadence: "idle" }; };
    f.player.liveSegmentWake = () => {
      segmentReads += 1;
      return { cadence: "timer", timerAfterMilliseconds: 250 };
    };
    f.player.submitBrowserPointerInputJson = () => { throw new Error("pointer rejected"); };
    f.render.port2.on("message", (message) => {
      if (message.type === "execution_wake") wakes.push(message.cadence);
    });
    const ready = next(f.control.port2);
    endpoint = await f.attach();
    await ready;
    const reply = await request(f.control.port2, "browser_pointer_input", 190, continuationWakeInputCases[2][1]);
    assert.equal(reply.type, "error");
    assert.match(reply.message, /pointer rejected/);
    await turn();
    assert.deepEqual(wakes, ["timer"]);
    assert.equal(segmentReads, 1);
    assert.equal(ordinaryReads, 0);
    assert.equal(f.stats().nativeInputs.length, 0);
    assert.equal(f.player.time(), 0);
  } finally { endpoint?.stop(); f.close(); }
});

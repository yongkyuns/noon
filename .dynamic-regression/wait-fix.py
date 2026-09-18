from pathlib import Path
import subprocess

path = Path('crates/noon-web/src/semantic_execution_player.rs')
s = path.read_text()
old = '''        let mut wake = self.session.wake_state();
        if callback_blocked || !self.clock.is_playing() {
            wake = wake.without_timeline_wake();
        } else if let Some(loop_duration) = self.clock.loop_duration() {
            if self.session.has_replay_timeline_work() {
                wake = wake.with_additional_timeline(TimelineWakeState::Deadline(loop_duration));
            }
        }
        let plan = BrowserExecutionWakePlan::from_runtime(wake);'''
assert s.count(old) == 1
s = s.replace(old, '        let plan = self.execution_wake_plan();')
key = '''    /// Begin the next browser wall-time interval after required host work.'''
assert s.count(key) == 1
s = s.replace(key, '''    #[cfg(any(target_arch = "wasm32", test))]
    fn execution_wake_plan(&self) -> BrowserExecutionWakePlan {
        let callback_blocked =
            self.pending_callback_phase.is_some() || self.session.callback_termination().is_some();
        let mut wake = self.session.wake_state();
        if callback_blocked || !self.clock.is_playing() {
            wake = wake.without_timeline_wake();
        } else if let Some(loop_duration) = self.clock.loop_duration() {
            // A completed authored wait has duration even without animated
            // channels. An un-authored static scene still remains fully idle.
            let authored_interval = self.live_segment.is_some_and(|receipt| {
                receipt.segment().end_time() > 0.0
            });
            if self.session.has_replay_timeline_work() || authored_interval {
                wake = wake.with_additional_timeline(TimelineWakeState::Deadline(loop_duration));
            }
        }
        BrowserExecutionWakePlan::from_runtime(wake)
    }

    /// Observe elapsed playback during a visually static interval without evaluating
    /// a frame, publishing a delta, invoking callbacks or changing either clock.
    /// Active animation/callback work stays pinned to its coherent runtime sample.
    /// Sleeping waits project the existing Rust clock only up to the next runtime
    /// barrier; they cannot speculate past a source segment or a loop boundary.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn playback_time_at(&self, wall_time_ms: f64) -> Result<f64, String> {
        if !wall_time_ms.is_finite() {
            return Err("playback observation requires a finite wall timestamp".to_owned());
        }
        let current = self.time();
        if self.pending_callback_phase.is_some() || self.session.callback_termination().is_some() {
            return Ok(current);
        }
        if let Some(LiveSegmentReceipt::Pending(segment)) = self.live_segment {
            return Ok(match self.session.segment_state(segment).timeline() {
                TimelineWakeState::Deadline(deadline) => self.live_wake_clock
                    .scene_time_at(wall_time_ms)
                    .unwrap_or(current)
                    .min(deadline)
                    .max(current),
                TimelineWakeState::Continuous | TimelineWakeState::Quiescent => current,
            });
        }
        if !self.clock.is_playing() {
            return Ok(current);
        }
        let BrowserExecutionCadence::TimerAtSceneTime(deadline) = self.execution_wake_plan().cadence() else {
            return Ok(current);
        };
        // Project through the same loop-aware deadline conversion used by wake
        // delivery. A copy keeps observation from starting/reanchoring playback.
        let remaining = self.clock.clone()
            .timer_delay_milliseconds(deadline, wall_time_ms, current)
            .map_err(|error| error.to_string())?;
        Ok((deadline - remaining / 1_000.0).min(deadline).max(current))
    }

''' + key)
key = '''    /// Reanchor the next browser interval after a required callback completes.'''
assert s.count(key) == 1
s = s.replace(key, '''    /// Current playback position during a static wait, without a new runtime frame.
    #[cfg(any(target_arch = "wasm32", test))]
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen(js_name = playbackTimeAt))]
    pub fn playback_time_at_wasm(&self, wall_time_ms: f64) -> Result<f64, String> {
        self.playback_time_at(wall_time_ms)
    }

''' + key)
key = '''        player.interrupt_callback_phase_json(&phase).unwrap();
        let termination: serde_json::Value ='''
assert s.count(key) == 1
s = s.replace(key, '''        assert_eq!(player.playback_time_at(50_000.0).unwrap(), player.time());
        player.interrupt_callback_phase_json(&phase).unwrap();
        assert_eq!(player.playback_time_at(60_000.0).unwrap(), player.time());
        let termination: serde_json::Value =''')
key = '''    #[test]
    fn generic_wake_suppresses_timeline_cadence_while_paused() {'''
assert s.count(key) == 1
s = s.replace(key, '''    #[test]
    fn wait_observations_advance_without_runtime_frames_or_publications() {
        let mut scene = noon::Scene::new();
        let circle = scene.circle(0.4).unwrap();
        scene.add(&circle).unwrap();
        let session = scene.execution_session().unwrap();
        let mut player = SemanticExecutionPlayer::from_live_session(
            session, std::rc::Rc::clone(scene.integration_store()), scene.root(), 3.0, 71,
        ).unwrap();
        player.initial_delta_json().unwrap();
        player.live_wait(2.0).unwrap();
        assert_eq!(player.playback_time_at(900.0).unwrap(), 0.0);
        let wake = player.live_segment_wake(1_000.0).unwrap();
        assert_eq!(wake.cadence(), "timer");
        let frame = player.session.frame().clone();
        let clock = player.clock.clone();
        for (wall, elapsed) in [(1_250.0, 0.25), (1_500.0, 0.5), (2_500.0, 1.5), (4_000.0, 2.0)] {
            assert_eq!(player.playback_time_at(wall).unwrap(), elapsed);
            assert_eq!(player.session.frame(), &frame);
            assert_eq!(player.clock, clock);
            assert!(player.drain_delta_json().unwrap().is_none());
        }
        assert!(player.playback_time_at(f64::NAN).is_err());
        assert!(player.live_drive_segment_from_wall_time(3_000.0).unwrap().reached_endpoint());
        player.live_complete_segment().unwrap();
        assert_eq!(player.time(), 2.0);
        player.live_wait(1.0).unwrap();
        player.live_segment_wake(8_000.0).unwrap();
        assert_eq!(player.playback_time_at(8_500.0).unwrap(), 2.5);
        assert_eq!(player.playback_time_at(10_000.0).unwrap(), 3.0);
        player.live_drive_segment_to_authored_time(3.0).unwrap();
        player.live_complete_segment().unwrap();
        player.drain_delta_json().unwrap();
        // Pure waits still have a replay clock, even with zero animation tracks.
        assert!(!player.session.has_replay_timeline_work());
        player.seek_delta_json(0.0).unwrap();
        player.resume();
        assert_eq!(player.execution_wake(20_000.0).unwrap().cadence(), "timer");
        assert_eq!(player.playback_time_at(20_500.0).unwrap(), 0.5);
        assert_eq!(player.time(), 0.0);
    }

    #[test]
    fn replay_wait_observation_is_loop_bounded_and_does_not_advance_active_animation() {
        let mut player = animated_player();
        player.initial_delta_json().unwrap();
        player.execution_wake(10_000.0).unwrap();
        assert_eq!(player.playback_time_at(10_500.0).unwrap(), 0.0);
        player.tick_callback_phase_json(11_000.0).unwrap();
        player.drain_delta_json().unwrap();
        let frame = player.session.frame().clone();
        let clock = player.clock.clone();
        assert_eq!(player.playback_time_at(11_250.0).unwrap(), 1.25);
        assert_eq!(player.playback_time_at(11_750.0).unwrap(), 1.75);
        assert_eq!(player.playback_time_at(12_500.0).unwrap(), 2.0);
        assert_eq!(player.session.frame(), &frame);
        assert_eq!(player.clock, clock);
        assert!(player.drain_delta_json().unwrap().is_none());
        player.pause();
        assert_eq!(player.playback_time_at(20_000.0).unwrap(), 1.0);
        player.resume();
        player.execution_wake(30_000.0).unwrap();
        assert_eq!(player.playback_time_at(30_250.0).unwrap(), 1.25);
    }

''' + key)
path.write_text(s)

path = Path('web/semantic-engine-endpoint.js')
s = path.read_text()
old = '''      type, time: player.time(), playing: player.isPlaying(), nextPatchSequence: "0",'''
assert s.count(old) == 1
s = s.replace(old, '''      type,
      // Observe the Rust clock during idle waits. Command acknowledgements and
      // external samples retain exact evaluated-frame time, independent of wall time.
      time: type === "state" && pacing === SEMANTIC_PACING_REALTIME
        ? player.playbackTimeAt(performance.now()) : player.time(),
      playing: player.isPlaying(), nextPatchSequence: "0",''')
old = '''          case "pause":
            player.pause();
            observeExecutionWake(performance.now(), true);
            break;'''
assert s.count(old) == 1
s = s.replace(old, '''          case "pause": {
            latestTick = null;
            if (pacing === SEMANTIC_PACING_REALTIME) {
              const time = player.playbackTimeAt(performance.now());
              // Commit elapsed static time before freezing, so pause/resume does
              // not jump back to the previous rendered frame's timestamp.
              if (time > player.time()) await advanceToAuthoredTime(time);
            }
            player.pause();
            observeExecutionWake(performance.now(), true);
            break;
          }''')
path.write_text(s)

path = Path('web/semantic-engine-endpoint.test.mjs')
s = path.read_text()
assert s.count('    time: () => time, isPlaying: () => playing,') == 1
s = s.replace('    time: () => time, isPlaying: () => playing,', '    time: () => time, playbackTimeAt: () => time, isPlaying: () => playing,')
s += '''


test("real-time state observes the Rust wait clock without driving or publishing", async () => {
  const f = fixture();
  let endpoint;
  const observations = [];
  f.player.playbackTimeAt = now => { observations.push(now); return 0.75; };
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach(); await ready;
    const before = f.stats();
    const observed = await request(f.control.port2, "state", 900);
    assert.equal(observed.time, 0.75);
    assert.equal(f.player.time(), 0);
    assert.equal(observations.length, 1);
    assert.ok(Number.isFinite(observations[0]));
    assert.equal(f.stats().drained, before.drained);
    assert.deepEqual(f.stats().continuationDriveTimes, []);
    assert.deepEqual(f.stats().authoredSampleTimes, []);
  } finally { endpoint?.stop(); f.close(); }
});

test("pausing in a wait commits the observed time before freezing the replay clock", async () => {
  const f = fixture(); let endpoint;
  f.player.playbackTimeAt = () => f.player.isPlaying() ? 0.75 : f.player.time();
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach(); await ready;
    const paused = await request(f.control.port2, "pause", 901);
    assert.equal(paused.time, 0.75);
    assert.equal(paused.playing, false);
    assert.equal(f.player.time(), 0.75);
    assert.equal((await request(f.control.port2, "state", 902)).time, 0.75);
  } finally { endpoint?.stop(); f.close(); }
});


test("external-sample state never projects wall time", async () => {
  const f = fixture("transferable", null, { generation: 93, onComplete() {}, onError() {} }, { pacing: "external_samples" });
  let endpoint;
  f.player.playbackTimeAt = () => { throw new Error("unexpected wall-time projection"); };
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach(); await ready;
    f.player.seekDeltaJson(0.25);
    assert.equal((await request(f.control.port2, "state", 903)).time, 0.25);
    assert.deepEqual(f.stats().authoredSampleTimes, []);
  } finally { endpoint?.stop(); f.close(); }
});

test("seek acknowledgements retain the exact evaluated time", async () => {
  const f = fixture(); let endpoint;
  f.player.playbackTimeAt = () => { throw new Error("unexpected wall-time projection"); };
  try {
    const ready = next(f.control.port2);
    endpoint = await f.attach(); await ready;
    assert.equal((await request(f.control.port2, "seek", 904, { time: 0.25 })).time, 0.25);
  } finally { endpoint?.stop(); f.close(); }
});
'''
path.write_text(s)

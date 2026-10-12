//! Test the actual Rust presentation clock, not a JavaScript time accumulator.
//! Host continuation/callback reanchoring is separately tracked by #1948.
use noon_web::PlaybackClock;

#[test]
fn frame_density_does_not_change_elapsed_playback_time() {
    for fps in [5, 15, 24, 30, 60, 120] {
        let mut clock = PlaybackClock::once();
        assert_eq!(clock.scene_time(12_000.0).unwrap(), 0.0);
        for frame in 1..=2 * fps {
            let seconds = f64::from(frame) / f64::from(fps);
            let actual = clock.scene_time(12_000.0 + 1_000.0 * seconds).unwrap();
            assert!((actual - seconds).abs() < 1.0e-12);
        }
        assert_eq!(clock.scene_time(14_000.0).unwrap(), 2.0);
    }
}

#[test]
fn dropped_wakes_and_long_stalls_do_not_discard_elapsed_time() {
    let mut clock = PlaybackClock::once();
    clock.scene_time(1_000.0).unwrap();
    for elapsed_ms in [7.0, 16.0, 201.0, 202.0, 937.0, 1_701.0, 2_000.0, 9_000.0] {
        let actual = clock.scene_time(1_000.0 + elapsed_ms).unwrap();
        assert!((actual - elapsed_ms / 1_000.0).abs() < 1.0e-12);
    }
}

#[test]
fn only_explicit_pause_and_seek_change_the_playback_time_mapping() {
    let mut clock = PlaybackClock::once();
    clock.scene_time(1_000.0).unwrap();
    assert_eq!(clock.scene_time(1_750.0).unwrap(), 0.75);
    clock.pause();
    assert_eq!(clock.scene_time(9_000.0).unwrap(), 0.75);
    clock.resume();
    assert_eq!(clock.scene_time(10_000.0).unwrap(), 0.75);
    assert_eq!(clock.scene_time(10_250.0).unwrap(), 1.0);
    clock.seek(12.5).unwrap();
    assert_eq!(clock.scene_time(20_000.0).unwrap(), 12.5);
    assert_eq!(clock.scene_time(22_000.0).unwrap(), 14.5);
}

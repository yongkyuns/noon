//! Process fixtures use a file gate, not timing assumptions, to keep stdin blocked.
#![cfg(target_os = "linux")]

use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);
const BYTES: usize = 1024 * 1024;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "noon-pipeline-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }

    fn command(&self, name: &str) -> Command {
        let mut command = Command::new("python3");
        command.args(["-c", r#"
import fcntl, pathlib, sys, time
# Make the outstanding-write assertions independent of the runner pipe size.
fcntl.fcntl(0, fcntl.F_SETPIPE_SZ, 4096)
root, name = pathlib.Path(sys.argv[1]), sys.argv[2]
(root / (name + '.ready')).write_text('ready')
while not (root / (name + '.release')).exists():
    time.sleep(0.001)
with (root / (name + '.raw')).open('wb') as output:
    while True:
        data = sys.stdin.buffer.read(8191)
        if not data:
            break
        output.write(data)
"#]);
        command.arg(&self.0).arg(name);
        command
    }

    fn ready(&self, name: &str) {
        wait_for(&self.0.join(format!("{name}.ready")));
    }

    fn release(&self, name: &str) {
        fs::write(self.0.join(format!("{name}.release")), b"release").unwrap();
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn wait_for(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        assert!(start.elapsed() < Duration::from_secs(10), "process fixture did not start");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
#[ignore = "requires Linux and Python3; selected by native output gate"]
fn two_buffers_overlap_blocked_input_and_match_serial_bytes() {
    let fixture = Fixture::new();
    let cancellation = CaptureCancellation::default();
    let mut encoder = Encoder::new(
        &mut fixture.command("pipeline"), BYTES, cancellation.clone(), Duration::from_secs(10),
    ).unwrap();
    fixture.ready("pipeline");
    let addresses: Vec<_> = encoder.buffers.iter().map(|b| b.as_ptr()).collect();
    let mut pixels = vec![17; BYTES];
    encoder.enqueue(&pixels).unwrap();
    pixels.fill(34);
    encoder.enqueue(&pixels).unwrap();
    // The child cannot read until this test releases it. Both enqueues already
    // returned, so the producer can do new render work while pipe I/O is blocked.
    assert_eq!(encoder.pending.len(), FRAME_BUFFERS);
    assert!(encoder.buffers.is_empty());
    pixels.fill(255); // Mutating the capture buffer cannot alter either queued frame.
    fixture.release("pipeline");
    for value in [51, 68, 85] {
        pixels.fill(value);
        encoder.enqueue(&pixels).unwrap();
        assert!(encoder.pending.len() <= FRAME_BUFFERS);
        assert_eq!(encoder.pending.len() + encoder.buffers.len(), FRAME_BUFFERS);
    }
    encoder.drain().unwrap();
    assert_eq!(encoder.buffers.len(), FRAME_BUFFERS);
    assert!(encoder.buffers.iter().all(|b| addresses.contains(&b.as_ptr())));
    encoder.finish().unwrap();

    fixture.release("serial");
    let mut serial = Encoder::new(
        &mut fixture.command("serial"), BYTES, cancellation, Duration::from_secs(10),
    ).unwrap();
    for value in [17, 34, 51, 68, 85] {
        pixels.fill(value);
        serial.write(&pixels).unwrap();
        assert!(serial.pending.is_empty());
    }
    serial.finish().unwrap();
    let actual = fs::read(fixture.0.join("pipeline.raw")).unwrap();
    assert_eq!(actual, fs::read(fixture.0.join("serial.raw")).unwrap());
    assert_eq!(actual.len(), 5 * BYTES);
    for (index, frame) in actual.as_chunks::<BYTES>().0.iter().enumerate() {
        let expected = [17, 34, 51, 68, 85][index];
        assert!(frame.iter().all(|&byte| byte == expected));
    }
}

#[test]
#[ignore = "requires Linux and Python3; selected by native output gate"]
fn a_full_pipeline_blocks_a_third_frame_instead_of_allocating_or_dropping() {
    let fixture = Fixture::new();
    let mut encoder = Encoder::new(
        &mut fixture.command("blocked"), BYTES, CaptureCancellation::default(), Duration::from_secs(10),
    ).unwrap();
    fixture.ready("blocked");
    encoder.enqueue(&vec![1; BYTES]).unwrap();
    encoder.enqueue(&vec![2; BYTES]).unwrap();
    assert_eq!(encoder.pending.len(), FRAME_BUFFERS);
    // Start the bounded stall check only after process startup and two accepted
    // writes. The child cannot consume data, regardless of runner scheduling.
    encoder.timeout = Duration::from_millis(200);
    for pending in &mut encoder.pending {
        pending.started = Instant::now();
    }
    assert_eq!(encoder.enqueue(&vec![3; BYTES]).unwrap_err().kind(), io::ErrorKind::TimedOut);
    assert!(!encoder.active);
    assert!(encoder.reaped);
    assert_eq!(encoder.next_sequence, 2);
}

#[test]
#[ignore = "requires Linux and Python3; selected by native output gate"]
fn finish_drains_both_outstanding_frames_before_sending_eof() {
    let fixture = Fixture::new();
    let mut encoder = Encoder::new(
        &mut fixture.command("finish"), BYTES, CaptureCancellation::default(), Duration::from_secs(10),
    ).unwrap();
    fixture.ready("finish");
    encoder.enqueue(&vec![71; BYTES]).unwrap();
    encoder.enqueue(&vec![72; BYTES]).unwrap();
    assert_eq!(encoder.pending.len(), FRAME_BUFFERS);
    fixture.release("finish");
    // No explicit drain here: successful finish itself must prove every write.
    encoder.finish().unwrap();
    let actual = fs::read(fixture.0.join("finish.raw")).unwrap();
    assert_eq!(actual.len(), 2 * BYTES);
    assert!(actual[..BYTES].iter().all(|&byte| byte == 71));
    assert!(actual[BYTES..].iter().all(|&byte| byte == 72));
}

#[test]
#[ignore = "requires Linux and Python3; selected by native output gate"]
fn cancelling_two_outstanding_writes_does_not_deadlock_on_reply_capacity() {
    let fixture = Fixture::new();
    let cancellation = CaptureCancellation::default();
    let mut encoder = Encoder::new(
        &mut fixture.command("cancel"), BYTES, cancellation.clone(), Duration::from_secs(10),
    ).unwrap();
    fixture.ready("cancel");
    encoder.enqueue(&vec![0; BYTES]).unwrap();
    encoder.enqueue(&vec![0; BYTES]).unwrap();
    assert_eq!(encoder.pending.len(), FRAME_BUFFERS);
    cancellation.cancel();
    let start = Instant::now();
    assert_eq!(encoder.finish().unwrap_err().kind(), io::ErrorKind::Interrupted);
    assert!(start.elapsed() < Duration::from_secs(10));
}

#[test]
#[ignore = "requires Linux and Python3; selected by native output gate"]
fn successful_child_exit_cannot_hide_a_failed_outstanding_write() {
    let fixture = Fixture::new();
    let mut command = Command::new("python3");
    command.args(["-c", r#"
import os, pathlib, sys
os.close(0)
pathlib.Path(sys.argv[1]).write_text('ready')
sys.exit(0)
"#]).arg(fixture.0.join("closed.ready"));
    let mut encoder = Encoder::new(
        &mut command, BYTES, CaptureCancellation::default(), Duration::from_secs(10),
    ).unwrap();
    fixture.ready("closed");
    // Submission may observe disconnection immediately or the write reply later.
    // Neither case can turn into a successful finished output.
    let _ = encoder.enqueue(&vec![17; BYTES]);
    assert!(encoder.finish().is_err());
}

mod media;

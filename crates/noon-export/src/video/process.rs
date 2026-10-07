//! An owned encoder child with bounded stderr and interruptible pipe writes.
//! Only the trusted direct child is managed; this is not a process-tree sandbox.
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::CaptureCancellation;
use super::VideoError;

const TAIL_BYTES: usize = 16 * 1024;
const POLL: Duration = Duration::from_millis(10);

#[derive(Clone, Copy)]
enum Stop {
    Cancelled,
    Timeout,
}

#[derive(Default)]
struct Watch {
    deadline: Option<Instant>,
    failure: Option<Stop>,
    done: bool,
}

pub(super) struct EncoderProcess {
    child: Arc<Mutex<Child>>,
    input: Option<ChildStdin>,
    watch: Arc<Mutex<Watch>>,
    tail: Arc<Mutex<VecDeque<u8>>>,
    monitor: Option<JoinHandle<()>>,
    reader: Option<JoinHandle<io::Result<()>>>,
    stopped: bool,
}

impl EncoderProcess {
    pub fn start(command: &mut Command, cancellation: CaptureCancellation) -> Result<Self, VideoError> {
        if cancellation.is_cancelled() {
            return Err(VideoError::Cancelled);
        }
        let mut child = command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn()?;
        let input = child.stdin.take().expect("piped encoder stdin");
        let mut stderr = child.stderr.take().expect("piped encoder stderr");
        let mut this = Self {
            child: Arc::new(Mutex::new(child)), input: Some(input),
            watch: Arc::new(Mutex::new(Watch::default())),
            tail: Arc::new(Mutex::new(VecDeque::with_capacity(TAIL_BYTES))),
            monitor: None, reader: None, stopped: false,
        };
        let tail = Arc::clone(&this.tail);
        this.reader = Some(thread::Builder::new().name("noon-encoder-stderr".into()).spawn(move || {
            let mut block = [0; 4096];
            loop {
                match stderr.read(&mut block) {
                    Ok(0) => return Ok(()),
                    Ok(count) => {
                        let mut tail = tail.lock().unwrap_or_else(|e| e.into_inner());
                        let remove = (tail.len() + count).saturating_sub(TAIL_BYTES);
                        tail.drain(..remove);
                        tail.extend(&block[..count]);
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }
        })?);
        let watch = Arc::clone(&this.watch);
        let child = Arc::clone(&this.child);
        this.monitor = Some(thread::Builder::new().name("noon-encoder-watch".into()).spawn(move || {
            loop {
                let stop = {
                    let mut state = watch.lock().unwrap_or_else(|e| e.into_inner());
                    if state.done { return; }
                    let stop = if cancellation.is_cancelled() {
                        Some(Stop::Cancelled)
                    } else if state.deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                        Some(Stop::Timeout)
                    } else { None };
                    if stop.is_some() { state.failure = stop; }
                    stop
                };
                if stop.is_some() {
                    // Terminating our child releases a blocked write_all. No
                    // scene, callback or GPU owner is transferred to this thread.
                    let _ = child.lock().unwrap_or_else(|e| e.into_inner()).kill();
                    return;
                }
                thread::sleep(POLL);
            }
        })?);
        Ok(this)
    }

    fn arm(&self, timeout: Duration) -> Result<(), VideoError> {
        let deadline = Instant::now().checked_add(timeout).filter(|_| !timeout.is_zero())
            .ok_or(VideoError::Configuration("encoder timeout must be finite and positive"))?;
        self.watch.lock().unwrap_or_else(|e| e.into_inner()).deadline = Some(deadline);
        Ok(())
    }

    fn disarm(&self) -> Option<Stop> {
        let mut watch = self.watch.lock().unwrap_or_else(|e| e.into_inner());
        watch.deadline = None;
        watch.failure
    }

    pub fn write_frame(&mut self, rgba: &[u8], timeout: Duration) -> Result<(), VideoError> {
        if self.stopped { return Err(VideoError::Inactive); }
        self.arm(timeout)?;
        let result = self.input.as_mut().ok_or(VideoError::Inactive)?.write_all(rgba);
        let stopped = self.disarm();
        if result.is_err() || stopped.is_some() {
            self.abort();
            return Err(self.failure("write frame", result.err().map(|e| e.to_string())));
        }
        Ok(())
    }

    pub fn finish(mut self, timeout: Duration) -> Result<(), VideoError> {
        self.arm(timeout)?;
        self.input.take(); // EOF: flush delayed encoder packets and finalize MP4.
        let exit = loop {
            let status = self.child.lock().unwrap_or_else(|e| e.into_inner()).try_wait();
            match status {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => thread::sleep(POLL),
                Err(error) => break Err(error),
            }
        };
        let stopped = self.disarm();
        let success = exit.as_ref().is_ok_and(|status| status.success()) && stopped.is_none();
        let detail = match &exit {
            Ok(status) => Some(format!("encoder exited with {status}")),
            Err(error) => Some(error.to_string()),
        };
        if !success {
            self.abort();
            return Err(self.failure("finalize video", detail));
        }
        self.stopped = true;
        self.watch.lock().unwrap_or_else(|e| e.into_inner()).done = true;
        if let Some(monitor) = self.monitor.take() { let _ = monitor.join(); }
        if let Some(reader) = self.reader.take() {
            reader.join().map_err(|_| VideoError::Configuration("encoder stderr reader panicked"))??;
        }
        Ok(())
    }

    fn failure(&self, operation: &'static str, detail: Option<String>) -> VideoError {
        let stopped = self.watch.lock().unwrap_or_else(|e| e.into_inner()).failure;
        if matches!(stopped, Some(Stop::Cancelled)) { return VideoError::Cancelled; }
        let detail = if matches!(stopped, Some(Stop::Timeout)) {
            "encoder operation exceeded its deadline".to_owned()
        } else { detail.unwrap_or_else(|| "encoder pipe failed".into()) };
        let bytes: Vec<_> = self.tail.lock().unwrap_or_else(|e| e.into_inner()).iter().copied().collect();
        VideoError::Encoder { operation, detail, stderr: String::from_utf8_lossy(&bytes).into_owned() }
    }

    pub fn abort(&mut self) {
        if self.stopped { return; }
        self.watch.lock().unwrap_or_else(|e| e.into_inner()).done = true;
        {
            let mut child = self.child.lock().unwrap_or_else(|e| e.into_inner());
            if !matches!(child.try_wait(), Ok(Some(_))) { let _ = child.kill(); }
        }
        self.input.take();
        if let Some(monitor) = self.monitor.take() { let _ = monitor.join(); }
        let _ = self.child.lock().unwrap_or_else(|e| e.into_inner()).wait();
        if let Some(reader) = self.reader.take() { let _ = reader.join(); }
        self.stopped = true;
    }
}

impl Drop for EncoderProcess {
    fn drop(&mut self) { self.abort(); }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spawn only this test in a separate executable, never invoke a shell or
    // mutate the parent environment. Ordinary test runs return immediately.
    #[test]
    fn encoder_child() {
        let Ok(mode) = std::env::var("NOON_TEST_ENCODER_CHILD") else { return; };
        match mode.as_str() {
            "flood" => {
                let mut stderr = io::stderr().lock();
                for _ in 0..128 { stderr.write_all(&[b'x'; 4096]).unwrap(); }
                stderr.write_all(b"FINAL-ENCODER-DIAGNOSTIC").unwrap();
                let _ = io::copy(&mut io::stdin().lock(), &mut io::sink());
                std::process::exit(17);
            }
            "stall" => thread::sleep(Duration::from_secs(60)),
            "finish-stall" => {
                let _ = io::copy(&mut io::stdin().lock(), &mut io::sink());
                thread::sleep(Duration::from_secs(60));
            }
            _ => std::process::exit(3),
        }
    }

    fn child(mode: &str, cancel: CaptureCancellation) -> EncoderProcess {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args(["--exact", "video::process::tests::encoder_child", "--nocapture"])
            .env("NOON_TEST_ENCODER_CHILD", mode);
        EncoderProcess::start(&mut command, cancel).unwrap()
    }

    #[test]
    fn stderr_is_drained_and_only_a_bounded_tail_is_retained() {
        let process = child("flood", CaptureCancellation::default());
        let error = process.finish(Duration::from_secs(10)).unwrap_err();
        let VideoError::Encoder { stderr, .. } = error else { panic!("wrong error"); };
        assert!(stderr.len() <= TAIL_BYTES);
        assert!(stderr.ends_with("FINAL-ENCODER-DIAGNOSTIC"));
    }

    #[test]
    fn stalled_pipe_and_delayed_finalize_are_terminated() {
        let mut process = child("stall", CaptureCancellation::default());
        let error = process.write_frame(&vec![0; 4 * 1024 * 1024], Duration::from_millis(500)).unwrap_err();
        assert!(error.to_string().contains("deadline"));
        let process = child("finish-stall", CaptureCancellation::default());
        assert!(process.finish(Duration::from_millis(500)).unwrap_err().to_string().contains("deadline"));
    }

    #[test]
    fn cancellation_interrupts_encoder_backpressure() {
        let cancel = CaptureCancellation::default();
        let mut process = child("stall", cancel.clone());
        cancel.cancel();
        assert!(matches!(process.write_frame(&vec![0; 4 * 1024 * 1024], Duration::from_secs(10)), Err(VideoError::Cancelled)));
    }
}

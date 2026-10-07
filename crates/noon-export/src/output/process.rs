//! Bounded native encoder transport. Only owned pixels cross the writer thread.
use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::CaptureCancellation;

const STDERR_LIMIT: usize = 64 * 1024;
const POLL: Duration = Duration::from_millis(10);
type Reply = (io::Result<()>, Vec<u8>);

pub(super) struct Encoder {
    child: Child,
    input: Option<SyncSender<Vec<u8>>>,
    replies: Receiver<Reply>,
    writer: Option<JoinHandle<()>>,
    stderr: Option<JoinHandle<io::Result<Vec<u8>>>>,
    buffer: Option<Vec<u8>>,
    cancellation: CaptureCancellation,
    timeout: Duration,
    active: bool,
    reaped: bool,
}

impl Encoder {
    pub fn new(
        command: &mut Command,
        bytes: usize,
        cancellation: CaptureCancellation,
        timeout: Duration,
    ) -> io::Result<Self> {
        let mut buffer = Vec::new();
        buffer.try_reserve_exact(bytes).map_err(io::Error::other)?;
        buffer.resize(bytes, 0);
        let (input, receiver) = mpsc::sync_channel::<Vec<u8>>(1);
        let (reply, replies) = mpsc::sync_channel(1);
        let child = command.stdin(Stdio::piped()).stdout(Stdio::null())
            .stderr(Stdio::piped()).spawn()?;
        let mut encoder = Self {
            child, input: Some(input), replies, writer: None, stderr: None,
            buffer: Some(buffer), cancellation, timeout, active: true, reaped: false,
        };
        let mut stdin = encoder.child.stdin.take()
            .ok_or_else(|| io::Error::other("encoder has no input pipe"))?;
        let mut stderr = encoder.child.stderr.take()
            .ok_or_else(|| io::Error::other("encoder has no diagnostic pipe"))?;
        encoder.stderr = Some(thread::Builder::new().name("noon-encoder-stderr".into())
            .spawn(move || {
                let mut tail = VecDeque::with_capacity(STDERR_LIMIT);
                let mut chunk = [0; 8192];
                loop {
                    match stderr.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            for &byte in &chunk[..n] {
                                if tail.len() == STDERR_LIMIT { tail.pop_front(); }
                                tail.push_back(byte);
                            }
                        }
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(e) => return Err(e),
                    }
                }
                Ok(tail.into_iter().collect())
            })?);
        encoder.writer = Some(thread::Builder::new().name("noon-encoder-input".into())
            .spawn(move || {
                while let Ok(pixels) = receiver.recv() {
                    let result = stdin.write_all(&pixels);
                    let failed = result.is_err();
                    if reply.send((result, pixels)).is_err() || failed { break; }
                }
                // Closing stdin is the encoder's EOF/flush signal.
            })?);
        Ok(encoder)
    }

    fn check_wait(&self, start: Instant) -> io::Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "export cancelled"));
        }
        if start.elapsed() >= self.timeout {
            return Err(io::Error::new(io::ErrorKind::TimedOut, "encoder I/O deadline exceeded"));
        }
        Ok(())
    }

    pub fn write(&mut self, pixels: &[u8]) -> io::Result<()> {
        let result = self.write_inner(pixels);
        if let Err(error) = result {
            self.active = false;
            let diagnostics = self.abort();
            return Err(io::Error::new(error.kind(), format!("{error}; {diagnostics}")));
        }
        Ok(())
    }

    fn write_inner(&mut self, pixels: &[u8]) -> io::Result<()> {
        if !self.active { return Err(io::Error::other("encoder is inactive")); }
        let start = Instant::now();
        self.check_wait(start)?;
        let mut buffer = self.buffer.take().ok_or_else(|| io::Error::other("frame already in flight"))?;
        if buffer.len() != pixels.len() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "wrong raw frame length"));
        }
        buffer.copy_from_slice(pixels);
        // There is at most ONE unacknowledged frame. The bounded channel is empty
        // here; no source state or callback ever moves to this worker.
        self.input.as_ref().ok_or_else(|| io::Error::other("encoder input closed"))?
            .send(buffer).map_err(|_| io::Error::other("encoder writer stopped"))?;
        loop {
            self.check_wait(start)?;
            match self.replies.recv_timeout(POLL) {
                Ok((result, buffer)) => { self.buffer = Some(buffer); return result; }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("encoder writer disconnected"));
                }
            }
        }
    }

    pub fn finish(mut self) -> io::Result<String> {
        if !self.active { return Err(io::Error::other("cannot finish a failed encoder")); }
        self.input.take();
        let start = Instant::now();
        let status = loop {
            self.check_wait(start)?;
            if let Some(status) = self.child.try_wait()? { break status; }
            thread::sleep(POLL);
        };
        self.reaped = true;
        self.active = false;
        let diagnostics = self.join_threads()?;
        if !status.success() {
            return Err(io::Error::other(format!("FFmpeg exited {status}: {diagnostics}")));
        }
        Ok(diagnostics)
    }

    fn join_threads(&mut self) -> io::Result<String> {
        let writer_ok = self.writer.take().map(|thread| thread.join()).transpose();
        let diagnostics = match self.stderr.take() {
            Some(thread) => thread.join().map_err(|_| io::Error::other("diagnostic worker panicked"))??,
            None => Vec::new(),
        };
        writer_ok.map_err(|_| io::Error::other("encoder writer panicked"))?;
        Ok(String::from_utf8_lossy(&diagnostics).into_owned())
    }

    fn abort(&mut self) -> String {
        self.input.take();
        if !self.reaped {
            let _ = self.child.kill();
            self.reaped = self.child.wait().is_ok();
        }
        self.join_threads().unwrap_or_else(|e| e.to_string())
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // Trusted FFmpeg executable only. OS termination/reaping calls cannot be
        // hard-preempted, nor can unrelated descendants inheriting its pipe FDs.
        self.abort();
    }
}

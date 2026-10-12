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
const FRAME_BUFFERS: usize = 2;

struct Pending {
    sequence: u64,
    started: Instant,
}

struct Reply {
    sequence: u64,
    result: io::Result<()>,
    pixels: Vec<u8>,
    completed: Instant,
}

pub(super) struct Encoder {
    child: Child,
    input: Option<SyncSender<(u64, Vec<u8>)>>,
    replies: Receiver<Reply>,
    writer: Option<JoinHandle<()>>,
    stderr: Option<JoinHandle<io::Result<Vec<u8>>>>,
    buffers: Vec<Vec<u8>>,
    pending: VecDeque<Pending>,
    next_sequence: u64,
    frame_bytes: usize,
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
        bytes.checked_mul(FRAME_BUFFERS).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "encoder buffer size overflow")
        })?;
        let mut buffers = Vec::with_capacity(FRAME_BUFFERS);
        for _ in 0..FRAME_BUFFERS {
            let mut buffer = Vec::new();
            buffer.try_reserve_exact(bytes).map_err(io::Error::other)?;
            buffer.resize(bytes, 0);
            buffers.push(buffer);
        }
        let (input, receiver) = mpsc::sync_channel::<(u64, Vec<u8>)>(FRAME_BUFFERS);
        // There can be no more replies than frame buffers, including during
        // abort. The writer cannot deadlock on its reply queue while we join it.
        let (reply, replies) = mpsc::sync_channel(FRAME_BUFFERS);
        let child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()?;
        let mut encoder = Self {
            child,
            input: Some(input),
            replies,
            writer: None,
            stderr: None,
            buffers,
            pending: VecDeque::with_capacity(FRAME_BUFFERS),
            next_sequence: 0,
            frame_bytes: bytes,
            cancellation,
            timeout,
            active: true,
            reaped: false,
        };
        let mut stdin = encoder
            .child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("encoder has no input pipe"))?;
        let mut stderr = encoder
            .child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("encoder has no diagnostic pipe"))?;
        encoder.stderr = Some(
            thread::Builder::new()
                .name("noon-encoder-stderr".into())
                .spawn(move || {
                    let mut tail = VecDeque::with_capacity(STDERR_LIMIT);
                    let mut chunk = [0; 8192];
                    loop {
                        match stderr.read(&mut chunk) {
                            Ok(0) => break,
                            Ok(n) => {
                                for &byte in &chunk[..n] {
                                    if tail.len() == STDERR_LIMIT {
                                        tail.pop_front();
                                    }
                                    tail.push_back(byte);
                                }
                            }
                            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                            Err(e) => return Err(e),
                        }
                    }
                    Ok(tail.into_iter().collect())
                })?,
        );
        encoder.writer = Some(
            thread::Builder::new()
                .name("noon-encoder-input".into())
                .spawn(move || {
                    while let Ok((sequence, pixels)) = receiver.recv() {
                        let result = stdin.write_all(&pixels);
                        let failed = result.is_err();
                        let result = Reply {
                            sequence,
                            result,
                            pixels,
                            completed: Instant::now(),
                        };
                        if reply.send(result).is_err() || failed {
                            break;
                        }
                    }
                    // Closing stdin is the encoder's EOF/flush signal.
                })?,
        );
        Ok(encoder)
    }

    fn check_wait(&self, start: Instant) -> io::Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "export cancelled",
            ));
        }
        if start.elapsed() >= self.timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "encoder I/O deadline exceeded",
            ));
        }
        Ok(())
    }

    fn fail(&mut self, error: io::Error) -> io::Error {
        self.active = false;
        let diagnostics = self.abort();
        io::Error::new(error.kind(), format!("{error}; {diagnostics}"))
    }

    /// Synchronous transport is retained for the one-frame capability probe and
    /// as a serial reference in tests. Both modes use the same worker and pool.
    pub fn write(&mut self, pixels: &[u8]) -> io::Result<()> {
        self.enqueue(pixels)?;
        self.drain().map_err(|error| self.fail(error))
    }

    /// Accept into the bounded pool, not necessarily into the encoder process.
    /// Rendering can continue while the worker writes these owned bytes. When
    /// both buffers are outstanding, wait for the oldest one: never allocate a
    /// third, skip a frame, reorder work or borrow the producer's reusable pixels.
    pub fn enqueue(&mut self, pixels: &[u8]) -> io::Result<()> {
        self.enqueue_inner(pixels).map_err(|error| self.fail(error))
    }

    fn enqueue_inner(&mut self, pixels: &[u8]) -> io::Result<()> {
        if !self.active {
            return Err(io::Error::other("encoder is inactive"));
        }
        self.check_wait(Instant::now())?;
        if pixels.len() != self.frame_bytes {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "wrong raw frame length",
            ));
        }
        self.collect_ready()?;
        if self.buffers.is_empty() {
            self.wait_one()?;
        }
        let mut buffer = self
            .buffers
            .pop()
            .ok_or_else(|| io::Error::other("encoder pool exhausted"))?;
        buffer.copy_from_slice(pixels);
        let sequence = self.next_sequence;
        let next_sequence = sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("encoder sequence exhausted"))?;
        let started = Instant::now();
        self.input
            .as_ref()
            .ok_or_else(|| io::Error::other("encoder input closed"))?
            .try_send((sequence, buffer))
            .map_err(|_| io::Error::other("encoder writer stopped or input invariant failed"))?;
        self.pending.push_back(Pending { sequence, started });
        self.next_sequence = next_sequence;
        Ok(())
    }

    fn accept_reply(&mut self, reply: Reply) -> io::Result<()> {
        let pending = self
            .pending
            .pop_front()
            .ok_or_else(|| io::Error::other("unsolicited encoder reply"))?;
        if reply.sequence != pending.sequence || reply.pixels.len() != self.frame_bytes {
            return Err(io::Error::other(
                "encoder reply does not match the oldest frame",
            ));
        }
        reply.result?;
        // Compare actual I/O completion, not the time the caller collects the
        // reply: a long source callback must not time out already completed I/O.
        if reply.completed.duration_since(pending.started) >= self.timeout {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "encoder I/O deadline exceeded",
            ));
        }
        self.buffers.push(reply.pixels);
        Ok(())
    }

    fn collect_ready(&mut self) -> io::Result<()> {
        while !self.pending.is_empty() {
            match self.replies.try_recv() {
                Ok(reply) => self.accept_reply(reply)?,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(io::Error::other("encoder writer disconnected"));
                }
            }
        }
        if let Some(pending) = self.pending.front() {
            self.check_wait(pending.started)?;
        }
        Ok(())
    }

    fn wait_one(&mut self) -> io::Result<()> {
        loop {
            if self.cancellation.is_cancelled() {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "export cancelled",
                ));
            }
            match self.replies.try_recv() {
                Ok(reply) => return self.accept_reply(reply),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(io::Error::other("encoder writer disconnected"));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            let pending = self
                .pending
                .front()
                .ok_or_else(|| io::Error::other("no encoder write pending"))?;
            self.check_wait(pending.started)?;
            let remaining = self.timeout.saturating_sub(pending.started.elapsed());
            match self.replies.recv_timeout(POLL.min(remaining)) {
                Ok(reply) => return self.accept_reply(reply),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(io::Error::other("encoder writer disconnected"));
                }
            }
        }
    }

    fn drain(&mut self) -> io::Result<()> {
        while !self.pending.is_empty() {
            self.wait_one()?;
        }
        Ok(())
    }

    pub fn finish(mut self) -> io::Result<String> {
        let result = self.finish_inner();
        result.map_err(|error| self.fail(error))
    }

    fn finish_inner(&mut self) -> io::Result<String> {
        if !self.active {
            return Err(io::Error::other("cannot finish a failed encoder"));
        }
        // A child exiting successfully does not prove queued bytes were written.
        // Every accepted frame must receive a successful reply BEFORE EOF/flush.
        self.drain()?;
        self.input.take();
        let start = Instant::now();
        let status = loop {
            self.check_wait(start)?;
            if let Some(status) = self.child.try_wait()? {
                break status;
            }
            thread::sleep(POLL);
        };
        self.reaped = true;
        self.active = false;
        let diagnostics = self.join_threads()?;
        if !status.success() {
            return Err(io::Error::other(format!(
                "FFmpeg exited {status}: {diagnostics}"
            )));
        }
        Ok(diagnostics)
    }

    fn join_threads(&mut self) -> io::Result<String> {
        let writer_ok = self.writer.take().map(|thread| thread.join()).transpose();
        let diagnostics = match self.stderr.take() {
            Some(thread) => thread
                .join()
                .map_err(|_| io::Error::other("diagnostic worker panicked"))??,
            None => Vec::new(),
        };
        writer_ok.map_err(|_| io::Error::other("encoder writer panicked"))?;
        Ok(String::from_utf8_lossy(&diagnostics).into_owned())
    }

    fn abort(&mut self) -> String {
        self.active = false;
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

#[cfg(test)]
mod tests;

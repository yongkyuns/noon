//! Presentation reads ready pixels while one worker decodes a bounded window.

use std::{
    collections::VecDeque,
    io,
    sync::{Arc, Condvar, Mutex, Weak},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use super::{NativeVideoAsset, NativeVideoFrames, Process};

const CACHE_BYTES: usize = 32 * 1024 * 1024;
const MAX_FRAMES: usize = 12;

type Pixels = Arc<Vec<u8>>;

#[derive(Default)]
struct State {
    requested: Option<usize>,
    frames: VecDeque<(usize, Pixels)>,
    process: Weak<Mutex<Process>>,
    error: Option<String>,
    stopped: bool,
}

#[derive(Default)]
struct Worker {
    state: Mutex<State>,
    changed: Condvar,
}

/// A disposable decode window for one asset. Realtime consumers can sample
/// without waiting for decoding. Offline sampling can wait for the same frame.
pub struct NativeVideoCache {
    worker: Arc<Worker>,
    thread: Option<JoinHandle<()>>,
    frame_count: usize,
}

impl NativeVideoCache {
    /// `wake` signals that a requested frame or a decoder error is ready.
    /// Platform hosts can request a redraw rather than poll a loading timer.
    pub fn new(asset: Arc<NativeVideoAsset>, wake: impl Fn() + Send + 'static) -> io::Result<Self> {
        let worker = Arc::new(Worker::default());
        let frame_count = asset.frame_index().timestamps().len();
        let thread = thread::Builder::new()
            .name("noon-video-decode".into())
            .spawn({
                let worker = Arc::clone(&worker);
                move || {
                    if let Err(error) = worker.decode(&asset, frame_count, &wake) {
                        worker.state.lock().unwrap().error = Some(error.to_string());
                        worker.changed.notify_all();
                        wake();
                    }
                }
            })?;
        Ok(Self {
            worker,
            thread: Some(thread),
            frame_count,
        })
    }

    /// Queue or replace the seek target and borrow a cached frame if available.
    pub fn try_frame(&self, index: usize) -> io::Result<Option<Pixels>> {
        let mut state = self.worker.state.lock().unwrap();
        self.request(&mut state, index)?;
        state.frame(index)
    }

    /// Wait for the same frame used by realtime playback. Only export and
    /// other offline consumers should use this blocking entry point.
    pub fn wait_frame(&self, index: usize, timeout: Duration) -> io::Result<Pixels> {
        let start = Instant::now();
        let mut state = self.worker.state.lock().unwrap();
        self.request(&mut state, index)?;
        loop {
            if let Some(pixels) = state.frame(index)? {
                return Ok(pixels);
            }
            if state.requested != Some(index) {
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "video seek superseded",
                ));
            }
            let remaining = timeout.saturating_sub(start.elapsed());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "video frame was not ready before the deadline",
                ));
            }
            (state, _) = self.worker.changed.wait_timeout(state, remaining).unwrap();
        }
    }

    fn request(&self, state: &mut State, index: usize) -> io::Result<()> {
        if index >= self.frame_count {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "video frame is outside the asset",
            ));
        }
        if state.requested != Some(index) {
            state.requested = Some(index);
            self.worker.changed.notify_all();
        }
        Ok(())
    }
}

impl noon_core::VideoFrameWindow for NativeVideoCache {
    fn try_frame(&self, index: usize) -> io::Result<Option<Pixels>> {
        self.try_frame(index)
    }

    fn wait_frame(&self, index: usize, timeout: Duration) -> io::Result<Pixels> {
        self.wait_frame(index, timeout)
    }
}

impl Drop for NativeVideoCache {
    fn drop(&mut self) {
        let process = {
            let mut state = self.worker.state.lock().unwrap();
            state.stopped = true;
            state.process.upgrade()
        };
        self.worker.changed.notify_all();
        if let Some(process) = process {
            let _ = process.lock().unwrap().0.kill();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl State {
    fn frame(&self, index: usize) -> io::Result<Option<Pixels>> {
        if let Some(error) = &self.error {
            return Err(io::Error::other(error.clone()));
        }
        Ok(self
            .frames
            .front()
            .and_then(|(first, _)| index.checked_sub(*first))
            .and_then(|offset| self.frames.get(offset))
            .map(|(_, pixels)| Arc::clone(pixels)))
    }
}

impl Worker {
    fn decode(
        &self,
        asset: &NativeVideoAsset,
        frame_count: usize,
        wake: &impl Fn(),
    ) -> io::Result<()> {
        let (width, height) = asset.dimensions();
        let capacity = (CACHE_BYTES / (width as usize * height as usize * 4)).min(MAX_FRAMES);
        let lookbehind = capacity / 3;
        let lookahead = capacity - lookbehind - 1;
        let mut stream: Option<NativeVideoFrames<'_>> = None;
        loop {
            let index = {
                let mut state = self.state.lock().unwrap();
                loop {
                    if state.stopped {
                        return Ok(());
                    }
                    if let Some(request) = state.requested {
                        let first = request.saturating_sub(lookbehind);
                        while state
                            .frames
                            .front()
                            .is_some_and(|(index, _)| *index < first)
                        {
                            state.frames.pop_front();
                        }
                        let next = state.frames.back().map_or(request, |(index, _)| index + 1);
                        let right = (request + lookahead).min(frame_count - 1);
                        if next <= right
                            && state
                                .frames
                                .front()
                                .is_none_or(|(index, _)| *index <= request)
                        {
                            break next.max(request);
                        }
                        if request < next && state.frame(request)?.is_none() {
                            // A backward seek outside the retained window.
                            state.frames.clear();
                            break request;
                        }
                    }
                    state = self.changed.wait(state).unwrap();
                }
            };

            if stream.as_ref().is_none_or(|stream| stream.next != index) {
                // Cancel the old subprocess before starting another. Its pipe
                // backpressure bounds decoding while the window is full.
                drop(stream.take());
                let next = asset.decode_from(index)?;
                let mut state = self.state.lock().unwrap();
                if state.stopped {
                    return Ok(());
                }
                state.process = Arc::downgrade(&next.process);
                stream = Some(next);
            }
            let pixels = stream.as_mut().unwrap().read_frame()?.ok_or_else(|| {
                io::Error::other("decoder ended before the requested indexed frame")
            })?;
            let ready = {
                let mut state = self.state.lock().unwrap();
                let request = state.requested.expect("a decode follows a request");
                if !(request.saturating_sub(lookbehind)..=request + lookahead).contains(&index) {
                    continue;
                }
                if state
                    .frames
                    .back()
                    .is_some_and(|(previous, _)| previous + 1 != index)
                {
                    state.frames.clear();
                }
                state.frames.push_back((index, Arc::new(pixels)));
                index == request
            };
            self.changed.notify_all();
            if ready {
                wake();
            }
        }
    }
}

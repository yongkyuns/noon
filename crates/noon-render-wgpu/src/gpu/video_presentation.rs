//! Disposable decoded media for a presentation using the ordinary raster pass.

use std::{io, sync::Arc, time::Duration};

use noon_core::{RasterImageResourceHandle, VideoFrameWindow, VideoRasterResource};

use super::GpuRenderer;

type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
pub struct VideoPresentation {
    videos: Vec<VideoOutput>,
}

struct VideoOutput {
    handle: RasterImageResourceHandle,
    resource: VideoRasterResource,
    window: Box<dyn VideoFrameWindow>,
    frame: Option<(usize, Arc<Vec<u8>>)>,
}

impl VideoPresentation {
    pub fn new(
        videos: &[(RasterImageResourceHandle, VideoRasterResource)],
        wake: Wake,
    ) -> io::Result<Self> {
        let videos = videos
            .iter()
            .map(|(handle, resource)| {
                let wake = Arc::clone(&wake);
                let window = resource
                    .source()
                    .clone()
                    .open_window(Box::new(move || wake()))?;
                let start = resource.timing().scene_start();
                if let Some(index) = resource
                    .timing()
                    .frame_at(resource.source().frame_index(), start)
                {
                    // Decode ahead of the first active scene sample, even for
                    // a clip that starts during an otherwise static wait.
                    window.try_frame(index)?;
                }
                Ok(VideoOutput {
                    handle: *handle,
                    resource: resource.clone(),
                    window,
                    frame: None,
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Self { videos })
    }

    /// Nonblocking gate for an exact sample from the existing viewer clock.
    pub fn prepare_realtime(&mut self, scene_time: f64) -> io::Result<bool> {
        self.prepare(scene_time, None)
    }

    /// Offline export requests the same sample, on its rational frame grid.
    pub fn prepare_export(&mut self, scene_time: f64, timeout: Duration) -> io::Result<()> {
        self.prepare(scene_time, Some(timeout)).map(|_| ())
    }

    fn prepare(&mut self, scene_time: f64, timeout: Option<Duration>) -> io::Result<bool> {
        let mut ready = true;
        for video in &mut self.videos {
            let index = video
                .resource
                .timing()
                .frame_at(video.resource.source().frame_index(), scene_time);
            video.frame = if let Some(index) = index {
                let pixels = if let Some(timeout) = timeout {
                    Some(video.window.wait_frame(index, timeout)?)
                } else {
                    video.window.try_frame(index)?
                };
                ready &= pixels.is_some();
                pixels.map(|pixels| (index, pixels))
            } else {
                None
            };
        }
        Ok(ready)
    }

    pub fn upload(&self, renderer: &mut GpuRenderer, queue: &wgpu::Queue) -> io::Result<usize> {
        let Some(images) = renderer.images.as_mut() else {
            return Ok(0);
        };
        let mut bytes = 0;
        for video in &self.videos {
            bytes += images.write_video_frame(
                queue,
                video.handle,
                video
                    .frame
                    .as_ref()
                    .map(|(index, pixels)| (*index, pixels.as_slice())),
            )?;
        }
        Ok(bytes)
    }
}

#[cfg(all(target_arch = "wasm32", feature = "web"))]
impl GpuRenderer {
    pub fn has_video_frame(&self, handle: RasterImageResourceHandle, index: usize) -> bool {
        self.images
            .as_ref()
            .is_some_and(|images| images.has_video_frame(handle, index))
    }

    /// Upload CPU pixels on a browser adapter without usable external copies.
    pub fn write_rgba_video_frame(
        &mut self,
        queue: &wgpu::Queue,
        handle: RasterImageResourceHandle,
        index: usize,
        pixels: &[u8],
    ) -> io::Result<usize> {
        self.images.as_mut().map_or(Ok(0), |images| {
            images.write_video_frame(queue, handle, Some((index, pixels)))
        })
    }

    /// Upload a browser-owned frame into the same retained raster slot used by
    /// native video. The caller resolves the frame index from Rust scene time.
    pub fn write_external_video_frame(
        &mut self,
        queue: &wgpu::Queue,
        handle: RasterImageResourceHandle,
        frame: Option<(usize, wgpu::ExternalImageSource)>,
    ) -> io::Result<usize> {
        self.images.as_mut().map_or(Ok(0), |images| {
            images.write_external_video_frame(queue, handle, frame)
        })
    }
}

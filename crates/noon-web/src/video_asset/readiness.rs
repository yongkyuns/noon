//! Browser decoder acknowledgements. Frame and audio boundaries use the same
//! clip timing as the renderer. The window carries codec residency only.
use std::{collections::HashMap, ops::Range};

use noon_core::{RasterImageResourceHandle, VideoRasterResource};

pub(crate) type VideoRasters = [(RasterImageResourceHandle, VideoRasterResource)];
type ClipKey = (u64, u64);
const LOOKAHEAD_SECONDS: f64 = 0.15;

pub(crate) fn clip_key(handle: RasterImageResourceHandle) -> String {
    format!("{}:{}", handle.arena, handle.id.get())
}

fn identity(handle: RasterImageResourceHandle) -> ClipKey {
    (handle.arena, handle.id.get())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VideoDemand {
    clip: String,
    asset_id: u32,
    index: usize,
}

#[derive(Default)]
pub(crate) struct BrowserVideoReadiness {
    windows: HashMap<ClipKey, Range<usize>>,
    demands: HashMap<ClipKey, VideoDemand>,
    pub(crate) pending: Option<f64>,
}

impl BrowserVideoReadiness {
    pub(crate) fn update(
        &mut self,
        video: &VideoRasters,
        clip: &str,
        start: usize,
        end: usize,
    ) -> Result<bool, String> {
        let parsed = clip
            .split_once(':')
            .and_then(|(arena, id)| Some((arena.parse().ok()?, id.parse().ok()?)))
            .ok_or("invalid decoder clip identity")?;
        let Some((_, resource)) = video.iter().find(|(handle, _)| identity(*handle) == parsed)
        else {
            return Ok(false);
        };
        if start > end || end > resource.source().frame_index().timestamps().len() {
            return Err("invalid decoded video window".into());
        }
        let range = start..end;
        if self.windows.get(&parsed) == Some(&range) {
            return Ok(false);
        }
        self.windows.insert(parsed, range);
        Ok(true)
    }

    /// The next unready scene boundary. Return None when no clip bounds
    /// advancement. The clip's exit requires no decoded picture.
    pub(crate) fn boundary(&self, video: &VideoRasters, time: f64) -> Option<f64> {
        video
            .iter()
            .filter_map(|(handle, raster)| {
                let timing = raster.timing();
                let base = time.max(timing.scene_start());
                let source = raster.source();
                source.external_asset_id()?;
                let index = source.frame_index();
                let frame = timing.frame_at(index, base)?;
                let Some(window) = self
                    .windows
                    .get(&identity(*handle))
                    .filter(|span| span.contains(&frame))
                else {
                    return Some(base);
                };
                let boundary = timing.frame_boundary(index, window.end);
                (boundary < timing.scene_end()).then_some(boundary)
            })
            .reduce(f64::min)
    }

    /// One Rust-owned playback barrier. A missed frame freezes the last
    /// coherent sample. Retry the exact held target on a codec acknowledgement.
    pub(crate) fn allow_sample(
        &mut self,
        video: &VideoRasters,
        current: f64,
        requested: f64,
        selected: f64,
    ) -> bool {
        let held = self.pending.is_some();
        let origin = if held || selected < current {
            selected
        } else {
            current
        };
        if let Some(boundary) = self
            .boundary(video, origin)
            .filter(|&time| time <= selected)
        {
            self.demand(video, boundary, LOOKAHEAD_SECONDS);
            self.pending = Some(if held || selected < current {
                requested
            } else {
                requested - (selected - boundary)
            });
            return false;
        }
        self.pending = None;
        self.demand(video, selected, LOOKAHEAD_SECONDS);
        true
    }

    pub(crate) fn next_preload(&self, video: &VideoRasters, time: f64) -> Option<f64> {
        video
            .iter()
            .filter_map(|(handle, raster)| {
                let source = raster.source();
                source.external_asset_id()?;
                let timing = raster.timing();
                let deadline = timing.scene_start() - LOOKAHEAD_SECONDS;
                if deadline <= time {
                    return None;
                }
                let first = timing.frame_at(source.frame_index(), timing.scene_start())?;
                (!self
                    .windows
                    .get(&identity(*handle))
                    .is_some_and(|range| range.contains(&first)))
                .then_some(deadline)
            })
            .reduce(f64::min)
    }

    /// Ask the codec host to warm only the target or a clip about to enter.
    /// The host deduplicates requests against its current window.
    fn demand(&mut self, video: &VideoRasters, time: f64, lookahead: f64) {
        for (handle, raster) in video {
            let source = raster.source();
            let Some(asset_id) = source.external_asset_id() else {
                continue;
            };
            let timing = raster.timing();
            if time < timing.scene_start() - lookahead {
                continue;
            }
            let Some(index) = timing.frame_at(source.frame_index(), time.max(timing.scene_start()))
            else {
                continue;
            };
            if self
                .windows
                .get(&identity(*handle))
                .is_some_and(|range| range.contains(&index))
            {
                continue;
            }
            self.demands.insert(
                identity(*handle),
                VideoDemand {
                    clip: clip_key(*handle),
                    asset_id,
                    index,
                },
            );
        }
    }

    pub(crate) fn take_demands(&mut self) -> Option<String> {
        (!self.demands.is_empty()).then(|| {
            let demands = self
                .demands
                .drain()
                .map(|(_, demand)| demand)
                .collect::<Vec<_>>();
            serde_json::to_string(&demands).expect("video demand has only integer fields")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video_asset::BrowserVideoSource;
    use noon_core::RasterImageResourceId;
    use std::sync::Arc;

    #[test]
    fn ready_window_gates_trimmed_pts_and_frees_the_clip_exit() {
        let handle = RasterImageResourceHandle {
            arena: 1,
            id: RasterImageResourceId::new(2),
            version: 0,
        };
        let source = BrowserVideoSource::new(7, 160, 90, vec![0.0, 0.25, 0.5, 0.875], 1.0).unwrap();
        let clip = VideoRasterResource::new(Arc::new(source), 1.0, 0.125..0.75).unwrap();
        let clip = [(handle, clip)];
        let mut ready = BrowserVideoReadiness::default();
        let key = clip_key(handle);
        assert_eq!(ready.boundary(&clip, 0.5), Some(1.0));
        ready.demand(&clip, 0.9, 0.125);
        let demand: serde_json::Value =
            serde_json::from_str(&ready.take_demands().unwrap()).unwrap();
        assert_eq!(
            demand[0],
            serde_json::json!({"clip": key, "assetId": 7, "index": 0})
        );
        assert!(ready.update(&clip, &key, 0, 2).unwrap());
        assert_eq!(ready.boundary(&clip, 1.125), Some(1.375));
        assert!(!ready.update(&clip, "1:8", 0, 3).unwrap());
        assert!(ready.update(&clip, &key, 1, 3).unwrap());
        assert_eq!(ready.boundary(&clip, 1.375), None);
        assert_eq!(ready.boundary(&clip, 1.01), Some(1.01));
        assert_eq!(ready.boundary(&clip, 1.625), None);
    }
}

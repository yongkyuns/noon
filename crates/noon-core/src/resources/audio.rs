//! Immutable, backend-neutral PCM resources. File decoding and device output
//! belong to platform adapters; this arena owns no scene identity or clock.

use std::{
    collections::HashMap,
    mem::size_of,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

/// Default limit for PCM and resource headers retained by one audio arena.
pub const DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET: usize = 64 * 1024 * 1024;

static NEXT_AUDIO_RESOURCE_ARENA: AtomicU64 = AtomicU64::new(1);

fn next_audio_resource_arena() -> u64 {
    NEXT_AUDIO_RESOURCE_ARENA
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("audio resource arena identity exhausted")
}

/// Resource identity only; authored clips use the scene's semantic node IDs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AudioResourceId(u64);

impl AudioResourceId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Capability to resolve one immutable PCM payload in its owning arena.
///
/// Entries are immutable and IDs are not reused, so the initial version is zero.
/// Arena provenance prevents a foreign or cloned store's equal slot from aliasing
/// this resource. Copying a handle does not retain the payload after retirement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AudioResourceHandle {
    pub arena: u64,
    pub id: AudioResourceId,
    pub version: u64,
}

/// Validated immutable mono/stereo interleaved PCM.
///
/// Every sample is finite; amplitudes are not silently clipped or normalized.
/// Codec, resampling, mixer and output policies do not belong to the resource.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioResource {
    sample_rate: u32,
    channels: u16,
    samples: Arc<[f32]>,
}

impl AudioResource {
    /// Prepare a canonical payload before admitting it to a semantic store.
    ///
    /// `max_pcm_bytes` bounds retained sample bytes, not allocations an upstream
    /// decoder has already made. Validation scans PCM once; later arena lookups
    /// and shared snapshots do not inspect or copy its samples.
    pub fn from_interleaved(
        sample_rate: u32,
        channels: u16,
        samples: impl Into<Arc<[f32]>>,
        max_pcm_bytes: usize,
    ) -> Result<Self, AudioResourceError> {
        if !(8_000..=192_000).contains(&sample_rate) {
            return Err(AudioResourceError::UnsupportedSampleRate { sample_rate });
        }
        if !matches!(channels, 1 | 2) {
            return Err(AudioResourceError::UnsupportedChannelCount { channels });
        }
        let samples = samples.into();
        if samples.is_empty() {
            return Err(AudioResourceError::EmptyPcm);
        }
        if samples.len() % usize::from(channels) != 0 {
            return Err(AudioResourceError::IncompleteFrame {
                samples: samples.len(),
                channels,
            });
        }
        let bytes = samples
            .len()
            .checked_mul(size_of::<f32>())
            .ok_or(AudioResourceError::StorageSizeOverflow)?;
        if bytes > max_pcm_bytes {
            return Err(AudioResourceError::PcmByteLimitExceeded {
                bytes,
                limit: max_pcm_bytes,
            });
        }
        if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
            return Err(AudioResourceError::NonFiniteSample { index });
        }
        Ok(Self {
            sample_rate,
            channels,
            samples,
        })
    }

    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub const fn channels(&self) -> u16 {
        self.channels
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    pub fn duration_seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate)
    }

    pub fn pcm_bytes(&self) -> usize {
        // Checked once during construction, and the immutable payload cannot grow.
        self.samples.len() * size_of::<f32>()
    }

    pub fn retained_bytes(&self) -> usize {
        size_of::<Self>().saturating_add(self.pcm_bytes())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AudioResourceStats {
    pub live_resources: usize,
    pub retained_bytes: usize,
    pub pcm_bytes: usize,
}

/// Storage identity for already prepared immutable PCM, not a content hash.
/// The indexed arena entry pins the allocation until this key is removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct SharedAudioPayload {
    samples_address: usize,
    samples: usize,
    sample_rate: u32,
    channels: u16,
}

impl SharedAudioPayload {
    fn of(resource: &AudioResource) -> Self {
        Self {
            samples_address: resource.samples.as_ptr() as usize,
            samples: resource.samples.len(),
            sample_rate: resource.sample_rate,
            channels: resource.channels,
        }
    }
}

/// Bounded arena of immutable audio resources.
///
/// Explicit insertion always allocates a new identity. Atomic Scene admission
/// can reuse a previously admitted clone of the same immutable PCM allocation
/// and format; this does not compare, hash, or scan sample contents.
/// Get, shared retention and individual removal are expected O(1) and never scan
/// PCM. The byte budget counts resource headers and PCM retained by this arena,
/// excluding map bookkeeping and immutable snapshots held by other owners.
#[derive(Debug)]
pub struct AudioResourceArena {
    namespace: u64,
    entries: HashMap<AudioResourceId, Arc<AudioResource>>,
    shared_payloads: HashMap<SharedAudioPayload, AudioResourceId>,
    next_id: u64,
    byte_budget: usize,
    retained_bytes: usize,
    pcm_bytes: usize,
}

impl Default for AudioResourceArena {
    fn default() -> Self {
        Self::with_byte_budget(DEFAULT_AUDIO_RESOURCE_BYTE_BUDGET)
    }
}

impl Clone for AudioResourceArena {
    fn clone(&self) -> Self {
        Self {
            namespace: next_audio_resource_arena(),
            entries: self.entries.clone(),
            shared_payloads: self.shared_payloads.clone(),
            next_id: self.next_id,
            byte_budget: self.byte_budget,
            retained_bytes: self.retained_bytes,
            pcm_bytes: self.pcm_bytes,
        }
    }
}

impl AudioResourceArena {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_byte_budget(byte_budget: usize) -> Self {
        Self {
            namespace: next_audio_resource_arena(),
            entries: HashMap::new(),
            shared_payloads: HashMap::new(),
            next_id: 0,
            byte_budget,
            retained_bytes: 0,
            pcm_bytes: 0,
        }
    }

    /// Admit a previously validated payload without rescanning its PCM.
    /// Budget or identity exhaustion leaves the arena unchanged.
    pub fn insert(
        &mut self,
        resource: AudioResource,
    ) -> Result<AudioResourceHandle, AudioResourceError> {
        let required_bytes = resource.retained_bytes();
        let retained_bytes = self
            .retained_bytes
            .checked_add(required_bytes)
            .filter(|bytes| *bytes <= self.byte_budget)
            .ok_or(AudioResourceError::ByteBudgetExceeded {
                retained_bytes: self.retained_bytes,
                required_bytes,
                budget: self.byte_budget,
            })?;
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(AudioResourceError::IdentityExhausted)?;
        let handle = AudioResourceHandle {
            arena: self.namespace,
            id: AudioResourceId::new(self.next_id),
            version: 0,
        };
        self.pcm_bytes += resource.pcm_bytes();
        self.entries.insert(handle.id, Arc::new(resource));
        self.retained_bytes = retained_bytes;
        self.next_id = next_id;
        Ok(handle)
    }

    /// Reuse one prepared payload at the Scene's atomic publication boundary.
    /// The boolean records whether rollback owns a newly inserted resource.
    pub(crate) fn admit_shared(
        &mut self,
        resource: AudioResource,
    ) -> Result<(AudioResourceHandle, bool), AudioResourceError> {
        let key = SharedAudioPayload::of(&resource);
        if let Some(&id) = self.shared_payloads.get(&key) {
            return Ok((
                AudioResourceHandle {
                    arena: self.namespace,
                    id,
                    version: 0,
                },
                false,
            ));
        }
        let handle = self.insert(resource)?;
        self.shared_payloads.insert(key, handle.id);
        Ok((handle, true))
    }

    /// Validate and admit a payload atomically. Decode/preparation allocations
    /// remain the caller's responsibility, outside this retained-resource budget.
    pub fn insert_interleaved(
        &mut self,
        sample_rate: u32,
        channels: u16,
        samples: impl Into<Arc<[f32]>>,
    ) -> Result<AudioResourceHandle, AudioResourceError> {
        let max_pcm_bytes = self.byte_budget.saturating_sub(size_of::<AudioResource>());
        self.insert(AudioResource::from_interleaved(
            sample_rate,
            channels,
            samples,
            max_pcm_bytes,
        )?)
    }

    pub(crate) const fn namespace(&self) -> u64 {
        self.namespace
    }

    pub fn get(&self, handle: AudioResourceHandle) -> Option<&AudioResource> {
        if handle.arena != self.namespace || handle.version != 0 {
            return None;
        }
        self.entries.get(&handle.id).map(Arc::as_ref)
    }

    /// Pin an immutable snapshot independently of future arena retirement.
    pub fn get_shared(&self, handle: AudioResourceHandle) -> Option<Arc<AudioResource>> {
        if handle.arena != self.namespace || handle.version != 0 {
            return None;
        }
        self.entries.get(&handle.id).cloned()
    }

    /// Retire one resource after its durable semantic references are gone, or
    /// roll back an unpublished admission. Older compiled snapshots remain valid
    /// through their own Arcs; a bare copied handle becomes stale immediately.
    pub fn remove(&mut self, handle: AudioResourceHandle) -> Option<Arc<AudioResource>> {
        if handle.arena != self.namespace || handle.version != 0 {
            return None;
        }
        let resource = self.entries.remove(&handle.id)?;
        let key = SharedAudioPayload::of(&resource);
        if self.shared_payloads.get(&key) == Some(&handle.id) {
            self.shared_payloads.remove(&key);
        }
        self.retained_bytes -= resource.retained_bytes();
        self.pcm_bytes -= resource.pcm_bytes();
        Some(resource)
    }

    pub fn stats(&self) -> AudioResourceStats {
        AudioResourceStats {
            live_resources: self.entries.len(),
            retained_bytes: self.retained_bytes,
            pcm_bytes: self.pcm_bytes,
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioResourceError {
    UnsupportedSampleRate {
        sample_rate: u32,
    },
    UnsupportedChannelCount {
        channels: u16,
    },
    EmptyPcm,
    IncompleteFrame {
        samples: usize,
        channels: u16,
    },
    NonFiniteSample {
        index: usize,
    },
    StorageSizeOverflow,
    PcmByteLimitExceeded {
        bytes: usize,
        limit: usize,
    },
    ByteBudgetExceeded {
        retained_bytes: usize,
        required_bytes: usize,
        budget: usize,
    },
    IdentityExhausted,
}

impl std::fmt::Display for AudioResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSampleRate { sample_rate } => {
                write!(f, "audio sample rate must be 8000..=192000 Hz, got {sample_rate}")
            }
            Self::UnsupportedChannelCount { channels } => {
                write!(f, "audio must be mono or stereo, got {channels} channels")
            }
            Self::EmptyPcm => f.write_str("audio PCM must contain at least one frame"),
            Self::IncompleteFrame { samples, channels } => {
                write!(f, "audio PCM has {samples} samples, not whole {channels}-channel frames")
            }
            Self::NonFiniteSample { index } => write!(f, "audio sample {index} is nonfinite"),
            Self::StorageSizeOverflow => f.write_str("audio PCM exceeds addressable storage"),
            Self::PcmByteLimitExceeded { bytes, limit } => {
                write!(f, "audio PCM requires {bytes} bytes, exceeding limit {limit}")
            }
            Self::ByteBudgetExceeded { retained_bytes, required_bytes, budget } => write!(
                f,
                "audio resource requires {required_bytes} bytes with {retained_bytes} retained, exceeding budget {budget}"
            ),
            Self::IdentityExhausted => f.write_str("audio resource identity exhausted"),
        }
    }
}

impl std::error::Error for AudioResourceError {}

#[cfg(test)]
mod tests;

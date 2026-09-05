use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use serde::Serialize;

const QUERY_COUNT: u32 = 2;
const READBACK_BYTES: u64 = 16;
const READBACK_SLOTS: usize = 4;
const COMPLETED_SAMPLE_LIMIT: usize = 256;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GpuTimestampMetrics {
    timestamp_supported: bool,
    samples: u64,
    dropped: u64,
    failed: u64,
    in_flight: u64,
    render_pass_ms: Vec<f64>,
}

struct ReadbackSlot {
    query_set: wgpu::QuerySet,
    resolve_buffer: wgpu::Buffer,
    readback_buffer: wgpu::Buffer,
    mapping: Rc<Cell<bool>>,
}

/// Bounded asynchronous timestamp diagnostics for the reusable renderer.
///
/// The browser host chooses whether to enable it after negotiating the device
/// feature. Presentation only encodes queries and schedules mapping; it never
/// waits for GPU completion.
pub(crate) struct GpuTimestampProfiler {
    slots: Vec<ReadbackSlot>,
    next_slot: usize,
    timestamp_period_ns: f64,
    metrics: Rc<RefCell<GpuTimestampMetrics>>,
    epoch: Rc<Cell<u64>>,
}

impl GpuTimestampProfiler {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let slots = (0..READBACK_SLOTS)
            .map(|_| ReadbackSlot {
                query_set: device.create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("Noon execution render timestamp queries"),
                    ty: wgpu::QueryType::Timestamp,
                    count: QUERY_COUNT,
                }),
                // WebGPU permits QUERY_RESOLVE with COPY_SRC, while mapped
                // buffers require COPY_DST. Keep these roles separate.
                resolve_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Noon execution render timestamp resolve"),
                    size: READBACK_BYTES,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                readback_buffer: device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Noon execution render timestamp readback"),
                    size: READBACK_BYTES,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                mapping: Rc::new(Cell::new(false)),
            })
            .collect();
        Self {
            slots,
            next_slot: 0,
            timestamp_period_ns: f64::from(queue.get_timestamp_period()),
            metrics: Rc::new(RefCell::new(GpuTimestampMetrics {
                timestamp_supported: true,
                ..Default::default()
            })),
            epoch: Rc::new(Cell::new(0)),
        }
    }

    pub(crate) fn reserve_slot(&mut self) -> Option<usize> {
        for _ in 0..self.slots.len() {
            let slot = self.next_slot;
            self.next_slot = (self.next_slot + 1) % self.slots.len();
            if !self.slots[slot].mapping.replace(true) {
                return Some(slot);
            }
        }
        let mut metrics = self.metrics.borrow_mut();
        metrics.dropped = metrics.dropped.saturating_add(1);
        None
    }

    pub(crate) fn query_set(&self, slot: usize) -> &wgpu::QuerySet {
        &self.slots[slot].query_set
    }

    pub(crate) fn resolve(&self, encoder: &mut wgpu::CommandEncoder, slot: usize) {
        let slot = &self.slots[slot];
        encoder.resolve_query_set(&slot.query_set, 0..QUERY_COUNT, &slot.resolve_buffer, 0);
        encoder.copy_buffer_to_buffer(
            &slot.resolve_buffer,
            0,
            &slot.readback_buffer,
            0,
            READBACK_BYTES,
        );
    }

    pub(crate) fn map_after_submit(&self, slot: usize) {
        let readback_buffer = self.slots[slot].readback_buffer.clone();
        let callback_buffer = readback_buffer.clone();
        let mapping = self.slots[slot].mapping.clone();
        let metrics = self.metrics.clone();
        let epoch = self.epoch.clone();
        let callback_epoch = epoch.get();
        let timestamp_period_ns = self.timestamp_period_ns;
        readback_buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            if epoch.get() == callback_epoch {
                match result {
                    Ok(()) => {
                        let timestamps = callback_buffer
                            .get_mapped_range(..)
                            .ok()
                            .and_then(|bytes| decode_timestamp_pair(&bytes));
                        callback_buffer.unmap();
                        let mut metrics = metrics.borrow_mut();
                        match timestamps.and_then(|[start, end]| {
                            end.checked_sub(start)
                                .map(|ticks| ticks as f64 * timestamp_period_ns / 1_000_000.0)
                        }) {
                            Some(milliseconds)
                                if milliseconds.is_finite() && milliseconds >= 0.0 =>
                            {
                                metrics.samples = metrics.samples.saturating_add(1);
                                if metrics.render_pass_ms.len() < COMPLETED_SAMPLE_LIMIT {
                                    metrics.render_pass_ms.push(milliseconds);
                                } else {
                                    metrics.dropped = metrics.dropped.saturating_add(1);
                                }
                            }
                            _ => metrics.failed = metrics.failed.saturating_add(1),
                        }
                    }
                    Err(_) => {
                        let mut metrics = metrics.borrow_mut();
                        metrics.failed = metrics.failed.saturating_add(1);
                    }
                }
            } else if result.is_ok() {
                callback_buffer.unmap();
            }
            mapping.set(false);
        });
    }

    pub(crate) fn reset(&self) -> bool {
        if self.slots.iter().any(|slot| slot.mapping.get()) {
            return false;
        }
        self.epoch.set(self.epoch.get().wrapping_add(1));
        *self.metrics.borrow_mut() = GpuTimestampMetrics {
            timestamp_supported: true,
            ..Default::default()
        };
        true
    }

    pub(crate) fn take_metrics(&self) -> GpuTimestampMetrics {
        let mut metrics = self.metrics.borrow_mut();
        let mut result = metrics.clone();
        result.render_pass_ms = std::mem::take(&mut metrics.render_pass_ms);
        result.in_flight = self.slots.iter().filter(|slot| slot.mapping.get()).count() as u64;
        result
    }
}

fn decode_timestamp_pair(bytes: &[u8]) -> Option<[u64; 2]> {
    let bytes: &[u8; 16] = bytes.try_into().ok()?;
    Some([
        u64::from_le_bytes(bytes[..8].try_into().ok()?),
        u64::from_le_bytes(bytes[8..].try_into().ok()?),
    ])
}

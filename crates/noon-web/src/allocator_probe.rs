//! Opt-in WASM allocation evidence only; the production build excludes this module.
//! A bounded, allocation-free ring records the request before System::realloc,
//! so the last requested size survives an allocator trap if the instance remains
//! readable. Sampling via the wasm-bindgen export happens outside GlobalAlloc.
use std::alloc::{GlobalAlloc, Layout, System};
use std::fmt::Write as _;
use std::sync::atomic::{AtomicU32, Ordering};

const CAPACITY: usize = 128;
struct Request {
    sequence: AtomicU32,
    old: AtomicU32,
    new: AtomicU32,
    align: AtomicU32,
}
impl Request {
    const fn empty() -> Self {
        Self {
            sequence: AtomicU32::new(0), old: AtomicU32::new(0),
            new: AtomicU32::new(0), align: AtomicU32::new(0),
        }
    }
}
static REQUESTS: [Request; CAPACITY] = [const { Request::empty() }; CAPACITY];
static REALLOCS: AtomicU32 = AtomicU32::new(0);
static ALLOCS: AtomicU32 = AtomicU32::new(0);
static FAILED: AtomicU32 = AtomicU32::new(0);
static BIGGEST_REQUEST: AtomicU32 = AtomicU32::new(0);

pub struct TracingAllocator;
unsafe impl GlobalAlloc for TracingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        let ptr = unsafe { System.alloc(layout) };
        if ptr.is_null() { FAILED.fetch_add(1, Ordering::Relaxed); }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        ALLOCS.fetch_add(1, Ordering::Relaxed);
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if ptr.is_null() { FAILED.fetch_add(1, Ordering::Relaxed); }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout); }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // No allocation, JS interop or locks inside this logging path.
        let sequence = REALLOCS.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
        let entry = &REQUESTS[sequence as usize % CAPACITY];
        entry.sequence.store(0, Ordering::Relaxed);
        entry.old.store(layout.size() as u32, Ordering::Relaxed);
        entry.new.store(new_size as u32, Ordering::Relaxed);
        entry.align.store(layout.align() as u32, Ordering::Relaxed);
        entry.sequence.store(sequence, Ordering::Release);
        BIGGEST_REQUEST.fetch_max(new_size as u32, Ordering::Relaxed);
        let next = unsafe { System.realloc(ptr, layout, new_size) };
        if next.is_null() { FAILED.fetch_add(1, Ordering::Relaxed); }
        next
    }
}

/// Snapshot atomics onto the stack before building a String, which itself allocates.
pub fn snapshot() -> String {
    let last = REALLOCS.load(Ordering::Acquire);
    let allocs = ALLOCS.load(Ordering::Relaxed);
    let failed = FAILED.load(Ordering::Relaxed);
    let biggest = BIGGEST_REQUEST.load(Ordering::Relaxed);
    let mut records = [(0u32, 0u32, 0u32, 0u32); 32];
    let mut n = 0usize;
    for offset in (0..32u32).rev() {
        if last <= offset { continue; }
        let sequence = last - offset;
        let entry = &REQUESTS[sequence as usize % CAPACITY];
        if entry.sequence.load(Ordering::Acquire) == sequence {
            records[n] = (sequence,
                entry.old.load(Ordering::Relaxed),
                entry.new.load(Ordering::Relaxed),
                entry.align.load(Ordering::Relaxed));
            n += 1;
        }
    }
    let mut out = format!(
        "{{\"reallocs\":{last},\"allocs\":{allocs},\"null_results\":{failed},\"largest_realloc_bytes\":{biggest},\"recent\":["
    );
    for (index, &(sequence, old, new, align)) in records[..n].iter().enumerate() {
        if index > 0 { out.push(','); }
        let _ = write!(out, "[{sequence},{old},{new},{align}]");
    }
    out.push_str("]}");
    out
}

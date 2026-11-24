//! Fixed-size accounting for the vectors grown by explicit object maintenance.
//! No scene rows/history are traversed and no resource ownership is introduced.
use super::PrefixError;
use crate::SceneInstance;

impl SceneInstance {
    /// Provisional combined ceiling for five compiler and ten runtime object
    /// vectors. Excludes maps, tracks, indirect/shared content, identity/spatial
    /// indices, source history, allocator overhead/peaks and renderer residency.
    pub const MAX_PREFIX_OBJECT_VECTOR_BYTES: usize = 64 * 1024 * 1024;
}

fn vector_bytes<T>(values: &Vec<T>, minimum: usize, spare: bool) -> Option<usize> {
    let slots = if spare {
        values.capacity()
    } else {
        values.len()
    };
    slots.max(minimum).checked_mul(std::mem::size_of::<T>())
}

fn measure(runtime: &SceneInstance, minimum: usize, spare: bool) -> Option<usize> {
    let mut bytes = if spare {
        runtime.compiled.prefix_object_reserved_bytes(minimum)?
    } else {
        runtime.compiled.prefix_object_clone_bytes(minimum)?
    };
    macro_rules! add {
        ($values:expr) => {
            bytes = bytes.checked_add(vector_bytes(&$values, minimum, spare)?)?;
        };
    }
    add!(runtime.frame.objects);
    add!(runtime.frame.presences);
    add!(runtime.frame.reveals);
    add!(runtime.frame.morphs);
    add!(runtime.frame.render_geometries);
    add!(runtime.frame.render_transforms);
    add!(runtime.frame.family_animations);
    add!(runtime.frame.family_animation_plan_indices);
    add!(runtime.painter_ranks);
    add!(runtime.painter_order);
    Some(bytes)
}

pub(super) fn clone_bytes(runtime: &SceneInstance, minimum: usize) -> Option<usize> {
    measure(runtime, minimum, false)
}

pub(super) fn reserved_bytes(runtime: &SceneInstance, minimum: usize) -> Option<usize> {
    measure(runtime, minimum, true)
}

fn admit(bytes: Option<usize>, minimum: usize, limit_bytes: usize) -> Result<usize, PrefixError> {
    bytes
        .filter(|&bytes| bytes <= limit_bytes)
        .ok_or(PrefixError::ObjectStorageBudgetExceeded {
            object_slots: minimum,
            required_bytes: bytes,
            limit_bytes,
        })
}

pub(super) fn preflight_clone(
    runtime: &SceneInstance,
    minimum: usize,
    limit_bytes: usize,
) -> Result<usize, PrefixError> {
    admit(clone_bytes(runtime, minimum), minimum, limit_bytes)
}

pub(super) fn preflight_reserved(
    runtime: &SceneInstance,
    minimum: usize,
    limit_bytes: usize,
) -> Result<usize, PrefixError> {
    admit(reserved_bytes(runtime, minimum), minimum, limit_bytes)
}

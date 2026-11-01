//! One bulk language admission into ordinary typed composition children.
//! Packed arrays exist only at this host boundary; shared Rust owns validation,
//! activation capture, timing, interpolation and completion.

use super::{CanonicalAuthoringScene, OrdinaryCompositionChild};
use noon_core::{AnimationDefaults, AnimationOptions, ObjectId, RateFunction, SemanticVec3};
use std::{rc::Rc, sync::Arc};

const HAS_RUN_TIME: u8 = 1;
const HAS_LAG: u8 = 2;
const HAS_PATH_ARC: u8 = 4;
const HAS_REVERSE: u8 = 8;
const REVERSE: u8 = 16;
const OPTION_MASK: u8 = HAS_RUN_TIME | HAS_LAG | HAS_PATH_ARC | HAS_REVERSE | REVERSE;

fn rate(value: &str) -> Result<Option<RateFunction>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        RateFunction::from_semantic_id(value)
            .map(Some)
            .ok_or_else(|| format!("unsupported rate function semantic id: {value}"))
    }
}

/// Resolve already-bound wrapper identities in their original canonical owner.
/// No semantic identity, frame, schedule or target copy is created here.
#[allow(clippy::too_many_arguments)]
pub(super) fn children(
    context: &CanonicalAuthoringScene,
    object_ids: &[ObjectId],
    sample_ends: &[u32],
    phases: &[f64],
    centers_xyz: &[f64],
    rotation_offsets: &[f64],
    option_presence: &[u8],
    option_values: &[f64],
    rate_functions: &[String],
    play_run_time: Option<f64>,
    play_rate_function: &str,
    play_lag_ratio: Option<f64>,
) -> Result<Vec<OrdinaryCompositionChild>, String> {
    let count = object_ids.len();
    if count == 0
        || sample_ends.len() != count
        || option_presence.len() != count
        || rate_functions.len() != count
        || count.checked_mul(3) != Some(option_values.len())
        || phases.len().checked_mul(3) != Some(centers_xyz.len())
        || rotation_offsets.len() != phases.len()
        || sample_ends.last().map(|&end| end as usize) != Some(phases.len())
    {
        return Err("SampledMotion batch has inconsistent record or sample array lengths".into());
    }
    let play = AnimationOptions {
        run_time: play_run_time,
        rate_func: rate(play_rate_function)?,
        lag_ratio: play_lag_ratio,
        ..AnimationOptions::new()
    };
    let mut result = Vec::with_capacity(count);
    let mut start = 0;
    for (index, id) in object_ids.iter().enumerate() {
        let end = sample_ends[index] as usize;
        if end <= start || end > phases.len() {
            return Err("SampledMotion batch sample ends must be strictly increasing".into());
        }
        let mask = option_presence[index];
        if mask & !OPTION_MASK != 0 || mask & REVERSE != 0 && mask & HAS_REVERSE == 0 {
            return Err("SampledMotion batch has invalid option presence bits".into());
        }
        let values = &option_values[index * 3..index * 3 + 3];
        let local = AnimationOptions {
            run_time: (mask & HAS_RUN_TIME != 0).then_some(values[0]),
            lag_ratio: (mask & HAS_LAG != 0).then_some(values[1]),
            path_arc: (mask & HAS_PATH_ARC != 0).then_some(values[2]),
            reverse_rate_function: (mask & HAS_REVERSE != 0).then_some(mask & REVERSE != 0),
            rate_func: rate(&rate_functions[index])?,
            ..AnimationOptions::new()
        };
        let resolved = noon_core::resolve_sampled_motion_animation_options(
            AnimationDefaults::MANIM,
            local,
            play,
        )
        .map_err(|error| error.to_string())?;
        let node = context
            .bindings
            .get(id)
            .ok_or("SampledMotion target must already belong to this Scene")?;
        let target = noon::Mobject::from_node(Rc::clone(context.scene.integration_store()), *node)
            .map_err(|error| error.to_string())?;
        let centers = centers_xyz[start * 3..end * 3]
            .as_chunks::<3>()
            .0
            .iter()
            .map(|point| SemanticVec3::new(point[0], point[1], point[2]))
            .collect();
        let motion = noon_core::SampledMotionData::new(
            phases[start..end].to_vec(),
            centers,
            rotation_offsets[start..end].to_vec(),
        )
        .map_err(|error| error.to_string())?;
        result.push(OrdinaryCompositionChild::SampledMotion {
            target,
            motion: Arc::new(motion),
            options: AnimationOptions::new()
                .run_time(resolved.run_time)
                .rate_func(resolved.rate_func)
                .reverse_rate_function(resolved.reverse_rate_function),
        });
        start = end;
    }
    Ok(result)
}

#[cfg(test)]
mod tests;

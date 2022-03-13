use std::marker::PhantomData;

use crate::Color;

/// Path flattenening tolerance for normal shapes under normal condition.
pub const EPS: f32 = 0.01;

/// Path flattenening tolerance for interpolation and other
/// tasks where computation may be higher than usual.
pub const EPS_LOW: f32 = 0.1;

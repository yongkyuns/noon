//! Animation options, composition/time maps and typed family execution data.
//!
//! These shared declarations and plans own no clock or mutable playback state.
//! SemanticStore owns authored meaning; lowering and Runtime consume the same
//! renderer-independent contracts through the crate's public API.

mod composition;
pub use composition::*;

mod family_animation;
pub use family_animation::*;

mod family_plan;
pub use family_plan::*;

mod family_timing;
pub use family_timing::*;

mod family_transform_correspondence;
pub use family_transform_correspondence::*;

mod member_plan;
pub use member_plan::*;

mod options;
pub use options::*;

mod retained_members;
pub use retained_members::*;

mod text_members;
pub use text_members::*;

mod timeline;
pub use timeline::*;

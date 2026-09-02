include!("gpu_geometry.rs");

mod retained_text {
    include!("gpu/retained_text.rs");

    mod family_prepare {
        include!("gpu/retained_family_prepare.rs");
        include!("gpu/retained_family_draw_border_prepare.rs");
    }
    pub use family_prepare::*;
}
pub use retained_text::*;

#[path = "gpu/retained_family_reveal.rs"]
mod retained_family_reveal;
pub use retained_family_reveal::*;

#[path = "gpu/retained_family_draw_border_then_fill.rs"]
mod retained_family_draw_border_then_fill;
pub use retained_family_draw_border_then_fill::*;

include!("gpu_geometry.rs");

mod retained_text {
    include!("gpu/retained_text.rs");

    mod family_prepare {
        include!("gpu/retained_family_prepare.rs");
        include!("gpu/retained_family_draw_border_prepare.rs");
        include!("gpu/retained_family_animation_prepare.rs");
        include!("gpu/retained_family_plan_set_prepare.rs");
    }
    pub use family_prepare::*;
}
pub use retained_text::*;

mod retained_family_reveal;
pub use retained_family_reveal::*;

mod retained_family_draw_border_then_fill;
pub use retained_family_draw_border_then_fill::*;

mod retained_family_reveal_scene;
pub use retained_family_reveal_scene::*;

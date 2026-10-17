//! Retained glyph preparation and GPU residency within the shared renderer.
//!
//! These helpers consume shared text/runtime data. They own disposable raster,
//! atlas and draw caches, while the main renderer preserves shared painter order.

pub mod atlas;
mod preparation;
pub use preparation::*;

#[cfg_attr(test, allow(clippy::single_range_in_vec_init))]
mod gpu;
pub use gpu::*;

/// Resolve retained text paint without bypassing the owning text's fill alpha.
/// Lowering folds semantic fill opacity into `object_fill.alpha`. Intrinsic
/// source colors replace RGB but remain subject to that alpha, while inherited
/// paint already contains it and must not be multiplied twice. The independent
/// object-composite opacity is applied by each ordinary draw/preparation path.
pub(crate) fn resolve_text_paint(
    intrinsic: Option<noon_core::Color>,
    object_fill: Option<noon_core::Color>,
) -> Option<noon_core::Color> {
    intrinsic
        .map(|mut paint| {
            paint.alpha *= object_fill.map_or(1.0, |fill| fill.alpha);
            paint
        })
        .or(object_fill)
}

#[cfg(test)]
mod paint_tests {
    use super::resolve_text_paint;
    use noon_core::Color;

    #[test]
    fn intrinsic_text_alpha_multiplies_owner_fill_alpha_without_recoloring() {
        let intrinsic = Color::rgba(0.2, 0.6, 0.8, 0.5);
        let owner = Color::rgba(1.0, 0.0, 0.0, 0.25);
        assert_eq!(
            resolve_text_paint(Some(intrinsic), Some(owner)),
            Some(Color::rgba(0.2, 0.6, 0.8, 0.125))
        );
        assert_eq!(
            resolve_text_paint(
                Some(intrinsic),
                Some(Color {
                    alpha: 0.0,
                    ..owner
                })
            ),
            Some(Color {
                alpha: 0.0,
                ..intrinsic
            })
        );
    }

    #[test]
    fn inherited_text_alpha_is_applied_once_and_missing_owner_preserves_intrinsic() {
        let paint = Color::rgba(0.2, 0.6, 0.8, 0.25);
        assert_eq!(resolve_text_paint(None, Some(paint)), Some(paint));
        assert_eq!(resolve_text_paint(Some(paint), None), Some(paint));
        assert_eq!(resolve_text_paint(None, None), None);
    }
}

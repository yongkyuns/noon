//! Shared spotlight construction expressed through ordinary semantic transactions.

use noon_core::{
    AnimationOptions, Color, SemanticLocalNodeToken, SemanticMutationTransaction,
    SemanticNodeCreation, SemanticObjectState, SemanticPaint, SemanticStyle, SemanticVec3,
    StoredGeometry, DEFAULT_FRAME_HEIGHT, DEFAULT_FRAME_WIDTH,
};

/// A fixed focus point and the appearance of its transient spotlight.
#[derive(Clone, Copy, Debug)]
pub struct FocusOnOptions {
    pub point: (f64, f64),
    pub opacity: f64,
    pub color: Color,
}

impl FocusOnOptions {
    pub const fn new(point: (f64, f64)) -> Self {
        Self {
            point,
            opacity: 0.2,
            color: Color::GREY,
        }
    }

    pub(crate) fn stage(
        self,
        transaction: &mut SemanticMutationTransaction,
        options: AnimationOptions,
    ) -> Result<(SemanticLocalNodeToken, SemanticLocalNodeToken), String> {
        let (x, y) = self.point;
        if !x.is_finite()
            || !y.is_finite()
            || x.abs() > f32::MAX as f64
            || y.abs() > f32::MAX as f64
            || !self.opacity.is_finite()
            || !(0.0..=1.0).contains(&self.opacity)
            || ![
                self.color.red,
                self.color.green,
                self.color.blue,
                self.color.alpha,
            ]
            .iter()
            .all(|value| value.is_finite())
        {
            return Err("FocusOn requires a finite 2D point/color and opacity in [0, 1]".into());
        }
        if options.introducer == Some(false)
            || options.remover == Some(false)
            || options.lag_ratio.is_some_and(|value| value != 0.0)
            || options.path_arc.is_some_and(|value| value != 0.0)
            || options.reverse_rate_function == Some(true)
        {
            return Err(
                "FocusOn has fixed transient membership without lag, path arcs or rate reversal"
                    .into(),
            );
        }
        let mut source = SemanticObjectState::new(StoredGeometry::Circle {
            radius: (DEFAULT_FRAME_WIDTH + DEFAULT_FRAME_HEIGHT) * 0.5,
        });
        source.style = SemanticStyle {
            fill: Some(SemanticPaint::Solid(Color {
                alpha: 1.0,
                ..self.color
            })),
            fill_opacity: 0.0,
            ..SemanticStyle::default()
        };
        let mut target = source.clone();
        target.transform.translation = SemanticVec3::new(x, y, 0.0);
        target.transform.scale = SemanticVec3::new(0.0, 0.0, 1.0);
        target.style.fill_opacity = self.opacity;
        let source = transaction.create_node(SemanticNodeCreation::object(source));
        let target = transaction.create_node(SemanticNodeCreation::object(target));
        // Membership belongs to the containing transaction. The ordinary transform
        // declaration retains the authored endpoints for deterministic execution.
        let options = options
            .run_time(options.run_time.unwrap_or(2.0))
            .introducer(false)
            .remover(false);
        let animation = transaction.create_transform_animation(source, target, options);
        Ok((source, animation))
    }
}

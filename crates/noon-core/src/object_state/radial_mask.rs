//! Checked radial visibility values. Geometry, clocks and attachment identity
//! remain owned by the existing scene/runtime; these are inert appearance data.
use super::effect_parameters::{interpolate, sealed};
use crate::{Color, EffectParameter, EffectParameterError, EffectUpdate, EffectValue};

/// Independent radial appearance channels, shared by all authoring frontends.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
pub enum RadialParameter {
    Center,
    Axes,
    Roughness,
    Frequency,
    RimWidth,
    InnerColor,
    OuterColor,
    Hole,
}
impl RadialParameter {
    pub const ALL: [Self; 8] = [
        Self::Center,
        Self::Axes,
        Self::Roughness,
        Self::Frequency,
        Self::RimWidth,
        Self::InnerColor,
        Self::OuterColor,
        Self::Hole,
    ];
}

/// One typed radial endpoint. A hole is discrete; other channels interpolate.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum RadialValue {
    Center([f64; 2]),
    Axes([f64; 2]),
    Roughness(f64),
    Frequency(f64),
    RimWidth(f64),
    InnerColor(Color),
    OuterColor(Color),
    Hole(bool),
}
impl RadialValue {
    pub const fn parameter(self) -> RadialParameter {
        match self {
            Self::Center(_) => RadialParameter::Center,
            Self::Axes(_) => RadialParameter::Axes,
            Self::Roughness(_) => RadialParameter::Roughness,
            Self::Frequency(_) => RadialParameter::Frequency,
            Self::RimWidth(_) => RadialParameter::RimWidth,
            Self::InnerColor(_) => RadialParameter::InnerColor,
            Self::OuterColor(_) => RadialParameter::OuterColor,
            Self::Hole(_) => RadialParameter::Hole,
        }
    }
    pub fn validate(self) -> Result<(), EffectParameterError> {
        let finite_range = |v: f64, lo: f64, hi: f64| v.is_finite() && (lo..=hi).contains(&v);
        let valid = match self {
            Self::Center(v) => v.into_iter().all(|v| finite_range(v, 0.0, 1.0)),
            Self::Axes(v) => v.into_iter().all(|v| finite_range(v, 0.05, 4.0)),
            Self::Roughness(v) | Self::RimWidth(v) => finite_range(v, 0.0, 1.0),
            Self::Frequency(v) => finite_range(v, 1.0, 64.0),
            Self::InnerColor(c) | Self::OuterColor(c) => [c.red, c.green, c.blue, c.alpha]
                .into_iter()
                .all(|v| v.is_finite() && (0.0..=1.0).contains(&v)),
            Self::Hole(_) => true,
        };
        valid
            .then_some(())
            .ok_or(EffectParameterError::InvalidRadialParameter(
                self.parameter(),
            ))
    }
    pub fn interpolate(self, target: Self, alpha: f64) -> Option<Self> {
        let pair =
            |a: [f64; 2], b: [f64; 2]| std::array::from_fn(|i| interpolate(a[i], b[i], alpha));
        Some(match (self, target) {
            (Self::Center(a), Self::Center(b)) => Self::Center(pair(a, b)),
            (Self::Axes(a), Self::Axes(b)) => Self::Axes(pair(a, b)),
            (Self::Roughness(a), Self::Roughness(b)) => Self::Roughness(interpolate(a, b, alpha)),
            (Self::Frequency(a), Self::Frequency(b)) => Self::Frequency(interpolate(a, b, alpha)),
            (Self::RimWidth(a), Self::RimWidth(b)) => Self::RimWidth(interpolate(a, b, alpha)),
            (Self::InnerColor(a), Self::InnerColor(b)) => {
                Self::InnerColor(super::interpolate_premultiplied_color(a, b, alpha))
            }
            (Self::OuterColor(a), Self::OuterColor(b)) => {
                Self::OuterColor(super::interpolate_premultiplied_color(a, b, alpha))
            }
            (Self::Hole(a), Self::Hole(b)) if a == b => Self::Hole(a),
            _ => return None,
        })
    }
}

/// Partial radial fields. None preserves the current field; no allocation or
/// additional mutable owner is needed for one local parameter write.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RadialUpdate {
    pub center: Option<[f64; 2]>,
    pub axes: Option<[f64; 2]>,
    pub roughness: Option<f64>,
    pub frequency: Option<f64>,
    pub rim_width: Option<f64>,
    pub inner_color: Option<Color>,
    pub outer_color: Option<Color>,
    pub hole: Option<bool>,
}
impl RadialUpdate {
    pub fn values(self) -> impl Iterator<Item = RadialValue> {
        [
            self.center.map(RadialValue::Center),
            self.axes.map(RadialValue::Axes),
            self.roughness.map(RadialValue::Roughness),
            self.frequency.map(RadialValue::Frequency),
            self.rim_width.map(RadialValue::RimWidth),
            self.inner_color.map(RadialValue::InnerColor),
            self.outer_color.map(RadialValue::OuterColor),
            self.hole.map(RadialValue::Hole),
        ]
        .into_iter()
        .flatten()
    }
    pub fn get(self, parameter: RadialParameter) -> Option<RadialValue> {
        self.values().find(|v| v.parameter() == parameter)
    }
    pub fn set(&mut self, value: RadialValue) {
        match value {
            RadialValue::Center(v) => self.center = Some(v),
            RadialValue::Axes(v) => self.axes = Some(v),
            RadialValue::Roughness(v) => self.roughness = Some(v),
            RadialValue::Frequency(v) => self.frequency = Some(v),
            RadialValue::RimWidth(v) => self.rim_width = Some(v),
            RadialValue::InnerColor(v) => self.inner_color = Some(v),
            RadialValue::OuterColor(v) => self.outer_color = Some(v),
            RadialValue::Hole(v) => self.hole = Some(v),
        }
    }
}

/// Shared radial-edge presets. Selecting one writes ordinary mask parameters;
/// leaving it omitted preserves an existing mask's authored edge treatment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RadialEdge {
    Clean,
    Burn,
}
impl RadialEdge {
    /// Fill preset fields without replacing explicitly supplied fields.
    pub fn resolve(self, mut update: EffectUpdate) -> EffectUpdate {
        let defaults = self.parameters();
        update.softness = update.softness.or(defaults.softness);
        for value in defaults.radial.values() {
            if update.radial.get(value.parameter()).is_none() {
                update.radial.set(value);
            }
        }
        update
    }
    pub fn parameters(self) -> EffectUpdate {
        let preset = match self {
            Self::Clean => RadialMask::default(),
            Self::Burn => RadialMask::burn(),
        };
        EffectUpdate {
            softness: Some(preset.softness),
            radial: RadialUpdate {
                roughness: Some(preset.roughness),
                frequency: Some(preset.frequency),
                rim_width: Some(preset.rim_width),
                inner_color: Some(preset.inner_color),
                outer_color: Some(preset.outer_color),
                ..RadialUpdate::default()
            },
            ..EffectUpdate::default()
        }
    }
}

/// Radial visibility over the capture's normalized, upright source bounds.
/// Removal progress is always zero for the original source and one for empty.
/// Center/axes use that box; edge distances use the ellipse's radial units.
/// `hole` removes the inside of an expanding frontier; otherwise an iris retains
/// its inside. The optional rim is clipped to the original painted coverage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RadialMask {
    progress: f64,
    softness: f64,
    seed: u32,
    center: [f64; 2],
    axes: [f64; 2],
    roughness: f64,
    frequency: f64,
    rim_width: f64,
    inner_color: Color,
    outer_color: Color,
    hole: bool,
}
impl Default for RadialMask {
    fn default() -> Self {
        Self {
            progress: 0.0,
            softness: 0.02,
            seed: 0,
            center: [0.5, 0.5],
            axes: [0.5, 0.5],
            roughness: 0.0,
            frequency: 12.0,
            rim_width: 0.0,
            inner_color: Color::rgba(1.0, 0.85, 0.24, 1.0),
            outer_color: Color::rgba(0.24, 0.035, 0.01, 1.0),
            hole: false,
        }
    }
}
impl RadialMask {
    pub fn new(update: EffectUpdate) -> Result<Self, EffectParameterError> {
        Self::default().with_update(update, None)
    }
    /// Resolve a partial edit and optional preset in the shared semantic layer.
    /// Explicit fields win over the preset; omitted fields preserve this value.
    pub fn with_update(
        self,
        update: EffectUpdate,
        edge: Option<RadialEdge>,
    ) -> Result<Self, EffectParameterError> {
        let update = match edge {
            Some(edge) => edge.resolve(update),
            None => update,
        };
        update.apply_to(self)
    }
    /// A useful warm edge, resolved into the same ordinary mask parameters.
    pub fn burn() -> Self {
        Self {
            roughness: 0.10,
            rim_width: 0.12,
            softness: 0.025,
            ..Self::default()
        }
    }
    pub const fn progress(self) -> f64 {
        self.progress
    }
    pub const fn softness(self) -> f64 {
        self.softness
    }
    pub const fn seed(self) -> u32 {
        self.seed
    }
    pub const fn center(self) -> [f64; 2] {
        self.center
    }
    pub const fn axes(self) -> [f64; 2] {
        self.axes
    }
    pub const fn roughness(self) -> f64 {
        self.roughness
    }
    pub const fn frequency(self) -> f64 {
        self.frequency
    }
    pub const fn rim_width(self) -> f64 {
        self.rim_width
    }
    pub const fn inner_color(self) -> Color {
        self.inner_color
    }
    pub const fn outer_color(self) -> Color {
        self.outer_color
    }
    pub const fn hole(self) -> bool {
        self.hole
    }
    pub const fn is_neutral(self) -> bool {
        self.progress == 0.0
    }
    pub fn parameters(self) -> RadialUpdate {
        RadialUpdate {
            center: Some(self.center),
            axes: Some(self.axes),
            roughness: Some(self.roughness),
            frequency: Some(self.frequency),
            rim_width: Some(self.rim_width),
            inner_color: Some(self.inner_color),
            outer_color: Some(self.outer_color),
            hole: Some(self.hole),
        }
    }
    pub fn update(self) -> EffectUpdate {
        EffectUpdate {
            progress: Some(self.progress),
            softness: Some(self.softness),
            seed: Some(self.seed),
            radial: self.parameters(),
            ..EffectUpdate::default()
        }
    }
}
impl sealed::Sealed for RadialMask {}
impl EffectValue for RadialMask {
    fn apply_effect_update(self, update: &EffectUpdate) -> Result<Self, EffectParameterError> {
        for parameter in update.parameters() {
            if !matches!(
                parameter,
                EffectParameter::Progress
                    | EffectParameter::Softness
                    | EffectParameter::Seed
                    | EffectParameter::Radial(_)
            ) {
                return Err(EffectParameterError::UnsupportedParameter(parameter));
            }
        }
        update.validate()?;
        let r = update.radial;
        Ok(Self {
            progress: update.progress.unwrap_or(self.progress),
            softness: update.softness.unwrap_or(self.softness),
            seed: update.seed.unwrap_or(self.seed),
            center: r.center.unwrap_or(self.center),
            axes: r.axes.unwrap_or(self.axes),
            roughness: r.roughness.unwrap_or(self.roughness),
            frequency: r.frequency.unwrap_or(self.frequency),
            rim_width: r.rim_width.unwrap_or(self.rim_width),
            inner_color: r.inner_color.unwrap_or(self.inner_color),
            outer_color: r.outer_color.unwrap_or(self.outer_color),
            hole: r.hole.unwrap_or(self.hole),
        })
    }
    fn validate_effect_value_update(&self, _target: &Self) -> Result<(), EffectParameterError> {
        Ok(())
    }
    fn validate_effect_transition(&self, target: &Self) -> Result<(), EffectParameterError> {
        if self.seed != target.seed {
            return Err(EffectParameterError::DiscreteTransition(
                EffectParameter::Seed,
            ));
        }
        if self.hole != target.hole {
            return Err(EffectParameterError::DiscreteTransition(
                EffectParameter::Radial(RadialParameter::Hole),
            ));
        }
        Ok(())
    }
    fn sample_effect_parameters(
        &self,
        target: &Self,
        request: &EffectUpdate,
        alpha: f64,
    ) -> Result<EffectUpdate, EffectParameterError> {
        let mut result = EffectUpdate {
            progress: request
                .progress
                .map(|_| interpolate(self.progress, target.progress, alpha)),
            softness: request
                .softness
                .map(|_| interpolate(self.softness, target.softness, alpha)),
            seed: request.seed,
            ..EffectUpdate::default()
        };
        for v in request.radial.values() {
            let parameter = v.parameter();
            let from = self.parameters().get(parameter).expect("radial field");
            let to = target.parameters().get(parameter).expect("radial field");
            result.radial.set(from.interpolate(to, alpha).ok_or(
                EffectParameterError::DiscreteTransition(EffectParameter::Radial(parameter)),
            )?);
        }
        result.apply_to(*self)?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_fields_and_burn_defaults_are_inert_checked_values() {
        let previous = RadialMask::burn();
        let request = EffectUpdate::default()
            .progress(0.5)
            .reveal_center([0.3, 0.7]);
        let next = request.apply_to(previous).unwrap();
        assert_eq!(next.progress(), 0.5);
        assert_eq!(next.center(), [0.3, 0.7]);
        assert_eq!(next.rim_width(), previous.rim_width());
        assert_eq!(
            request
                .prepare(previous)
                .unwrap()
                .sample(0.0)
                .unwrap()
                .apply_to(previous)
                .unwrap(),
            previous
        );
        assert_eq!(
            request
                .prepare(previous)
                .unwrap()
                .sample(1.0)
                .unwrap()
                .apply_to(previous)
                .unwrap(),
            next
        );
        for value in [
            RadialValue::Center([f64::NAN, 0.0]),
            RadialValue::Axes([0.0, 0.5]),
            RadialValue::Roughness(-1.0),
            RadialValue::Frequency(65.0),
            RadialValue::RimWidth(2.0),
        ] {
            assert!(EffectUpdate::default()
                .radial_value(value)
                .apply_to(previous)
                .is_err());
        }
        assert!(EffectUpdate::default()
            .reveal_hole(true)
            .prepare(previous)
            .is_err());
        assert!(EffectUpdate::default().seed(1).prepare(previous).is_err());
        assert!(EffectUpdate::default()
            .reveal_hole(true)
            .apply_to(previous)
            .is_ok());
    }
}

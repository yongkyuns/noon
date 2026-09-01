/// ManimCE v0.21 `ImplicitFunction` defaults.
pub const MANIM_DEFAULT_IMPLICIT_MIN_DEPTH: u32 = 5;
pub const MANIM_DEFAULT_IMPLICIT_MAX_QUADS: usize = 1500;
pub const MANIM_DEFAULT_IMPLICIT_TOLERANCE_DIVISOR: f64 = 1000.0;

/// Finite, non-degenerate authoring domain for one implicit contour extraction.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImplicitContourDomain {
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
}

impl ImplicitContourDomain {
    pub fn new(
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
    ) -> Result<Self, ImplicitContourError> {
        if [x_min, x_max, y_min, y_max]
            .into_iter()
            .any(|value| !value.is_finite())
        {
            return Err(ImplicitContourError::NonFiniteDomain);
        }
        if x_min >= x_max || y_min >= y_max {
            return Err(ImplicitContourError::InvalidDomain {
                x_min,
                x_max,
                y_min,
                y_max,
            });
        }
        Ok(Self {
            x_min,
            x_max,
            y_min,
            y_max,
        })
    }

    pub const fn x_min(self) -> f64 {
        self.x_min
    }

    pub const fn x_max(self) -> f64 {
        self.x_max
    }

    pub const fn y_min(self) -> f64 {
        self.y_min
    }

    pub const fn y_max(self) -> f64 {
        self.y_max
    }

    /// Default per-axis isoline tolerance used by the pinned isosurfaces behavior.
    pub fn default_tolerance(self) -> [f64; 2] {
        [
            (self.x_max - self.x_min) / MANIM_DEFAULT_IMPLICIT_TOLERANCE_DIVISOR,
            (self.y_max - self.y_min) / MANIM_DEFAULT_IMPLICIT_TOLERANCE_DIVISOR,
        ]
    }
}

/// Renderer-independent request metadata for adaptive implicit contour extraction.
///
/// This deliberately contains no callback or generated geometry. Rust chooses
/// evaluation coordinates in the contour session that consumes this request;
/// host-language functions are authoring-time evaluators only.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImplicitContourRequest {
    domain: ImplicitContourDomain,
    min_depth: u32,
    max_quads: usize,
    use_smoothing: bool,
}

impl ImplicitContourRequest {
    pub fn new(
        domain: ImplicitContourDomain,
        min_depth: u32,
        max_quads: usize,
        use_smoothing: bool,
    ) -> Result<Self, ImplicitContourError> {
        if max_quads == 0 {
            return Err(ImplicitContourError::InvalidMaxQuads(max_quads));
        }
        Ok(Self {
            domain,
            min_depth,
            max_quads,
            use_smoothing,
        })
    }

    pub fn manim_default(domain: ImplicitContourDomain) -> Self {
        Self {
            domain,
            min_depth: MANIM_DEFAULT_IMPLICIT_MIN_DEPTH,
            max_quads: MANIM_DEFAULT_IMPLICIT_MAX_QUADS,
            use_smoothing: true,
        }
    }

    pub const fn domain(self) -> ImplicitContourDomain {
        self.domain
    }

    pub const fn min_depth(self) -> u32 {
        self.min_depth
    }

    pub const fn max_quads(self) -> usize {
        self.max_quads
    }

    pub const fn use_smoothing(self) -> bool {
        self.use_smoothing
    }

    pub fn tolerance(self) -> [f64; 2] {
        self.domain.default_tolerance()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImplicitContourError {
    NonFiniteDomain,
    InvalidDomain {
        x_min: f64,
        x_max: f64,
        y_min: f64,
        y_max: f64,
    },
    InvalidMaxQuads(usize),
}

impl std::fmt::Display for ImplicitContourError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            Self::NonFiniteDomain => formatter.write_str("implicit contour domain must be finite"),
            Self::InvalidDomain {
                x_min,
                x_max,
                y_min,
                y_max,
            } => write!(
                formatter,
                "implicit contour bounds must be strictly increasing: x=[{x_min}, {x_max}], y=[{y_min}, {y_max}]"
            ),
            Self::InvalidMaxQuads(max_quads) => {
                write!(formatter, "implicit contour max_quads must be non-zero: {max_quads}")
            }
        }
    }
}

impl std::error::Error for ImplicitContourError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manim_defaults_are_explicit_and_domain_scaled() {
        let domain = ImplicitContourDomain::new(-7.0, 7.0, -4.0, 4.0).unwrap();
        let request = ImplicitContourRequest::manim_default(domain);
        assert_eq!(request.min_depth(), 5);
        assert_eq!(request.max_quads(), 1500);
        assert!(request.use_smoothing());
        assert_eq!(request.tolerance(), [0.014, 0.008]);
    }

    #[test]
    fn min_depth_is_independent_of_max_quad_budget() {
        let domain = ImplicitContourDomain::new(-1.0, 1.0, -1.0, 1.0).unwrap();
        let request = ImplicitContourRequest::new(domain, 8, 1, false).unwrap();
        assert_eq!(request.min_depth(), 8);
        assert_eq!(request.max_quads(), 1);
        assert!(!request.use_smoothing());
    }

    #[test]
    fn rejects_nonfinite_degenerate_and_reversed_domains() {
        assert_eq!(
            ImplicitContourDomain::new(f64::NAN, 1.0, -1.0, 1.0),
            Err(ImplicitContourError::NonFiniteDomain)
        );
        assert!(matches!(
            ImplicitContourDomain::new(1.0, 1.0, -1.0, 1.0),
            Err(ImplicitContourError::InvalidDomain { .. })
        ));
        assert!(matches!(
            ImplicitContourDomain::new(-1.0, 1.0, 2.0, -2.0),
            Err(ImplicitContourError::InvalidDomain { .. })
        ));
    }

    #[test]
    fn zero_quad_budget_is_rejected_before_evaluation() {
        let domain = ImplicitContourDomain::new(-1.0, 1.0, -1.0, 1.0).unwrap();
        assert_eq!(
            ImplicitContourRequest::new(domain, 5, 0, true),
            Err(ImplicitContourError::InvalidMaxQuads(0))
        );
    }
}

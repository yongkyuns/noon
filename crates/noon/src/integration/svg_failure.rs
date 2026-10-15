//! Shared SVG error projection for native and browser language bindings.
//! This conversion belongs beside the shared type, not in a downstream binding.

use super::AuthoringFailure;
use crate::SvgAuthoringError;

impl From<SvgAuthoringError> for AuthoringFailure {
    fn from(error: SvgAuthoringError) -> Self {
        use SvgAuthoringError as E;
        let message = error.to_string();
        match error {
            E::Xml(_) => Self::new("invalid_input", "svg.invalid_xml", message),
            E::Parse(_) => Self::new("invalid_input", "svg.parse", message),
            E::Unsupported(_) => Self::new("unsupported_operation", "svg.unsupported", message),
            E::InvalidTargetDimension { .. } => {
                Self::new("invalid_input", "svg.invalid_target_dimension", message)
            }
            E::Authoring(cause) => {
                let nested = AuthoringFailure::from(cause);
                Self {
                    category: nested.category,
                    code: "svg.authoring",
                    message,
                    cause: Some(Box::new(nested)),
                }
            }
            E::InvalidImportKey => Self::new("invalid_input", "svg.authoring", message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AuthoringError, MobjectFamily, Scene};
    use std::rc::Rc;

    #[test]
    fn svg_failure_keeps_input_diagnostics() {
        let error = SvgAuthoringError::InvalidTargetDimension {
            name: "height",
            value: -1.0,
        };
        let message = error.to_string();
        let failure = AuthoringFailure::from(error);
        assert_eq!(failure.category, "invalid_input");
        assert_eq!(failure.code, "svg.invalid_target_dimension");
        assert_eq!(failure.message, message);
        assert!(failure.cause.is_none());
    }

    #[test]
    fn svg_failure_preserves_nested_authoring_category() {
        let failure =
            AuthoringFailure::from(SvgAuthoringError::Authoring(AuthoringError::ForeignStore));
        assert_eq!(failure.category, "foreign_handle");
        assert_eq!(failure.code, "svg.authoring");
        assert_eq!(failure.cause.unwrap().code, "authoring.foreign_store");
    }

    #[test]
    fn malformed_svg_uses_the_same_projection_without_browser_dependencies() {
        let scene = Scene::new();
        let error = MobjectFamily::from_svg_str(Rc::clone(scene.integration_store()), "<svg>")
            .expect_err("malformed XML must fail");
        let failure = AuthoringFailure::from(error);
        assert_eq!(failure.category, "invalid_input");
        assert_eq!(failure.code, "svg.invalid_xml");
    }
}

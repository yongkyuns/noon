//! Inert validated path input shared by native and browser language adapters.
use crate::AuthoringError;
use noon_core::{Vec2, VectorPath};

#[derive(Clone, Debug, Default)]
pub struct AuthoringVectorPath {
    path: VectorPath,
}

pub fn authoring_path_point(x: f64, y: f64) -> Result<Vec2, AuthoringError> {
    let value = super::authoring_xy_f64(x, y)?;
    Ok(Vec2::new(value.x as f32, value.y as f32))
}

impl AuthoringVectorPath {
    pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), AuthoringError> {
        let to = authoring_path_point(x, y)?;
        self.path = std::mem::take(&mut self.path).move_to(to);
        Ok(())
    }
    pub fn line_to(&mut self, x: f64, y: f64) -> Result<(), AuthoringError> {
        let to = authoring_path_point(x, y)?;
        self.path = std::mem::take(&mut self.path).line_to(to);
        Ok(())
    }
    pub fn quadratic_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) -> Result<(), AuthoringError> {
        let control = authoring_path_point(cx, cy)?;
        let to = authoring_path_point(x, y)?;
        self.path = std::mem::take(&mut self.path).quadratic_to(control, to);
        Ok(())
    }
    pub fn cubic_to(
        &mut self,
        ax: f64,
        ay: f64,
        bx: f64,
        by: f64,
        x: f64,
        y: f64,
    ) -> Result<(), AuthoringError> {
        let first = authoring_path_point(ax, ay)?;
        let second = authoring_path_point(bx, by)?;
        let to = authoring_path_point(x, y)?;
        self.path = std::mem::take(&mut self.path).cubic_to(first, second, to);
        Ok(())
    }
    pub fn close(&mut self) {
        self.path = std::mem::take(&mut self.path).close();
    }
    pub fn into_path(self) -> VectorPath {
        self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_later_control_preserves_all_existing_commands() {
        let mut builder = AuthoringVectorPath::default();
        builder.move_to(0.0, 0.0).unwrap();
        builder.line_to(1.0, 0.0).unwrap();
        let before = builder.clone().into_path();
        assert!(builder.cubic_to(1.0, 2.0, f64::NAN, 3.0, 4.0, 5.0).is_err());
        assert_eq!(builder.clone().into_path(), before);
        assert!(builder.quadratic_to(1.0, 2.0, f64::INFINITY, 0.0).is_err());
        assert_eq!(builder.into_path(), before);
    }
}

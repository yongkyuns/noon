use super::*;

pub trait WithPosition: WithId {
    fn move_to(&self, x: f32, y: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::to(Position { x, y }).into(),
        }
    }
    fn move_by(&self, x: f32, y: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::by(Position { x, y }).into(),
        }
    }
    fn shift(&self, direction: Vector) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::by(Position {
                x: direction.x,
                y: direction.y,
            })
            .into(),
        }
    }
    fn move_to_object(&self, object: impl Into<Entity>) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::<Position>::to_target(object.into()).into(),
        }
    }
}

pub trait WithAngle: WithId {
    fn set_angle(&self, to_radians: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: vec![Animation::to(Angle(to_radians)).into()],
        }
    }
    fn rotate(&self, by_radians: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: vec![Animation::by(Angle(by_radians)).into()],
        }
    }
}
pub trait WithSize: WithId {
    fn set_size(&self, width: f32, height: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: vec![Animation::to(Size::from(width, height)).into()],
        }
    }
    fn scale(&self, by: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: vec![Animation::times(Size::from(by, by)).into()],
        }
    }
    fn scale_x(&self, x: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: vec![Animation::times(Size::from(x, 1.0)).into()],
        }
    }
    fn scale_y(&self, y: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: vec![Animation::times(Size::from(1.0, y)).into()],
        }
    }
    fn scale_xy(&self, x: f32, y: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: vec![Animation::times(Size::from(x, y)).into()],
        }
    }
}

pub trait WithFontSize: WithId {
    fn set_font_size(&self, size: u32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: vec![Animation::to(FontSize(size)).into()],
        }
    }
}

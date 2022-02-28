use super::*;

pub trait WithPosition: WithId {
    fn move_to(&self, x: f32, y: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::to(Position { x, y }).into(),
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
    fn set_angle(&self, angle: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: vec![Animation::to(Angle(angle)).into()],
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
}
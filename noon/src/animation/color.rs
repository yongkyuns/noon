use crate::{Color, ColorExtension};

use super::*;

pub trait WithStrokeWeight: WithId {
    fn set_stroke_weight(&self, weight: f32) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::to(StrokeWeight(weight)).into(),
        }
    }
    fn set_stroke_weight_from(&self, entity: impl Into<Entity>) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::<StrokeWeight>::to_target(entity.into()).into(),
        }
    }
}

pub trait WithStroke: WithId {
    fn set_stroke_color(&self, color: Color) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::to(StrokeColor(color)).into(),
        }
    }
    fn set_stroke_color_from(&self, entity: impl Into<Entity>) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::<StrokeColor>::to_target(entity.into()).into(),
        }
    }
}

pub trait WithFill: WithId {
    fn set_fill_color(&self, color: Color) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::to(FillColor(color)).into(),
        }
    }
    fn set_fill_color_from(&self, entity: impl Into<Entity>) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::<FillColor>::to_target(entity.into()).into(),
        }
    }
}

pub trait WithColor: WithId {
    fn set_color(&self, color: Color) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: vec![
                Animation::to(StrokeColor(color.brighten())).into(),
                Animation::to(FillColor(color)).into(),
            ],
        }
    }
    fn set_color_from(&self, entity: impl Into<Entity>) -> EntityAnimations {
        let entity: Entity = entity.into();
        EntityAnimations {
            entity: self.id(),
            animations: vec![
                Animation::<StrokeColor>::to_target(entity).into(),
                Animation::<FillColor>::to_target(entity).into(),
            ],
        }
    }
}

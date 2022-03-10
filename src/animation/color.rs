use crate::{Color, ColorExtension, Path};

use super::*;

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

pub trait WithPath: WithId {
    fn show_creation(&self) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: vec![
                Animation::<Opacity>::to(Opacity::FULL)
                    .with_duration(0.0)
                    .into(),
                Animation::<PathCompletion>::to(PathCompletion(1.0)).into(),
            ],
        }
    }
    fn fade_in(&self) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::to(Opacity(1.0)).into(),
        }
    }
    fn fade_out(&self) -> EntityAnimations {
        EntityAnimations {
            entity: self.id(),
            animations: Animation::to(Opacity(0.0)).into(),
        }
    }
    fn morph(&self, entity: impl Into<Entity>) -> EntityAnimations {
        let entity: Entity = entity.into();
        EntityAnimations {
            entity: self.id(),
            animations: vec![
                Animation::<Path>::to_target(entity).into(),
                Animation::<StrokeColor>::to_target(entity).into(),
                Animation::<FillColor>::to_target(entity).into(),
                Animation::<Position>::to_target(entity).into(),
            ],
        }
    }
}

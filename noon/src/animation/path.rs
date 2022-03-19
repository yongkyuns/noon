use super::*;
use crate::Path;

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
            animations: vec![
                Animation::<PathCompletion>::to(PathCompletion(1.0))
                    .with_duration(0.0)
                    .into(),
                Animation::to(Opacity(1.0)).into(),
            ],
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
                Animation::<Angle>::to_target(entity).into(),
                Animation::<Scale>::to_target(entity).into(),
                Animation::<StrokeWeight>::to_target(entity).into(),
                Animation::<StrokeColor>::to_target(entity).into(),
                Animation::<FillColor>::to_target(entity).into(),
                Animation::<Position>::to_target(entity).into(),
            ],
        }
    }
}

pub trait Create<ObjectId: Into<Entity> + Copy> {
    fn scene_mut(&mut self) -> &mut Scene;
    fn make(&mut self) -> ObjectId;
    fn show(&mut self) -> ObjectId {
        let id = self.make();
        let animations = EntityAnimations {
            entity: id.into(),
            animations: vec![
                Animation::to(Opacity(1.0)).into(),
                Animation::to(PathCompletion(1.0)).into(),
            ],
        };

        AnimBuilder::new(self.scene_mut(), animations.into()).run_time(0.0);
        id
    }
}

use crate::component::Children;

use super::common::*;

#[derive(Component)]
pub struct Empty;

pub struct EmptyBuilder<'a> {
    size: Size,
    position: Position,
    angle: Angle,
    children: Children,
    scene: &'a mut Scene,
}

impl<'a> EmptyBuilder<'a> {
    fn new(scene: &'a mut Scene) -> Self {
        Self {
            size: Size {
                width: 1.0,
                height: 1.0,
            },
            position: Default::default(),
            angle: Default::default(),
            children: Default::default(),
            scene,
        }
    }

    pub fn add(mut self, entity: impl Into<Entity>) -> Self {
        self.children.add(entity);
        self
    }
}

crate::angle_builder!(EmptyBuilder);
crate::position_builder!(EmptyBuilder);
crate::size_builder!(EmptyBuilder);

impl Create<EmptyId> for EmptyBuilder<'_> {
    fn scene_mut(&mut self) -> &mut Scene {
        &mut self.scene
    }
    fn make(&mut self) -> EmptyId {
        // let depth = self.scene.increment_counter();
        let world = &mut self.scene.world;
        let id = world
            .spawn()
            .insert(Empty)
            .insert(self.children.clone())
            .insert(self.size)
            .insert(BoundingSize(self.size))
            .insert(Previous(self.size))
            .insert(self.position)
            .insert(self.angle)
            .id();

        id.into()
    }
}

pub fn empty(scene: &mut Scene) -> EmptyBuilder {
    EmptyBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct EmptyId(pub(crate) Entity);

impl WithPosition for EmptyId {}
impl WithAngle for EmptyId {}
impl WithSize for EmptyId {}

impl WithId for EmptyId {
    fn id(&self) -> Entity {
        self.0
    }
}

impl From<EmptyId> for Entity {
    fn from(id: EmptyId) -> Self {
        id.0
    }
}

impl From<Entity> for EmptyId {
    fn from(id: Entity) -> Self {
        EmptyId(id)
    }
}

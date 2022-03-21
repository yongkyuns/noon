use crate::{
    component::{Children, Parent},
    prelude::Direction,
    WithArrange,
};

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

        // Compute the centroid to place the group origin
        let mut points = Vec::new();
        for id in self.children.0.iter() {
            let entity = self.scene.world.entity(*id);
            if let Some(position) = entity.get::<Position>() {
                points.push(Point::new(position.x, position.y));
            }
        }
        self.position = Position::from_points(&points);

        let transform = Transform::translation(self.position.x, self.position.y);

        // Create the empty object
        let world = &mut self.scene.world;
        let id = world
            .spawn()
            .insert(Empty)
            .insert(self.children.clone())
            .insert(self.size)
            .insert(Scale::ONE)
            .insert(self.position)
            .insert(self.angle)
            .insert(transform)
            .insert(Origin::none())
            .id();

        // Change the position of previous
        for id in self.children.0.iter() {
            let mut entity = self.scene.world.entity_mut(*id);
            entity.insert(Parent(*id));
            if let Some(mut child_position) = entity.get_mut::<Position>() {
                let child_point: Point = (*child_position).into();
                let parent_point: Point = (self.position).into();
                let v = child_point - parent_point;
                *child_position = Position::new(v.x, v.y);
                // if let Some(mut child_angle) = entity.get_mut::<Angle>() {
                //     *child_angle = Angle(child_angle.0 - self.angle.0);
                // }
            }
        }

        id.into()
    }
}

pub fn empty(scene: &mut Scene) -> EmptyBuilder {
    EmptyBuilder::new(scene)
}

#[derive(Debug, Copy, Clone)]
pub struct EmptyId(pub(crate) Entity);

// impl EmptyId {
//     fn arrange(&self, direction: Direction) -> EntityAnimations {
//         EntityAnimations {
//             entity: self.0,
//             animations: Animation::<Arrange>::to().into(),
//         }
//     }
// }

crate::into_entity!(EmptyId);

impl WithPosition for EmptyId {}
impl WithAngle for EmptyId {}
impl WithSize for EmptyId {}
impl WithArrange for EmptyId {}

// impl WithId for EmptyId {
//     fn id(&self) -> Entity {
//         self.0
//     }
// }

// impl From<EmptyId> for Entity {
//     fn from(id: EmptyId) -> Self {
//         id.0
//     }
// }

// impl From<Entity> for EmptyId {
//     fn from(id: Entity) -> Self {
//         EmptyId(id)
//     }
// }

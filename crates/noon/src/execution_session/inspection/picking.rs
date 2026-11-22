//! Read-only spatial pick data from a coherent renderer publication. Only effective
//! mesh rows are retained, sharing the immutable resource allocations.

use std::sync::Arc;

use noon_core::{
    GeometryRef, GeometryResource, MeshBounds3D, MeshResource, ObjectId, SemanticCamera3D,
    SemanticProjection3D, SemanticVec3 as Vec3, SemanticWorldTransform3D, Vec2,
};
use noon_runtime::RendererPublication;

#[derive(Clone, PartialEq)]
struct PickMesh {
    id: ObjectId,
    world: SemanticWorldTransform3D,
    mesh: Arc<MeshResource>,
}

/// Derived spatial bounds updated from the same sparse changes as the renderer.
/// A displayed frame retains an immutable root and shared mesh resources.
#[derive(Default)]
pub struct SpatialPickCache {
    root: Option<Arc<PickNode>>,
    capacity: usize,
}

#[derive(Clone)]
pub struct SpatialPickFrame {
    camera: SemanticCamera3D,
    root: Option<Arc<PickNode>>,
}

struct PickNode {
    bounds: MeshBounds3D,
    contents: PickContents,
}

enum PickContents {
    Branch([Option<Arc<PickNode>>; 2]),
    Mesh(PickMesh),
}

impl PickNode {
    fn branch(children: [Option<Arc<Self>>; 2]) -> Option<Arc<Self>> {
        let bounds = match &children {
            [Some(left), Some(right)] => join_bounds(left.bounds, right.bounds),
            [Some(child), None] | [None, Some(child)] => child.bounds,
            [None, None] => return None,
        };
        Some(Arc::new(Self {
            bounds,
            contents: PickContents::Branch(children),
        }))
    }

    /// Replace only the leaf's ancestor path. Unchanged subtrees can stay
    /// shared with any frame whose presentation receipt is still retained.
    fn update(
        old: Option<&Arc<Self>>,
        width: usize,
        index: usize,
        row: Option<PickMesh>,
    ) -> Option<Arc<Self>> {
        if width == 1 {
            let row = row?;
            if let Some(old) = old {
                if let PickContents::Mesh(mesh) = &old.contents {
                    if row.id == mesh.id
                        && row.world == mesh.world
                        && Arc::ptr_eq(&row.mesh, &mesh.mesh)
                    {
                        return Some(Arc::clone(old));
                    }
                }
            }
            let bounds = row.mesh.bounds();
            let mut world_bounds = None;
            for x in [bounds.min.x, bounds.max.x] {
                for y in [bounds.min.y, bounds.max.y] {
                    for z in [bounds.min.z, bounds.max.z] {
                        let point = row.world.transform_point(Vec3::new(x, y, z))?;
                        let corner = MeshBounds3D {
                            min: point,
                            max: point,
                        };
                        world_bounds =
                            Some(world_bounds.map_or(corner, |other| join_bounds(other, corner)));
                    }
                }
            }
            return Some(Arc::new(Self {
                bounds: world_bounds?,
                contents: PickContents::Mesh(row),
            }));
        }
        let half = width / 2;
        let side = usize::from(index >= half);
        let children = match old.map(|node| &node.contents) {
            Some(PickContents::Branch(children)) => children.clone(),
            _ => [None, None],
        };
        let next = Self::update(children[side].as_ref(), half, index % half, row);
        if children[side]
            .as_ref()
            .zip(next.as_ref())
            .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
            || (children[side].is_none() && next.is_none())
        {
            return old.cloned();
        }
        let mut children = children;
        children[side] = next;
        Self::branch(children)
    }

    fn pick(&self, ray: &Ray, near: f64, best: &mut f64, target: &mut Option<ObjectId>) {
        if !ray.intersects_bounds(self.bounds.min, self.bounds.max, near, *best) {
            return;
        }
        let row = match &self.contents {
            PickContents::Branch(children) => {
                for child in children.iter().flatten() {
                    child.pick(ray, near, best, target);
                }
                return;
            }
            PickContents::Mesh(row) => row,
        };
        let Some(ray) = Ray::in_mesh(ray.origin, ray.direction, row.world) else {
            return;
        };
        let bounds = row.mesh.bounds();
        if !ray.intersects_bounds(bounds.min, bounds.max, near, *best) {
            return;
        }
        let positions = row.mesh.positions();
        for indices in row.mesh.indices().as_chunks::<3>().0 {
            let [a, b, c] = indices.map(|i| positions[i as usize]);
            if let Some(depth) = ray
                .triangle(a, b, c)
                .filter(|depth| *depth >= near && *depth <= *best)
            {
                *best = depth;
                *target = Some(row.id);
            }
        }
    }
}

fn join_bounds(a: MeshBounds3D, b: MeshBounds3D) -> MeshBounds3D {
    MeshBounds3D {
        min: Vec3::new(
            a.min.x.min(b.min.x),
            a.min.y.min(b.min.y),
            a.min.z.min(b.min.z),
        ),
        max: Vec3::new(
            a.max.x.max(b.max.x),
            a.max.y.max(b.max.y),
            a.max.z.max(b.max.z),
        ),
    }
}

impl SpatialPickCache {
    pub fn capture(
        &mut self,
        publication: &RendererPublication<'_>,
        camera_override: Option<SemanticCamera3D>,
        snapshot: bool,
    ) -> Option<SpatialPickFrame> {
        let frame = publication.frame();
        let camera = camera_override.or_else(|| {
            let index = publication.object_index(publication.primary_view()?)?;
            let row = &frame.objects[index];
            let world = row.world_transform()?;
            SemanticCamera3D::new(world.translation, world.rotation, row.camera_projection()?)
        });
        let Some(camera) = camera else {
            *self = Self::default();
            return None;
        };

        let resources = publication.geometry_resources();
        let row_at = |index: usize| {
            let row = frame.objects.get(index)?;
            let spatial = row.spatial.as_deref()?;
            if !frame.is_drawable(index)
                || spatial.draw_kind != noon_compile::CompiledSpatialDrawKind::Mesh
                || row.appearance <= 0.0
                || row.style.opacity <= 0.0
                || !row.style.fill.is_some_and(|fill| fill.alpha > 0.0)
            {
                return None;
            }
            let GeometryRef::External(id) = row.content.geometry()? else {
                return None;
            };
            let GeometryResource::Mesh(mesh) = resources.get(resources.current_handle(*id)?)?
            else {
                return None;
            };
            Some(PickMesh {
                id: row.id,
                world: spatial.world,
                mesh: Arc::clone(mesh),
            })
        };
        let changes = publication.changes();
        if self.capacity == 0 || snapshot || changes.is_all() {
            self.capacity = frame.objects.len().next_power_of_two();
            self.root = None;
            for index in publication
                .painter_order()
                .iter()
                .map(|&index| index as usize)
            {
                if let Some(row) = row_at(index) {
                    self.root =
                        PickNode::update(self.root.as_ref(), self.capacity, index, Some(row));
                }
            }
        } else {
            while self.capacity < frame.objects.len() {
                self.capacity *= 2;
                self.root = PickNode::branch([self.root.take(), None]);
            }
            for &index in changes.object_indices() {
                if index < self.capacity {
                    self.root =
                        PickNode::update(self.root.as_ref(), self.capacity, index, row_at(index));
                }
            }
        }
        Some(SpatialPickFrame {
            camera,
            root: self.root.clone(),
        })
    }
}

impl SpatialPickFrame {
    /// Pick the closest filled triangle at a viewport position, inside the
    /// camera clip range. Position uses viewport-height units from the upper left.
    pub fn pick(&self, position: Vec2, aspect: f64) -> Option<ObjectId> {
        let x = 2.0 * f64::from(position.x) - aspect;
        let y = 1.0 - 2.0 * f64::from(position.y);
        if !aspect.is_finite()
            || aspect <= 0.0
            || !x.is_finite()
            || !y.is_finite()
            || x.abs() > aspect
            || y.abs() > 1.0
        {
            return None;
        }
        let (origin, direction, near, far) = match self.camera.projection {
            SemanticProjection3D::Perspective {
                vertical_fov_radians,
                near,
                far,
            } => {
                let tangent = (vertical_fov_radians * 0.5).tan();
                (
                    Vec3::ZERO,
                    Vec3::new(x * tangent, y * tangent, -1.0),
                    near,
                    far,
                )
            }
            SemanticProjection3D::Orthographic { height, near, far } => (
                Vec3::new(x * height * 0.5, y * height * 0.5, 0.0),
                Vec3::new(0.0, 0.0, -1.0),
                near,
                far,
            ),
        };
        let origin = add(
            self.camera.position,
            self.camera.orientation.rotate_vector(origin)?,
        );
        let direction = self.camera.orientation.rotate_vector(direction)?;
        let mut best = far;
        let mut target = None;
        self.root
            .as_ref()?
            .pick(&Ray { origin, direction }, near, &mut best, &mut target);
        target
    }
}

struct Ray {
    origin: Vec3,
    direction: Vec3,
}

impl Ray {
    fn in_mesh(origin: Vec3, direction: Vec3, world: SemanticWorldTransform3D) -> Option<Self> {
        let rotation = world.rotation.inverse();
        let scale = world.scale;
        let inverse_scale = |v: Vec3| Vec3::new(v.x / scale.x, v.y / scale.y, v.z / scale.z);
        let origin = inverse_scale(rotation.rotate_vector(sub(origin, world.translation))?);
        let direction = inverse_scale(rotation.rotate_vector(direction)?);
        (origin.is_finite() && direction.is_finite()).then_some(Self { origin, direction })
    }

    fn intersects_bounds(&self, min: Vec3, max: Vec3, mut start: f64, mut end: f64) -> bool {
        for (origin, direction, low, high) in [
            (self.origin.x, self.direction.x, min.x, max.x),
            (self.origin.y, self.direction.y, min.y, max.y),
            (self.origin.z, self.direction.z, min.z, max.z),
        ] {
            if direction == 0.0 {
                if !(low..=high).contains(&origin) {
                    return false;
                }
                continue;
            }
            let a = (low - origin) / direction;
            let b = (high - origin) / direction;
            start = start.max(a.min(b));
            end = end.min(a.max(b));
            if start > end {
                return false;
            }
        }
        true
    }

    fn triangle(&self, a: Vec3, b: Vec3, c: Vec3) -> Option<f64> {
        let ab = sub(b, a);
        let ac = sub(c, a);
        let p = cross(self.direction, ac);
        let determinant = dot(ab, p);
        if determinant == 0.0 || !determinant.is_finite() {
            return None;
        }
        let offset = sub(self.origin, a);
        let u = dot(offset, p) / determinant;
        let q = cross(offset, ab);
        let v = dot(self.direction, q) / determinant;
        ((0.0..=1.0).contains(&u) && v >= 0.0 && u + v <= 1.0).then(|| dot(ac, q) / determinant)
    }
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    Vec3::new(
        a.y * b.z - a.z * b.y,
        a.z * b.x - a.x * b.z,
        a.x * b.y - a.y * b.x,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spatial_bounds_update_keeps_the_displayed_subtrees() {
        let mesh = Arc::new(
            MeshResource::new(
                vec![
                    Vec3::ZERO,
                    Vec3::new(1.0, 0.0, 0.0),
                    Vec3::new(0.0, 1.0, 0.0),
                ],
                None,
                vec![0, 1, 2],
            )
            .unwrap(),
        );
        let row = |id, x| PickMesh {
            id: ObjectId::new(id),
            world: SemanticWorldTransform3D {
                translation: Vec3::new(x, 0.0, 0.0),
                ..SemanticWorldTransform3D::IDENTITY
            },
            mesh: Arc::clone(&mesh),
        };
        let left = PickNode::update(None, 4, 0, Some(row(1, 0.0))).unwrap();
        let displayed = PickNode::update(Some(&left), 4, 3, Some(row(2, 7.0))).unwrap();
        let updated = PickNode::update(Some(&displayed), 4, 0, Some(row(1, 3.0))).unwrap();
        let right = |node: &PickNode| match &node.contents {
            PickContents::Branch([_, Some(right)]) => Arc::clone(right),
            _ => panic!("expected two subtrees"),
        };
        assert!(Arc::ptr_eq(&right(&displayed), &right(&updated)));
        assert_eq!(displayed.bounds.min.x, 0.0);
        assert_eq!(updated.bounds.min.x, 3.0);
    }
}

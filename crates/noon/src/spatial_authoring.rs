//! Spatial objects use ordinary semantic identities, publication, and handles.

use crate::{AuthoringError, DeclaredAnimation, Mobject, MobjectFamily, Scene};
use noon_core::{
    Color, GeometryResource, MeshResource, SemanticCamera3D, SemanticMutationTransaction,
    SemanticNodeCreation, SemanticObjectRole, SemanticObjectState, SemanticPaint,
    SemanticSpatialMaterial, SemanticStyle, SemanticVec3, SemanticWorldTransform3D, StoredGeometry,
    StrokeWidthMode,
};
use noon_geometry::{CairoSurfaceGrid, SurfaceGrid};
use std::{rc::Rc, sync::Arc};

/// Inert mesh constructor input. Generation/sampling happens once before admission;
/// instance motion subsequently changes the ordinary effective world transform.
#[derive(Clone, Debug)]
pub struct MeshOptions {
    pub geometry: MeshResource,
    pub transform: SemanticWorldTransform3D,
    pub style: SemanticStyle,
    pub material: SemanticSpatialMaterial,
    pub surface_uv_cell: Option<[usize; 2]>,
}

impl MeshOptions {
    /// Opaque unlit fill with no stroke is supported by the retained mesh lane.
    pub fn new(geometry: MeshResource) -> Self {
        Self {
            geometry,
            transform: SemanticWorldTransform3D::IDENTITY,
            style: SemanticStyle {
                fill: Some(SemanticPaint::Solid(Color::BLUE)),
                stroke: None,
                stroke_width: 0.0,
                ..SemanticStyle::default()
            },
            material: SemanticSpatialMaterial::Unlit,
            surface_uv_cell: None,
        }
    }

    pub fn with_transform(mut self, transform: SemanticWorldTransform3D) -> Self {
        self.transform = transform;
        self
    }

    pub fn with_style(mut self, style: SemanticStyle) -> Self {
        self.style = style;
        self
    }

    pub fn with_material(mut self, material: SemanticSpatialMaterial) -> Self {
        self.material = material;
        self
    }

    pub fn with_surface_uv_cell(mut self, uv_cell: [usize; 2]) -> Self {
        self.surface_uv_cell = Some(uv_cell);
        self
    }

    fn into_resource(
        self,
    ) -> (
        GeometryResource,
        impl FnOnce(noon_core::GeometryResourceHandle) -> SemanticObjectState,
    ) {
        let Self {
            geometry,
            transform,
            style,
            material,
            surface_uv_cell,
        } = self;
        let state = move |handle| {
            let mut state = SemanticObjectState::new(StoredGeometry::Resource(handle));
            state.transform = transform.into();
            state.style = style;
            state.set_spatial_material(material);
            state.set_surface_uv_cell(surface_uv_cell);
            state
        };
        (GeometryResource::Mesh(Arc::new(geometry)), state)
    }
}

/// Shared Rust appearance profile for a sampled surface. Cell topology and UV
/// membership come from [`SurfaceGrid`]; this profile only assigns paint and
/// material to those already sampled cells.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceOptions {
    pub fill_colors: [Color; 2],
    pub fill_opacity: f64,
    pub stroke_color: Color,
    pub stroke_width: f64,
    pub stroke_opacity: f64,
    pub material: SemanticSpatialMaterial,
}

impl Default for SurfaceOptions {
    fn default() -> Self {
        Self {
            fill_colors: [Color::BLUE_D, Color::BLUE_E],
            fill_opacity: 1.0,
            stroke_color: Color::from_hex(0xBBBBBB),
            // Manim Cairo's 0.5-pixel Surface stroke in the normalized scene
            // units used by Noon (0.01 scene units per pixel).
            stroke_width: 0.005,
            stroke_opacity: 1.0,
            material: SemanticSpatialMaterial::PointLit,
        }
    }
}

/// A normal semantic family of sampled cells. Cell UV roles are stored on the
/// semantic leaves and are recovered from current shared family membership.
#[derive(Clone, Debug)]
pub struct SurfaceFamily {
    family: MobjectFamily,
}

impl SurfaceFamily {
    /// Wrap an existing family only when every leaf carries a semantic UV role.
    pub fn from_family(family: MobjectFamily) -> Result<Self, AuthoringError> {
        family.validate()?;
        let leaves = family
            .integration_store()
            .borrow()
            .ordered_leaf_nodes(family.node_id())
            .map_err(AuthoringError::from)?;
        if leaves.is_empty() {
            return Err(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::SurfaceCellRole,
            ));
        }
        let store = family.integration_store();
        let borrowed = store.borrow();
        for leaf in leaves {
            let state = borrowed
                .semantic_object_state_checked(leaf)
                .map_err(AuthoringError::from)?;
            validate_surface_leaf(&borrowed, state)?;
        }
        drop(borrowed);
        Ok(Self { family })
    }

    pub fn family(&self) -> &MobjectFamily {
        &self.family
    }

    pub fn into_family(self) -> MobjectFamily {
        self.family
    }

    /// Reassign cell colors from retained UV roles in one atomic authored edit.
    /// Use [`Scene::set_surface_checkerboard`] when publishing through a live owner.
    pub fn set_fill_by_checkerboard(
        &self,
        colors: [Color; 2],
        opacity: f64,
    ) -> Result<(), AuthoringError> {
        let transaction = self.prepare_checkerboard_transaction(colors, opacity)?;
        transaction
            .apply(&mut self.family.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    pub(crate) fn prepare_checkerboard_transaction(
        &self,
        colors: [Color; 2],
        opacity: f64,
    ) -> Result<SemanticMutationTransaction, AuthoringError> {
        validate_unit_interval("fill opacity", opacity)?;
        let leaves = self
            .family
            .integration_store()
            .borrow()
            .ordered_leaf_nodes(self.family.node_id())
            .map_err(AuthoringError::from)?;
        let store = self.family.integration_store();
        let borrowed = store.borrow();
        let mut transaction = SemanticMutationTransaction::new();
        for leaf in leaves {
            let state = borrowed
                .semantic_object_state_checked(leaf)
                .map_err(AuthoringError::from)?;
            validate_surface_leaf(&borrowed, state)?;
            let [u, v] = state.surface_uv_cell().ok_or(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::SurfaceCellRole,
            ))?;
            let previous = state.style.clone();
            let mut style = previous.clone();
            style.fill = Some(SemanticPaint::Solid(if (u % 2 + v % 2) % 2 == 0 {
                colors[0]
            } else {
                colors[1]
            }));
            style.fill_opacity = opacity;
            if style != previous {
                transaction.replace_style(leaf, style);
            }
        }
        Ok(transaction)
    }

    pub fn set_style(&self, update: crate::StyleUpdate) -> Result<(), AuthoringError> {
        self.family.set_style(update)
    }

    pub fn set_fill(
        &self,
        color: Option<Color>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.family.set_fill(color, opacity)
    }

    pub fn set_stroke(
        &self,
        color: Option<Color>,
        width: Option<f64>,
        opacity: Option<f64>,
    ) -> Result<(), AuthoringError> {
        self.family.set_stroke(color, width, opacity)
    }

    pub fn world_affine(&mut self, edit: crate::WorldAffineEdit) -> Result<(), AuthoringError> {
        self.family.world_affine(edit)
    }
}

fn validate_unit_interval(name: &str, value: f64) -> Result<(), AuthoringError> {
    if !(0.0..=1.0).contains(&value) {
        return Err(AuthoringError::InvalidOpacity {
            name: name.to_owned(),
            value,
        });
    }
    Ok(())
}

fn validate_surface_leaf(
    store: &noon_core::SemanticStore,
    state: &SemanticObjectState,
) -> Result<[usize; 2], AuthoringError> {
    let cell = state.surface_uv_cell().ok_or(AuthoringError::Unsupported(
        crate::UnsupportedAuthoringOperation::SurfaceCellRole,
    ))?;
    let Some(noon_core::StoredGeometry::Resource(handle)) = state.content.geometry() else {
        return Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::SurfaceCellRole,
        ));
    };
    let Some(GeometryResource::Mesh(mesh)) = store.geometry_resources().get(handle) else {
        return Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::SurfaceCellRole,
        ));
    };
    // A Surface cell is a retained four-corner quad split along the canonical
    // v-low/u-high diagonal. UV metadata on an arbitrary mesh is insufficient.
    let cairo_appearance = state.spatial_material() == SemanticSpatialMaterial::CairoSurface
        && mesh.cairo_appearance().is_some();
    if mesh.positions().len() != 4
        || mesh.normals().is_none_or(|normals| normals.len() != 4)
        || (!cairo_appearance && !mesh.has_usable_normals())
        || mesh.indices() != [0, 1, 3, 1, 2, 3]
    {
        return Err(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::SurfaceCellRole,
        ));
    }
    Ok(cell)
}

fn surface_mesh_options(
    cell: noon_geometry::SurfaceCell,
    cairo_appearance: Option<noon_core::CairoSurfaceAppearance>,
    options: SurfaceOptions,
) -> Result<MeshOptions, AuthoringError> {
    let [u, v] = cell.uv_cell;
    let mesh = cell
        .into_mesh_resource()
        .map_err(|_| AuthoringError::NonFiniteGeometry)?;
    let mesh = if let Some(appearance) = cairo_appearance {
        mesh.with_cairo_appearance(appearance)
            .map_err(|_| AuthoringError::NonFiniteGeometry)?
    } else {
        if options.material == SemanticSpatialMaterial::CairoSurface {
            return Err(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::CairoSurfaceAppearanceRequired,
            ));
        }
        mesh
    };
    let mut result = MeshOptions::new(mesh);
    result.style = SemanticStyle {
        fill: Some(SemanticPaint::Solid(if (u % 2 + v % 2) % 2 == 0 {
            options.fill_colors[0]
        } else {
            options.fill_colors[1]
        })),
        fill_opacity: options.fill_opacity,
        stroke: Some(SemanticPaint::Solid(options.stroke_color)),
        stroke_width: options.stroke_width,
        stroke_opacity: options.stroke_opacity,
        stroke_width_mode: StrokeWidthMode::ScreenSpace,
        ..SemanticStyle::default()
    };
    result.material = options.material;
    Ok(result)
}

fn validate_surface_options(options: SurfaceOptions) -> Result<(), AuthoringError> {
    validate_unit_interval("fill opacity", options.fill_opacity)?;
    validate_unit_interval("stroke opacity", options.stroke_opacity)?;
    if !options.stroke_width.is_finite() || options.stroke_width < 0.0 {
        return Err(AuthoringError::NegativeStrokeWidth(options.stroke_width));
    }
    Ok(())
}

pub(crate) fn publish_mesh_creation(
    store: &mut noon_core::SemanticStore,
    options: MeshOptions,
    publish: impl FnOnce(
        &mut noon_core::SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError>,
) -> Result<noon_core::SemanticNodeId, AuthoringError> {
    let (resource, make_state) = options.into_resource();
    store.with_geometry_resources([resource], |store, handles| {
        let mut transaction = SemanticMutationTransaction::new();
        let object = transaction.create_node(SemanticNodeCreation::object(make_state(handles[0])));
        publish(store, transaction)?
            .resolve(object)
            .ok_or(AuthoringError::UnresolvedCreatedNode(object))
    })
}

pub(crate) fn publish_mesh_family(
    store: &mut noon_core::SemanticStore,
    options: Vec<MeshOptions>,
    publish: impl FnOnce(
        &mut noon_core::SemanticStore,
        SemanticMutationTransaction,
    ) -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError>,
) -> Result<noon_core::SemanticNodeId, AuthoringError> {
    let (resources, constructors): (Vec<_>, Vec<_>) =
        options.into_iter().map(MeshOptions::into_resource).unzip();
    store.with_geometry_resources(resources, |store, handles| {
        let mut transaction = SemanticMutationTransaction::new();
        let family = transaction.create_node(SemanticNodeCreation::family());
        for (make_state, handle) in constructors.into_iter().zip(handles) {
            let face = transaction.create_node(SemanticNodeCreation::object(make_state(*handle)));
            transaction.add_member(family, face);
        }
        publish(store, transaction)?
            .resolve(family)
            .ok_or(AuthoringError::UnresolvedCreatedNode(family))
    })
}

impl Scene {
    /// Apply world motion from this owner's coherent current state while running.
    pub fn world_affine(
        &mut self,
        target: crate::MobjectTarget<'_>,
        edit: crate::WorldAffineEdit,
    ) -> Result<(), AuthoringError> {
        if let crate::MobjectTarget::Object(object) = &target {
            crate::camera_motion_authoring::ensure_camera_motion_closed(object)?;
        }
        let transaction = if let Some(session) = self.running_execution() {
            crate::world_affine::prepare_world_affine_with(
                self.integration_store(),
                target,
                edit,
                |store, node| effective_world_or_authored(session, store, node),
            )?
        } else {
            crate::world_affine::prepare_world_affine(self.integration_store(), target, edit)?
        };
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Read current published pose without substituting authored base state.
    pub fn effective_world_transform(
        &self,
        object: &Mobject,
    ) -> Result<SemanticWorldTransform3D, AuthoringError> {
        self.require_object(object)?;
        let session = self.running_execution().ok_or(AuthoringError::Unsupported(
            crate::UnsupportedAuthoringOperation::EffectiveStateUnavailable,
        ))?;
        effective_world(
            session,
            &self.integration_store().borrow(),
            object.node_id(),
        )
    }

    /// Read the center of the object's effective world-space bounds. While cold,
    /// the authored pose is the effective pose; detached live objects fall
    /// back to their authored pose after session provenance has been checked.
    pub fn effective_world_center(&self, object: &Mobject) -> Result<SemanticVec3, AuthoringError> {
        self.require_object(object)?;
        let state = object.state()?;
        let world = match self.running_execution() {
            Some(session) => effective_world_or_authored(
                session,
                &self.integration_store().borrow(),
                object.node_id(),
            )?,
            None => object.world_transform()?,
        };
        crate::world_affine::world_bounds_center(
            &self.integration_store().borrow(),
            object.node_id(),
            &state,
            world,
        )
    }

    /// Read the center of a family's coherent effective world-space bounds.
    /// Detached live leaves use authored poses after session provenance checks.
    pub fn effective_world_family_center(
        &self,
        family: &MobjectFamily,
    ) -> Result<SemanticVec3, AuthoringError> {
        let target = crate::MobjectTarget::Family(family);
        let session = self.running_execution();
        crate::world_affine::target_world_center_with(
            self.integration_store(),
            target,
            |store, node, state| match session {
                Some(session) => effective_world_or_authored(session, store, node),
                None => state
                    .transform
                    .world_transform()
                    .ok_or(AuthoringError::NonFiniteObjectState),
            },
        )
    }

    /// Admit one detached mesh through the Scene-owned atomic resource publication.
    pub fn mesh(&mut self, options: MeshOptions) -> Result<Mobject, AuthoringError> {
        let node = self.with_semantic_publication(|store, publish| {
            publish_mesh_creation(store, options, publish)
        })?;
        Mobject::from_node(Rc::clone(self.integration_store()), node)
    }

    /// Admit a detached family of independently addressable mesh faces/caps.
    /// Family and resources share one rollback boundary and semantic identity space.
    pub fn mesh_family(
        &mut self,
        options: Vec<MeshOptions>,
    ) -> Result<MobjectFamily, AuthoringError> {
        let node = self.with_semantic_publication(|store, publish| {
            publish_mesh_family(store, options, publish)
        })?;
        MobjectFamily::from_node(Rc::clone(self.integration_store()), node)
    }

    /// Publish a rectangular prism as six immutable, individually ordered face
    /// meshes in one ordinary semantic family transaction. `fill_opacity` is
    /// retained per face for the renderer's existing depth-sorted alpha path.
    pub fn prism_face_family(
        &mut self,
        size: SemanticVec3,
        fill_color: Color,
        fill_opacity: f64,
    ) -> Result<MobjectFamily, AuthoringError> {
        validate_unit_interval("fill opacity", fill_opacity)?;
        let faces =
            noon_geometry::prism_faces(size).map_err(|_| AuthoringError::NonFiniteGeometry)?;
        let options = faces
            .into_iter()
            .map(|geometry| {
                let mut mesh = MeshOptions::new(geometry);
                mesh.style.fill = Some(SemanticPaint::Solid(fill_color));
                mesh.style.fill_opacity = fill_opacity;
                mesh.style.stroke = None;
                mesh.style.stroke_width = 0.0;
                mesh
            })
            .collect();
        self.mesh_family(options)
    }

    /// Publish a cube through the same six-face family path as a prism.
    pub fn cube_face_family(
        &mut self,
        side_length: f64,
        fill_color: Color,
        fill_opacity: f64,
    ) -> Result<MobjectFamily, AuthoringError> {
        self.prism_face_family(
            SemanticVec3::new(side_length, side_length, side_length),
            fill_color,
            fill_opacity,
        )
    }

    /// Publish an already sampled UV grid as ordinary semantic mesh leaves,
    /// retaining its UV roles for later atomic checkerboard changes.
    pub fn surface_family(
        &mut self,
        grid: SurfaceGrid,
        options: SurfaceOptions,
    ) -> Result<SurfaceFamily, AuthoringError> {
        validate_surface_options(options)?;
        let mut meshes = Vec::with_capacity(grid.plan().cell_count());
        for cell in grid.cells() {
            let uv_cell = cell.uv_cell;
            meshes.push(surface_mesh_options(cell, None, options)?.with_surface_uv_cell(uv_cell));
        }
        let family = self.mesh_family(meshes)?;
        SurfaceFamily::from_family(family)
    }

    /// Publish Manim-Cairo sampled UV cells with their immutable endpoint and
    /// span metadata. This material remains distinct from native PointLit.
    pub fn surface_cairo_family(
        &mut self,
        grid: CairoSurfaceGrid,
        options: SurfaceOptions,
    ) -> Result<SurfaceFamily, AuthoringError> {
        validate_surface_options(options)?;
        if options.material != SemanticSpatialMaterial::CairoSurface {
            return Err(AuthoringError::Unsupported(
                crate::UnsupportedAuthoringOperation::CairoSurfaceAppearanceRequired,
            ));
        }
        let mut meshes = Vec::with_capacity(grid.grid().plan().cell_count());
        for (cell, appearance) in grid.cells() {
            let uv_cell = cell.uv_cell;
            meshes.push(
                surface_mesh_options(cell, Some(appearance), options)?
                    .with_surface_uv_cell(uv_cell),
            );
        }
        let family = self.mesh_family(meshes)?;
        SurfaceFamily::from_family(family)
    }

    /// Atomically recolor a sampled Surface through the owning Scene. The same
    /// prepared edit is used for cold authoring and active LiveSession work.
    pub fn set_surface_checkerboard(
        &mut self,
        surface: &SurfaceFamily,
        colors: [Color; 2],
        opacity: f64,
    ) -> Result<(), AuthoringError> {
        if !Rc::ptr_eq(self.integration_store(), surface.family.integration_store()) {
            return Err(AuthoringError::ForeignStore);
        }
        let transaction = surface.prepare_checkerboard_transaction(colors, opacity)?;
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Initialize this Scene's one 3D camera before root content is attached.
    /// It remains an ordinary semantic Mobject whose world tracks drive Runtime.
    pub fn camera_3d(&mut self, camera: SemanticCamera3D) -> Result<Mobject, AuthoringError> {
        if !self
            .integration_store()
            .borrow()
            .node(self.root())
            .ok_or(AuthoringError::NonFiniteObjectState)?
            .members()
            .is_empty()
        {
            return Err(AuthoringError::CameraRequiresEmptyScene(self.root()));
        }
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
        state.set_role(SemanticObjectRole::Camera3D);
        state
            .set_camera_projection(Some(camera.projection))
            .map_err(|_| AuthoringError::NonFiniteObjectState)?;
        state.transform = SemanticWorldTransform3D::new(
            camera.position,
            camera.orientation,
            SemanticVec3::new(1.0, 1.0, 1.0),
        )
        .ok_or(AuthoringError::NonFiniteObjectState)?
        .into();
        self.create_spatial_role(state, true)
    }

    /// Create a detached point light. Color and intensity use ordinary effective
    /// fill/opacity properties; motion uses the same world transform as meshes.
    pub fn point_light_3d(
        &mut self,
        position: SemanticVec3,
        color: Color,
        intensity: f64,
    ) -> Result<Mobject, AuthoringError> {
        if !(0.0..=1.0).contains(&intensity) {
            return Err(AuthoringError::InvalidOpacity {
                name: "light intensity".into(),
                value: intensity,
            });
        }
        let mut state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.0 });
        state.set_role(SemanticObjectRole::PointLight3D);
        state.transform.translation = position;
        state.style = SemanticStyle {
            fill: Some(SemanticPaint::Solid(color)),
            fill_opacity: intensity,
            stroke: None,
            stroke_width: 0.0,
            ..SemanticStyle::default()
        };
        self.create_spatial_role(state, false)
    }

    pub(crate) fn create_spatial_role(
        &mut self,
        state: SemanticObjectState,
        attach: bool,
    ) -> Result<Mobject, AuthoringError> {
        let mut transaction = SemanticMutationTransaction::new();
        let node = transaction.create_node(SemanticNodeCreation::object(state));
        if attach {
            transaction.add_member(self.root(), node);
        }
        let node = self
            .apply_semantic_transaction(transaction)?
            .resolve(node)
            .ok_or(AuthoringError::UnresolvedCreatedNode(node))?;
        Mobject::from_node(Rc::clone(self.integration_store()), node)
    }

    /// Publish a complete authored world transform before or after bootstrap.
    /// Timeline/reactive effective state remains Runtime's responsibility.
    pub fn set_world_transform(
        &mut self,
        object: &Mobject,
        world: SemanticWorldTransform3D,
    ) -> Result<(), AuthoringError> {
        self.require_object(object)?;
        crate::camera_motion_authoring::ensure_camera_motion_closed(object)?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_object_transform(object.node_id(), world.into());
        self.apply_semantic_transaction(transaction).map(|_| ())
    }

    /// Declare shared world-transform-to intent before execution bootstrap.
    /// Activation captures the then-current effective pose through the ordinary
    /// LiveSession animation path.
    pub fn declare_world_transform(
        &self,
        object: &Mobject,
        target: SemanticWorldTransform3D,
        options: noon_core::AnimationOptions,
    ) -> Result<DeclaredAnimation, String> {
        self.require_object(object)
            .map_err(|error| error.to_string())?;
        if self.running_execution().is_some() {
            return Err(
                "world track declaration requires unstarted execution; use a live composition"
                    .into(),
            );
        }
        if options.lag_ratio.is_some()
            || options.path_arc.is_some()
            || options.reverse_rate_function.is_some()
            || options.remover.is_some()
            || options.introducer.is_some()
        {
            return Err("world tracks support duration and rate function options".into());
        }
        let mut transaction = SemanticMutationTransaction::new();
        let animation =
            transaction.create_world_transform_animation(object.node_id(), target, options);
        let node = transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map_err(|error| error.to_string())?
            .resolve(animation)
            .ok_or_else(|| "world track declaration was not resolved".to_owned())?;
        Ok(DeclaredAnimation::new(
            Rc::clone(self.integration_store()),
            node,
        ))
    }
}

impl MobjectFamily {
    /// Authored world affine edit; Scene/LiveSession publish edits into execution.
    pub fn world_affine(&mut self, edit: crate::WorldAffineEdit) -> Result<(), AuthoringError> {
        let transaction = crate::world_affine::prepare_world_affine(
            self.integration_store(),
            (&*self).into(),
            edit,
        )?;
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    /// Cold-authoring mesh family without creating an extra Scene root.
    pub fn from_meshes(
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        options: Vec<MeshOptions>,
    ) -> Result<Self, AuthoringError> {
        let node = publish_mesh_family(&mut store.borrow_mut(), options, |store, transaction| {
            transaction.apply(store).map_err(AuthoringError::from)
        })?;
        Self::from_node(store, node)
    }
}

impl Mobject {
    /// Authored world affine edit; Scene/LiveSession publish edits into execution.
    pub fn world_affine(&mut self, edit: crate::WorldAffineEdit) -> Result<(), AuthoringError> {
        let transaction = crate::world_affine::prepare_world_affine(
            self.integration_store(),
            (&*self).into(),
            edit,
        )?;
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }

    /// Cold-authoring constructor; running scenes use their publication owner.
    pub fn from_mesh(
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        options: MeshOptions,
    ) -> Result<Self, AuthoringError> {
        let node =
            publish_mesh_creation(&mut store.borrow_mut(), options, |store, transaction| {
                transaction.apply(store).map_err(AuthoringError::from)
            })?;
        Self::from_node(store, node)
    }

    /// Authored full-precision pose. Reading effective Runtime state requires Scene.
    pub fn world_transform(&self) -> Result<SemanticWorldTransform3D, AuthoringError> {
        self.state()?
            .transform
            .world_transform()
            .ok_or(AuthoringError::NonFiniteObjectState)
    }

    /// Authored world-space bounds center, with the same semantics as default
    /// world-affine pivots.
    pub fn world_center(&self) -> Result<SemanticVec3, AuthoringError> {
        let state = self.state()?;
        crate::world_affine::world_bounds_center(
            &self.integration_store().borrow(),
            self.node_id(),
            &state,
            self.world_transform()?,
        )
    }

    /// Edit authored state/targets; use Scene::set_world_transform for live publication.
    pub fn set_world_transform(
        &mut self,
        world: SemanticWorldTransform3D,
    ) -> Result<(), AuthoringError> {
        let mut transaction = SemanticMutationTransaction::new();
        transaction.set_object_transform(self.node_id(), world.into());
        transaction
            .apply(&mut self.integration_store().borrow_mut())
            .map(|_| ())
            .map_err(AuthoringError::from)
    }
}

pub(crate) fn effective_world(
    session: &crate::ExecutionSession,
    store: &noon_core::SemanticStore,
    node: noon_core::SemanticNodeId,
) -> Result<SemanticWorldTransform3D, AuthoringError> {
    session
        .effective_semantic_object(store, node)?
        .object
        .world_transform()
        .ok_or(AuthoringError::NonFiniteObjectState)
}

pub(crate) fn effective_world_or_authored(
    session: &crate::ExecutionSession,
    store: &noon_core::SemanticStore,
    node: noon_core::SemanticNodeId,
) -> Result<SemanticWorldTransform3D, AuthoringError> {
    session.require_published_store(store)?;
    if session.execution_object_id(node).is_none() {
        return store
            .semantic_object_state_checked(node)?
            .transform
            .world_transform()
            .ok_or(AuthoringError::NonFiniteObjectState);
    }
    effective_world(session, store, node)
}

#[cfg(test)]
mod tests;

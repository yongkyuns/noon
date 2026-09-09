//! Shared geometry authoring over store-owned semantic object state.
//!
//! Handles retain only their originating store and generational identity. All
//! durable edits use the canonical transaction vocabulary; snapshots are explicit
//! migration/export adapters owned for deletion by #958/#959.
use crate::AuthoringError;
use noon_core::{
    Bounds2D64, Color, GeometryRef, GeometryResource, PathCommand, SemanticGeometryContent,
    SemanticGeometryLayout, SemanticMutationImpact, SemanticMutationTransaction,
    SemanticNodeCreation, SemanticNodeId, SemanticObjectContent, SemanticObjectProperty,
    SemanticObjectState, SemanticPaint, SemanticStore, SemanticStyle, SemanticTransform2_5D,
    SemanticVec3, StoredGeometry, StrokeCap, StrokeJoin, StrokeWidthMode, Transform2D, Vec2,
    VectorPath,
};
use std::{cell::RefCell, rc::Rc};
mod bounds;
mod layout;
mod manim_geometry;
mod style;
use bounds::{layout_for_content, transform_layout_xy};
pub(crate) use style::{
    edit_color, edit_disable_fill, edit_disable_stroke, edit_fill, edit_fill_color,
    edit_fill_opacity, edit_manim_opacity, edit_object_opacity, edit_stroke, edit_stroke_color,
    edit_stroke_opacity, manim_color_from_effective,
};
use style::{edit_stroke_width, parse_stroke_cap, parse_stroke_join, parse_stroke_width_mode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManimNextToArgs {
    pub direction: (f64, f64),
    pub buff: f64,
    pub aligned_edge: (f64, f64),
    pub mask: (f64, f64),
}

/// Dimension matching applies height then width; stretch overrides both.
/// Center matching runs last, after the target dimensions have been resolved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ManimBecomeOptions {
    pub match_height: bool,
    pub match_width: bool,
    pub match_center: bool,
    pub stretch: bool,
}

/// World-space endpoints of one analytic Manim Line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManimLineEndpoints {
    pub start: (f64, f64),
    pub end: (f64, f64),
}

/// Inert, fully typed input for one ordinary Manim geometry object.
///
/// This owns no semantic identity, store, execution state, or clock. A live
/// session consumes it in one semantic transaction after every requested
/// constructor option has been validated against the shared semantic state.
#[derive(Clone, Debug)]
pub struct ManimGeometryOptions {
    geometry: GeometryRef,
    layout: SemanticGeometryLayout,
    transform: SemanticTransform2_5D,
    style: SemanticStyle,
}

impl ManimGeometryOptions {
    pub fn circle(radius: f64) -> Result<Self, String> {
        Ok(Self::new(
            GeometryRef::circle(positive_f32("radius", radius)?),
            manim_style(Color::RED),
        ))
    }

    pub fn ellipse(width: f64, height: f64) -> Result<Self, String> {
        let width = authoring_render_f64("width", width)?;
        let height = authoring_render_f64("height", height)?;
        if width <= 0.0 || height <= 0.0 {
            return Err("Ellipse width and height must be positive".into());
        }
        let mut options = Self::new(GeometryRef::circle(1.0), manim_style(Color::RED));
        options.layout = SemanticGeometryLayout::ManimEllipseControlHull;
        options.set_scale(width * 0.5, height * 0.5)?;
        Ok(options)
    }

    pub fn square(side: f64) -> Result<Self, String> {
        Self::rectangle(side, side)
    }

    pub fn rectangle(width: f64, height: f64) -> Result<Self, String> {
        Ok(Self::new(
            GeometryRef::rectangle(
                positive_f32("width", width)?,
                positive_f32("height", height)?,
            ),
            manim_style(Color::WHITE),
        ))
    }

    pub fn line(x1: f64, y1: f64, x2: f64, y2: f64) -> Result<Self, String> {
        Ok(Self::new(
            GeometryRef::line(semantic_xy(x1, y1)?, semantic_xy(x2, y2)?),
            manim_style(Color::WHITE),
        ))
    }

    pub fn path(path: VectorPath) -> Result<Self, String> {
        if !path.is_finite() {
            return Err("geometry must be finite".into());
        }
        Ok(Self::new(
            GeometryRef::path(path),
            manim_style(Color::WHITE),
        ))
    }

    pub fn surrounding_rectangle(
        bounds: Bounds2D64,
        buff_x: f64,
        buff_y: f64,
        corner_radius: f64,
    ) -> Result<Self, String> {
        let mut options = Self::matcher_rectangle(bounds, buff_x, buff_y, corner_radius)?;
        options.style = manim_style(Color::from_hex(0xFFFF00));
        options.set_translation(
            (bounds.min_x + bounds.max_x) * 0.5,
            (bounds.min_y + bounds.max_y) * 0.5,
        )?;
        Ok(options)
    }

    pub fn background_rectangle(
        bounds: Bounds2D64,
        buff_x: f64,
        buff_y: f64,
        corner_radius: f64,
        fill_opacity: f64,
    ) -> Result<Self, String> {
        let mut options = Self::matcher_rectangle(bounds, buff_x, buff_y, corner_radius)?;
        options.style = manim_style(Color::BLACK);
        edit_fill(&mut options.style, 0.0, 0.0, 0.0, fill_opacity)?;
        options.style.stroke_width = 0.0;
        options.style.stroke_opacity = 0.0;
        options.set_translation(
            (bounds.min_x + bounds.max_x) * 0.5,
            (bounds.min_y + bounds.max_y) * 0.5,
        )?;
        Ok(options)
    }

    fn new(geometry: GeometryRef, style: SemanticStyle) -> Self {
        Self {
            geometry,
            layout: SemanticGeometryLayout::GeometryBounds,
            transform: SemanticTransform2_5D::default(),
            style,
        }
    }

    fn matcher_rectangle(
        bounds: Bounds2D64,
        buff_x: f64,
        buff_y: f64,
        corner_radius: f64,
    ) -> Result<Self, String> {
        for (name, value) in [
            ("bounds.min_x", bounds.min_x),
            ("bounds.min_y", bounds.min_y),
            ("bounds.max_x", bounds.max_x),
            ("bounds.max_y", bounds.max_y),
            ("buff_x", buff_x),
            ("buff_y", buff_y),
        ] {
            authoring_render_f64(name, value)?;
        }
        if bounds.max_x < bounds.min_x || bounds.max_y < bounds.min_y {
            return Err("shape matcher bounds must be ordered".into());
        }
        let path = crate::rounded_rectangle_authoring::manim_rounded_rectangle_path(
            positive_f32("width", bounds.width() + 2.0 * buff_x)?,
            positive_f32("height", bounds.height() + 2.0 * buff_y)?,
            [finite_f32("corner_radius", corner_radius)?; 4],
        )
        .map_err(|error| error.to_string())?;
        Ok(Self::new(
            GeometryRef::path(path),
            manim_style(Color::WHITE),
        ))
    }

    pub fn set_translation(&mut self, x: f64, y: f64) -> Result<(), String> {
        let value = authoring_xy_f64(x, y)?;
        self.transform.translation.x = value.x;
        self.transform.translation.y = value.y;
        Ok(())
    }

    pub fn set_scale(&mut self, x: f64, y: f64) -> Result<(), String> {
        let value = authoring_xy_f64(x, y)?;
        self.transform.scale.x = value.x;
        self.transform.scale.y = value.y;
        Ok(())
    }

    pub fn scale_by(&mut self, x: f64, y: f64) -> Result<(), String> {
        let value = authoring_xy_f64(x, y)?;
        let next_x = self.transform.scale.x * value.x;
        let next_y = self.transform.scale.y * value.y;
        SemanticVec3::new(next_x, next_y, self.transform.scale.z)
            .lower_xy_f32()
            .map_err(|error| error.to_string())?;
        self.transform.scale.x = next_x;
        self.transform.scale.y = next_y;
        Ok(())
    }

    pub fn set_rotation(&mut self, angle: f64) -> Result<(), String> {
        self.transform.rotation_z = authoring_render_f64("rotation", angle)?;
        Ok(())
    }

    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), String> {
        edit_color(&mut self.style, red, green, blue, alpha)
    }

    pub fn disable_fill(&mut self) {
        edit_disable_fill(&mut self.style);
    }

    pub fn set_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), String> {
        edit_fill(&mut self.style, red, green, blue, opacity)
    }

    pub fn set_fill_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        edit_fill_color(&mut self.style, red, green, blue, alpha)
    }

    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), String> {
        edit_fill_opacity(&mut self.style, opacity)
    }

    pub fn disable_stroke(&mut self) {
        edit_disable_stroke(&mut self.style);
    }

    pub fn set_stroke(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), String> {
        edit_stroke(&mut self.style, red, green, blue, opacity)
    }

    pub fn set_stroke_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        edit_stroke_color(&mut self.style, red, green, blue, alpha)
    }

    pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), String> {
        edit_stroke_opacity(&mut self.style, opacity)
    }

    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), String> {
        edit_stroke_width(&mut self.style, width)
    }

    pub fn set_stroke_width_mode(&mut self, mode: &str) -> Result<(), String> {
        self.style.stroke_width_mode = parse_stroke_width_mode(mode)?;
        Ok(())
    }

    pub fn set_stroke_join(&mut self, join: &str) -> Result<(), String> {
        self.style.stroke_join = parse_stroke_join(join)?;
        Ok(())
    }

    pub fn set_stroke_cap(&mut self, cap: &str) -> Result<(), String> {
        self.style.stroke_cap = parse_stroke_cap(cap)?;
        Ok(())
    }

    pub fn set_object_opacity(&mut self, opacity: f64) -> Result<(), String> {
        edit_object_opacity(&mut self.style, opacity)
    }

    pub(crate) fn into_state(
        self,
        store: &mut SemanticStore,
    ) -> Result<SemanticObjectState, String> {
        if !self.geometry.is_finite()
            || !self.transform.translation.is_finite()
            || !self.transform.scale.is_finite()
            || !self.transform.rotation_z.is_finite()
            || !self.style.is_finite()
        {
            return Err("geometry, transform, and style must be finite".into());
        }
        let geometry = import_geometry(store, self.geometry)?;
        let content = SemanticGeometryContent::with_layout(geometry, self.layout)
            .map_err(|error| error.to_owned())?;
        let mut state = SemanticObjectState::new(content);
        state.transform = self.transform;
        state.style = self.style;
        Ok(state)
    }
}

/// An aliasing handle to one node. Use `copy_handle` for an independent object.
#[derive(Clone, Debug)]
pub struct Mobject {
    store: Rc<RefCell<SemanticStore>>,
    id: SemanticNodeId,
}

impl PartialEq for Mobject {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.store, &other.store) && self.id == other.id
    }
}

impl Mobject {
    pub fn new(
        store: Rc<RefCell<SemanticStore>>,
        state: SemanticObjectState,
    ) -> Result<Self, String> {
        validate_content(&store.borrow(), state.content).map_err(|error| error.to_string())?;
        let mut transaction = SemanticMutationTransaction::new();
        transaction.add_node(SemanticNodeCreation::object(state));
        let result = transaction
            .apply(&mut store.borrow_mut())
            .map_err(|error| error.to_string())?;
        let [SemanticMutationImpact::NodeAdded { node: id }] = result.impacts() else {
            unreachable!("one object creation produces one identity")
        };
        Ok(Self { store, id: *id })
    }

    pub fn from_node(
        store: Rc<RefCell<SemanticStore>>,
        id: SemanticNodeId,
    ) -> Result<Self, String> {
        let handle = Self { store, id };
        handle.validate().map_err(|error| error.to_string())?;
        Ok(handle)
    }

    pub fn store(&self) -> &Rc<RefCell<SemanticStore>> {
        &self.store
    }
    pub fn node_id(&self) -> SemanticNodeId {
        self.id
    }
    pub fn state(&self) -> Result<SemanticObjectState, String> {
        self.validate().map_err(|error| error.to_string())?;
        self.store
            .borrow()
            .semantic_object_state_checked(self.id)
            .cloned()
            .map_err(|error| error.to_string())
    }
    /// Validate this handle without mutation, preserving typed identity/resource errors.
    pub fn validate(&self) -> Result<(), AuthoringError> {
        let store = self.store.borrow();
        let state = store.semantic_object_state_checked(self.id)?;
        validate_content(&store, state.content)?;
        Ok(())
    }
    pub fn require_same_store(&self, other: &Self) -> Result<(), String> {
        if !Rc::ptr_eq(&self.store, &other.store) {
            return Err("mobjects belong to different authoring stores".into());
        }
        self.validate().map_err(|error| error.to_string())?;
        other.validate().map_err(|error| error.to_string())
    }

    /// Commit presentation changes atomically while retaining node-owned identity,
    /// source/painter metadata, role, bindings, and family membership.
    pub fn commit_state(&mut self, state: SemanticObjectState) -> Result<(), String> {
        validate_content(&self.store.borrow(), state.content).map_err(|error| error.to_string())?;
        let previous = self.state()?;
        let mut transaction = SemanticMutationTransaction::new();
        stage_state_changes(&mut transaction, self.id, &previous, &state);
        transaction
            .apply(&mut self.store.borrow_mut())
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub fn copy_handle(&self) -> Result<Self, String> {
        Self::new(Rc::clone(&self.store), self.state()?)
    }
    pub fn target_editor(&self) -> Result<Self, String> {
        self.copy_handle()
    }

    pub fn from_geometry(
        store: Rc<RefCell<SemanticStore>>,
        geometry: GeometryRef,
        style: SemanticStyle,
    ) -> Result<Self, String> {
        Self::from_geometry_state(store, geometry, SemanticTransform2_5D::default(), style)
    }
    pub fn from_manim_geometry(
        store: Rc<RefCell<SemanticStore>>,
        options: ManimGeometryOptions,
    ) -> Result<Self, String> {
        let state = options.into_state(&mut store.borrow_mut())?;
        Self::new(store, state)
    }

    fn from_geometry_state(
        store: Rc<RefCell<SemanticStore>>,
        geometry: GeometryRef,
        transform: SemanticTransform2_5D,
        style: SemanticStyle,
    ) -> Result<Self, String> {
        Self::from_manim_geometry(
            store,
            ManimGeometryOptions {
                geometry,
                layout: SemanticGeometryLayout::GeometryBounds,
                transform,
                style,
            },
        )
    }
    pub fn manim_circle(store: Rc<RefCell<SemanticStore>>, radius: f64) -> Result<Self, String> {
        Self::from_manim_geometry(store, ManimGeometryOptions::circle(radius)?)
    }
    pub fn manim_ellipse(
        store: Rc<RefCell<SemanticStore>>,
        width: f64,
        height: f64,
    ) -> Result<Self, String> {
        Self::from_manim_geometry(store, ManimGeometryOptions::ellipse(width, height)?)
    }
    pub fn manim_square(store: Rc<RefCell<SemanticStore>>, side: f64) -> Result<Self, String> {
        Self::from_manim_geometry(store, ManimGeometryOptions::square(side)?)
    }
    pub fn manim_rectangle(
        store: Rc<RefCell<SemanticStore>>,
        width: f64,
        height: f64,
    ) -> Result<Self, String> {
        Self::from_manim_geometry(store, ManimGeometryOptions::rectangle(width, height)?)
    }
    pub fn manim_line(
        store: Rc<RefCell<SemanticStore>>,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    ) -> Result<Self, String> {
        Self::from_manim_geometry(store, ManimGeometryOptions::line(x1, y1, x2, y2)?)
    }

    pub fn wire_translation(&self) -> Result<(f64, f64), String> {
        let t = self
            .state()?
            .transform
            .translation
            .lower_xy_f32()
            .map_err(|e| e.to_string())?;
        Ok((t.x as f64, t.y as f64))
    }
    pub fn wire_scale(&self) -> Result<(f64, f64), String> {
        let t = self
            .state()?
            .transform
            .scale
            .lower_xy_f32()
            .map_err(|e| e.to_string())?;
        Ok((t.x as f64, t.y as f64))
    }
    pub fn wire_rotation(&self) -> Result<f64, String> {
        Ok(finite_f32("rotation", self.state()?.transform.rotation_z)? as f64)
    }
    pub fn wire_fill(&self) -> Result<Option<(f64, f64, f64, f64)>, String> {
        let s = self.state()?.style;
        Ok(solid_color_with_opacity(s.fill.as_ref(), s.fill_opacity).map(color_tuple))
    }
    pub fn wire_stroke(&self) -> Result<Option<(f64, f64, f64, f64)>, String> {
        let s = self.state()?.style;
        Ok(solid_color_with_opacity(s.stroke.as_ref(), s.stroke_opacity).map(color_tuple))
    }
    pub fn wire_stroke_width(&self) -> Result<f64, String> {
        Ok(finite_f32("stroke width", self.state()?.style.stroke_width)? as f64)
    }
    pub fn wire_object_opacity(&self) -> Result<f64, String> {
        Ok(self.state()?.style.object_opacity as f32 as f64)
    }

    /// Return this analytic Line's authored endpoints in world space.
    pub fn manim_line_endpoints(&self) -> Result<ManimLineEndpoints, String> {
        let state = self.state()?;
        line_endpoints_for_state(&state, state.transform)
    }

    /// Return Manim's stroke-first color without applying object opacity.
    pub fn manim_color(&self) -> Result<Color, String> {
        style::manim_color_from_semantic(&self.state()?.style)
    }

    pub(crate) fn manim_line_endpoints_at(
        &self,
        transform: Transform2D,
    ) -> Result<ManimLineEndpoints, String> {
        let state = self.state()?;
        line_endpoints_for_state(
            &state,
            semantic_transform_with_effective_affine(state.transform, transform),
        )
    }

    pub fn layout_bounds(&self) -> Result<Option<Bounds2D64>, String> {
        let store = self.store.borrow();
        let state = store
            .semantic_object_state_checked(self.id)
            .map_err(|e| e.to_string())?;
        layout_for_content(&store, state.content, state.transform)
    }

    /// Resolve authored content through an effective renderer-independent
    /// transform. Live layout queries use this one-object semantic calculation
    /// rather than renderer visibility bounds, which include stroke expansion.
    pub(crate) fn layout_bounds_at(
        &self,
        transform: Transform2D,
    ) -> Result<Option<Bounds2D64>, String> {
        let store = self.store.borrow();
        let state = store
            .semantic_object_state_checked(self.id)
            .map_err(|error| error.to_string())?;
        let semantic_transform =
            semantic_transform_with_effective_affine(state.transform, transform);
        layout_for_content(&store, state.content, semantic_transform)
    }

    pub fn center(&self) -> Result<(f64, f64), String> {
        if let Some(b) = self.layout_bounds()? {
            Ok(((b.min_x + b.max_x) * 0.5, (b.min_y + b.max_y) * 0.5))
        } else {
            let t = self.state()?.transform.translation;
            Ok((t.x, t.y))
        }
    }
    pub fn width(&self) -> Result<f64, String> {
        Ok(self.layout_bounds()?.map_or(0.0, Bounds2D64::width))
    }
    pub fn height(&self) -> Result<f64, String> {
        Ok(self.layout_bounds()?.map_or(0.0, Bounds2D64::height))
    }

    pub fn become_handle(
        &mut self,
        other: &Self,
        options: ManimBecomeOptions,
    ) -> Result<(), String> {
        self.require_same_store(other)?;
        let source = self.state()?;
        let target = other.state()?;
        let state = prepare_become_state(&self.store.borrow(), &source, target, options)?;
        self.commit_state(state)
    }

    /// Match this analytic Line's immutable local endpoints to another analytic
    /// Line's world endpoints using one rotation, translation, and uniform scale.
    /// Content and paint remain owned by this object.
    pub fn match_line_handle(&mut self, other: &Self) -> Result<(), String> {
        self.require_same_store(other)?;
        let target = other.state()?;
        if target.transform.scale.x != target.transform.scale.y {
            return Err("Line.match_points target has unsupported nonuniform scaling".into());
        }
        let StoredGeometry::Line { start, end } = target
            .content
            .geometry()
            .ok_or("Line.match_points requires an analytic Line target")?
        else {
            return Err("Line.match_points requires an analytic Line target".into());
        };
        let target_start = semantic_transform_point(target.transform, start)?;
        let target_end = semantic_transform_point(target.transform, end)?;
        let transform = self.line_match_transform(target_start, target_end)?;
        let mut state = self.state()?;
        state.transform.translation.x = f64::from(transform.translation.x);
        state.transform.translation.y = f64::from(transform.translation.y);
        state.transform.rotation_z = f64::from(transform.rotation);
        state.transform.scale.x = f64::from(transform.scale.x);
        state.transform.scale.y = f64::from(transform.scale.y);
        self.commit_state(state)
    }

    /// Derive the effective transform that maps this analytic Line's immutable
    /// local endpoints onto two requested world endpoints. This is pure: callback
    /// hosts can validate first, then stage the returned transform in their phase
    /// overlay without editing authored state.
    pub fn line_match_transform(
        &self,
        target_start: Vec2,
        target_end: Vec2,
    ) -> Result<Transform2D, String> {
        let state = self.state()?;
        let StoredGeometry::Line { start, end } = state
            .content
            .geometry()
            .ok_or("Line.match_points requires an analytic Line source")?
        else {
            return Err("Line.match_points requires an analytic Line source".into());
        };
        line_match_transform(start, end, target_start, target_end)
    }
    pub fn manim_scale(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.validate().map_err(|error| error.to_string())?;
        let center = self.center()?;
        self.scale_about_center(x, y, center)
    }
    fn scale_about_center(&mut self, x: f64, y: f64, center: (f64, f64)) -> Result<(), String> {
        let mut state = self.state()?;
        scale_state_about_center(&self.store.borrow(), &mut state, x, y, center)?;
        self.commit_state(state)
    }
    pub fn replace_handle(
        &mut self,
        other: &Self,
        dim_to_match: u32,
        stretch: bool,
    ) -> Result<(), String> {
        self.require_same_store(other)?;
        if dim_to_match > 1 {
            return Err("replace supports width (0) or height (1)".into());
        }
        let (w, h) = (self.width()?, self.height()?);
        let (tw, th) = (other.width()?, other.height()?);
        let (x, y) = if stretch {
            if w == 0.0 || h == 0.0 {
                return Err("cannot stretch-replace an object with zero width or height".into());
            }
            (tw / w, th / h)
        } else {
            let (a, b) = if dim_to_match == 0 { (w, tw) } else { (h, th) };
            if a == 0.0 {
                return Err("cannot replace along a zero-length dimension".into());
            }
            (b / a, b / a)
        };
        self.scale_about_center(x, y, other.center()?)
    }
    pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.validate().map_err(|error| error.to_string())?;
        semantic_xy(x, y)?;
        let center = self.center()?;
        self.shift(x - center.0, y - center.1)
    }

    pub fn critical_point(&self, direction_x: f64, direction_y: f64) -> Result<(f64, f64), String> {
        let Some(bounds) = self.layout_bounds()? else {
            return self.center();
        };
        let center = self.center()?;
        Ok((
            if direction_x < 0.0 {
                bounds.min_x
            } else if direction_x > 0.0 {
                bounds.max_x
            } else {
                center.0
            },
            if direction_y < 0.0 {
                bounds.min_y
            } else if direction_y > 0.0 {
                bounds.max_y
            } else {
                center.1
            },
        ))
    }

    pub fn shift(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.validate().map_err(|error| error.to_string())?;
        let mut state = self.state()?;
        let offset = authoring_xy_f64(x, y)?;
        let translation = SemanticVec3::new(
            state.transform.translation.x + offset.x,
            state.transform.translation.y + offset.y,
            state.transform.translation.z,
        );
        translation
            .lower_xy_f32()
            .map_err(|error| error.to_string())?;
        state.transform.translation = translation;
        self.commit_state(state)
    }

    pub fn set_translation(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.validate().map_err(|error| error.to_string())?;
        let mut state = self.state()?;
        let value = authoring_xy_f64(x, y)?;
        state.transform.translation.x = value.x;
        state.transform.translation.y = value.y;
        self.commit_state(state)
    }

    pub fn set_scale(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.validate().map_err(|error| error.to_string())?;
        let mut state = self.state()?;
        let value = authoring_xy_f64(x, y)?;
        state.transform.scale.x = value.x;
        state.transform.scale.y = value.y;
        self.commit_state(state)
    }

    pub fn set_rotation(&mut self, angle: f64) -> Result<(), String> {
        self.validate().map_err(|error| error.to_string())?;
        let mut state = self.state()?;
        state.transform.rotation_z = authoring_render_f64("rotation", angle)?;
        self.commit_state(state)
    }

    pub fn scale(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.validate().map_err(|error| error.to_string())?;
        let mut state = self.state()?;
        let x = authoring_render_f64("scale.x", x)?;
        let y = authoring_render_f64("scale.y", y)?;
        let scale = SemanticVec3::new(
            state.transform.scale.x * x,
            state.transform.scale.y * y,
            state.transform.scale.z,
        );
        scale.lower_xy_f32().map_err(|error| error.to_string())?;
        state.transform.scale = scale;
        self.commit_state(state)
    }

    pub fn rotate(&mut self, angle: f64) -> Result<(), String> {
        self.validate().map_err(|error| error.to_string())?;
        let mut state = self.state()?;
        let angle = authoring_render_f64("rotation", angle)?;
        let rotation = state.transform.rotation_z + angle;
        finite_f32("rotation result", rotation)?;
        state.transform.rotation_z = rotation;
        self.commit_state(state)
    }

    pub fn rotate_about_point(
        &mut self,
        angle: f64,
        point_x: f64,
        point_y: f64,
    ) -> Result<(), String> {
        self.validate().map_err(|error| error.to_string())?;
        let mut state = self.state()?;
        let ((translation_x, translation_y), rotation) = rotate_affine_about_point(
            (state.transform.translation.x, state.transform.translation.y),
            state.transform.rotation_z,
            angle,
            (point_x, point_y),
        )?;
        state.transform.translation.x = translation_x;
        state.transform.translation.y = translation_y;
        state.transform.rotation_z = rotation;
        self.commit_state(state)
    }
}

fn line_endpoints_for_state(
    state: &SemanticObjectState,
    transform: SemanticTransform2_5D,
) -> Result<ManimLineEndpoints, String> {
    let StoredGeometry::Line { start, end } = state
        .content
        .geometry()
        .ok_or("Line endpoint queries require an analytic Line")?
    else {
        return Err("Line endpoint queries require an analytic Line".into());
    };
    Ok(ManimLineEndpoints {
        start: transform_layout_xy(transform, f64::from(start.x), f64::from(start.y)),
        end: transform_layout_xy(transform, f64::from(end.x), f64::from(end.y)),
    })
}

fn semantic_transform_with_effective_affine(
    mut authored: SemanticTransform2_5D,
    effective: Transform2D,
) -> SemanticTransform2_5D {
    authored.translation.x = f64::from(effective.translation.x);
    authored.translation.y = f64::from(effective.translation.y);
    authored.scale.x = f64::from(effective.scale.x);
    authored.scale.y = f64::from(effective.scale.y);
    authored.rotation_z = f64::from(effective.rotation);
    authored
}

pub(crate) fn stage_state_changes(
    transaction: &mut SemanticMutationTransaction,
    target: SemanticNodeId,
    previous: &SemanticObjectState,
    next: &SemanticObjectState,
) {
    if previous.content != next.content {
        transaction.replace_content(target, next.content);
    }
    if previous.transform.translation != next.transform.translation {
        transaction.set_property(
            target,
            SemanticObjectProperty::Translation,
            next.transform.translation,
        );
    }
    if previous.transform.scale != next.transform.scale {
        transaction.set_property(target, SemanticObjectProperty::Scale, next.transform.scale);
    }
    if previous.transform.rotation_z != next.transform.rotation_z {
        transaction.set_property(
            target,
            SemanticObjectProperty::RotationZ,
            next.transform.rotation_z,
        );
    }
    if previous.style != next.style {
        transaction.replace_style(target, next.style.clone());
    }
}

pub(crate) fn prepare_become_state(
    store: &SemanticStore,
    source: &SemanticObjectState,
    mut target: SemanticObjectState,
    options: ManimBecomeOptions,
) -> Result<SemanticObjectState, String> {
    validate_content(store, source.content).map_err(|error| error.to_string())?;
    validate_content(store, target.content).map_err(|error| error.to_string())?;

    if options.stretch {
        let source_width = state_dimension(store, source, true)?;
        let source_height = state_dimension(store, source, false)?;
        let target_width = state_dimension(store, &target, true)?;
        let target_height = state_dimension(store, &target, false)?;
        if target_width == 0.0 || target_height == 0.0 {
            return Err("cannot stretch a zero-width or zero-height target".into());
        }
        let center = state_center(store, &target)?;
        scale_state_about_center(
            store,
            &mut target,
            source_width / target_width,
            source_height / target_height,
            center,
        )?;
    } else {
        if options.match_height {
            let source_height = state_dimension(store, source, false)?;
            let target_height = state_dimension(store, &target, false)?;
            if target_height == 0.0 {
                return Err("cannot match height from a zero-height target".into());
            }
            let center = state_center(store, &target)?;
            let factor = source_height / target_height;
            scale_state_about_center(store, &mut target, factor, factor, center)?;
        }
        if options.match_width {
            let source_width = state_dimension(store, source, true)?;
            let target_width = state_dimension(store, &target, true)?;
            if target_width == 0.0 {
                return Err("cannot match width from a zero-width target".into());
            }
            let center = state_center(store, &target)?;
            let factor = source_width / target_width;
            scale_state_about_center(store, &mut target, factor, factor, center)?;
        }
    }
    if options.match_center {
        let source_center = state_center(store, source)?;
        let target_center = state_center(store, &target)?;
        target.transform.translation.x += source_center.0 - target_center.0;
        target.transform.translation.y += source_center.1 - target_center.1;
        target
            .transform
            .translation
            .lower_xy_f32()
            .map_err(|error| error.to_string())?;
    }
    Ok(target)
}

fn state_dimension(
    store: &SemanticStore,
    state: &SemanticObjectState,
    horizontal: bool,
) -> Result<f64, String> {
    Ok(
        layout_for_content(store, state.content, state.transform)?.map_or(0.0, |bounds| {
            if horizontal {
                bounds.width()
            } else {
                bounds.height()
            }
        }),
    )
}

fn state_center(store: &SemanticStore, state: &SemanticObjectState) -> Result<(f64, f64), String> {
    Ok(layout_for_content(store, state.content, state.transform)?
        .map(|bounds| {
            (
                (bounds.min_x + bounds.max_x) * 0.5,
                (bounds.min_y + bounds.max_y) * 0.5,
            )
        })
        .unwrap_or((state.transform.translation.x, state.transform.translation.y)))
}

fn scale_state_about_center(
    store: &SemanticStore,
    state: &mut SemanticObjectState,
    x: f64,
    y: f64,
    center: (f64, f64),
) -> Result<(), String> {
    state.transform.scale.x *= authoring_render_f64("scale.x", x)?;
    state.transform.scale.y *= authoring_render_f64("scale.y", y)?;
    state
        .transform
        .scale
        .lower_xy_f32()
        .map_err(|error| error.to_string())?;
    let scaled_center = state_center(store, state)?;
    state.transform.translation.x += center.0 - scaled_center.0;
    state.transform.translation.y += center.1 - scaled_center.1;
    state
        .transform
        .translation
        .lower_xy_f32()
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn rotate_affine_about_point(
    translation: (f64, f64),
    rotation: f64,
    angle: f64,
    pivot: (f64, f64),
) -> Result<((f64, f64), f64), String> {
    let angle = authoring_render_f64("rotation", angle)?;
    let pivot_x = authoring_render_f64("rotation pivot.x", pivot.0)?;
    let pivot_y = authoring_render_f64("rotation pivot.y", pivot.1)?;
    let translation_x = authoring_render_f64("translation.x", translation.0)?;
    let translation_y = authoring_render_f64("translation.y", translation.1)?;
    let rotation = authoring_render_f64("rotation", rotation)?;
    let relative_x = translation_x - pivot_x;
    let relative_y = translation_y - pivot_y;
    let cosine = angle.cos();
    let sine = angle.sin();
    Ok((
        (
            authoring_render_f64(
                "rotation result translation.x",
                pivot_x + relative_x * cosine - relative_y * sine,
            )?,
            authoring_render_f64(
                "rotation result translation.y",
                pivot_y + relative_x * sine + relative_y * cosine,
            )?,
        ),
        authoring_render_f64("rotation result", rotation + angle)?,
    ))
}

fn semantic_transform_point(transform: SemanticTransform2_5D, point: Vec2) -> Result<Vec2, String> {
    if transform.scale.x != transform.scale.y {
        return Err("Line.match_points target has unsupported nonuniform scaling".into());
    }
    let scale = authoring_render_f64("Line.match_points target scale", transform.scale.x)?;
    let rotation = authoring_render_f64("Line.match_points target rotation", transform.rotation_z)?;
    let translation_x = authoring_render_f64(
        "Line.match_points target translation.x",
        transform.translation.x,
    )?;
    let translation_y = authoring_render_f64(
        "Line.match_points target translation.y",
        transform.translation.y,
    )?;
    let x = f64::from(point.x) * scale;
    let y = f64::from(point.y) * scale;
    let (sine, cosine) = rotation.sin_cos();
    semantic_xy(
        x * cosine - y * sine + translation_x,
        x * sine + y * cosine + translation_y,
    )
}

/// Shared analytic Line endpoint matching used by authored and callback paths.
pub fn line_match_transform(
    source_start: Vec2,
    source_end: Vec2,
    target_start: Vec2,
    target_end: Vec2,
) -> Result<Transform2D, String> {
    let finite = |point: Vec2| point.x.is_finite() && point.y.is_finite();
    if !finite(source_start) || !finite(source_end) || !finite(target_start) || !finite(target_end)
    {
        return Err("Line.match_points endpoints must be finite".into());
    }
    let source_x = f64::from(source_end.x - source_start.x);
    let source_y = f64::from(source_end.y - source_start.y);
    let target_x = f64::from(target_end.x - target_start.x);
    let target_y = f64::from(target_end.y - target_start.y);
    let source_length = source_x.hypot(source_y);
    let target_length = target_x.hypot(target_y);
    if source_length == 0.0 || target_length == 0.0 {
        return Err("Line.match_points requires nondegenerate source and target Lines".into());
    }
    let scale = target_length / source_length;
    let rotation = target_y.atan2(target_x) - source_y.atan2(source_x);
    let (sine, cosine) = rotation.sin_cos();
    let local_x = f64::from(source_start.x) * scale;
    let local_y = f64::from(source_start.y) * scale;
    let translation_x = f64::from(target_start.x) - (local_x * cosine - local_y * sine);
    let translation_y = f64::from(target_start.y) - (local_x * sine + local_y * cosine);
    Ok(Transform2D {
        translation: Vec2::new(
            finite_f32("Line.match_points translation.x", translation_x)?,
            finite_f32("Line.match_points translation.y", translation_y)?,
        ),
        rotation: finite_f32("Line.match_points rotation", rotation)?,
        scale: {
            let scale = finite_f32("Line.match_points uniform scale", scale)?;
            Vec2::new(scale, scale)
        },
    })
}

fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    authoring_render_f64(name, value).map(|value| value as f32)
}

pub fn authoring_render_f64(name: &str, value: f64) -> Result<f64, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value)
}

fn unit_opacity(name: &str, value: f64) -> Result<f64, String> {
    let value = authoring_render_f64(name, value)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(format!("{name} must be between 0 and 1"));
    }
    Ok(value)
}

fn opaque_color(name: &str, red: f64, green: f64, blue: f64) -> Result<Color, String> {
    Ok(Color::rgba(
        finite_f32(&format!("{name}.red"), red)?,
        finite_f32(&format!("{name}.green"), green)?,
        finite_f32(&format!("{name}.blue"), blue)?,
        1.0,
    ))
}

pub(crate) fn solid_color_with_opacity(
    paint: Option<&SemanticPaint>,
    opacity: f64,
) -> Option<Color> {
    let SemanticPaint::Solid(color) = paint? else {
        return None;
    };
    Some(Color {
        alpha: (f64::from(color.alpha) * opacity) as f32,
        ..*color
    })
}

fn semantic_xy(x: f64, y: f64) -> Result<Vec2, String> {
    authoring_xy_f64(x, y)?
        .lower_xy_f32()
        .map_err(|error| error.to_string())
}

pub fn authoring_xy_f64(x: f64, y: f64) -> Result<SemanticVec3, String> {
    let value = SemanticVec3::new(x, y, 0.0);
    value.lower_xy_f32().map_err(|error| error.to_string())?;
    Ok(value)
}

fn normalized_direction(x: f64, y: f64) -> Result<(f64, f64), String> {
    if !x.is_finite() || !y.is_finite() {
        return Err("direction must be finite".to_owned());
    }
    let length = x.hypot(y);
    if length == 0.0 {
        return Err("direction must be non-zero".to_owned());
    }
    Ok((x / length, y / length))
}

fn positive_f32(name: &str, value: f64) -> Result<f32, String> {
    let value = finite_f32(name, value)?;
    if value <= 0.0 {
        return Err(format!("{name} must be positive"));
    }
    Ok(value)
}
fn manim_style(color: Color) -> SemanticStyle {
    SemanticStyle {
        fill: Some(SemanticPaint::Solid(color)),
        fill_opacity: 0.0,
        stroke: Some(SemanticPaint::Solid(color)),
        stroke_opacity: 1.0,
        stroke_width: 0.04,
        stroke_width_mode: StrokeWidthMode::ScreenSpace,
        stroke_join: StrokeJoin::Miter,
        stroke_cap: StrokeCap::Butt,
        object_opacity: 1.0,
    }
}

fn color_tuple(color: Color) -> (f64, f64, f64, f64) {
    (
        color.red as f64,
        color.green as f64,
        color.blue as f64,
        color.alpha as f64,
    )
}

pub(crate) fn import_geometry(
    store: &mut SemanticStore,
    geometry: GeometryRef,
) -> Result<StoredGeometry, String> {
    if !geometry.is_finite() {
        return Err("geometry must be finite".into());
    }
    match geometry {
        GeometryRef::Circle { radius } => Ok(StoredGeometry::Circle { radius }),
        GeometryRef::Rectangle { size } => Ok(StoredGeometry::Rectangle { size }),
        GeometryRef::Line { start, end } => Ok(StoredGeometry::Line { start, end }),
        GeometryRef::VectorPath(path) => {
            Ok(StoredGeometry::Resource(store.insert_geometry_path(path)?))
        }
        GeometryRef::External(_) => {
            Err("external geometry must resolve to an immutable semantic resource".into())
        }
    }
}

#[cfg(test)]
mod tests;

fn validate_content(
    store: &SemanticStore,
    content: SemanticObjectContent,
) -> Result<(), AuthoringError> {
    match content {
        SemanticObjectContent::Geometry(content) => {
            if let StoredGeometry::Resource(handle) = content.geometry() {
                store
                    .geometry_resources()
                    .get(handle)
                    .ok_or(AuthoringError::MissingGeometryResource(handle))?;
            }
        }
        SemanticObjectContent::Text(handle) => {
            store
                .text_resources()
                .get(handle)
                .ok_or(AuthoringError::MissingTextResource(handle))?;
        }
    }
    Ok(())
}

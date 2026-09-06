//! Shared geometry authoring over store-owned semantic object state.
//!
//! Handles retain only their originating store and generational identity. All
//! durable edits use the canonical transaction vocabulary; snapshots are explicit
//! migration/export adapters owned for deletion by #958/#959.
use noon_core::{
    Bounds2D64, Color, GeometryRef, GeometryResource, PathCommand, SemanticMutationImpact,
    SemanticMutationTransaction, SemanticNodeCreation, SemanticNodeId, SemanticObjectContent,
    SemanticObjectProperty, SemanticObjectState, SemanticPaint, SemanticStore, SemanticStyle,
    SemanticTransform2_5D, SemanticVec3, StoredGeometry, StrokeCap, StrokeJoin, StrokeWidthMode,
    Transform2D, Vec2, VectorPath,
};
use std::{cell::RefCell, rc::Rc};
mod bounds;
mod layout;
mod style;
use bounds::layout_for_content;
pub(crate) use style::{
    edit_color, edit_disable_fill, edit_disable_stroke, edit_fill, edit_fill_color,
    edit_fill_opacity, edit_manim_opacity, edit_object_opacity, edit_stroke, edit_stroke_color,
    edit_stroke_opacity,
};
use style::{edit_stroke_width, parse_stroke_cap, parse_stroke_join, parse_stroke_width_mode};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManimNextToArgs {
    pub direction: (f64, f64),
    pub buff: f64,
    pub aligned_edge: (f64, f64),
    pub mask: (f64, f64),
}

/// Inert, fully typed input for one ordinary Manim primitive.
///
/// This owns no semantic identity, store, execution state, or clock. A live
/// session consumes it in one semantic transaction after every requested
/// constructor option has been validated against the shared semantic state.
#[derive(Clone, Debug)]
pub struct ManimPrimitiveOptions {
    state: SemanticObjectState,
}

impl ManimPrimitiveOptions {
    pub fn circle(radius: f64) -> Result<Self, String> {
        let mut state = SemanticObjectState::new(StoredGeometry::Circle {
            radius: positive_f32("radius", radius)?,
        });
        state.style = manim_style(Color::RED);
        Ok(Self { state })
    }

    pub fn square(side: f64) -> Result<Self, String> {
        let side = positive_f32("side", side)?;
        let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: Vec2::new(side, side),
        });
        state.style = manim_style(Color::WHITE);
        Ok(Self { state })
    }

    pub fn set_translation(&mut self, x: f64, y: f64) -> Result<(), String> {
        let value = authoring_xy_f64(x, y)?;
        self.state.transform.translation.x = value.x;
        self.state.transform.translation.y = value.y;
        Ok(())
    }

    pub fn set_scale(&mut self, x: f64, y: f64) -> Result<(), String> {
        let value = authoring_xy_f64(x, y)?;
        self.state.transform.scale.x = value.x;
        self.state.transform.scale.y = value.y;
        Ok(())
    }

    pub fn set_rotation(&mut self, angle: f64) -> Result<(), String> {
        self.state.transform.rotation_z = authoring_render_f64("rotation", angle)?;
        Ok(())
    }

    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), String> {
        edit_color(&mut self.state.style, red, green, blue, alpha)
    }

    pub fn disable_fill(&mut self) {
        edit_disable_fill(&mut self.state.style);
    }

    pub fn set_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), String> {
        edit_fill(&mut self.state.style, red, green, blue, opacity)
    }

    pub fn set_fill_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        edit_fill_color(&mut self.state.style, red, green, blue, alpha)
    }

    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), String> {
        edit_fill_opacity(&mut self.state.style, opacity)
    }

    pub fn disable_stroke(&mut self) {
        edit_disable_stroke(&mut self.state.style);
    }

    pub fn set_stroke(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), String> {
        edit_stroke(&mut self.state.style, red, green, blue, opacity)
    }

    pub fn set_stroke_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        edit_stroke_color(&mut self.state.style, red, green, blue, alpha)
    }

    pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), String> {
        edit_stroke_opacity(&mut self.state.style, opacity)
    }

    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), String> {
        edit_stroke_width(&mut self.state.style, width)
    }

    pub fn set_stroke_width_mode(&mut self, mode: &str) -> Result<(), String> {
        self.state.style.stroke_width_mode = parse_stroke_width_mode(mode)?;
        Ok(())
    }

    pub fn set_stroke_join(&mut self, join: &str) -> Result<(), String> {
        self.state.style.stroke_join = parse_stroke_join(join)?;
        Ok(())
    }

    pub fn set_stroke_cap(&mut self, cap: &str) -> Result<(), String> {
        self.state.style.stroke_cap = parse_stroke_cap(cap)?;
        Ok(())
    }

    pub fn set_object_opacity(&mut self, opacity: f64) -> Result<(), String> {
        edit_object_opacity(&mut self.state.style, opacity)
    }

    pub(crate) fn into_state(self) -> SemanticObjectState {
        self.state
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
        validate_content(&store.borrow(), state.content)?;
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
        handle.validate()?;
        Ok(handle)
    }

    pub fn store(&self) -> &Rc<RefCell<SemanticStore>> {
        &self.store
    }
    pub fn node_id(&self) -> SemanticNodeId {
        self.id
    }
    pub fn state(&self) -> Result<SemanticObjectState, String> {
        self.validate()?;
        self.store
            .borrow()
            .semantic_object_state_checked(self.id)
            .cloned()
            .map_err(|error| error.to_string())
    }
    pub fn validate(&self) -> Result<(), String> {
        let store = self.store.borrow();
        let state = store
            .semantic_object_state_checked(self.id)
            .map_err(|error| error.to_string())?;
        validate_content(&store, state.content)?;
        Ok(())
    }
    pub fn require_same_store(&self, other: &Self) -> Result<(), String> {
        if !Rc::ptr_eq(&self.store, &other.store) {
            return Err("mobjects belong to different authoring stores".into());
        }
        self.validate()?;
        other.validate()
    }

    /// Commit presentation changes atomically while retaining node-owned identity,
    /// source/painter metadata, role, bindings, and family membership.
    pub fn commit_state(&mut self, state: SemanticObjectState) -> Result<(), String> {
        validate_content(&self.store.borrow(), state.content)?;
        let previous = self.state()?;
        let mut transaction = SemanticMutationTransaction::new();
        if previous.content != state.content {
            transaction.replace_content(self.id, state.content);
        }
        if previous.transform.translation != state.transform.translation {
            transaction.set_property(
                self.id,
                SemanticObjectProperty::Translation,
                state.transform.translation,
            );
        }
        if previous.transform.scale != state.transform.scale {
            transaction.set_property(
                self.id,
                SemanticObjectProperty::Scale,
                state.transform.scale,
            );
        }
        if previous.transform.rotation_z != state.transform.rotation_z {
            transaction.set_property(
                self.id,
                SemanticObjectProperty::RotationZ,
                state.transform.rotation_z,
            );
        }
        if previous.style != state.style {
            transaction.replace_style(self.id, state.style);
        }
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
        if !geometry.is_finite() || !style.is_finite() {
            return Err("geometry and style must be finite".into());
        }
        let content = import_geometry(&mut store.borrow_mut(), geometry)?;
        let mut state = SemanticObjectState::new(content);
        state.style = style;
        Self::new(store, state)
    }
    pub fn manim_circle(store: Rc<RefCell<SemanticStore>>, radius: f64) -> Result<Self, String> {
        Self::new(store, ManimPrimitiveOptions::circle(radius)?.into_state())
    }
    pub fn manim_square(store: Rc<RefCell<SemanticStore>>, side: f64) -> Result<Self, String> {
        Self::new(store, ManimPrimitiveOptions::square(side)?.into_state())
    }
    pub fn manim_rectangle(
        store: Rc<RefCell<SemanticStore>>,
        width: f64,
        height: f64,
    ) -> Result<Self, String> {
        Self::from_geometry(
            store,
            GeometryRef::rectangle(
                positive_f32("width", width)?,
                positive_f32("height", height)?,
            ),
            manim_style(Color::WHITE),
        )
    }
    pub fn manim_line(
        store: Rc<RefCell<SemanticStore>>,
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
    ) -> Result<Self, String> {
        Self::from_geometry(
            store,
            GeometryRef::line(semantic_xy(x1, y1)?, semantic_xy(x2, y2)?),
            manim_style(Color::WHITE),
        )
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
        Ok(legacy_solid_color(s.fill.as_ref(), s.fill_opacity).map(color_tuple))
    }
    pub fn wire_stroke(&self) -> Result<Option<(f64, f64, f64, f64)>, String> {
        let s = self.state()?.style;
        Ok(legacy_solid_color(s.stroke.as_ref(), s.stroke_opacity).map(color_tuple))
    }
    pub fn wire_stroke_width(&self) -> Result<f64, String> {
        Ok(finite_f32("stroke width", self.state()?.style.stroke_width)? as f64)
    }
    pub fn wire_object_opacity(&self) -> Result<f64, String> {
        Ok(self.state()?.style.object_opacity as f32 as f64)
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
        let mut semantic_transform = state.transform;
        semantic_transform.translation.x = f64::from(transform.translation.x);
        semantic_transform.translation.y = f64::from(transform.translation.y);
        semantic_transform.scale.x = f64::from(transform.scale.x);
        semantic_transform.scale.y = f64::from(transform.scale.y);
        semantic_transform.rotation_z = f64::from(transform.rotation);
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

    pub fn become_handle(&mut self, other: &Self) -> Result<(), String> {
        self.require_same_store(other)?;
        self.commit_state(other.state()?)
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
        self.validate()?;
        let center = self.center()?;
        self.scale_about_center(x, y, center)
    }
    fn scale_about_center(&mut self, x: f64, y: f64, center: (f64, f64)) -> Result<(), String> {
        let mut state = self.state()?;
        state.transform.scale.x *= authoring_render_f64("scale.x", x)?;
        state.transform.scale.y *= authoring_render_f64("scale.y", y)?;
        state
            .transform
            .scale
            .lower_xy_f32()
            .map_err(|e| e.to_string())?;
        let bounds = layout_for_content(&self.store.borrow(), state.content, state.transform)?;
        let scaled_center = bounds
            .map(|b| ((b.min_x + b.max_x) * 0.5, (b.min_y + b.max_y) * 0.5))
            .unwrap_or((state.transform.translation.x, state.transform.translation.y));
        state.transform.translation.x += center.0 - scaled_center.0;
        state.transform.translation.y += center.1 - scaled_center.1;
        state
            .transform
            .translation
            .lower_xy_f32()
            .map_err(|e| e.to_string())?;
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
        self.validate()?;
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
        self.validate()?;
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
        self.validate()?;
        let mut state = self.state()?;
        let value = authoring_xy_f64(x, y)?;
        state.transform.translation.x = value.x;
        state.transform.translation.y = value.y;
        self.commit_state(state)
    }

    pub fn set_scale(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        let value = authoring_xy_f64(x, y)?;
        state.transform.scale.x = value.x;
        state.transform.scale.y = value.y;
        self.commit_state(state)
    }

    pub fn set_rotation(&mut self, angle: f64) -> Result<(), String> {
        self.validate()?;
        let mut state = self.state()?;
        state.transform.rotation_z = authoring_render_f64("rotation", angle)?;
        self.commit_state(state)
    }

    pub fn scale(&mut self, x: f64, y: f64) -> Result<(), String> {
        self.validate()?;
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
        self.validate()?;
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
        self.validate()?;
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

pub(crate) fn legacy_solid_color(paint: Option<&SemanticPaint>, opacity: f64) -> Option<Color> {
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

fn validate_content(store: &SemanticStore, content: SemanticObjectContent) -> Result<(), String> {
    match content {
        SemanticObjectContent::Geometry(StoredGeometry::Resource(handle)) => {
            store
                .geometry_resources()
                .get(handle)
                .ok_or("unknown or stale geometry resource")?;
        }
        SemanticObjectContent::Text(handle) => {
            store
                .text_resources()
                .get(handle)
                .ok_or("unknown or stale text resource")?;
        }
        SemanticObjectContent::Geometry(_) => {}
    }
    Ok(())
}

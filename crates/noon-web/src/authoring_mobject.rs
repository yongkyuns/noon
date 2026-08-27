use noon_core::{
    Bounds2D64, Color, GeometryRef, ObjectSnapshot, PathCommand, SemanticNodeId, SemanticNodeKind,
    SemanticPaint, SemanticStore, SemanticStyle, SemanticTransform2_5D, SemanticVec3, Style, Vec2,
    VectorPath,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManimNextToArgs {
    direction: (f64, f64),
    buff: f64,
    aligned_edge: (f64, f64),
    mask: (f64, f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrontendMobjectHandle {
    snapshot: ObjectSnapshot,
    semantic_transform: SemanticTransform2_5D,
    semantic_style: SemanticStyle,
}

impl FrontendMobjectHandle {
    pub fn from_snapshot(snapshot: ObjectSnapshot) -> Self {
        let transform = snapshot.transform;
        let semantic_transform = SemanticTransform2_5D {
            translation: SemanticVec3::new(
                f64::from(transform.translation.x),
                f64::from(transform.translation.y),
                0.0,
            ),
            scale: SemanticVec3::new(
                f64::from(transform.scale.x),
                f64::from(transform.scale.y),
                1.0,
            ),
            rotation_z: f64::from(transform.rotation),
        };
        let semantic_style = authoring_style_from_legacy(snapshot.style);
        Self {
            snapshot,
            semantic_transform,
            semantic_style,
        }
    }

    pub fn from_json(snapshot_json: &str) -> Result<Self, String> {
        serde_json::from_str(snapshot_json)
            .map(Self::from_snapshot)
            .map_err(|error| format!("invalid mobject snapshot: {error}"))
    }

    pub fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }

    pub fn snapshot_json(&self) -> Result<String, String> {
        serde_json::to_string(&self.snapshot)
            .map_err(|error| format!("unable to serialize mobject snapshot: {error}"))
    }

    pub fn wire_translation(&self) -> (f64, f64) {
        let value = self.snapshot.transform.translation;
        (f64::from(value.x), f64::from(value.y))
    }

    pub fn wire_scale(&self) -> (f64, f64) {
        let value = self.snapshot.transform.scale;
        (f64::from(value.x), f64::from(value.y))
    }

    pub fn wire_rotation(&self) -> f64 {
        f64::from(self.snapshot.transform.rotation)
    }

    pub fn wire_fill(&self) -> Option<(f64, f64, f64, f64)> {
        self.snapshot.style.fill.map(|color| {
            (
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                f64::from(color.alpha),
            )
        })
    }

    pub fn wire_stroke(&self) -> Option<(f64, f64, f64, f64)> {
        self.snapshot.style.stroke.map(|color| {
            (
                f64::from(color.red),
                f64::from(color.green),
                f64::from(color.blue),
                f64::from(color.alpha),
            )
        })
    }

    pub fn wire_stroke_width(&self) -> f64 {
        f64::from(self.snapshot.style.stroke_width)
    }

    pub fn wire_object_opacity(&self) -> f64 {
        f64::from(self.snapshot.style.opacity)
    }

    /// Clone this semantic state into a detached target editor. The returned
    /// handle is the editor: all target mutations stay in Rust until its final
    /// snapshot is requested for animation lowering.
    pub fn target_editor(&self) -> Self {
        self.clone()
    }

    pub fn replace_json(&mut self, snapshot_json: &str) -> Result<(), String> {
        *self = Self::from_json(snapshot_json)?;
        Ok(())
    }

    pub fn layout_bounds(&self) -> Option<Bounds2D64> {
        snapshot_layout_bounds(&self.snapshot, self.semantic_transform)
    }

    pub fn center(&self) -> (f64, f64) {
        self.layout_bounds()
            .map(|bounds| {
                (
                    (bounds.min_x + bounds.max_x) * 0.5,
                    (bounds.min_y + bounds.max_y) * 0.5,
                )
            })
            .unwrap_or_else(|| {
                let translation = self.semantic_transform.translation;
                (translation.x, translation.y)
            })
    }

    pub fn width(&self) -> f64 {
        self.layout_bounds().map_or(0.0, Bounds2D64::width)
    }

    pub fn height(&self) -> f64 {
        self.layout_bounds().map_or(0.0, Bounds2D64::height)
    }

    pub fn critical_point(&self, direction_x: f64, direction_y: f64) -> (f64, f64) {
        let Some(bounds) = self.layout_bounds() else {
            return self.center();
        };
        let center = self.center();
        (
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
        )
    }

    pub fn shift(&mut self, x: f64, y: f64) -> Result<(), String> {
        let offset = semantic_xy_f64(x, y)?;
        let translation = SemanticVec3::new(
            self.semantic_transform.translation.x + offset.x,
            self.semantic_transform.translation.y + offset.y,
            self.semantic_transform.translation.z,
        );
        translation
            .lower_xy_f32()
            .map_err(|error| error.to_string())?;
        self.semantic_transform.translation = translation;
        self.sync_legacy_transform()
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), String> {
        semantic_xy(x, y)?;
        let center = self.center();
        self.shift(x - center.0, y - center.1)
    }

    /// ManimCE-compatible move_to for a leaf mobject target.
    ///
    /// `aligned_edge` selects matching critical points and `coor_mask` suppresses
    /// translation components. Frontends only coerce host vectors; placement math
    /// stays in this shared semantic handle.
    pub fn manim_move_to_handle(
        &mut self,
        other: &Self,
        aligned_edge_x: f64,
        aligned_edge_y: f64,
        mask_x: f64,
        mask_y: f64,
    ) -> Result<(), String> {
        let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y)?;
        let mask = semantic_xy_f64(mask_x, mask_y)?;
        let source = self.critical_point(edge.x, edge.y);
        let target = other.critical_point(edge.x, edge.y);
        self.shift(
            (target.0 - source.0) * mask.x,
            (target.1 - source.1) * mask.y,
        )
    }

    /// ManimCE-compatible move_to for a point target.
    pub fn manim_move_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        aligned_edge_x: f64,
        aligned_edge_y: f64,
        mask_x: f64,
        mask_y: f64,
    ) -> Result<(), String> {
        let point = semantic_xy_f64(point_x, point_y)?;
        let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y)?;
        let mask = semantic_xy_f64(mask_x, mask_y)?;
        let source = self.critical_point(edge.x, edge.y);
        self.shift((point.x - source.0) * mask.x, (point.y - source.1) * mask.y)
    }

    /// ManimCE-compatible next_to for a leaf mobject target.
    ///
    /// Unlike Noon's generic `next_to_handle`, Manim intentionally does not
    /// normalize `direction`: both critical-point selection and `direction * buff`
    /// use the supplied vector directly.
    pub fn manim_next_to_handle(
        &mut self,
        other: &Self,
        args: ManimNextToArgs,
    ) -> Result<(), String> {
        let direction = semantic_xy_f64(args.direction.0, args.direction.1)?;
        let edge = semantic_xy_f64(args.aligned_edge.0, args.aligned_edge.1)?;
        let mask = semantic_xy_f64(args.mask.0, args.mask.1)?;
        let buff = render_f64("buffer", args.buff)?;
        let source = self.critical_point(edge.x - direction.x, edge.y - direction.y);
        let target = other.critical_point(edge.x + direction.x, edge.y + direction.y);
        self.shift(
            (target.0 - source.0 + direction.x * buff) * mask.x,
            (target.1 - source.1 + direction.y * buff) * mask.y,
        )
    }

    /// ManimCE-compatible next_to for a point target.
    pub fn manim_next_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        args: ManimNextToArgs,
    ) -> Result<(), String> {
        let point = semantic_xy_f64(point_x, point_y)?;
        let direction = semantic_xy_f64(args.direction.0, args.direction.1)?;
        let edge = semantic_xy_f64(args.aligned_edge.0, args.aligned_edge.1)?;
        let mask = semantic_xy_f64(args.mask.0, args.mask.1)?;
        let buff = render_f64("buffer", args.buff)?;
        let source = self.critical_point(edge.x - direction.x, edge.y - direction.y);
        self.shift(
            (point.x - source.0 + direction.x * buff) * mask.x,
            (point.y - source.1 + direction.y * buff) * mask.y,
        )
    }

    pub fn scale(&mut self, x: f64, y: f64) -> Result<(), String> {
        let x = render_f64("scale.x", x)?;
        let y = render_f64("scale.y", y)?;
        let scale = SemanticVec3::new(
            self.semantic_transform.scale.x * x,
            self.semantic_transform.scale.y * y,
            self.semantic_transform.scale.z,
        );
        scale.lower_xy_f32().map_err(|error| error.to_string())?;
        self.semantic_transform.scale = scale;
        self.sync_legacy_transform()
    }

    pub fn rotate(&mut self, angle: f64) -> Result<(), String> {
        let angle = render_f64("rotation", angle)?;
        let rotation = self.semantic_transform.rotation_z + angle;
        finite_f32("rotation result", rotation)?;
        self.semantic_transform.rotation_z = rotation;
        self.sync_legacy_transform()
    }

    pub fn rotate_about_point(
        &mut self,
        angle: f64,
        point_x: f64,
        point_y: f64,
    ) -> Result<(), String> {
        let angle = render_f64("rotation", angle)?;
        let pivot = semantic_xy_f64(point_x, point_y)?;
        let rotation = self.semantic_transform.rotation_z + angle;
        finite_f32("rotation result", rotation)?;

        let translation = self.semantic_transform.translation;
        let relative_x = translation.x - pivot.x;
        let relative_y = translation.y - pivot.y;
        let cosine = angle.cos();
        let sine = angle.sin();
        let next_translation = SemanticVec3::new(
            pivot.x + relative_x * cosine - relative_y * sine,
            pivot.y + relative_x * sine + relative_y * cosine,
            translation.z,
        );
        next_translation
            .lower_xy_f32()
            .map_err(|error| error.to_string())?;
        self.semantic_transform.translation = next_translation;
        self.semantic_transform.rotation_z = rotation;
        self.sync_legacy_transform()
    }

    pub fn set_color(&mut self, red: f64, green: f64, blue: f64, alpha: f64) -> Result<(), String> {
        let color = opaque_color("color", red, green, blue)?;
        let opacity = unit_opacity("color.alpha", alpha)?;
        let had_fill = self.semantic_style.fill.is_some();
        let had_stroke = self.semantic_style.stroke.is_some();
        if had_fill {
            self.semantic_style.fill = Some(SemanticPaint::Solid(color));
            self.semantic_style.fill_opacity = opacity;
        }
        if had_stroke {
            self.semantic_style.stroke = Some(SemanticPaint::Solid(color));
            self.semantic_style.stroke_opacity = opacity;
        }
        if !had_fill && !had_stroke {
            self.semantic_style.fill = Some(SemanticPaint::Solid(color));
            self.semantic_style.fill_opacity = opacity;
        }
        self.sync_legacy_style();
        Ok(())
    }

    pub fn disable_fill(&mut self) {
        self.semantic_style.fill = None;
        self.sync_legacy_style();
    }

    pub fn set_fill_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        let color = opaque_color("fill", red, green, blue)?;
        let requested_opacity = unit_opacity("fill.alpha", alpha)?;
        if self.semantic_style.fill.is_none() {
            self.semantic_style.fill_opacity = requested_opacity;
        }
        self.semantic_style.fill = Some(SemanticPaint::Solid(color));
        self.sync_legacy_style();
        Ok(())
    }

    pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let opacity = unit_opacity("fill opacity", opacity)?;
        if self.semantic_style.fill.is_none() {
            self.semantic_style.fill = Some(SemanticPaint::Solid(Color::WHITE));
        }
        self.semantic_style.fill_opacity = opacity;
        self.sync_legacy_style();
        Ok(())
    }

    /// Apply a Manim-style fill color and opacity as one target-state edit.
    ///
    /// This keeps the common `.animate.set_fill(...)` operation entirely inside
    /// the shared handle. Validate both inputs before changing state so a bad
    /// opacity cannot leave a partially edited target.
    pub fn set_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<(), String> {
        let color = opaque_color("fill", red, green, blue)?;
        let opacity = unit_opacity("fill opacity", opacity)?;
        self.semantic_style.fill = Some(SemanticPaint::Solid(color));
        self.semantic_style.fill_opacity = opacity;
        self.sync_legacy_style();
        Ok(())
    }

    pub fn fill_opacity(&self) -> f64 {
        if self.semantic_style.fill.is_some() {
            self.semantic_style.fill_opacity
        } else {
            0.0
        }
    }

    pub fn disable_stroke(&mut self) {
        self.semantic_style.stroke = None;
        self.sync_legacy_style();
    }

    pub fn set_stroke_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        let color = opaque_color("stroke", red, green, blue)?;
        let requested_opacity = unit_opacity("stroke.alpha", alpha)?;
        if self.semantic_style.stroke.is_none() {
            self.semantic_style.stroke_opacity = requested_opacity;
        }
        self.semantic_style.stroke = Some(SemanticPaint::Solid(color));
        self.sync_legacy_style();
        Ok(())
    }

    pub fn set_stroke_width(&mut self, width: f64) -> Result<(), String> {
        let width = render_f64("stroke width", width)?;
        if width < 0.0 {
            return Err("stroke width must be non-negative".to_owned());
        }
        self.semantic_style.stroke_width = width;
        if self.semantic_style.stroke.is_none() {
            self.semantic_style.stroke = Some(SemanticPaint::Solid(Color::WHITE));
            self.semantic_style.stroke_opacity = 1.0;
        }
        self.sync_legacy_style();
        Ok(())
    }

    pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let opacity = unit_opacity("stroke opacity", opacity)?;
        if self.semantic_style.stroke.is_none() {
            self.semantic_style.stroke = Some(SemanticPaint::Solid(Color::WHITE));
        }
        self.semantic_style.stroke_opacity = opacity;
        self.sync_legacy_style();
        Ok(())
    }

    pub fn stroke_opacity(&self) -> f64 {
        if self.semantic_style.stroke.is_some() {
            self.semantic_style.stroke_opacity
        } else {
            0.0
        }
    }

    pub fn set_opacity(&mut self, opacity: f64) -> Result<(), String> {
        let opacity = unit_opacity("opacity", opacity)?;
        if self.semantic_style.fill.is_some() {
            self.semantic_style.fill_opacity = opacity;
        }
        if self.semantic_style.stroke.is_some() {
            self.semantic_style.stroke_opacity = opacity;
        }
        self.sync_legacy_style();
        Ok(())
    }

    fn sync_legacy_transform(&mut self) -> Result<(), String> {
        self.snapshot.transform.translation = self
            .semantic_transform
            .translation
            .lower_xy_f32()
            .map_err(|error| error.to_string())?;
        self.snapshot.transform.scale = self
            .semantic_transform
            .scale
            .lower_xy_f32()
            .map_err(|error| error.to_string())?;
        self.snapshot.transform.rotation =
            finite_f32("rotation", self.semantic_transform.rotation_z)?;
        Ok(())
    }

    fn sync_legacy_style(&mut self) {
        self.snapshot.style.fill = legacy_solid_color(
            self.semantic_style.fill.as_ref(),
            self.semantic_style.fill_opacity,
        );
        self.snapshot.style.stroke = legacy_solid_color(
            self.semantic_style.stroke.as_ref(),
            self.semantic_style.stroke_opacity,
        );
        self.snapshot.style.stroke_width = self.semantic_style.stroke_width as f32;
        self.snapshot.style.stroke_width_mode = self.semantic_style.stroke_width_mode;
        self.snapshot.style.opacity = self.semantic_style.object_opacity as f32;
    }

    pub fn become_handle(&mut self, other: &Self) {
        self.snapshot = other.snapshot.clone();
        self.semantic_transform = other.semantic_transform;
        self.semantic_style = other.semantic_style.clone();
    }

    pub fn replace_handle(
        &mut self,
        other: &Self,
        dim_to_match: u32,
        stretch: bool,
    ) -> Result<(), String> {
        if dim_to_match > 1 {
            return Err(
                "replace currently supports width (0) or height (1) in the 2D authoring model"
                    .to_owned(),
            );
        }
        let target_center = other.center();
        let source_width = self.width();
        let source_height = self.height();
        let target_width = other.width();
        let target_height = other.height();

        if stretch {
            if source_width == 0.0 || source_height == 0.0 {
                return Err("cannot stretch-replace an object with zero width or height".to_owned());
            }
            self.scale(target_width / source_width, target_height / source_height)?;
        } else {
            let (source_length, target_length) = if dim_to_match == 0 {
                (source_width, target_width)
            } else {
                (source_height, target_height)
            };
            if source_length == 0.0 {
                return Err("cannot replace along a zero-length dimension".to_owned());
            }
            let factor = target_length / source_length;
            self.scale(factor, factor)?;
        }
        self.move_to(target_center.0, target_center.1)
    }

    pub fn next_to_handle(
        &mut self,
        other: &Self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> Result<(), String> {
        let (axis_x, axis_y) = normalized_direction(direction_x, direction_y)?;
        let source = self.critical_point(-axis_x, -axis_y);
        let target = other.critical_point(axis_x, axis_y);
        self.shift(
            target.0 - source.0 + axis_x * buff,
            target.1 - source.1 + axis_y * buff,
        )
    }

    pub fn next_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> Result<(), String> {
        semantic_xy(point_x, point_y)?;
        let (axis_x, axis_y) = normalized_direction(direction_x, direction_y)?;
        let source = self.critical_point(-axis_x, -axis_y);
        self.shift(
            point_x - source.0 + axis_x * buff,
            point_y - source.1 + axis_y * buff,
        )
    }

    pub fn align_to_handle(
        &mut self,
        other: &Self,
        direction_x: f64,
        direction_y: f64,
    ) -> Result<(), String> {
        finite_f32("direction.x", direction_x)?;
        finite_f32("direction.y", direction_y)?;
        let source = self.critical_point(direction_x, direction_y);
        let target = other.critical_point(direction_x, direction_y);
        self.shift(
            if direction_x == 0.0 {
                0.0
            } else {
                target.0 - source.0
            },
            if direction_y == 0.0 {
                0.0
            } else {
                target.1 - source.1
            },
        )
    }

    pub fn align_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        direction_x: f64,
        direction_y: f64,
    ) -> Result<(), String> {
        semantic_xy(point_x, point_y)?;
        let source = self.critical_point(direction_x, direction_y);
        self.shift(
            if direction_x == 0.0 {
                0.0
            } else {
                point_x - source.0
            },
            if direction_y == 0.0 {
                0.0
            } else {
                point_y - source.1
            },
        )
    }

    pub fn align_on_frame(
        &mut self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> Result<(), String> {
        finite_f32("direction.x", direction_x)?;
        finite_f32("direction.y", direction_y)?;
        let point = self.critical_point(direction_x, direction_y);
        let mut shift_x = 0.0;
        let mut shift_y = 0.0;
        if direction_x != 0.0 {
            let target = direction_x.signum() * f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5;
            shift_x = target - point.0 - direction_x * buff;
        }
        if direction_y != 0.0 {
            let target = direction_y.signum() * f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5;
            shift_y = target - point.1 - direction_y * buff;
        }
        self.shift(shift_x, shift_y)
    }
}

/// Shared target-family construction used by frontend Group/VGroup animation builders.
///
/// The Python/JS wrapper tree is host-language identity metadata only. This editor
/// snapshots the source family's authoritative ordered membership, validates each
/// wrapper pair against that order, and constructs the target family in the same
/// semantic store. Leaf target state is edited through `FrontendMobjectHandle`.
#[derive(Clone, Debug)]
pub struct FrontendFamilyTargetEditor {
    source_members: Vec<SemanticNodeId>,
    target: SemanticNodeId,
    next_index: usize,
}

impl FrontendFamilyTargetEditor {
    pub fn begin(store: &mut SemanticStore, source: SemanticNodeId) -> Result<Self, String> {
        let source_members = {
            let source_node = store
                .node(source)
                .ok_or_else(|| format!("unknown source family semantic node {source:?}"))?;
            if !matches!(source_node.kind(), SemanticNodeKind::Family) {
                return Err(format!("source semantic node {source:?} is not a family"));
            }
            source_node.members().to_vec()
        };
        let target = store.insert_family();
        Ok(Self {
            source_members,
            target,
            next_index: 0,
        })
    }

    pub const fn target_id(&self) -> SemanticNodeId {
        self.target
    }

    pub fn accept_member(
        &mut self,
        store: &mut SemanticStore,
        source_member: SemanticNodeId,
        target_member: SemanticNodeId,
    ) -> Result<(), String> {
        let expected = self
            .source_members
            .get(self.next_index)
            .copied()
            .ok_or_else(|| "family target editor received too many members".to_owned())?;
        if expected != source_member {
            return Err(format!(
                "family target source member mismatch at index {}: expected {expected:?}, got {source_member:?}",
                self.next_index
            ));
        }
        store
            .add_member(self.target, target_member)
            .map_err(|error| error.to_string())?;
        self.next_index += 1;
        Ok(())
    }

    pub fn finish(&self) -> Result<SemanticNodeId, String> {
        if self.next_index != self.source_members.len() {
            return Err(format!(
                "family target editor is incomplete: accepted {} of {} members",
                self.next_index,
                self.source_members.len()
            ));
        }
        Ok(self.target)
    }
}

fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    render_f64(name, value).map(|value| value as f32)
}

fn render_f64(name: &str, value: f64) -> Result<f64, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value)
}

fn unit_opacity(name: &str, value: f64) -> Result<f64, String> {
    let value = render_f64(name, value)?;
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

fn authoring_style_from_legacy(style: Style) -> SemanticStyle {
    let mut semantic = SemanticStyle::from_legacy(style);
    if let Some(SemanticPaint::Solid(color)) = &mut semantic.fill {
        semantic.fill_opacity = f64::from(color.alpha);
        color.alpha = 1.0;
    }
    if let Some(SemanticPaint::Solid(color)) = &mut semantic.stroke {
        semantic.stroke_opacity = f64::from(color.alpha);
        color.alpha = 1.0;
    }
    semantic
}

fn legacy_solid_color(paint: Option<&SemanticPaint>, opacity: f64) -> Option<Color> {
    let SemanticPaint::Solid(color) = paint? else {
        return None;
    };
    Some(Color {
        alpha: opacity as f32,
        ..*color
    })
}

fn semantic_xy(x: f64, y: f64) -> Result<Vec2, String> {
    semantic_xy_f64(x, y)?
        .lower_xy_f32()
        .map_err(|error| error.to_string())
}

fn semantic_xy_f64(x: f64, y: f64) -> Result<SemanticVec3, String> {
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

fn include_layout_point(bounds: &mut Option<Bounds2D64>, point: (f64, f64)) {
    if let Some(bounds) = bounds {
        bounds.include(point.0, point.1);
    } else {
        *bounds = Some(Bounds2D64::point(point.0, point.1));
    }
}

fn transform_layout_point(transform: SemanticTransform2_5D, point: Vec2) -> (f64, f64) {
    let x = f64::from(point.x) * transform.scale.x;
    let y = f64::from(point.y) * transform.scale.y;
    let sine = transform.rotation_z.sin();
    let cosine = transform.rotation_z.cos();
    (
        x * cosine - y * sine + transform.translation.x,
        x * sine + y * cosine + transform.translation.y,
    )
}

fn quadratic_layout_point(p0: (f64, f64), p1: (f64, f64), p2: (f64, f64), t: f64) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
        u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
    )
}

fn cubic_layout_point(
    p0: (f64, f64),
    p1: (f64, f64),
    p2: (f64, f64),
    p3: (f64, f64),
    t: f64,
) -> (f64, f64) {
    let u = 1.0 - t;
    (
        u * u * u * p0.0 + 3.0 * u * u * t * p1.0 + 3.0 * u * t * t * p2.0 + t * t * t * p3.0,
        u * u * u * p0.1 + 3.0 * u * u * t * p1.1 + 3.0 * u * t * t * p2.1 + t * t * t * p3.1,
    )
}

fn cubic_layout_derivative_roots(p0: f64, p1: f64, p2: f64, p3: f64) -> Vec<f64> {
    let a = -p0 + 3.0 * p1 - 3.0 * p2 + p3;
    let b = 2.0 * (p0 - 2.0 * p1 + p2);
    let c = p1 - p0;
    let epsilon = 1.0e-14;
    if a.abs() <= epsilon {
        if b.abs() <= epsilon {
            return Vec::new();
        }
        return vec![-c / b];
    }
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return Vec::new();
    }
    let root = discriminant.sqrt();
    let mut roots = vec![(-b + root) / (2.0 * a)];
    if root > epsilon {
        roots.push((-b - root) / (2.0 * a));
    }
    roots
}

fn transformed_path_layout_bounds(
    path: &VectorPath,
    transform: SemanticTransform2_5D,
) -> Option<Bounds2D64> {
    let mut bounds = None;
    let mut current = None;
    let mut subpath_start = None;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                let point = transform_layout_point(transform, to);
                include_layout_point(&mut bounds, point);
                current = Some(point);
                subpath_start = Some(point);
            }
            PathCommand::LineTo { to } => {
                let end = transform_layout_point(transform, to);
                if let Some(start) = current {
                    include_layout_point(&mut bounds, start);
                }
                include_layout_point(&mut bounds, end);
                current = Some(end);
            }
            PathCommand::QuadraticTo { control, to } => {
                let end = transform_layout_point(transform, to);
                let Some(start) = current else {
                    include_layout_point(&mut bounds, end);
                    current = Some(end);
                    continue;
                };
                let control = transform_layout_point(transform, control);
                include_layout_point(&mut bounds, start);
                include_layout_point(&mut bounds, end);
                for axis in 0..2 {
                    let (p0, p1, p2) = if axis == 0 {
                        (start.0, control.0, end.0)
                    } else {
                        (start.1, control.1, end.1)
                    };
                    let denominator = p0 - 2.0 * p1 + p2;
                    if denominator.abs() <= 1.0e-14 {
                        continue;
                    }
                    let t = (p0 - p1) / denominator;
                    if (0.0..1.0).contains(&t) {
                        include_layout_point(
                            &mut bounds,
                            quadratic_layout_point(start, control, end, t),
                        );
                    }
                }
                current = Some(end);
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let end = transform_layout_point(transform, to);
                let Some(start) = current else {
                    include_layout_point(&mut bounds, end);
                    current = Some(end);
                    continue;
                };
                let control1 = transform_layout_point(transform, control1);
                let control2 = transform_layout_point(transform, control2);
                include_layout_point(&mut bounds, start);
                include_layout_point(&mut bounds, end);
                let mut roots =
                    cubic_layout_derivative_roots(start.0, control1.0, control2.0, end.0);
                roots.extend(cubic_layout_derivative_roots(
                    start.1, control1.1, control2.1, end.1,
                ));
                for t in roots {
                    if (0.0..1.0).contains(&t) {
                        include_layout_point(
                            &mut bounds,
                            cubic_layout_point(start, control1, control2, end, t),
                        );
                    }
                }
                current = Some(end);
            }
            PathCommand::Close => {
                if let Some(end) = current {
                    include_layout_point(&mut bounds, end);
                }
                if let Some(start) = subpath_start {
                    include_layout_point(&mut bounds, start);
                    current = Some(start);
                }
            }
        }
    }
    bounds
}

fn snapshot_layout_bounds(
    snapshot: &ObjectSnapshot,
    transform: SemanticTransform2_5D,
) -> Option<Bounds2D64> {
    match &snapshot.geometry {
        GeometryRef::Circle { radius } => {
            let radius = f64::from(*radius);
            let sine = transform.rotation_z.sin();
            let cosine = transform.rotation_z.cos();
            let half_width = radius * (transform.scale.x * cosine).hypot(transform.scale.y * sine);
            let half_height = radius * (transform.scale.x * sine).hypot(transform.scale.y * cosine);
            Some(Bounds2D64 {
                min_x: transform.translation.x - half_width,
                min_y: transform.translation.y - half_height,
                max_x: transform.translation.x + half_width,
                max_y: transform.translation.y + half_height,
            })
        }
        GeometryRef::Rectangle { size } => {
            let half_x = f64::from(size.x) * 0.5;
            let half_y = f64::from(size.y) * 0.5;
            let mut bounds = None;
            for (x, y) in [
                (-half_x, -half_y),
                (-half_x, half_y),
                (half_x, -half_y),
                (half_x, half_y),
            ] {
                let sine = transform.rotation_z.sin();
                let cosine = transform.rotation_z.cos();
                let x = x * transform.scale.x;
                let y = y * transform.scale.y;
                include_layout_point(
                    &mut bounds,
                    (
                        x * cosine - y * sine + transform.translation.x,
                        x * sine + y * cosine + transform.translation.y,
                    ),
                );
            }
            bounds
        }
        GeometryRef::Line { start, end } => {
            let mut bounds = None;
            include_layout_point(&mut bounds, transform_layout_point(transform, *start));
            include_layout_point(&mut bounds, transform_layout_point(transform, *end));
            bounds
        }
        GeometryRef::VectorPath(path) => transformed_path_layout_bounds(path, transform),
        GeometryRef::External(_) => None,
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::{cell::RefCell, rc::Rc};

    use wasm_bindgen::prelude::*;

    use super::{
        FrontendFamilyTargetEditor, FrontendMobjectHandle, ManimNextToArgs, SemanticNodeId,
        SemanticStore,
    };

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    type SharedSemanticStore = Rc<RefCell<SemanticStore>>;

    #[wasm_bindgen]
    pub struct WasmAuthoringStore {
        semantics: SharedSemanticStore,
    }

    #[wasm_bindgen]
    impl WasmAuthoringStore {
        #[wasm_bindgen(constructor)]
        pub fn new() -> Self {
            Self {
                semantics: Rc::new(RefCell::new(SemanticStore::new())),
            }
        }

        #[wasm_bindgen(js_name = createMobject)]
        pub fn create_mobject(
            &self,
            snapshot_json: &str,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            let handle = FrontendMobjectHandle::from_json(snapshot_json).map_err(js_error)?;
            let id = self.semantics.borrow_mut().insert_authoring_object();
            Ok(WasmAuthoringMobjectHandle(
                handle,
                Some(Rc::clone(&self.semantics)),
                Some(id),
            ))
        }

        #[wasm_bindgen(js_name = createFamily)]
        pub fn create_family(&self) -> WasmAuthoringFamilyHandle {
            let id = self.semantics.borrow_mut().insert_family();
            WasmAuthoringFamilyHandle {
                semantics: Rc::clone(&self.semantics),
                id,
            }
        }
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyHandle {
        semantics: SharedSemanticStore,
        id: SemanticNodeId,
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyTargetEditor {
        semantics: SharedSemanticStore,
        editor: FrontendFamilyTargetEditor,
    }

    impl WasmAuthoringFamilyTargetEditor {
        fn mobject_member_id(
            &self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<SemanticNodeId, JsValue> {
            let store = member.1.as_ref().ok_or_else(|| {
                JsValue::from_str("mobject is not attached to a shared authoring store")
            })?;
            if !Rc::ptr_eq(&self.semantics, store) {
                return Err(JsValue::from_str(
                    "family target editor and mobject belong to different authoring stores",
                ));
            }
            member
                .2
                .ok_or_else(|| JsValue::from_str("mobject has no semantic identity"))
        }

        fn family_member_id(
            &self,
            member: &WasmAuthoringFamilyHandle,
        ) -> Result<SemanticNodeId, JsValue> {
            if !Rc::ptr_eq(&self.semantics, &member.semantics) {
                return Err(JsValue::from_str(
                    "family target editor and family belong to different authoring stores",
                ));
            }
            Ok(member.id)
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyTargetEditor {
        #[wasm_bindgen(js_name = acceptMobject)]
        pub fn accept_mobject(
            &mut self,
            source: &WasmAuthoringMobjectHandle,
            target: &WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let source_id = self.mobject_member_id(source)?;
            let target_id = self.mobject_member_id(target)?;
            self.editor
                .accept_member(&mut self.semantics.borrow_mut(), source_id, target_id)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = acceptFamily)]
        pub fn accept_family(
            &mut self,
            source: &WasmAuthoringFamilyHandle,
            target: &WasmAuthoringFamilyHandle,
        ) -> Result<(), JsValue> {
            let source_id = self.family_member_id(source)?;
            let target_id = self.family_member_id(target)?;
            self.editor
                .accept_member(&mut self.semantics.borrow_mut(), source_id, target_id)
                .map_err(js_error)
        }

        pub fn finish(&self) -> Result<WasmAuthoringFamilyHandle, JsValue> {
            let id = self.editor.finish().map_err(js_error)?;
            Ok(WasmAuthoringFamilyHandle {
                semantics: Rc::clone(&self.semantics),
                id,
            })
        }
    }

    impl WasmAuthoringFamilyHandle {
        fn object_member_id(
            &self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<SemanticNodeId, JsValue> {
            let store = member.1.as_ref().ok_or_else(|| {
                JsValue::from_str("mobject is not attached to a shared authoring store")
            })?;
            if !Rc::ptr_eq(&self.semantics, store) {
                return Err(JsValue::from_str(
                    "family and mobject belong to different authoring stores",
                ));
            }
            member
                .2
                .ok_or_else(|| JsValue::from_str("mobject has no semantic identity"))
        }

        fn family_member_id(
            &self,
            member: &WasmAuthoringFamilyHandle,
        ) -> Result<SemanticNodeId, JsValue> {
            if !Rc::ptr_eq(&self.semantics, &member.semantics) {
                return Err(JsValue::from_str(
                    "families belong to different authoring stores",
                ));
            }
            Ok(member.id)
        }

        fn add_id(&mut self, member: SemanticNodeId) -> Result<bool, JsValue> {
            let before = self.member_count();
            self.semantics
                .borrow_mut()
                .add_member(self.id, member)
                .map_err(|error| js_error(error.to_string()))?;
            Ok(self.member_count() != before)
        }

        fn remove_id(&mut self, member: SemanticNodeId) -> Result<bool, JsValue> {
            self.semantics
                .borrow_mut()
                .remove_member(self.id, member)
                .map_err(|error| js_error(error.to_string()))
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyHandle {
        #[wasm_bindgen(js_name = targetEditor)]
        pub fn target_editor(&self) -> Result<WasmAuthoringFamilyTargetEditor, JsValue> {
            let editor =
                FrontendFamilyTargetEditor::begin(&mut self.semantics.borrow_mut(), self.id)
                    .map_err(js_error)?;
            Ok(WasmAuthoringFamilyTargetEditor {
                semantics: Rc::clone(&self.semantics),
                editor,
            })
        }

        #[wasm_bindgen(getter, js_name = semanticSlot)]
        pub fn semantic_slot(&self) -> u32 {
            self.id.slot()
        }

        #[wasm_bindgen(getter, js_name = semanticGeneration)]
        pub fn semantic_generation(&self) -> u32 {
            self.id.generation()
        }

        #[wasm_bindgen(getter, js_name = memberCount)]
        pub fn member_count(&self) -> usize {
            self.semantics
                .borrow()
                .node(self.id)
                .map_or(0, |node| node.members().len())
        }

        #[wasm_bindgen(js_name = memberSlot)]
        pub fn member_slot(&self, index: usize) -> Result<u32, JsValue> {
            self.semantics
                .borrow()
                .node(self.id)
                .and_then(|node| node.members().get(index).copied())
                .map(SemanticNodeId::slot)
                .ok_or_else(|| JsValue::from_str("family member index is out of bounds"))
        }

        #[wasm_bindgen(js_name = memberGeneration)]
        pub fn member_generation(&self, index: usize) -> Result<u32, JsValue> {
            self.semantics
                .borrow()
                .node(self.id)
                .and_then(|node| node.members().get(index).copied())
                .map(SemanticNodeId::generation)
                .ok_or_else(|| JsValue::from_str("family member index is out of bounds"))
        }

        #[wasm_bindgen(js_name = addMobject)]
        pub fn add_mobject(
            &mut self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<bool, JsValue> {
            let id = self.object_member_id(member)?;
            self.add_id(id)
        }

        #[wasm_bindgen(js_name = addFamily)]
        pub fn add_family(&mut self, member: &WasmAuthoringFamilyHandle) -> Result<bool, JsValue> {
            let id = self.family_member_id(member)?;
            self.add_id(id)
        }

        #[wasm_bindgen(js_name = removeMobject)]
        pub fn remove_mobject(
            &mut self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<bool, JsValue> {
            let id = self.object_member_id(member)?;
            self.remove_id(id)
        }

        #[wasm_bindgen(js_name = removeFamily)]
        pub fn remove_family(
            &mut self,
            member: &WasmAuthoringFamilyHandle,
        ) -> Result<bool, JsValue> {
            let id = self.family_member_id(member)?;
            self.remove_id(id)
        }
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringMobjectHandle(
        FrontendMobjectHandle,
        Option<SharedSemanticStore>,
        Option<SemanticNodeId>,
    );

    #[wasm_bindgen]
    impl WasmAuthoringMobjectHandle {
        #[wasm_bindgen(constructor)]
        pub fn new(snapshot_json: &str) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            FrontendMobjectHandle::from_json(snapshot_json)
                .map(|handle| WasmAuthoringMobjectHandle(handle, None, None))
                .map_err(js_error)
        }

        fn clone_with_handle(&self, handle: FrontendMobjectHandle) -> WasmAuthoringMobjectHandle {
            if let Some(store) = &self.1 {
                let id = store.borrow_mut().insert_authoring_object();
                WasmAuthoringMobjectHandle(handle, Some(Rc::clone(store)), Some(id))
            } else {
                WasmAuthoringMobjectHandle(handle, None, None)
            }
        }

        #[wasm_bindgen(getter, js_name = semanticSlot)]
        pub fn semantic_slot(&self) -> Result<u32, JsValue> {
            self.2
                .map(SemanticNodeId::slot)
                .ok_or_else(|| JsValue::from_str("mobject has no shared semantic identity"))
        }

        #[wasm_bindgen(getter, js_name = semanticGeneration)]
        pub fn semantic_generation(&self) -> Result<u32, JsValue> {
            self.2
                .map(SemanticNodeId::generation)
                .ok_or_else(|| JsValue::from_str("mobject has no shared semantic identity"))
        }

        #[wasm_bindgen(js_name = cloneHandle)]
        pub fn clone_handle(&self) -> WasmAuthoringMobjectHandle {
            self.clone_with_handle(self.0.clone())
        }

        /// Start a detached target-state edit from this handle without crossing
        /// the JS boundary with a serialized snapshot. The existing handle type
        /// is the editor; this alias makes that ownership explicit to frontends.
        #[wasm_bindgen(js_name = targetEditor)]
        pub fn target_editor(&self) -> WasmAuthoringMobjectHandle {
            self.clone_with_handle(self.0.target_editor())
        }

        #[wasm_bindgen(js_name = snapshotJson)]
        pub fn snapshot_json(&self) -> Result<String, JsValue> {
            self.0.snapshot_json().map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = wireTranslationX)]
        pub fn wire_translation_x(&self) -> f64 {
            self.0.wire_translation().0
        }

        #[wasm_bindgen(getter, js_name = wireTranslationY)]
        pub fn wire_translation_y(&self) -> f64 {
            self.0.wire_translation().1
        }

        #[wasm_bindgen(getter, js_name = wireScaleX)]
        pub fn wire_scale_x(&self) -> f64 {
            self.0.wire_scale().0
        }

        #[wasm_bindgen(getter, js_name = wireScaleY)]
        pub fn wire_scale_y(&self) -> f64 {
            self.0.wire_scale().1
        }

        #[wasm_bindgen(getter, js_name = wireRotation)]
        pub fn wire_rotation(&self) -> f64 {
            self.0.wire_rotation()
        }

        #[wasm_bindgen(getter, js_name = wireHasFill)]
        pub fn wire_has_fill(&self) -> bool {
            self.0.wire_fill().is_some()
        }

        #[wasm_bindgen(getter, js_name = wireFillRed)]
        pub fn wire_fill_red(&self) -> f64 {
            self.0.wire_fill().map_or(0.0, |value| value.0)
        }

        #[wasm_bindgen(getter, js_name = wireFillGreen)]
        pub fn wire_fill_green(&self) -> f64 {
            self.0.wire_fill().map_or(0.0, |value| value.1)
        }

        #[wasm_bindgen(getter, js_name = wireFillBlue)]
        pub fn wire_fill_blue(&self) -> f64 {
            self.0.wire_fill().map_or(0.0, |value| value.2)
        }

        #[wasm_bindgen(getter, js_name = wireFillAlpha)]
        pub fn wire_fill_alpha(&self) -> f64 {
            self.0.wire_fill().map_or(0.0, |value| value.3)
        }

        #[wasm_bindgen(getter, js_name = wireHasStroke)]
        pub fn wire_has_stroke(&self) -> bool {
            self.0.wire_stroke().is_some()
        }

        #[wasm_bindgen(getter, js_name = wireStrokeRed)]
        pub fn wire_stroke_red(&self) -> f64 {
            self.0.wire_stroke().map_or(0.0, |value| value.0)
        }

        #[wasm_bindgen(getter, js_name = wireStrokeGreen)]
        pub fn wire_stroke_green(&self) -> f64 {
            self.0.wire_stroke().map_or(0.0, |value| value.1)
        }

        #[wasm_bindgen(getter, js_name = wireStrokeBlue)]
        pub fn wire_stroke_blue(&self) -> f64 {
            self.0.wire_stroke().map_or(0.0, |value| value.2)
        }

        #[wasm_bindgen(getter, js_name = wireStrokeAlpha)]
        pub fn wire_stroke_alpha(&self) -> f64 {
            self.0.wire_stroke().map_or(0.0, |value| value.3)
        }

        #[wasm_bindgen(getter, js_name = wireStrokeWidth)]
        pub fn wire_stroke_width(&self) -> f64 {
            self.0.wire_stroke_width()
        }

        #[wasm_bindgen(getter, js_name = wireObjectOpacity)]
        pub fn wire_object_opacity(&self) -> f64 {
            self.0.wire_object_opacity()
        }

        #[wasm_bindgen(js_name = replaceSnapshotJson)]
        pub fn replace_snapshot_json(&mut self, snapshot_json: &str) -> Result<(), JsValue> {
            self.0.replace_json(snapshot_json).map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = centerX)]
        pub fn center_x(&self) -> f64 {
            self.0.center().0
        }

        #[wasm_bindgen(getter, js_name = centerY)]
        pub fn center_y(&self) -> f64 {
            self.0.center().1
        }

        #[wasm_bindgen(getter)]
        pub fn width(&self) -> f64 {
            self.0.width()
        }

        #[wasm_bindgen(getter)]
        pub fn height(&self) -> f64 {
            self.0.height()
        }

        #[wasm_bindgen(js_name = criticalX)]
        pub fn critical_x(&self, direction_x: f64, direction_y: f64) -> f64 {
            self.0.critical_point(direction_x, direction_y).0
        }

        #[wasm_bindgen(js_name = criticalY)]
        pub fn critical_y(&self, direction_x: f64, direction_y: f64) -> f64 {
            self.0.critical_point(direction_x, direction_y).1
        }

        pub fn shift(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.0.shift(x, y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = moveTo)]
        pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.0.move_to(x, y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = manimMoveToHandle)]
        pub fn manim_move_to_handle(
            &mut self,
            other: &WasmAuthoringMobjectHandle,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            self.0
                .manim_move_to_handle(&other.0, aligned_edge_x, aligned_edge_y, mask_x, mask_y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = manimMoveToPoint)]
        pub fn manim_move_to_point(
            &mut self,
            point_x: f64,
            point_y: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            self.0
                .manim_move_to_point(
                    point_x,
                    point_y,
                    aligned_edge_x,
                    aligned_edge_y,
                    mask_x,
                    mask_y,
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = manimNextToHandle)]
        pub fn manim_next_to_handle(
            &mut self,
            other: &WasmAuthoringMobjectHandle,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            self.0
                .manim_next_to_handle(
                    &other.0,
                    ManimNextToArgs {
                        direction: (direction_x, direction_y),
                        buff,
                        aligned_edge: (aligned_edge_x, aligned_edge_y),
                        mask: (mask_x, mask_y),
                    },
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = manimNextToPoint)]
        pub fn manim_next_to_point(
            &mut self,
            point_x: f64,
            point_y: f64,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            self.0
                .manim_next_to_point(
                    point_x,
                    point_y,
                    ManimNextToArgs {
                        direction: (direction_x, direction_y),
                        buff,
                        aligned_edge: (aligned_edge_x, aligned_edge_y),
                        mask: (mask_x, mask_y),
                    },
                )
                .map_err(js_error)
        }

        pub fn scale(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.0.scale(x, y).map_err(js_error)
        }

        pub fn rotate(&mut self, angle: f64) -> Result<(), JsValue> {
            self.0.rotate(angle).map_err(js_error)
        }

        #[wasm_bindgen(js_name = rotateAboutPoint)]
        pub fn rotate_about_point(
            &mut self,
            angle: f64,
            point_x: f64,
            point_y: f64,
        ) -> Result<(), JsValue> {
            self.0
                .rotate_about_point(angle, point_x, point_y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setColor)]
        pub fn set_color(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            self.0.set_color(red, green, blue, alpha).map_err(js_error)
        }

        #[wasm_bindgen(js_name = disableFill)]
        pub fn disable_fill(&mut self) {
            self.0.disable_fill();
        }

        #[wasm_bindgen(js_name = setFillColor)]
        pub fn set_fill_color(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            self.0
                .set_fill_color(red, green, blue, alpha)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setFillOpacity)]
        pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.0.set_fill_opacity(opacity).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setFill)]
        pub fn set_fill(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            opacity: f64,
        ) -> Result<(), JsValue> {
            self.0.set_fill(red, green, blue, opacity).map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = fillOpacity)]
        pub fn fill_opacity(&self) -> f64 {
            self.0.fill_opacity()
        }

        #[wasm_bindgen(js_name = disableStroke)]
        pub fn disable_stroke(&mut self) {
            self.0.disable_stroke();
        }

        #[wasm_bindgen(js_name = setStrokeColor)]
        pub fn set_stroke_color(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            self.0
                .set_stroke_color(red, green, blue, alpha)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStrokeWidth)]
        pub fn set_stroke_width(&mut self, width: f64) -> Result<(), JsValue> {
            self.0.set_stroke_width(width).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStrokeOpacity)]
        pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.0.set_stroke_opacity(opacity).map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = strokeOpacity)]
        pub fn stroke_opacity(&self) -> f64 {
            self.0.stroke_opacity()
        }

        #[wasm_bindgen(js_name = setOpacity)]
        pub fn set_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.0.set_opacity(opacity).map_err(js_error)
        }

        #[wasm_bindgen(js_name = becomeHandle)]
        pub fn become_handle(&mut self, other: &WasmAuthoringMobjectHandle) {
            self.0.become_handle(&other.0);
        }

        #[wasm_bindgen(js_name = replaceHandle)]
        pub fn replace_handle(
            &mut self,
            other: &WasmAuthoringMobjectHandle,
            dim_to_match: u32,
            stretch: bool,
        ) -> Result<(), JsValue> {
            self.0
                .replace_handle(&other.0, dim_to_match, stretch)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextToHandle)]
        pub fn next_to_handle(
            &mut self,
            other: &WasmAuthoringMobjectHandle,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
        ) -> Result<(), JsValue> {
            self.0
                .next_to_handle(&other.0, direction_x, direction_y, buff)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextToPoint)]
        pub fn next_to_point(
            &mut self,
            point_x: f64,
            point_y: f64,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
        ) -> Result<(), JsValue> {
            self.0
                .next_to_point(point_x, point_y, direction_x, direction_y, buff)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = alignToHandle)]
        pub fn align_to_handle(
            &mut self,
            other: &WasmAuthoringMobjectHandle,
            direction_x: f64,
            direction_y: f64,
        ) -> Result<(), JsValue> {
            self.0
                .align_to_handle(&other.0, direction_x, direction_y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = alignToPoint)]
        pub fn align_to_point(
            &mut self,
            point_x: f64,
            point_y: f64,
            direction_x: f64,
            direction_y: f64,
        ) -> Result<(), JsValue> {
            self.0
                .align_to_point(point_x, point_y, direction_x, direction_y)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = alignOnFrame)]
        pub fn align_on_frame(
            &mut self,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
        ) -> Result<(), JsValue> {
            self.0
                .align_on_frame(direction_x, direction_y, buff)
                .map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{GeometryRef, ObjectSnapshot, Transform2D, VectorPath};

    use super::*;

    fn snapshot(geometry: GeometryRef) -> ObjectSnapshot {
        ObjectSnapshot {
            geometry,
            transform: Transform2D::default(),
            style: noon_core::Style::default(),
        }
    }

    #[test]
    fn handle_mutations_keep_state_in_shared_rust_semantics() {
        let mut handle = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(1.0)));
        handle.shift(2.0, -1.0).unwrap();
        handle.scale(1.5, 0.5).unwrap();
        assert_eq!(handle.center(), (2.0, -1.0));
        assert_eq!(handle.width(), 3.0);
        assert_eq!(handle.height(), 1.0);
        assert_eq!(
            handle.snapshot().transform.translation,
            Vec2::new(2.0, -1.0)
        );
    }

    #[test]
    fn authoring_transform_keeps_f64_precision_until_render_lowering() {
        let mut handle =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(2.0, 1.0)));
        handle.shift(0.7, 0.3).unwrap();
        assert_eq!(handle.semantic_transform.translation.x, 0.7);
        assert_eq!(handle.semantic_transform.translation.y, 0.3);
        assert!((handle.critical_point(-1.0, 0.0).0 + 0.3).abs() < 1e-12);
        assert!((handle.critical_point(0.0, 1.0).1 - 0.8).abs() < 1e-12);
        assert_ne!(f64::from(handle.snapshot().transform.translation.x), 0.7);

        handle.scale(1.1, 0.9).unwrap();
        handle.rotate(0.2).unwrap();
        assert_eq!(handle.semantic_transform.scale.x, 1.1);
        assert_eq!(handle.semantic_transform.scale.y, 0.9);
        assert_eq!(handle.semantic_transform.rotation_z, 0.2);
    }

    #[test]
    fn pivoted_rotation_preserves_offset_line_center() {
        let mut handle = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::line(
            Vec2::ZERO,
            Vec2::new(1.0, 0.0),
        )));
        handle.shift(2.0, 0.0).unwrap();
        let pivot = handle.center();
        assert!((pivot.0 - 2.5).abs() < 1e-12);
        assert!(pivot.1.abs() < 1e-12);
        handle
            .rotate_about_point(std::f64::consts::FRAC_PI_2, pivot.0, pivot.1)
            .unwrap();
        let center = handle.center();
        assert!((center.0 - 2.5).abs() < 1e-9);
        assert!(center.1.abs() < 1e-9);
        assert!((handle.semantic_transform.translation.x - 2.5).abs() < 1e-12);
        assert!((handle.semantic_transform.translation.y + 0.5).abs() < 1e-12);
        assert!((handle.semantic_transform.rotation_z - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    }

    #[test]
    fn vector_path_layout_uses_extrema_not_control_hull() {
        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .quadratic_to(Vec2::new(0.0, 2.0), Vec2::new(1.0, 0.0));
        let handle = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::path(path)));
        let bounds = handle.layout_bounds().unwrap();
        assert!((bounds.min_x + 1.0).abs() < 1e-9);
        assert!((bounds.max_x - 1.0).abs() < 1e-9);
        assert!(bounds.min_y.abs() < 1e-9);
        assert!((bounds.max_y - 1.0).abs() < 1e-9);
        assert!((handle.height() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn transformed_layout_bounds_match_manim_world_extrema() {
        let mut ellipse = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(1.0)));
        ellipse.scale(2.0, 1.0).unwrap();
        ellipse.rotate(std::f64::consts::FRAC_PI_4).unwrap();
        assert!((ellipse.width() - 10.0_f64.sqrt()).abs() < 1e-12);
        assert!((ellipse.height() - 10.0_f64.sqrt()).abs() < 1e-12);

        let mut diagonal = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::line(
            Vec2::ZERO,
            Vec2::new(1.0, 1.0),
        )));
        diagonal.rotate(std::f64::consts::FRAC_PI_4).unwrap();
        assert!(diagonal.width().abs() < 1e-12);
        assert!((diagonal.height() - 2.0_f64.sqrt()).abs() < 1e-12);

        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .quadratic_to(Vec2::new(0.0, 2.0), Vec2::new(1.0, 0.0));
        let mut curve = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::path(path)));
        curve.rotate(std::f64::consts::FRAC_PI_4).unwrap();
        let expected = 9.0 * 2.0_f64.sqrt() / 8.0;
        assert!((curve.width() - expected).abs() < 1e-12);
        assert!((curve.height() - expected).abs() < 1e-12);
    }

    #[test]
    fn layout_operations_are_shared_and_deterministic() {
        let left = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(0.5)));
        let mut right =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(1.0, 1.0)));
        right.next_to_handle(&left, 1.0, 0.0, 0.25).unwrap();
        assert!((right.center().0 - 1.25).abs() < 1e-9);
        right.align_on_frame(1.0, 1.0, 0.5).unwrap();
        let bounds = right.layout_bounds().unwrap();
        assert!(
            (bounds.max_x - (f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5 - 0.5)).abs() < 1e-6
        );
        assert!(
            (bounds.max_y - (f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5 - 0.5)).abs() < 1e-6
        );
    }

    #[test]
    fn manim_leaf_placement_preserves_raw_direction_edges_and_masks() {
        let reference =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(2.0, 2.0)));
        let mut diagonal =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(2.0, 2.0)));
        diagonal
            .manim_next_to_handle(
                &reference,
                ManimNextToArgs {
                    direction: (1.0, 1.0),
                    buff: 0.25,
                    aligned_edge: (0.0, 0.0),
                    mask: (1.0, 1.0),
                },
            )
            .unwrap();
        assert!((diagonal.center().0 - 2.25).abs() < 1e-12);
        assert!((diagonal.center().1 - 2.25).abs() < 1e-12);

        let mut moved =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(1.0, 1.0)));
        moved.shift(0.0, -2.0).unwrap();
        moved
            .manim_move_to_handle(&reference, -1.0, 1.0, 1.0, 0.0)
            .unwrap();
        assert!((moved.center().0 + 0.5).abs() < 1e-12);
        assert!((moved.center().1 + 2.0).abs() < 1e-12);

        let mut aligned =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(1.0, 1.0)));
        aligned.shift(0.0, -1.0).unwrap();
        aligned.align_to_handle(&reference, 1.0, 0.0).unwrap();
        assert!((aligned.center().0 - 0.5).abs() < 1e-12);
        assert!((aligned.center().1 + 1.0).abs() < 1e-12);
    }

    #[test]
    fn shared_style_mutations_preserve_independent_channels() {
        let mut value = snapshot(GeometryRef::circle(1.0));
        value.style.fill = Some(Color::rgba(1.0, 0.0, 0.0, 0.4));
        value.style.stroke = Some(Color::rgba(0.0, 0.0, 1.0, 0.7));
        let mut handle = FrontendMobjectHandle::from_snapshot(value);

        handle.set_fill_color(0.0, 1.0, 0.0, 1.0).unwrap();
        assert!((handle.fill_opacity() - 0.4).abs() < 1e-6);
        handle.set_fill_opacity(0.25).unwrap();
        handle.set_stroke_width(3.5).unwrap();
        handle.set_stroke_opacity(0.6).unwrap();
        assert_eq!(handle.fill_opacity(), 0.25);
        assert_eq!(handle.stroke_opacity(), 0.6);
        assert!((handle.snapshot().style.stroke_width - 3.5).abs() < 1e-6);
        assert!((handle.snapshot().style.stroke.unwrap().alpha - 0.6).abs() < 1e-6);

        handle.set_opacity(0.2).unwrap();
        assert_eq!(handle.fill_opacity(), 0.2);
        assert_eq!(handle.stroke_opacity(), 0.2);
        handle.disable_fill();
        assert_eq!(handle.fill_opacity(), 0.0);
    }

    #[test]
    fn target_editor_alias_supports_moving_around_without_snapshot_round_trips() {
        let base = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(1.0)));
        let mut target = base.target_editor();

        target.shift(-1.0, 0.0).unwrap();
        target.set_fill(1.0, 0.525, 0.184, 0.5).unwrap();
        target.scale(0.3, 0.3).unwrap();
        target.rotate(0.4).unwrap();

        assert_eq!(base.center(), (0.0, 0.0));
        assert_eq!(target.semantic_transform.translation.x, -1.0);
        assert_eq!(target.semantic_transform.scale.x, 0.3);
        assert_eq!(target.semantic_transform.rotation_z, 0.4);
        assert_eq!(target.fill_opacity(), 0.5);
        let fill = target.snapshot().style.fill.unwrap();
        assert_eq!(fill.red, 1.0);
        assert_eq!(fill.green, 0.525);
        assert_eq!(fill.blue, 0.184);
        assert_eq!(fill.alpha, 0.5);
    }

    #[test]
    fn target_editor_clone_alias_is_independent_and_set_fill_is_transactional() {
        let base = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(1.0)));
        let mut target = base.target_editor();
        let sibling = target.target_editor();

        target.shift(2.0, 0.0).unwrap();
        target.set_fill(0.0, 1.0, 0.0, 0.25).unwrap();
        assert_eq!(base.center(), (0.0, 0.0));
        assert_eq!(sibling.center(), (0.0, 0.0));
        assert_eq!(sibling.fill_opacity(), 1.0);
        let sibling_fill = sibling.snapshot().style.fill.unwrap();
        assert_eq!(sibling_fill.red, 1.0);
        assert_eq!(sibling_fill.green, 1.0);

        let before = target.clone();
        assert!(target.set_fill(1.0, 0.0, 0.0, 2.0).is_err());
        assert_eq!(target, before);
    }

    #[test]
    fn family_target_editor_builds_target_from_shared_source_order() {
        let mut store = SemanticStore::new();
        let source_a = store.insert_authoring_object();
        let source_b = store.insert_authoring_object();
        let source_family = store.insert_family();
        store.add_member(source_family, source_a).unwrap();
        store.add_member(source_family, source_b).unwrap();

        let target_a = store.insert_authoring_object();
        let target_b = store.insert_authoring_object();
        let mut editor = FrontendFamilyTargetEditor::begin(&mut store, source_family).unwrap();
        assert!(store.node(editor.target_id()).unwrap().members().is_empty());

        editor
            .accept_member(&mut store, source_a, target_a)
            .unwrap();
        editor
            .accept_member(&mut store, source_b, target_b)
            .unwrap();
        let target_family = editor.finish().unwrap();

        assert_eq!(
            store.node(source_family).unwrap().members(),
            &[source_a, source_b]
        );
        assert_eq!(
            store.node(target_family).unwrap().members(),
            &[target_a, target_b]
        );
        assert!(store
            .node(target_a)
            .unwrap()
            .parents()
            .contains(&target_family));
        assert!(store
            .node(target_b)
            .unwrap()
            .parents()
            .contains(&target_family));
    }

    #[test]
    fn family_target_editor_rejects_wrapper_reordering_and_incomplete_targets() {
        let mut store = SemanticStore::new();
        let source_a = store.insert_authoring_object();
        let source_b = store.insert_authoring_object();
        let source_family = store.insert_family();
        store.add_member(source_family, source_a).unwrap();
        store.add_member(source_family, source_b).unwrap();
        let target_a = store.insert_authoring_object();

        let mut editor = FrontendFamilyTargetEditor::begin(&mut store, source_family).unwrap();
        let error = editor
            .accept_member(&mut store, source_b, target_a)
            .unwrap_err();
        assert!(error.contains("mismatch at index 0"));
        assert!(store.node(editor.target_id()).unwrap().members().is_empty());
        assert!(editor.finish().unwrap_err().contains("accepted 0 of 2"));
    }

    #[test]
    fn become_and_replace_keep_state_inside_shared_handle() {
        let mut source = FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(0.5)));
        source.shift(-2.0, 0.5).unwrap();
        let mut target =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(2.0, 1.0)));
        target.shift(1.0, -0.25).unwrap();

        source.become_handle(&target);
        assert_eq!(source.snapshot(), target.snapshot());

        let mut replacement =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(0.25)));
        replacement.replace_handle(&target, 0, false).unwrap();
        assert!((replacement.width() - 2.0).abs() < 1e-6);
        assert!((replacement.height() - 2.0).abs() < 1e-6);
        assert!((replacement.center().0 - 1.0).abs() < 1e-6);
        assert!((replacement.center().1 + 0.25).abs() < 1e-6);

        let mut stretched =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::circle(0.25)));
        stretched.replace_handle(&target, 0, true).unwrap();
        assert!((stretched.width() - 2.0).abs() < 1e-6);
        assert!((stretched.height() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn wire_projection_matches_lowered_snapshot_after_shared_edits() {
        let mut value = snapshot(GeometryRef::rectangle(2.0, 1.0));
        value.style.fill = Some(Color::rgba(0.2, 0.3, 0.4, 0.5));
        value.style.stroke = Some(Color::rgba(0.6, 0.7, 0.8, 0.9));
        let mut handle = FrontendMobjectHandle::from_snapshot(value);

        handle.shift(0.7, -0.3).unwrap();
        handle.scale(1.1, 0.9).unwrap();
        handle.rotate(0.2).unwrap();
        handle.set_fill_opacity(0.25).unwrap();
        handle.set_stroke_width(3.5).unwrap();

        let snapshot = handle.snapshot();
        assert_eq!(
            handle.wire_translation(),
            (
                f64::from(snapshot.transform.translation.x),
                f64::from(snapshot.transform.translation.y),
            )
        );
        assert_eq!(
            handle.wire_scale(),
            (
                f64::from(snapshot.transform.scale.x),
                f64::from(snapshot.transform.scale.y),
            )
        );
        assert_eq!(
            handle.wire_rotation(),
            f64::from(snapshot.transform.rotation)
        );
        assert_eq!(handle.wire_fill().unwrap().3, 0.25_f32 as f64);
        assert_eq!(handle.wire_stroke_width(), 3.5_f32 as f64);
    }

    #[test]
    fn json_round_trip_preserves_wire_snapshot() {
        let handle =
            FrontendMobjectHandle::from_snapshot(snapshot(GeometryRef::rectangle(2.0, 3.0)));
        let json = handle.snapshot_json().unwrap();
        let restored = FrontendMobjectHandle::from_json(&json).unwrap();
        assert_eq!(restored, handle);
    }
}

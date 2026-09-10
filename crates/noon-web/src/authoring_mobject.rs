#[cfg(target_arch = "wasm32")]
pub(crate) fn gradient_colors(
    values: &[f64],
) -> Result<Vec<noon::Color>, crate::authoring_error::AuthoringFailure> {
    if !values.len().is_multiple_of(4) {
        return Err(crate::authoring_error::AuthoringFailure::new(
            "invalid_input",
            "gradient.invalid_components",
            "gradient colors require RGBA components",
        ));
    }
    values
        .chunks_exact(4)
        .map(|c| {
            family_color(true, c[0], c[1], c[2], c[3])
                .map(|color| color.expect("enabled color"))
                .map_err(|error| {
                    crate::authoring_error::AuthoringFailure::new(
                        "invalid_input",
                        "gradient.invalid_color",
                        error,
                    )
                })
        })
        .collect()
}

#[cfg(target_arch = "wasm32")]
#[allow(clippy::too_many_arguments)]
pub(crate) fn style_update(
    fill_enabled: bool,
    fill_red: f64,
    fill_green: f64,
    fill_blue: f64,
    fill_alpha: f64,
    fill_opacity: Option<f64>,
    stroke_enabled: bool,
    stroke_red: f64,
    stroke_green: f64,
    stroke_blue: f64,
    stroke_alpha: f64,
    stroke_width: Option<f64>,
    stroke_opacity: Option<f64>,
) -> Result<noon::StyleUpdate, String> {
    Ok(noon::StyleUpdate {
        fill_color: family_color(fill_enabled, fill_red, fill_green, fill_blue, fill_alpha)?,
        fill_opacity,
        stroke_color: family_color(
            stroke_enabled,
            stroke_red,
            stroke_green,
            stroke_blue,
            stroke_alpha,
        )?,
        stroke_width,
        stroke_opacity,
    })
}

#[cfg(target_arch = "wasm32")]
use noon::integration::authoring_render_f64 as render_f64;
pub use noon::{ManimNextToArgs, Mobject};
#[cfg(target_arch = "wasm32")]
use noon_core::SemanticNodeId;
#[cfg(any(target_arch = "wasm32", test))]
use noon_core::SemanticStore;

#[cfg(target_arch = "wasm32")]
pub(crate) fn family_color(
    enabled: bool,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) -> Result<Option<noon::Color>, String> {
    if !enabled {
        return Ok(None);
    }
    if ![red, green, blue, alpha]
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
    {
        return Err("family color components must be finite and between zero and one".into());
    }
    Ok(Some(noon::Color::rgba(
        red as f32,
        green as f32,
        blue as f32,
        alpha as f32,
    )))
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn text_authoring_f32(
    field: &str,
    value: f64,
) -> Result<f32, crate::authoring_error::AuthoringFailure> {
    let value = render_f64(field, value)? as f32;
    if !value.is_finite() {
        return Err(format!("{field} is outside the supported range").into());
    }
    Ok(value)
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn manim_text(
    source: &str,
    font_family: &str,
    font_size: f64,
    line_spacing: f64,
) -> Result<noon::Text, crate::authoring_error::AuthoringFailure> {
    let font_size = text_authoring_f32("font size", font_size)?;
    let line_spacing = text_authoring_f32("line spacing", line_spacing)?;
    if line_spacing != -1.0 && line_spacing <= -1.0 {
        return Err("line spacing must be -1 or greater than -1".into());
    }
    Ok(noon::Text::new(source)
        .with_font(font_family)
        .with_font_size(font_size)
        .with_line_spacing(line_spacing))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use crate::authoring_error::{js_error as typed_js_error, AuthoringFailure};
    use std::{cell::RefCell, rc::Rc};

    use noon::{FamilyLayout, FamilyLayoutTarget};
    use noon_core::Bounds2D64;
    use wasm_bindgen::prelude::*;

    use super::{Mobject, SemanticNodeId, SemanticStore};

    use crate::authoring_error::js_error;

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

        #[wasm_bindgen(js_name = createSceneContext)]
        pub fn create_scene_context(&self) -> crate::CanonicalAuthoringSceneContext {
            crate::CanonicalAuthoringSceneContext::with_store(Rc::clone(&self.semantics))
        }

        #[wasm_bindgen(js_name = createValueTracker)]
        pub fn create_value_tracker(
            &self,
            initial: f64,
        ) -> Result<crate::WasmValueTrackerHandle, JsValue> {
            let tracker = noon::ValueTracker::detached(Rc::clone(&self.semantics), initial)
                .map_err(js_error)?;
            Ok(crate::WasmValueTrackerHandle::from_tracker(
                tracker,
                Rc::clone(&self.semantics),
            ))
        }

        #[wasm_bindgen(js_name = createManimGeometry)]
        pub fn create_manim_geometry(
            &self,
            candidate: crate::WasmManimGeometryOptions,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            Mobject::from_manim_geometry(Rc::clone(&self.semantics), candidate.options)
                .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = createManimCircle)]
        pub fn create_manim_circle(
            &self,
            radius: f64,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            Mobject::manim_circle(Rc::clone(&self.semantics), radius)
                .map(|handle| WasmAuthoringMobjectHandle { handle })
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = createManimSquare)]
        pub fn create_manim_square(
            &self,
            side_length: f64,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            Mobject::manim_square(Rc::clone(&self.semantics), side_length)
                .map(|handle| WasmAuthoringMobjectHandle { handle })
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = createManimRectangle)]
        pub fn create_manim_rectangle(
            &self,
            width: f64,
            height: f64,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            Mobject::manim_rectangle(Rc::clone(&self.semantics), width, height)
                .map(|handle| WasmAuthoringMobjectHandle { handle })
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = createManimLine)]
        pub fn create_manim_line(
            &self,
            start_x: f64,
            start_y: f64,
            end_x: f64,
            end_y: f64,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            Mobject::manim_line(Rc::clone(&self.semantics), start_x, start_y, end_x, end_y)
                .map(|handle| WasmAuthoringMobjectHandle { handle })
                .map_err(js_error)
        }

        /// Shape native text into the same semantic store as geometry handles.
        #[wasm_bindgen(js_name = createManimText)]
        pub fn create_manim_text(
            &self,
            source: &str,
            font_family: &str,
            font_size: f64,
            line_spacing: f64,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            let text = super::manim_text(source, font_family, font_size, line_spacing)
                .map_err(js_error)?;
            Mobject::from_text(Rc::clone(&self.semantics), text)
                .map(|handle| WasmAuthoringMobjectHandle { handle })
                .map_err(js_error)
        }

        /// Compile Typst or MathTypst into the same semantic store as geometry handles.
        #[wasm_bindgen(js_name = createManimTypst)]
        pub fn create_manim_typst(
            &self,
            source: &str,
            math: bool,
            font_size: f64,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            let font_size = super::text_authoring_f32("font size", font_size).map_err(js_error)?;
            let handle = if math {
                Mobject::from_math_typst(
                    Rc::clone(&self.semantics),
                    noon::MathTypst::new(source).with_font_size(font_size),
                )
            } else {
                Mobject::from_typst(
                    Rc::clone(&self.semantics),
                    noon::Typst::new(source).with_font_size(font_size),
                )
            };
            handle
                .map(|handle| WasmAuthoringMobjectHandle { handle })
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = createFamily)]
        pub fn create_family(
            &self,
            batch: crate::WasmSceneMembershipBatch,
            z_index: f64,
        ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
            batch
                .create_family(Rc::clone(&self.semantics), z_index)
                .map(WasmAuthoringFamilyHandle::from_semantic_family)
                .map_err(js_error)
        }
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyHandle {
        family: noon::MobjectFamily,
    }

    /// Host-normalized options for one shared arrangement transaction.
    #[wasm_bindgen]
    pub struct WasmFamilyArrangeOptions {
        pub(crate) options: noon::FamilyArrangeOptions,
    }

    #[wasm_bindgen]
    impl WasmFamilyArrangeOptions {
        #[wasm_bindgen(js_name = setAligner)]
        pub fn set_aligner(&mut self, aligner: &WasmLayoutAnchor) {
            self.options.aligner = Some(aligner.anchor.clone());
        }
    }

    /// Transient grid call arguments; all sizing and validation belong to shared Rust.
    #[wasm_bindgen]
    pub struct WasmFamilyGridOptions {
        pub(crate) options: noon::FamilyGridOptions,
    }

    #[wasm_bindgen]
    impl WasmFamilyGridOptions {
        #[wasm_bindgen(js_name = setAlignment)]
        pub fn set_alignment(
            &mut self,
            x: f64,
            y: f64,
            rows: Option<String>,
            columns: Option<String>,
        ) {
            self.options.cell_alignment = (x, y);
            self.options.row_alignments = rows;
            self.options.column_alignments = columns;
        }
        #[wasm_bindgen(js_name = setFlow)]
        pub fn set_flow(&mut self, flow: &str) -> Result<(), JsValue> {
            self.options.flow = flow.parse().map_err(js_error)?;
            Ok(())
        }
        #[wasm_bindgen(js_name = setSizeLists)]
        pub fn set_size_lists(&mut self, rows: bool, columns: bool) {
            self.options.row_heights = rows.then(Vec::new);
            self.options.column_widths = columns.then(Vec::new);
        }
        #[wasm_bindgen(js_name = addRowHeight)]
        pub fn add_row_height(&mut self, value: Option<f64>) {
            self.options
                .row_heights
                .get_or_insert_with(Vec::new)
                .push(value);
        }
        #[wasm_bindgen(js_name = addColumnWidth)]
        pub fn add_column_width(&mut self, value: Option<f64>) {
            self.options
                .column_widths
                .get_or_insert_with(Vec::new)
                .push(value);
        }
    }

    /// Inert typed layout intent; identity and member selection stay in Rust.
    #[wasm_bindgen]
    pub struct WasmLayoutAnchor {
        pub(crate) anchor: noon::LayoutAnchor,
    }

    #[wasm_bindgen]
    impl WasmLayoutAnchor {
        pub fn scale(
            &self,
            scale_x: f64,
            scale_y: f64,
            x: f64,
            y: f64,
            about_point: bool,
        ) -> Result<(), JsValue> {
            let pivot = if about_point {
                noon::ManimRotationPivot::Point(x, y)
            } else {
                noon::ManimRotationPivot::Edge(x, y)
            };
            self.anchor.scale(scale_x, scale_y, pivot).map_err(js_error)
        }

        #[wasm_bindgen(js_name = zIndex)]
        pub fn z_index(&self) -> Result<f64, JsValue> {
            self.anchor.z_index().map_err(js_error)
        }

        #[wasm_bindgen(js_name = setZIndex)]
        pub fn set_z_index(&self, value: f64, family: bool) -> Result<(), JsValue> {
            self.anchor.set_z_index(value, family).map_err(js_error)
        }

        pub fn rotate(&self, angle: f64, x: f64, y: f64, about_point: bool) -> Result<(), JsValue> {
            let pivot = if about_point {
                noon::ManimRotationPivot::Point(x, y)
            } else {
                noon::ManimRotationPivot::Edge(x, y)
            };
            self.anchor.rotate(angle, pivot).map_err(js_error)
        }

        #[allow(clippy::too_many_arguments)]
        pub fn flip(
            &self,
            axis_x: f64,
            axis_y: f64,
            axis_z: f64,
            x: f64,
            y: f64,
            about_point: bool,
        ) -> Result<(), JsValue> {
            let pivot = if about_point {
                noon::ManimRotationPivot::Point(x, y)
            } else {
                noon::ManimRotationPivot::Edge(x, y)
            };
            self.anchor
                .flip(noon::SemanticVec3::new(axis_x, axis_y, axis_z), pivot)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = moveTo)]
        pub fn move_to(
            &self,
            target: &WasmLayoutAnchor,
            edge_x: f64,
            edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            self.anchor
                .layout()
                .map_err(js_error)?
                .move_to(
                    noon::FamilyLayoutTarget::Anchor(&target.anchor),
                    (edge_x, edge_y),
                    (mask_x, mask_y),
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = alignTo)]
        pub fn align_to(
            &self,
            target: &WasmLayoutAnchor,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<(), JsValue> {
            self.anchor
                .layout()
                .map_err(js_error)?
                .align_to(
                    noon::FamilyLayoutTarget::Anchor(&target.anchor),
                    (axis_x, axis_y),
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = rescaleToFit)]
        #[allow(clippy::too_many_arguments)]
        pub fn rescale_to_fit(
            &self,
            length: f64,
            dimension: u32,
            stretch: bool,
            x: f64,
            y: f64,
            about_point: bool,
        ) -> Result<(), JsValue> {
            let pivot = if about_point {
                noon::ManimRotationPivot::Point(x, y)
            } else {
                noon::ManimRotationPivot::Edge(x, y)
            };
            self.anchor
                .rescale_to_fit_with_pivot(
                    length,
                    dimension.try_into().map_err(js_error)?,
                    stretch,
                    pivot,
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = replaceLayout)]
        pub fn replace_layout(
            &self,
            target: &WasmLayoutAnchor,
            dimension: u32,
            stretch: bool,
        ) -> Result<(), JsValue> {
            self.anchor
                .replace_layout(
                    &target.anchor,
                    dimension.try_into().map_err(js_error)?,
                    stretch,
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = matchDimSize)]
        #[allow(clippy::too_many_arguments)]
        pub fn match_dim_size(
            &self,
            target: &WasmLayoutAnchor,
            dimension: u32,
            stretch: bool,
            x: f64,
            y: f64,
            about_point: bool,
        ) -> Result<(), JsValue> {
            let pivot = if about_point {
                noon::ManimRotationPivot::Point(x, y)
            } else {
                noon::ManimRotationPivot::Edge(x, y)
            };
            self.anchor
                .match_dim_size_with_pivot(
                    &target.anchor,
                    dimension.try_into().map_err(js_error)?,
                    stretch,
                    pivot,
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextTo)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to(
            &self,
            target: &WasmLayoutAnchor,
            aligner: &WasmLayoutAnchor,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            edge_x: f64,
            edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            self.anchor
                .next_to_aligned(
                    noon::FamilyLayoutTarget::Anchor(&target.anchor),
                    &aligner.anchor,
                    noon::ManimNextToArgs {
                        direction: (direction_x, direction_y),
                        buff,
                        aligned_edge: (edge_x, edge_y),
                        mask: (mask_x, mask_y),
                    },
                )
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = nextToPoint)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_point(
            &self,
            x: f64,
            y: f64,
            aligner: &WasmLayoutAnchor,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            edge_x: f64,
            edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            self.anchor
                .next_to_aligned(
                    noon::FamilyLayoutTarget::Point(x, y),
                    &aligner.anchor,
                    noon::ManimNextToArgs {
                        direction: (direction_x, direction_y),
                        buff,
                        aligned_edge: (edge_x, edge_y),
                        mask: (mask_x, mask_y),
                    },
                )
                .map_err(js_error)
        }
    }

    /// Real-WASM regression fixture only: invalidate a shared handle, not a JS wrapper.
    #[cfg(debug_assertions)]
    #[wasm_bindgen(js_name = authoringErrorStaleMobjectSmoke)]
    pub fn authoring_error_stale_mobject_smoke(
        store: &WasmAuthoringStore,
    ) -> WasmAuthoringMobjectHandle {
        let handle = Mobject::manim_circle(Rc::clone(&store.semantics), 1.0).unwrap();
        store
            .semantics
            .borrow_mut()
            .remove_node(handle.node_id())
            .unwrap();
        WasmAuthoringMobjectHandle::from_semantic_mobject(handle)
    }

    /// Thin browser wrapper over the shared authored family observation.
    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyLayout {
        layout: FamilyLayout,
    }

    impl WasmAuthoringFamilyLayout {
        pub(crate) fn bounds(&self) -> Bounds2D64 {
            self.layout
                .bounds()
                .unwrap_or_else(|| Bounds2D64::point(0.0, 0.0))
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyLayout {
        #[wasm_bindgen(getter, js_name = centerX)]
        pub fn center_x(&self) -> f64 {
            self.layout.center().0
        }

        #[wasm_bindgen(getter, js_name = centerY)]
        pub fn center_y(&self) -> f64 {
            self.layout.center().1
        }

        #[wasm_bindgen(getter)]
        pub fn width(&self) -> f64 {
            self.layout.width()
        }

        #[wasm_bindgen(getter)]
        pub fn height(&self) -> f64 {
            self.layout.height()
        }

        #[wasm_bindgen(js_name = alignOnFrame)]
        pub fn align_on_frame(
            &self,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
        ) -> Result<(), JsValue> {
            self.layout
                .align_on_frame((direction_x, direction_y), buff)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = shiftBy)]
        pub fn shift_by(&self, delta_x: f64, delta_y: f64) -> Result<(), JsValue> {
            self.layout.shift(delta_x, delta_y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = moveToPoint)]
        pub fn move_to_point(
            &self,
            point_x: f64,
            point_y: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            self.layout
                .move_to(
                    FamilyLayoutTarget::Point(point_x, point_y),
                    (aligned_edge_x, aligned_edge_y),
                    (mask_x, mask_y),
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = moveToMobject)]
        pub fn move_to_mobject(
            &self,
            target: &WasmAuthoringMobjectHandle,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            self.layout
                .move_to(
                    FamilyLayoutTarget::Mobject(&target.handle),
                    (aligned_edge_x, aligned_edge_y),
                    (mask_x, mask_y),
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = moveToFamily)]
        pub fn move_to_family(
            &self,
            target: &WasmAuthoringFamilyLayout,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<(), JsValue> {
            self.layout
                .move_to(
                    FamilyLayoutTarget::Family(&target.layout),
                    (aligned_edge_x, aligned_edge_y),
                    (mask_x, mask_y),
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = alignToPoint)]
        pub fn align_to_point(
            &self,
            point_x: f64,
            point_y: f64,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<(), JsValue> {
            self.layout
                .align_to(
                    FamilyLayoutTarget::Point(point_x, point_y),
                    (axis_x, axis_y),
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = alignToMobject)]
        pub fn align_to_mobject(
            &self,
            target: &WasmAuthoringMobjectHandle,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<(), JsValue> {
            self.layout
                .align_to(
                    FamilyLayoutTarget::Mobject(&target.handle),
                    (axis_x, axis_y),
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = alignToFamily)]
        pub fn align_to_family(
            &self,
            target: &WasmAuthoringFamilyLayout,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<(), JsValue> {
            self.layout
                .align_to(FamilyLayoutTarget::Family(&target.layout), (axis_x, axis_y))
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = criticalX)]
        pub fn critical_x(&self, direction_x: f64, _direction_y: f64) -> f64 {
            self.layout.critical_point(direction_x, _direction_y).0
        }

        #[wasm_bindgen(js_name = criticalY)]
        pub fn critical_y(&self, _direction_x: f64, direction_y: f64) -> f64 {
            self.layout.critical_point(_direction_x, direction_y).1
        }
    }

    /// Derived typed lookup used only to reconstruct host wrapper identities.
    #[wasm_bindgen]
    pub struct WasmFamilyCopy {
        copied: noon::FamilyCopy,
    }

    impl WasmFamilyCopy {
        pub(crate) fn from_copy(copied: noon::FamilyCopy) -> Self {
            Self { copied }
        }
    }

    #[wasm_bindgen]
    impl WasmFamilyCopy {
        pub fn root(&self) -> WasmAuthoringFamilyHandle {
            WasmAuthoringFamilyHandle::from_semantic_family(self.copied.root().clone())
        }

        #[wasm_bindgen(js_name = mobjectFor)]
        pub fn mobject_for(
            &self,
            source: &WasmAuthoringMobjectHandle,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            self.copied
                .mobject(&source.handle)
                .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = familyFor)]
        pub fn family_for(
            &self,
            source: &WasmAuthoringFamilyHandle,
        ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
            self.copied
                .family(&source.semantic_family()?)
                .map(WasmAuthoringFamilyHandle::from_semantic_family)
                .map_err(js_error)
        }
    }

    impl WasmAuthoringFamilyHandle {
        pub(crate) fn from_semantic_family(family: noon::MobjectFamily) -> Self {
            Self { family }
        }

        pub(crate) fn semantic_family(&self) -> Result<noon::MobjectFamily, JsValue> {
            self.family.validate().map_err(typed_js_error)?;
            Ok(self.family.clone())
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyHandle {
        #[wasm_bindgen(js_name = becomeFamily)]
        pub fn become_family(
            &self,
            target: &WasmAuthoringFamilyHandle,
            match_height: bool,
            match_width: bool,
            match_center: bool,
            stretch: bool,
        ) -> Result<(), JsValue> {
            self.semantic_family()?
                .become_family(
                    &target.semantic_family()?,
                    noon::ManimBecomeOptions {
                        match_height,
                        match_width,
                        match_center,
                        stretch,
                    },
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setColorGradient)]
        pub fn set_color_gradient(&mut self, values: &[f64]) -> Result<(), JsValue> {
            let colors = super::gradient_colors(values).map_err(js_error)?;
            self.semantic_family()?
                .set_color_by_gradient(&colors)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStyle)]
        #[allow(clippy::too_many_arguments)]
        pub fn set_style(
            &mut self,
            fill_enabled: bool,
            fill_red: f64,
            fill_green: f64,
            fill_blue: f64,
            fill_alpha: f64,
            fill_opacity: Option<f64>,
            stroke_enabled: bool,
            stroke_red: f64,
            stroke_green: f64,
            stroke_blue: f64,
            stroke_alpha: f64,
            stroke_width: Option<f64>,
            stroke_opacity: Option<f64>,
        ) -> Result<(), JsValue> {
            let update = super::style_update(
                fill_enabled,
                fill_red,
                fill_green,
                fill_blue,
                fill_alpha,
                fill_opacity,
                stroke_enabled,
                stroke_red,
                stroke_green,
                stroke_blue,
                stroke_alpha,
                stroke_width,
                stroke_opacity,
            )
            .map_err(js_error)?;
            self.semantic_family()?.set_style(update).map_err(js_error)
        }

        #[wasm_bindgen(js_name = matchStyle)]
        pub fn match_style(&self, target: &WasmAuthoringFamilyHandle) -> Result<(), JsValue> {
            self.semantic_family()?
                .match_style(&target.semantic_family()?)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setColor)]
        #[allow(clippy::too_many_arguments)]
        pub fn set_color(
            &self,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            self.semantic_family()?
                .set_color(red, green, blue, alpha)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setFill)]
        #[allow(clippy::too_many_arguments)]
        pub fn set_fill(
            &self,
            has_color: bool,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
            opacity: Option<f64>,
        ) -> Result<(), JsValue> {
            let color = crate::authoring_mobject::family_color(has_color, red, green, blue, alpha)
                .map_err(js_error)?;
            self.semantic_family()?
                .set_fill(color, opacity)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStroke)]
        #[allow(clippy::too_many_arguments)]
        pub fn set_stroke(
            &self,
            has_color: bool,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
            width: Option<f64>,
            opacity: Option<f64>,
        ) -> Result<(), JsValue> {
            let color = crate::authoring_mobject::family_color(has_color, red, green, blue, alpha)
                .map_err(js_error)?;
            self.semantic_family()?
                .set_stroke(color, width, opacity)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setOpacity)]
        #[allow(clippy::too_many_arguments)]
        pub fn set_opacity(&self, opacity: f64) -> Result<(), JsValue> {
            self.semantic_family()?
                .set_opacity(opacity)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = gridOptions)]
        pub fn grid_options(
            &self,
            rows: Option<u32>,
            columns: Option<u32>,
            gap_x: f64,
            gap_y: f64,
        ) -> WasmFamilyGridOptions {
            WasmFamilyGridOptions {
                options: noon::FamilyGridOptions {
                    rows: rows.map(|v| v as usize),
                    columns: columns.map(|v| v as usize),
                    gap: (gap_x, gap_y),
                    ..Default::default()
                },
            }
        }

        #[wasm_bindgen(js_name = arrangeInGrid)]
        pub fn arrange_in_grid(&self, options: &WasmFamilyGridOptions) -> Result<(), JsValue> {
            self.semantic_family()?
                .arrange_in_grid_with_options(&options.options)
                .map_err(js_error)
        }

        pub fn scale(&self, x: f64, y: f64) -> Result<(), JsValue> {
            self.semantic_family()?.scale(x, y).map_err(js_error)
        }

        pub fn rotate(&self, angle: f64, x: f64, y: f64, about_point: bool) -> Result<(), JsValue> {
            let pivot = if about_point {
                noon::ManimRotationPivot::Point(x, y)
            } else {
                noon::ManimRotationPivot::Edge(x, y)
            };
            self.semantic_family()?
                .rotate(angle, pivot)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = layoutAnchor)]
        pub fn layout_anchor(&self, index: Option<i32>) -> Result<WasmLayoutAnchor, JsValue> {
            let anchor = noon::LayoutAnchor::from(&self.semantic_family()?);
            Ok(WasmLayoutAnchor {
                anchor: match index {
                    Some(index) => anchor.member(index as isize),
                    None => anchor,
                },
            })
        }

        /// Read an immutable layout observation from the shared semantic family.
        pub fn layout(&self) -> Result<WasmAuthoringFamilyLayout, JsValue> {
            Ok(WasmAuthoringFamilyLayout {
                layout: self.semantic_family()?.layout().map_err(js_error)?,
            })
        }

        #[wasm_bindgen(js_name = arrangeOptions)]
        #[allow(clippy::too_many_arguments)]
        pub fn arrange_options(
            &self,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            center: bool,
            edge_x: f64,
            edge_y: f64,
            mask_x: f64,
            mask_y: f64,
            member_index: Option<i32>,
        ) -> WasmFamilyArrangeOptions {
            let mut options =
                noon::FamilyArrangeOptions::new(direction_x, direction_y, buff, center);
            options.placement.aligned_edge = (edge_x, edge_y);
            options.placement.mask = (mask_x, mask_y);
            options.member_index = member_index.map(|index| index as isize);
            WasmFamilyArrangeOptions { options }
        }

        /// Arrange authored state with all options in one semantic transaction.
        pub fn arrange(&self, options: &WasmFamilyArrangeOptions) -> Result<(), JsValue> {
            self.semantic_family()?
                .arrange_with_options(&options.options)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = copyFamily)]
        pub fn copy_family(
            &self,
            references: crate::WasmSceneMembershipBatch,
        ) -> Result<WasmFamilyCopy, JsValue> {
            self.semantic_family()?
                .copy_with_references(&references.copy_references().map_err(js_error)?)
                .map(WasmFamilyCopy::from_copy)
                .map_err(js_error)
        }

        /// Atomically apply Manim subset-display constructor semantics to every
        /// ordinary direct member before execution starts.
        #[wasm_bindgen(js_name = prepareSubsetDisplay)]
        pub fn prepare_subset_display(&self) -> Result<(), JsValue> {
            self.semantic_family()?
                .prepare_subset_display()
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = semanticSlot)]
        pub fn semantic_slot(&self) -> u32 {
            self.family.node_id().slot()
        }

        #[wasm_bindgen(getter, js_name = semanticGeneration)]
        pub fn semantic_generation(&self) -> u32 {
            self.family.node_id().generation()
        }

        #[wasm_bindgen(getter, js_name = memberCount)]
        pub fn member_count(&self) -> Result<usize, JsValue> {
            self.family
                .integration_store()
                .borrow()
                .semantic_family_checked(self.family.node_id())
                .map(|node| node.member_count())
                .map_err(typed_js_error)
        }

        /// Observe authoritative family order when a frontend requests members.
        #[wasm_bindgen(js_name = memberKeys)]
        pub fn member_keys(&self) -> Result<Vec<String>, JsValue> {
            let store = self.family.integration_store().borrow();
            let node = store
                .semantic_family_checked(self.family.node_id())
                .map_err(typed_js_error)?;
            Ok(node
                .members_iter()
                .map(|id| format!("{}:{}", id.slot(), id.generation()))
                .collect())
        }

        #[wasm_bindgen(js_name = memberSlot)]
        pub fn member_slot(&self, index: usize) -> Result<u32, JsValue> {
            self.family
                .integration_store()
                .borrow()
                .node(self.family.node_id())
                .and_then(|node| node.members().get(index).copied())
                .map(SemanticNodeId::slot)
                .ok_or_else(|| JsValue::from_str("family member index is out of bounds"))
        }

        #[wasm_bindgen(js_name = memberGeneration)]
        pub fn member_generation(&self, index: usize) -> Result<u32, JsValue> {
            self.family
                .integration_store()
                .borrow()
                .node(self.family.node_id())
                .and_then(|node| node.members().get(index).copied())
                .map(SemanticNodeId::generation)
                .ok_or_else(|| JsValue::from_str("family member index is out of bounds"))
        }

        /// Apply the complete authored edit before returning per-argument decisions.
        #[wasm_bindgen(js_name = editMembership)]
        pub fn edit_membership(
            &self,
            batch: crate::WasmSceneMembershipBatch,
        ) -> Result<Vec<u8>, JsValue> {
            batch
                .edit_family(&self.semantic_family()?)
                .map(|changed| changed.into_iter().map(u8::from).collect())
                .map_err(js_error)
        }
    }

    #[wasm_bindgen]
    pub struct WasmManimLineEndpoints {
        value: noon::ManimLineEndpoints,
    }

    impl WasmManimLineEndpoints {
        pub(crate) fn from_endpoints(value: noon::ManimLineEndpoints) -> Self {
            Self { value }
        }
    }

    #[wasm_bindgen]
    impl WasmManimLineEndpoints {
        #[wasm_bindgen(getter, js_name = startX)]
        pub fn start_x(&self) -> f64 {
            self.value.start.0
        }
        #[wasm_bindgen(getter, js_name = startY)]
        pub fn start_y(&self) -> f64 {
            self.value.start.1
        }
        #[wasm_bindgen(getter, js_name = endX)]
        pub fn end_x(&self) -> f64 {
            self.value.end.0
        }
        #[wasm_bindgen(getter, js_name = endY)]
        pub fn end_y(&self) -> f64 {
            self.value.end.1
        }
    }

    #[wasm_bindgen]
    pub struct WasmManimColor {
        value: noon_core::Color,
    }

    impl WasmManimColor {
        pub(crate) fn from_color(value: noon_core::Color) -> Self {
            Self { value }
        }
    }

    #[wasm_bindgen]
    impl WasmManimColor {
        #[wasm_bindgen(getter, js_name = red)]
        pub fn red(&self) -> f64 {
            f64::from(self.value.red)
        }
        #[wasm_bindgen(getter, js_name = green)]
        pub fn green(&self) -> f64 {
            f64::from(self.value.green)
        }
        #[wasm_bindgen(getter, js_name = blue)]
        pub fn blue(&self) -> f64 {
            f64::from(self.value.blue)
        }
        #[wasm_bindgen(getter, js_name = alpha)]
        pub fn alpha(&self) -> f64 {
            f64::from(self.value.alpha)
        }
    }

    /// Thin language wrapper over the same store-scoped handle used by Rust.
    #[wasm_bindgen]
    pub struct WasmAuthoringMobjectHandle {
        handle: Mobject,
    }

    impl WasmAuthoringMobjectHandle {
        pub(crate) fn from_semantic_mobject(handle: Mobject) -> Self {
            Self { handle }
        }

        pub(crate) fn semantic_mobject(&self) -> &Mobject {
            &self.handle
        }
        pub(crate) fn id_in_store(
            &self,
            semantics: &SharedSemanticStore,
            context: &str,
        ) -> Result<SemanticNodeId, JsValue> {
            if !Rc::ptr_eq(semantics, self.handle.integration_store()) {
                return Err(typed_js_error(
                    AuthoringFailure::from(noon::AuthoringError::ForeignStore).with_message(
                        format!("{context} and mobject belong to different authoring stores"),
                    ),
                ));
            }
            self.handle.validate().map_err(typed_js_error)?;
            Ok(self.handle.node_id())
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringMobjectHandle {
        #[wasm_bindgen(js_name = fillColor)]
        pub fn fill_color(&self) -> Result<Option<WasmManimColor>, JsValue> {
            self.handle
                .fill_color()
                .map(|color| color.map(WasmManimColor::from_color))
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = strokeColor)]
        pub fn stroke_color(&self) -> Result<Option<WasmManimColor>, JsValue> {
            self.handle
                .stroke_color()
                .map(|color| color.map(WasmManimColor::from_color))
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = strokeWidth)]
        pub fn stroke_width(&self) -> Result<f64, JsValue> {
            self.handle.stroke_width().map_err(js_error)
        }

        #[wasm_bindgen(js_name = setColorGradient)]
        pub fn set_color_gradient(&mut self, values: &[f64]) -> Result<(), JsValue> {
            let colors = super::gradient_colors(values).map_err(js_error)?;
            self.handle.set_color_by_gradient(&colors).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStyle)]
        #[allow(clippy::too_many_arguments)]
        pub fn set_style(
            &mut self,
            fill_enabled: bool,
            fill_red: f64,
            fill_green: f64,
            fill_blue: f64,
            fill_alpha: f64,
            fill_opacity: Option<f64>,
            stroke_enabled: bool,
            stroke_red: f64,
            stroke_green: f64,
            stroke_blue: f64,
            stroke_alpha: f64,
            stroke_width: Option<f64>,
            stroke_opacity: Option<f64>,
        ) -> Result<(), JsValue> {
            let update = super::style_update(
                fill_enabled,
                fill_red,
                fill_green,
                fill_blue,
                fill_alpha,
                fill_opacity,
                stroke_enabled,
                stroke_red,
                stroke_green,
                stroke_blue,
                stroke_alpha,
                stroke_width,
                stroke_opacity,
            )
            .map_err(js_error)?;
            self.handle.set_style(update).map_err(js_error)
        }

        #[wasm_bindgen(js_name = matchStyle)]
        pub fn match_style(&self, target: &WasmAuthoringMobjectHandle) -> Result<(), JsValue> {
            self.handle.match_style(&target.handle).map_err(js_error)
        }

        #[wasm_bindgen(js_name = layoutAnchor)]
        pub fn layout_anchor(&self, index: Option<i32>) -> Result<WasmLayoutAnchor, JsValue> {
            let anchor = noon::LayoutAnchor::from(&self.handle);
            Ok(WasmLayoutAnchor {
                anchor: match index {
                    Some(index) => anchor.member(index as isize),
                    None => anchor,
                },
            })
        }

        #[wasm_bindgen(js_name = manimLineEndpoints)]
        pub fn manim_line_endpoints(&self) -> Result<WasmManimLineEndpoints, JsValue> {
            self.handle
                .manim_line_endpoints()
                .map(WasmManimLineEndpoints::from_endpoints)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = manimColor)]
        pub fn manim_color(&self) -> Result<WasmManimColor, JsValue> {
            self.handle
                .manim_color()
                .map(WasmManimColor::from_color)
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = semanticSlot)]
        pub fn semantic_slot(&self) -> u32 {
            self.handle.node_id().slot()
        }
        #[wasm_bindgen(getter, js_name = semanticGeneration)]
        pub fn semantic_generation(&self) -> u32 {
            self.handle.node_id().generation()
        }
        #[wasm_bindgen(js_name = cloneHandle)]
        pub fn clone_handle(&self) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            self.handle
                .copy_handle()
                .map(|handle| Self { handle })
                .map_err(js_error)
        }
        #[wasm_bindgen(js_name = targetEditor)]
        pub fn target_editor(&self) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            self.clone_handle()
        }

        /// Analytic Line-to-Line point matching. Rust validates both operands and
        /// commits only the source transform, preserving its content and paint.
        #[wasm_bindgen(js_name = matchLine)]
        pub fn match_line(&mut self, target: &WasmAuthoringMobjectHandle) -> Result<(), JsValue> {
            self.handle
                .match_line_handle(&target.handle)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = snapshotJson)]
        pub fn snapshot_json(&self) -> Result<String, JsValue> {
            crate::geometry_export::mobject_json(&self.handle).map_err(js_error)
        }

        /// Read authored rotation directly, without lowering to a wire value.
        #[wasm_bindgen(getter)]
        pub fn rotation(&self) -> Result<f64, JsValue> {
            Ok(self.handle.state().map_err(js_error)?.transform.rotation_z)
        }

        #[wasm_bindgen(getter, js_name = centerX)]
        pub fn center_x(&self) -> Result<f64, JsValue> {
            Ok(self.handle.center().map_err(js_error)?.0)
        }

        #[wasm_bindgen(getter, js_name = centerY)]
        pub fn center_y(&self) -> Result<f64, JsValue> {
            Ok(self.handle.center().map_err(js_error)?.1)
        }

        #[wasm_bindgen(getter)]
        pub fn width(&self) -> Result<f64, JsValue> {
            Ok(self.handle.width().map_err(js_error)?)
        }

        #[wasm_bindgen(getter)]
        pub fn height(&self) -> Result<f64, JsValue> {
            Ok(self.handle.height().map_err(js_error)?)
        }

        #[wasm_bindgen(js_name = criticalX)]
        pub fn critical_x(&self, direction_x: f64, direction_y: f64) -> Result<f64, JsValue> {
            Ok(self
                .handle
                .critical_point(direction_x, direction_y)
                .map_err(js_error)?
                .0)
        }

        #[wasm_bindgen(js_name = criticalY)]
        pub fn critical_y(&self, direction_x: f64, direction_y: f64) -> Result<f64, JsValue> {
            Ok(self
                .handle
                .critical_point(direction_x, direction_y)
                .map_err(js_error)?
                .1)
        }

        pub fn shift(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.handle.shift(x, y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = moveTo)]
        pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.handle.move_to(x, y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setTranslation)]
        pub fn set_translation(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.handle.set_translation(x, y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setScale)]
        pub fn set_scale(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.handle.set_scale(x, y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setRotation)]
        pub fn set_rotation(&mut self, angle: f64) -> Result<(), JsValue> {
            self.handle.set_rotation(angle).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStrokeWidthMode)]
        pub fn set_stroke_width_mode(&mut self, mode: &str) -> Result<(), JsValue> {
            self.handle.set_stroke_width_mode(mode).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStrokeJoin)]
        pub fn set_stroke_join(&mut self, join: &str) -> Result<(), JsValue> {
            self.handle.set_stroke_join(join).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStrokeCap)]
        pub fn set_stroke_cap(&mut self, cap: &str) -> Result<(), JsValue> {
            self.handle.set_stroke_cap(cap).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setObjectOpacity)]
        pub fn set_object_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.handle.set_object_opacity(opacity).map_err(js_error)
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
            self.handle
                .manim_move_to_handle(
                    &other.handle,
                    aligned_edge_x,
                    aligned_edge_y,
                    mask_x,
                    mask_y,
                )
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
            self.handle
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

        pub fn scale(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.handle.manim_scale(x, y).map_err(js_error)
        }

        pub fn rotate(&mut self, angle: f64) -> Result<(), JsValue> {
            self.handle.rotate(angle).map_err(js_error)
        }

        #[wasm_bindgen(js_name = rotateAboutPoint)]
        pub fn rotate_about_point(
            &mut self,
            angle: f64,
            point_x: f64,
            point_y: f64,
        ) -> Result<(), JsValue> {
            self.handle
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
            self.handle
                .set_color(red, green, blue, alpha)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = disableFill)]
        pub fn disable_fill(&mut self) -> Result<(), JsValue> {
            self.handle.disable_fill().map_err(js_error)
        }

        #[wasm_bindgen(js_name = setFillColor)]
        pub fn set_fill_color(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            self.handle
                .set_fill_color(red, green, blue, alpha)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setFillOpacity)]
        pub fn set_fill_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.handle.set_fill_opacity(opacity).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setFill)]
        pub fn set_fill(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            opacity: f64,
        ) -> Result<(), JsValue> {
            self.handle
                .set_fill(red, green, blue, opacity)
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = fillOpacity)]
        pub fn fill_opacity(&self) -> Result<f64, JsValue> {
            Ok(self.handle.fill_opacity().map_err(js_error)?)
        }

        #[wasm_bindgen(js_name = disableStroke)]
        pub fn disable_stroke(&mut self) -> Result<(), JsValue> {
            self.handle.disable_stroke().map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStrokeColor)]
        pub fn set_stroke_color(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            self.handle
                .set_stroke_color(red, green, blue, alpha)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStrokeWidth)]
        pub fn set_stroke_width(&mut self, width: f64) -> Result<(), JsValue> {
            self.handle.set_stroke_width(width).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setStrokeOpacity)]
        pub fn set_stroke_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.handle.set_stroke_opacity(opacity).map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = strokeOpacity)]
        pub fn stroke_opacity(&self) -> Result<f64, JsValue> {
            Ok(self.handle.stroke_opacity().map_err(js_error)?)
        }

        #[wasm_bindgen(js_name = setOpacity)]
        pub fn set_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.handle.set_opacity(opacity).map_err(js_error)
        }

        #[wasm_bindgen(js_name = becomeHandle)]
        pub fn become_handle(
            &self,
            other: &WasmAuthoringMobjectHandle,
            match_height: bool,
            match_width: bool,
            match_center: bool,
            stretch: bool,
        ) -> Result<(), JsValue> {
            // Shared wrapper borrows allow become(self); the alias retains one semantic ID.
            self.handle
                .clone()
                .become_handle(
                    &other.handle,
                    noon::ManimBecomeOptions {
                        match_height,
                        match_width,
                        match_center,
                        stretch,
                    },
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = alignToHandle)]
        pub fn align_to_handle(
            &mut self,
            other: &WasmAuthoringMobjectHandle,
            direction_x: f64,
            direction_y: f64,
        ) -> Result<(), JsValue> {
            self.handle
                .align_to_handle(&other.handle, direction_x, direction_y)
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
            self.handle
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
            self.handle
                .align_on_frame(direction_x, direction_y, buff)
                .map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon::ManimGeometryOptions;
    use noon_core::{
        Color, GeometryRef, SemanticPaint, StoredGeometry, StrokeCap, StrokeJoin, StrokeWidthMode,
        Vec2, VectorPath,
    };

    use super::*;

    #[test]
    fn handle_mutations_keep_state_in_shared_rust_semantics() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut handle = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::circle(1.0),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        handle.shift(2.0, -1.0).unwrap();
        handle.scale(1.5, 0.5).unwrap();
        assert_eq!(handle.center().unwrap(), (2.0, -1.0));
        assert_eq!(handle.width().unwrap(), 3.0);
        assert_eq!(handle.height().unwrap(), 1.0);
        let translation = handle.state().unwrap().transform.translation;
        assert_eq!((translation.x, translation.y), (2.0, -1.0));
    }

    #[test]
    fn authoring_transform_keeps_f64_precision_until_render_lowering() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut handle = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::rectangle(2.0, 1.0),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        handle.shift(0.7, 0.3).unwrap();
        assert_eq!(handle.state().unwrap().transform.translation.x, 0.7);
        assert_eq!(handle.state().unwrap().transform.translation.y, 0.3);
        assert!((handle.critical_point(-1.0, 0.0).unwrap().0 + 0.3).abs() < 1e-12);
        assert!((handle.critical_point(0.0, 1.0).unwrap().1 - 0.8).abs() < 1e-12);
        assert_ne!(handle.wire_translation().unwrap().0, 0.7);

        handle.scale(1.1, 0.9).unwrap();
        handle.rotate(0.2).unwrap();
        assert_eq!(handle.state().unwrap().transform.scale.x, 1.1);
        assert_eq!(handle.state().unwrap().transform.scale.y, 0.9);
        assert_eq!(handle.state().unwrap().transform.rotation_z, 0.2);
    }

    #[test]
    fn pivoted_rotation_preserves_offset_line_center() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut handle = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::line(Vec2::ZERO, Vec2::new(1.0, 0.0)),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        handle.shift(2.0, 0.0).unwrap();
        let pivot = handle.center().unwrap();
        assert!((pivot.0 - 2.5).abs() < 1e-12);
        assert!(pivot.1.abs() < 1e-12);
        handle
            .rotate_about_point(std::f64::consts::FRAC_PI_2, pivot.0, pivot.1)
            .unwrap();
        let center = handle.center().unwrap();
        assert!((center.0 - 2.5).abs() < 1e-9);
        assert!(center.1.abs() < 1e-9);
        assert!((handle.state().unwrap().transform.translation.x - 2.5).abs() < 1e-12);
        assert!((handle.state().unwrap().transform.translation.y + 0.5).abs() < 1e-12);
        assert!(
            (handle.state().unwrap().transform.rotation_z - std::f64::consts::FRAC_PI_2).abs()
                < 1e-12
        );
    }

    #[test]
    fn vector_path_layout_uses_extrema_not_control_hull() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .quadratic_to(Vec2::new(0.0, 2.0), Vec2::new(1.0, 0.0));
        let handle = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::path(path),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        let bounds = handle.layout_bounds().unwrap().unwrap();
        assert!((bounds.min_x + 1.0).abs() < 1e-9);
        assert!((bounds.max_x - 1.0).abs() < 1e-9);
        assert!(bounds.min_y.abs() < 1e-9);
        assert!((bounds.max_y - 1.0).abs() < 1e-9);
        assert!((handle.height().unwrap() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn transformed_layout_bounds_match_manim_world_extrema() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut ellipse = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::circle(1.0),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        ellipse.scale(2.0, 1.0).unwrap();
        ellipse.rotate(std::f64::consts::FRAC_PI_4).unwrap();
        assert!((ellipse.width().unwrap() - 10.0_f64.sqrt()).abs() < 1e-12);
        assert!((ellipse.height().unwrap() - 10.0_f64.sqrt()).abs() < 1e-12);

        let mut diagonal = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::line(Vec2::ZERO, Vec2::new(1.0, 1.0)),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        diagonal.rotate(std::f64::consts::FRAC_PI_4).unwrap();
        assert!(diagonal.width().unwrap().abs() < 1e-12);
        assert!((diagonal.height().unwrap() - 2.0_f64.sqrt()).abs() < 1e-12);

        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .quadratic_to(Vec2::new(0.0, 2.0), Vec2::new(1.0, 0.0));
        let mut curve = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::path(path),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        curve.rotate(std::f64::consts::FRAC_PI_4).unwrap();
        let expected = 9.0 * 2.0_f64.sqrt() / 8.0;
        assert!((curve.width().unwrap() - expected).abs() < 1e-12);
        assert!((curve.height().unwrap() - expected).abs() < 1e-12);
    }

    #[test]
    fn manim_primitive_constructors_own_geometry_and_cairo_defaults() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let circle = Mobject::manim_circle(std::rc::Rc::clone(&authoring_store), 1.5).unwrap();
        let circle_state = circle.state().unwrap();
        assert_eq!(
            circle_state.content.geometry(),
            Some(StoredGeometry::Circle { radius: 1.5 })
        );
        let Some(SemanticPaint::Solid(fill)) = circle_state.style.fill else {
            panic!("Manim circle must retain a solid fill");
        };
        let Some(SemanticPaint::Solid(stroke)) = circle_state.style.stroke else {
            panic!("Manim circle must retain a solid stroke");
        };
        assert_eq!(fill.red, Color::RED.red);
        assert_eq!(circle_state.style.fill_opacity, 0.0);
        assert_eq!(stroke.red, Color::RED.red);
        assert_eq!(circle_state.style.stroke_opacity, 1.0);
        assert_eq!(circle_state.style.stroke_width, 0.04);
        assert_eq!(
            circle_state.style.stroke_width_mode,
            StrokeWidthMode::ScreenSpace
        );
        assert_eq!(circle_state.style.stroke_join, StrokeJoin::Miter);
        assert_eq!(circle_state.style.stroke_cap, StrokeCap::Butt);

        let line = Mobject::manim_line(std::rc::Rc::clone(&authoring_store), -2.0, 1.0, 3.0, -1.0)
            .unwrap();
        let line_state = line.state().unwrap();
        assert_eq!(
            line_state.content.geometry(),
            Some(StoredGeometry::Line {
                start: Vec2::new(-2.0, 1.0),
                end: Vec2::new(3.0, -1.0),
            })
        );
        let Some(SemanticPaint::Solid(line_stroke)) = line_state.style.stroke else {
            panic!("Manim line must retain a solid stroke");
        };
        assert_eq!(line_stroke.red, Color::WHITE.red);

        let mut square = Mobject::manim_square(std::rc::Rc::clone(&authoring_store), 2.0).unwrap();
        square.set_translation(2.0, 3.0).unwrap();
        square.set_scale(2.0, 0.5).unwrap();
        square.set_rotation(0.4).unwrap();
        square.set_stroke_width_mode("scale_with_object").unwrap();
        square.set_stroke_join("bevel").unwrap();
        square.set_stroke_cap("square").unwrap();
        square.set_object_opacity(0.8).unwrap();
        assert_eq!(square.wire_translation().unwrap(), (2.0, 3.0));
        assert_eq!(square.wire_scale().unwrap(), (2.0, 0.5));
        assert!((square.wire_rotation().unwrap() - 0.4_f32 as f64).abs() < 1e-7);
        let square_style = square.state().unwrap().style;
        assert_eq!(
            square_style.stroke_width_mode,
            StrokeWidthMode::ScaleWithObject
        );
        assert_eq!(square_style.stroke_join, StrokeJoin::Bevel);
        assert_eq!(square_style.stroke_cap, StrokeCap::Square);
        assert_eq!(square_style.object_opacity, 0.8);
    }

    #[test]
    fn layout_operations_are_shared_and_deterministic() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let left = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::circle(0.5),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        let mut right = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::rectangle(1.0, 1.0),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        right.next_to_handle(&left, 1.0, 0.0, 0.25).unwrap();
        assert!((right.center().unwrap().0 - 1.25).abs() < 1e-9);
        right.align_on_frame(1.0, 1.0, 0.5).unwrap();
        let bounds = right.layout_bounds().unwrap().unwrap();
        assert!(
            (bounds.max_x - (f64::from(noon_core::DEFAULT_FRAME_WIDTH) * 0.5 - 0.5)).abs() < 1e-6
        );
        assert!(
            (bounds.max_y - (f64::from(noon_core::DEFAULT_FRAME_HEIGHT) * 0.5 - 0.5)).abs() < 1e-6
        );
    }

    #[test]
    fn manim_leaf_placement_preserves_raw_direction_edges_and_masks() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let reference = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::rectangle(2.0, 2.0),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        let mut diagonal = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::rectangle(2.0, 2.0),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
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
        assert!((diagonal.center().unwrap().0 - 2.25).abs() < 1e-12);
        assert!((diagonal.center().unwrap().1 - 2.25).abs() < 1e-12);

        let mut moved = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::rectangle(1.0, 1.0),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        moved.shift(0.0, -2.0).unwrap();
        moved
            .manim_move_to_handle(&reference, -1.0, 1.0, 1.0, 0.0)
            .unwrap();
        assert!((moved.center().unwrap().0 + 0.5).abs() < 1e-12);
        assert!((moved.center().unwrap().1 + 2.0).abs() < 1e-12);

        let mut aligned = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::rectangle(1.0, 1.0),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        aligned.shift(0.0, -1.0).unwrap();
        aligned.align_to_handle(&reference, 1.0, 0.0).unwrap();
        assert!((aligned.center().unwrap().0 - 0.5).abs() < 1e-12);
        assert!((aligned.center().unwrap().1 + 1.0).abs() < 1e-12);
    }

    #[test]
    fn shared_style_mutations_preserve_independent_channels() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut options = ManimGeometryOptions::circle(1.0).unwrap();
        options.set_fill(1.0, 0.0, 0.0, 0.4).unwrap();
        options.set_stroke(0.0, 0.0, 1.0, 0.7).unwrap();
        let mut handle =
            Mobject::from_manim_geometry(std::rc::Rc::clone(&authoring_store), options).unwrap();

        handle.set_fill_color(0.0, 1.0, 0.0, 1.0).unwrap();
        assert!((handle.fill_opacity().unwrap() - 0.4).abs() < 1e-6);
        handle.set_fill_opacity(0.25).unwrap();
        handle.set_stroke_width(3.5).unwrap();
        handle.set_stroke_opacity(0.6).unwrap();
        assert_eq!(handle.fill_opacity().unwrap(), 0.25);
        assert_eq!(handle.stroke_opacity().unwrap(), 0.6);
        let style = handle.state().unwrap().style;
        assert!((style.stroke_width - 3.5).abs() < 1e-6);
        assert_eq!(style.stroke_opacity, 0.6);

        handle.set_opacity(0.2).unwrap();
        assert_eq!(handle.fill_opacity().unwrap(), 0.2);
        assert_eq!(handle.stroke_opacity().unwrap(), 0.2);
        handle.disable_fill().unwrap();
        assert_eq!(handle.fill_opacity().unwrap(), 0.0);
    }

    #[test]
    fn target_editor_alias_supports_moving_around_without_snapshot_round_trips() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let base = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::circle(1.0),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        let mut target = base.target_editor().unwrap();

        target.shift(-1.0, 0.0).unwrap();
        target.set_fill(1.0, 0.525, 0.184, 0.5).unwrap();
        target.scale(0.3, 0.3).unwrap();
        target.rotate(0.4).unwrap();

        assert_eq!(base.center().unwrap(), (0.0, 0.0));
        assert_eq!(target.state().unwrap().transform.translation.x, -1.0);
        assert_eq!(target.state().unwrap().transform.scale.x, 0.3);
        assert_eq!(target.state().unwrap().transform.rotation_z, 0.4);
        assert_eq!(target.fill_opacity().unwrap(), 0.5);
        let Some(SemanticPaint::Solid(fill)) = target.state().unwrap().style.fill else {
            panic!("target must retain a solid fill");
        };
        assert_eq!(fill.red, 1.0);
        assert_eq!(fill.green, 0.525);
        assert_eq!(fill.blue, 0.184);
    }

    #[test]
    fn target_editor_clone_alias_is_independent_and_set_fill_is_transactional() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let base = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::circle(1.0),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        let mut target = base.target_editor().unwrap();
        let sibling = target.target_editor().unwrap();

        target.shift(2.0, 0.0).unwrap();
        target.set_fill(0.0, 1.0, 0.0, 0.25).unwrap();
        assert_eq!(base.center().unwrap(), (0.0, 0.0));
        assert_eq!(sibling.center().unwrap(), (0.0, 0.0));
        assert_eq!(sibling.fill_opacity().unwrap(), 1.0);
        let Some(SemanticPaint::Solid(sibling_fill)) = sibling.state().unwrap().style.fill else {
            panic!("sibling must retain a solid fill");
        };
        assert_eq!(sibling_fill.red, 1.0);
        assert_eq!(sibling_fill.green, 1.0);

        let before = target.state().unwrap();
        assert!(target.set_fill(1.0, 0.0, 0.0, 2.0).is_err());
        assert_eq!(target.state().unwrap(), before);
    }

    #[test]
    fn family_layout_leaf_order_comes_from_shared_semantic_graph() {
        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let nested = store.insert_family();
        store.add_member(nested, first).unwrap();
        let outer = store.insert_family();
        store.add_member(outer, nested).unwrap();
        store.add_member(outer, second).unwrap();

        assert_eq!(
            store.ordered_leaf_nodes(outer).unwrap(),
            vec![first, second]
        );

        let alias = store.insert_family();
        store.add_member(alias, first).unwrap();
        let aliased_outer = store.insert_family();
        store.add_member(aliased_outer, nested).unwrap();
        store.add_member(aliased_outer, alias).unwrap();
        assert_eq!(
            store.ordered_leaf_nodes(aliased_outer).unwrap(),
            vec![first]
        );
    }

    #[test]
    fn become_and_replace_keep_state_inside_shared_handle() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut source = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::circle(0.5),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        source.shift(-2.0, 0.5).unwrap();
        let mut target = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::rectangle(2.0, 1.0),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        target.shift(1.0, -0.25).unwrap();

        source
            .become_handle(&target, noon::ManimBecomeOptions::default())
            .unwrap();
        let source_state = source.state().unwrap();
        let target_state = target.state().unwrap();
        assert_eq!(source_state.content, target_state.content);
        assert_eq!(source_state.transform, target_state.transform);
        assert_eq!(source_state.style, target_state.style);
        assert_ne!(
            source_state.presentation(),
            target_state.presentation(),
            "become must preserve each object's insertion order"
        );

        let mut replacement = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::circle(0.25),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        replacement.replace_handle(&target, 0, false).unwrap();
        assert!((replacement.width().unwrap() - 2.0).abs() < 1e-6);
        assert!((replacement.height().unwrap() - 2.0).abs() < 1e-6);
        assert!((replacement.center().unwrap().0 - 1.0).abs() < 1e-6);
        assert!((replacement.center().unwrap().1 + 0.25).abs() < 1e-6);

        let mut stretched = Mobject::from_geometry(
            std::rc::Rc::clone(&authoring_store),
            GeometryRef::circle(0.25),
            noon_core::SemanticStyle::default(),
        )
        .unwrap();
        stretched.replace_handle(&target, 0, true).unwrap();
        assert!((stretched.width().unwrap() - 2.0).abs() < 1e-6);
        assert!((stretched.height().unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn wire_projection_matches_typed_runtime_after_shared_edits() {
        let mut scene = noon::Scene::new();
        let authoring_store = std::rc::Rc::clone(scene.integration_store());
        let mut options = ManimGeometryOptions::rectangle(2.0, 1.0).unwrap();
        options.set_fill(0.2, 0.3, 0.4, 0.5).unwrap();
        options.set_stroke(0.6, 0.7, 0.8, 0.9).unwrap();
        let mut handle =
            Mobject::from_manim_geometry(std::rc::Rc::clone(&authoring_store), options).unwrap();

        handle.shift(0.7, -0.3).unwrap();
        handle.scale(1.1, 0.9).unwrap();
        handle.rotate(0.2).unwrap();
        handle.set_fill_opacity(0.25).unwrap();
        handle.set_stroke_width(3.5).unwrap();

        scene.add(&handle).unwrap();
        let session = scene.execution_session().unwrap();
        let effective = &session.frame().objects[0];
        assert_eq!(
            handle.wire_translation().unwrap(),
            (
                f64::from(effective.transform.translation.x),
                f64::from(effective.transform.translation.y),
            )
        );
        assert_eq!(
            handle.wire_scale().unwrap(),
            (
                f64::from(effective.transform.scale.x),
                f64::from(effective.transform.scale.y),
            )
        );
        assert_eq!(
            handle.wire_rotation().unwrap(),
            f64::from(effective.transform.rotation)
        );
        assert_eq!(handle.wire_fill().unwrap().unwrap().3, 0.25_f32 as f64);
        assert_eq!(handle.wire_stroke_width().unwrap(), 3.5_f32 as f64);
    }
}

#![cfg(target_arch = "wasm32")]

use crate::{WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmAuthoringStore};
use std::{cell::Cell, rc::Rc};
use wasm_bindgen::prelude::*;

use crate::authoring_error::{js_error, AuthoringFailure};

fn arrow_scale_js_error(error: noon::ArrowScaleError) -> JsValue {
    match error {
        noon::ArrowScaleError::Authoring(cause) => js_error(cause),
        other => js_error(AuthoringFailure::new(
            "unsupported_operation",
            "arrow.invalid_topology",
            other,
        )),
    }
}

#[derive(Clone, Debug)]
enum VectorFieldColoringDraft {
    Default,
    Single(noon::Color),
    Gradient {
        colors: Vec<noon::Color>,
        min: f64,
        max: f64,
        values: Option<Vec<Option<f64>>>,
    },
}

#[derive(Clone, Debug)]
struct VectorFieldDraft {
    ranges: noon::VectorFieldRanges2D,
    points: Vec<noon::VectorFieldPoint>,
    vectors: Vec<Option<noon::VectorFieldPoint>>,
    display_lengths: Option<Vec<Option<f64>>>,
    coloring: VectorFieldColoringDraft,
}

#[derive(Clone, Debug)]
enum ArrowRequest {
    Arrow(noon::ManimArrowOptions),
    VectorField(VectorFieldDraft),
}

/// Inert typed Arrow-family constructor intent. The same preparation-only wrapper
/// also carries a static ArrowVectorField draft so Python can evaluate callbacks
/// at Rust-owned sample points without inventing a second sampling contract.
#[wasm_bindgen]
pub struct WasmManimArrowOptions {
    request: ArrowRequest,
}

impl WasmManimArrowOptions {
    fn arrow_options_mut(&mut self) -> Result<&mut noon::ManimArrowOptions, JsValue> {
        match &mut self.request {
            ArrowRequest::Arrow(options) => Ok(options),
            ArrowRequest::VectorField(_) => Err(invalid_input(
                "vector_field.arrow_option",
                "Arrow option setters cannot be applied to an ArrowVectorField draft",
            )),
        }
    }

    fn vector_field_draft(&self) -> Result<&VectorFieldDraft, JsValue> {
        match &self.request {
            ArrowRequest::VectorField(draft) => Ok(draft),
            ArrowRequest::Arrow(_) => Err(invalid_input(
                "vector_field.not_draft",
                "vector-field sampling is available only on an ArrowVectorField draft",
            )),
        }
    }

    fn vector_field_draft_mut(&mut self) -> Result<&mut VectorFieldDraft, JsValue> {
        match &mut self.request {
            ArrowRequest::VectorField(draft) => Ok(draft),
            ArrowRequest::Arrow(_) => Err(invalid_input(
                "vector_field.not_draft",
                "vector-field sampling is available only on an ArrowVectorField draft",
            )),
        }
    }

    fn sample_point(&self, index: u32) -> Result<noon::VectorFieldPoint, JsValue> {
        let draft = self.vector_field_draft()?;
        draft.points.get(index as usize).copied().ok_or_else(|| {
            invalid_input(
                "vector_field.sample_index",
                format!("vector-field sample index {index} is out of range"),
            )
        })
    }
}

#[wasm_bindgen]
impl WasmManimArrowOptions {
    pub fn arrow(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
    ) -> Result<WasmManimArrowOptions, JsValue> {
        noon::ManimArrowOptions::arrow(start_x, start_y, end_x, end_y)
            .map(|options| Self {
                request: ArrowRequest::Arrow(options),
            })
            .map_err(js_error)
    }

    pub fn vector(direction_x: f64, direction_y: f64) -> Result<WasmManimArrowOptions, JsValue> {
        noon::ManimArrowOptions::vector(direction_x, direction_y)
            .map(|options| Self {
                request: ArrowRequest::Arrow(options),
            })
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = doubleArrow)]
    pub fn double_arrow(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
    ) -> Result<WasmManimArrowOptions, JsValue> {
        noon::ManimArrowOptions::double_arrow(start_x, start_y, end_x, end_y)
            .map(|options| Self {
                request: ArrowRequest::Arrow(options),
            })
            .map_err(js_error)
    }

    /// Start one static 2D field draft. Rust computes the exact Manim-compatible
    /// sample grid; the frontend only fills values returned by its Python callbacks.
    #[wasm_bindgen(js_name = vectorField)]
    #[allow(clippy::too_many_arguments)]
    pub fn vector_field(
        x_start: f64,
        x_end: f64,
        x_step: f64,
        y_start: f64,
        y_end: f64,
        y_step: f64,
        custom_length: bool,
    ) -> Result<WasmManimArrowOptions, JsValue> {
        let ranges = noon::VectorFieldRanges2D::new(
            noon::VectorFieldAxisRange::new(x_start, x_end, x_step),
            noon::VectorFieldAxisRange::new(y_start, y_end, y_step),
        );
        let plan =
            noon_geometry::plan_static_arrow_vector_field(|_| noon::VectorFieldPoint::ZERO, ranges)
                .map_err(vector_field_planning_error)?;
        let points = plan
            .samples
            .into_iter()
            .map(|sample| sample.point)
            .collect::<Vec<_>>();
        let vectors = vec![None; points.len()];
        let display_lengths = custom_length.then(|| vec![None; points.len()]);
        Ok(Self {
            request: ArrowRequest::VectorField(VectorFieldDraft {
                ranges,
                points,
                vectors,
                display_lengths,
                coloring: VectorFieldColoringDraft::Default,
            }),
        })
    }

    #[wasm_bindgen(getter, js_name = sampleCount)]
    pub fn sample_count(&self) -> Result<u32, JsValue> {
        let len = self.vector_field_draft()?.points.len();
        u32::try_from(len).map_err(|_| {
            invalid_input(
                "vector_field.sample_count",
                "vector-field sample count exceeds the browser bridge range",
            )
        })
    }

    #[wasm_bindgen(js_name = sampleX)]
    pub fn sample_x(&self, index: u32) -> Result<f64, JsValue> {
        self.sample_point(index).map(|point| point.x)
    }

    #[wasm_bindgen(js_name = sampleY)]
    pub fn sample_y(&self, index: u32) -> Result<f64, JsValue> {
        self.sample_point(index).map(|point| point.y)
    }

    #[wasm_bindgen(js_name = setVector)]
    pub fn set_vector(&mut self, index: u32, x: f64, y: f64) -> Result<(), JsValue> {
        let draft = self.vector_field_draft_mut()?;
        let slot = draft.vectors.get_mut(index as usize).ok_or_else(|| {
            invalid_input(
                "vector_field.sample_index",
                format!("vector-field sample index {index} is out of range"),
            )
        })?;
        *slot = Some(noon::VectorFieldPoint::new(x, y));
        Ok(())
    }

    #[wasm_bindgen(js_name = setDisplayLength)]
    pub fn set_display_length(&mut self, index: u32, value: f64) -> Result<(), JsValue> {
        let draft = self.vector_field_draft_mut()?;
        let Some(lengths) = draft.display_lengths.as_mut() else {
            return Err(invalid_input(
                "vector_field.default_length",
                "display lengths may only be supplied for a custom length function",
            ));
        };
        let slot = lengths.get_mut(index as usize).ok_or_else(|| {
            invalid_input(
                "vector_field.sample_index",
                format!("vector-field sample index {index} is out of range"),
            )
        })?;
        *slot = Some(value);
        Ok(())
    }

    #[wasm_bindgen(js_name = setFieldColor)]
    pub fn set_field_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        if [red, green, blue, alpha]
            .iter()
            .any(|value| !value.is_finite())
        {
            return Err(invalid_input(
                "vector_field.invalid_color",
                "vector-field color components must be finite",
            ));
        }
        self.vector_field_draft_mut()?.coloring = VectorFieldColoringDraft::Single(
            noon::Color::rgba(red as f32, green as f32, blue as f32, alpha as f32),
        );
        Ok(())
    }

    #[wasm_bindgen(js_name = setColorGradient)]
    pub fn set_color_gradient(
        &mut self,
        min: f64,
        max: f64,
        custom_scheme: bool,
    ) -> Result<(), JsValue> {
        let sample_count = self.vector_field_draft()?.points.len();
        self.vector_field_draft_mut()?.coloring = VectorFieldColoringDraft::Gradient {
            colors: Vec::new(),
            min,
            max,
            values: custom_scheme.then(|| vec![None; sample_count]),
        };
        Ok(())
    }

    #[wasm_bindgen(js_name = addColorGradientStop)]
    pub fn add_color_gradient_stop(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), JsValue> {
        if [red, green, blue, alpha]
            .iter()
            .any(|value| !value.is_finite())
        {
            return Err(invalid_input(
                "vector_field.invalid_color",
                "vector-field gradient color components must be finite",
            ));
        }
        let draft = self.vector_field_draft_mut()?;
        let VectorFieldColoringDraft::Gradient { colors, .. } = &mut draft.coloring else {
            return Err(invalid_input(
                "vector_field.not_gradient",
                "gradient stops require an ArrowVectorField color gradient",
            ));
        };
        colors.push(noon::Color::rgba(
            red as f32,
            green as f32,
            blue as f32,
            alpha as f32,
        ));
        Ok(())
    }

    #[wasm_bindgen(js_name = setColorValue)]
    pub fn set_color_value(&mut self, index: u32, value: f64) -> Result<(), JsValue> {
        let draft = self.vector_field_draft_mut()?;
        let VectorFieldColoringDraft::Gradient {
            values: Some(values),
            ..
        } = &mut draft.coloring
        else {
            return Err(invalid_input(
                "vector_field.default_color_scheme",
                "color values may only be supplied for a custom color scheme",
            ));
        };
        let slot = values.get_mut(index as usize).ok_or_else(|| {
            invalid_input(
                "vector_field.sample_index",
                format!("vector-field sample index {index} is out of range"),
            )
        })?;
        *slot = Some(value);
        Ok(())
    }

    #[wasm_bindgen(js_name = setBuff)]
    pub fn set_buff(&mut self, value: f64) -> Result<(), JsValue> {
        self.arrow_options_mut()?.set_buff(value).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setTipLength)]
    pub fn set_tip_length(&mut self, value: f64) -> Result<(), JsValue> {
        self.arrow_options_mut()?
            .set_tip_length(value)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setMaxTipLengthToLengthRatio)]
    pub fn set_max_tip_length_to_length_ratio(&mut self, value: f64) -> Result<(), JsValue> {
        self.arrow_options_mut()?
            .set_max_tip_length_to_length_ratio(value)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setMaxStrokeWidthToLengthRatio)]
    pub fn set_max_stroke_width_to_length_ratio(&mut self, value: f64) -> Result<(), JsValue> {
        self.arrow_options_mut()?
            .set_max_stroke_width_to_length_ratio(value)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setZIndex)]
    pub fn set_z_index(&mut self, value: f64) -> Result<(), JsValue> {
        self.arrow_options_mut()?
            .set_z_index(value)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setTranslation)]
    pub fn set_translation(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
        self.arrow_options_mut()?
            .set_translation(x, y)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setScale)]
    pub fn set_scale(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
        self.arrow_options_mut()?.set_scale(x, y).map_err(js_error)
    }

    #[wasm_bindgen(js_name = setRotation)]
    pub fn set_rotation(&mut self, value: f64) -> Result<(), JsValue> {
        self.arrow_options_mut()?
            .set_rotation(value)
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
        self.arrow_options_mut()?
            .set_color(red, green, blue, alpha)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeWidth)]
    pub fn set_stroke_width(&mut self, value: f64) -> Result<(), JsValue> {
        self.arrow_options_mut()?
            .set_stroke_width(value)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeWidthMode)]
    pub fn set_stroke_width_mode(&mut self, value: &str) -> Result<(), JsValue> {
        self.arrow_options_mut()?
            .set_stroke_width_mode(value)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeJoin)]
    pub fn set_stroke_join(&mut self, value: &str) -> Result<(), JsValue> {
        self.arrow_options_mut()?
            .set_stroke_join(value)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setStrokeCap)]
    pub fn set_stroke_cap(&mut self, value: &str) -> Result<(), JsValue> {
        self.arrow_options_mut()?
            .set_stroke_cap(value)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = setObjectOpacity)]
    pub fn set_object_opacity(&mut self, value: f64) -> Result<(), JsValue> {
        self.arrow_options_mut()?
            .set_object_opacity(value)
            .map_err(js_error)
    }
}

#[derive(Clone, Debug)]
enum PublishedArrowRequest {
    Arrow(noon::ManimArrow),
    VectorField(noon::ManimArrowVectorField),
}

/// Opaque wrapper around one atomically-published shared Arrow family or static
/// ArrowVectorField family. Existing Arrow accessors reject field-only use.
#[wasm_bindgen]
pub struct WasmAuthoringArrowHandle {
    published: PublishedArrowRequest,
}

impl WasmAuthoringArrowHandle {
    pub(crate) fn arrow(&self) -> Result<&noon::ManimArrow, JsValue> {
        match &self.published {
            PublishedArrowRequest::Arrow(arrow) => Ok(arrow),
            PublishedArrowRequest::VectorField(_) => Err(invalid_input(
                "vector_field.component",
                "single-Arrow component access is unavailable on an ArrowVectorField handle",
            )),
        }
    }

    fn vector(&self, index: u32) -> Result<&noon::ManimArrow, JsValue> {
        match &self.published {
            PublishedArrowRequest::VectorField(field) => {
                field.vectors().get(index as usize).ok_or_else(|| {
                    invalid_input(
                        "vector_field.vector_index",
                        format!("vector-field vector index {index} is out of range"),
                    )
                })
            }
            PublishedArrowRequest::Arrow(_) => Err(invalid_input(
                "vector_field.not_field",
                "vector member access is available only on an ArrowVectorField handle",
            )),
        }
    }
}

#[wasm_bindgen]
impl WasmAuthoringArrowHandle {
    pub fn family(&self) -> WasmAuthoringFamilyHandle {
        let family = match &self.published {
            PublishedArrowRequest::Arrow(arrow) => arrow.family(),
            PublishedArrowRequest::VectorField(field) => field.family(),
        };
        WasmAuthoringFamilyHandle::from_semantic_family(family.clone())
    }

    pub fn shaft(&self) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.arrow()
            .map(|arrow| WasmAuthoringMobjectHandle::from_semantic_mobject(arrow.shaft().clone()))
    }

    #[wasm_bindgen(js_name = endTip)]
    pub fn end_tip(&self) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.arrow()
            .map(|arrow| WasmAuthoringMobjectHandle::from_semantic_mobject(arrow.end_tip().clone()))
    }

    #[wasm_bindgen(getter, js_name = hasStartTip)]
    pub fn has_start_tip(&self) -> bool {
        matches!(
            &self.published,
            PublishedArrowRequest::Arrow(arrow) if arrow.start_tip().is_some()
        )
    }

    #[wasm_bindgen(js_name = startTip)]
    pub fn start_tip(&self) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.arrow()?
            .start_tip()
            .cloned()
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .ok_or_else(|| JsValue::from_str("Arrow has no start tip"))
    }

    #[wasm_bindgen(js_name = startX)]
    pub fn start_x(&self) -> Result<f64, JsValue> {
        self.arrow()?
            .manim_endpoints()
            .map(|endpoints| endpoints.start.0)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = startY)]
    pub fn start_y(&self) -> Result<f64, JsValue> {
        self.arrow()?
            .manim_endpoints()
            .map(|endpoints| endpoints.start.1)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = endX)]
    pub fn end_x(&self) -> Result<f64, JsValue> {
        self.arrow()?
            .manim_endpoints()
            .map(|endpoints| endpoints.end.0)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = endY)]
    pub fn end_y(&self) -> Result<f64, JsValue> {
        self.arrow()?
            .manim_endpoints()
            .map(|endpoints| endpoints.end.1)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = vectorX)]
    pub fn vector_x(&self) -> Result<f64, JsValue> {
        self.arrow()?
            .manim_vector()
            .map(|vector| vector.0)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = vectorY)]
    pub fn vector_y(&self) -> Result<f64, JsValue> {
        self.arrow()?
            .manim_vector()
            .map(|vector| vector.1)
            .map_err(js_error)
    }

    pub fn length(&self) -> Result<f64, JsValue> {
        self.arrow()?.manim_length().map_err(js_error)
    }

    pub fn angle(&self) -> Result<f64, JsValue> {
        self.arrow()?.manim_angle().map_err(js_error)
    }

    #[wasm_bindgen(js_name = unitVectorX)]
    pub fn unit_vector_x(&self) -> Result<f64, JsValue> {
        self.arrow()?
            .manim_unit_vector()
            .map(|vector| vector.0)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = unitVectorY)]
    pub fn unit_vector_y(&self) -> Result<f64, JsValue> {
        self.arrow()?
            .manim_unit_vector()
            .map(|vector| vector.1)
            .map_err(js_error)
    }

    pub fn scale(&self, factor: f64, scale_tips: bool) -> Result<(), JsValue> {
        self.arrow()?
            .scale(factor, scale_tips)
            .map_err(arrow_scale_js_error)
    }

    #[wasm_bindgen(getter, js_name = vectorCount)]
    pub fn vector_count(&self) -> u32 {
        match &self.published {
            PublishedArrowRequest::VectorField(field) => {
                u32::try_from(field.vectors().len()).unwrap_or(u32::MAX)
            }
            PublishedArrowRequest::Arrow(_) => 0,
        }
    }

    #[wasm_bindgen(js_name = vectorFamily)]
    pub fn vector_family(&self, index: u32) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        self.vector(index)
            .map(|arrow| WasmAuthoringFamilyHandle::from_semantic_family(arrow.family().clone()))
    }

    #[wasm_bindgen(js_name = vectorShaft)]
    pub fn vector_shaft(&self, index: u32) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.vector(index)
            .map(|arrow| WasmAuthoringMobjectHandle::from_semantic_mobject(arrow.shaft().clone()))
    }

    #[wasm_bindgen(js_name = vectorEndTip)]
    pub fn vector_end_tip(&self, index: u32) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        self.vector(index)
            .map(|arrow| WasmAuthoringMobjectHandle::from_semantic_mobject(arrow.end_tip().clone()))
    }

    #[wasm_bindgen(js_name = vectorStartX)]
    pub fn vector_start_x(&self, index: u32) -> Result<f64, JsValue> {
        self.vector(index)?
            .manim_endpoints()
            .map(|endpoints| endpoints.start.0)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = vectorStartY)]
    pub fn vector_start_y(&self, index: u32) -> Result<f64, JsValue> {
        self.vector(index)?
            .manim_endpoints()
            .map(|endpoints| endpoints.start.1)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = vectorEndX)]
    pub fn vector_end_x(&self, index: u32) -> Result<f64, JsValue> {
        self.vector(index)?
            .manim_endpoints()
            .map(|endpoints| endpoints.end.0)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = vectorEndY)]
    pub fn vector_end_y(&self, index: u32) -> Result<f64, JsValue> {
        self.vector(index)?
            .manim_endpoints()
            .map(|endpoints| endpoints.end.1)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = vectorVectorX)]
    pub fn vector_vector_x(&self, index: u32) -> Result<f64, JsValue> {
        self.vector(index)?
            .manim_vector()
            .map(|vector| vector.0)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = vectorVectorY)]
    pub fn vector_vector_y(&self, index: u32) -> Result<f64, JsValue> {
        self.vector(index)?
            .manim_vector()
            .map(|vector| vector.1)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = vectorLength)]
    pub fn vector_length(&self, index: u32) -> Result<f64, JsValue> {
        self.vector(index)?.manim_length().map_err(js_error)
    }

    #[wasm_bindgen(js_name = vectorAngle)]
    pub fn vector_angle(&self, index: u32) -> Result<f64, JsValue> {
        self.vector(index)?.manim_angle().map_err(js_error)
    }

    #[wasm_bindgen(js_name = vectorUnitVectorX)]
    pub fn vector_unit_vector_x(&self, index: u32) -> Result<f64, JsValue> {
        self.vector(index)?
            .manim_unit_vector()
            .map(|vector| vector.0)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = vectorUnitVectorY)]
    pub fn vector_unit_vector_y(&self, index: u32) -> Result<f64, JsValue> {
        self.vector(index)?
            .manim_unit_vector()
            .map(|vector| vector.1)
            .map_err(js_error)
    }
}

#[wasm_bindgen]
impl WasmAuthoringStore {
    /// Consume the full inert Arrow request and publish either one Arrow or one
    /// static vector-field family atomically through shared Rust authoring.
    #[wasm_bindgen(js_name = createManimArrow)]
    pub fn create_manim_arrow(
        &self,
        candidate: WasmManimArrowOptions,
    ) -> Result<WasmAuthoringArrowHandle, JsValue> {
        match candidate.request {
            ArrowRequest::Arrow(options) => {
                noon::ManimArrow::create(Rc::clone(&self.semantics), options)
                    .map(|arrow| WasmAuthoringArrowHandle {
                        published: PublishedArrowRequest::Arrow(arrow),
                    })
                    .map_err(js_error)
            }
            ArrowRequest::VectorField(draft) => {
                publish_vector_field(Rc::clone(&self.semantics), draft).map(|field| {
                    WasmAuthoringArrowHandle {
                        published: PublishedArrowRequest::VectorField(field),
                    }
                })
            }
        }
    }
}

fn publish_vector_field(
    semantics: Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    draft: VectorFieldDraft,
) -> Result<noon::ManimArrowVectorField, JsValue> {
    let vectors = complete_vectors(&draft)?;
    let VectorFieldDraft {
        ranges,
        points,
        vectors: _,
        display_lengths,
        coloring,
    } = draft;

    match (display_lengths, coloring) {
        (None, VectorFieldColoringDraft::Default) => {
            let cursor = Cell::new(0usize);
            noon::ManimArrowVectorField::create(
                semantics,
                |point| prepared_vector(&cursor, &points, &vectors, point),
                ranges,
            )
            .map_err(vector_field_authoring_error)
        }
        (Some(lengths), VectorFieldColoringDraft::Default) => {
            validate_custom_lengths(&vectors, &lengths)?;
            let cursor = Cell::new(0usize);
            noon::ManimArrowVectorField::create_with_length(
                semantics,
                |point| prepared_vector(&cursor, &points, &vectors, point),
                ranges,
                |_| prepared_length(&cursor, &lengths),
            )
            .map_err(vector_field_authoring_error)
        }
        (None, VectorFieldColoringDraft::Single(color)) => {
            let cursor = Cell::new(0usize);
            noon::ManimArrowVectorField::create_with_color(
                semantics,
                |point| prepared_vector(&cursor, &points, &vectors, point),
                ranges,
                color,
            )
            .map_err(vector_field_authoring_error)
        }
        (Some(lengths), VectorFieldColoringDraft::Single(color)) => {
            validate_custom_lengths(&vectors, &lengths)?;
            let cursor = Cell::new(0usize);
            noon::ManimArrowVectorField::create_with_length_and_color(
                semantics,
                |point| prepared_vector(&cursor, &points, &vectors, point),
                ranges,
                |_| prepared_length(&cursor, &lengths),
                color,
            )
            .map_err(vector_field_authoring_error)
        }
        (
            None,
            VectorFieldColoringDraft::Gradient {
                colors,
                min,
                max,
                values: None,
            },
        ) => {
            let cursor = Cell::new(0usize);
            noon::ManimArrowVectorField::create_with_gradient(
                semantics,
                |point| prepared_vector(&cursor, &points, &vectors, point),
                ranges,
                &colors,
                min,
                max,
            )
            .map_err(vector_field_authoring_error)
        }
        (
            Some(lengths),
            VectorFieldColoringDraft::Gradient {
                colors,
                min,
                max,
                values: None,
            },
        ) => {
            validate_custom_lengths(&vectors, &lengths)?;
            let cursor = Cell::new(0usize);
            noon::ManimArrowVectorField::create_with_length_and_gradient(
                semantics,
                |point| prepared_vector(&cursor, &points, &vectors, point),
                ranges,
                |_| prepared_length(&cursor, &lengths),
                &colors,
                min,
                max,
            )
            .map_err(vector_field_authoring_error)
        }
        (
            None,
            VectorFieldColoringDraft::Gradient {
                colors,
                min,
                max,
                values: Some(values),
            },
        ) => {
            validate_custom_color_values(&values)?;
            let cursor = Cell::new(0usize);
            let color_cursor = Cell::new(0usize);
            noon::ManimArrowVectorField::create_with_color_scheme(
                semantics,
                |point| prepared_vector(&cursor, &points, &vectors, point),
                ranges,
                &colors,
                min,
                max,
                |_| prepared_color_value(&color_cursor, &values),
            )
            .map_err(vector_field_authoring_error)
        }
        (
            Some(lengths),
            VectorFieldColoringDraft::Gradient {
                colors,
                min,
                max,
                values: Some(values),
            },
        ) => {
            validate_custom_lengths(&vectors, &lengths)?;
            validate_custom_color_values(&values)?;
            let cursor = Cell::new(0usize);
            let color_cursor = Cell::new(0usize);
            noon::ManimArrowVectorField::create_with_length_and_color_scheme(
                semantics,
                |point| prepared_vector(&cursor, &points, &vectors, point),
                ranges,
                |_| prepared_length(&cursor, &lengths),
                &colors,
                min,
                max,
                |_| prepared_color_value(&color_cursor, &values),
            )
            .map_err(vector_field_authoring_error)
        }
    }
}

fn prepared_vector(
    cursor: &Cell<usize>,
    points: &[noon::VectorFieldPoint],
    vectors: &[noon::VectorFieldPoint],
    point: noon::VectorFieldPoint,
) -> noon::VectorFieldPoint {
    let index = cursor.get();
    cursor.set(index + 1);
    debug_assert_eq!(points[index], point);
    vectors[index]
}

fn prepared_length(cursor: &Cell<usize>, lengths: &[Option<f64>]) -> f64 {
    let index = cursor.get() - 1;
    lengths[index].expect("custom non-zero vector length was preflighted")
}

fn prepared_color_value(cursor: &Cell<usize>, values: &[Option<f64>]) -> f64 {
    let index = cursor.get();
    cursor.set(index + 1);
    values[index].expect("custom color value was preflighted")
}

fn complete_vectors(draft: &VectorFieldDraft) -> Result<Vec<noon::VectorFieldPoint>, JsValue> {
    draft
        .vectors
        .iter()
        .enumerate()
        .map(|(index, value)| {
            value.ok_or_else(|| {
                invalid_input(
                    "vector_field.missing_vector",
                    format!("vector-field sample {index} has no callback result"),
                )
            })
        })
        .collect()
}

fn validate_custom_lengths(
    vectors: &[noon::VectorFieldPoint],
    lengths: &[Option<f64>],
) -> Result<(), JsValue> {
    for (index, (vector, length)) in vectors.iter().zip(lengths).enumerate() {
        if vector.length() != 0.0 && length.is_none() {
            return Err(invalid_input(
                "vector_field.missing_length",
                format!("vector-field sample {index} has no custom display length"),
            ));
        }
    }
    Ok(())
}

fn validate_custom_color_values(values: &[Option<f64>]) -> Result<(), JsValue> {
    for (index, value) in values.iter().enumerate() {
        if value.is_none() {
            return Err(invalid_input(
                "vector_field.missing_color_value",
                format!("vector-field sample {index} has no custom color-scheme value"),
            ));
        }
    }
    Ok(())
}

fn vector_field_planning_error(error: noon::StaticVectorFieldError) -> JsValue {
    use noon::StaticVectorFieldError as E;
    let code = match error {
        E::InvalidRange { .. } => "vector_field.invalid_range",
        E::SampleCountOverflow => "vector_field.sample_count_overflow",
        E::NonFiniteFieldOutput { .. } => "vector_field.non_finite_output",
        E::NonFiniteDisplayedLength { .. } => "vector_field.non_finite_length",
    };
    js_error(AuthoringFailure::new("invalid_input", code, error))
}

fn vector_field_authoring_error(error: noon::ArrowVectorFieldAuthoringError) -> JsValue {
    match error {
        noon::ArrowVectorFieldAuthoringError::Planning(cause) => vector_field_planning_error(cause),
        noon::ArrowVectorFieldAuthoringError::Authoring(cause) => js_error(cause),
        noon::ArrowVectorFieldAuthoringError::InvalidColorConfiguration(reason) => {
            js_error(AuthoringFailure::new(
                "invalid_input",
                "vector_field.invalid_color_configuration",
                reason,
            ))
        }
        noon::ArrowVectorFieldAuthoringError::NonFiniteColorSchemeOutput { sample_index } => {
            js_error(AuthoringFailure::new(
                "invalid_input",
                "vector_field.non_finite_color_scheme",
                format!("vector-field color scheme returned a non-finite value at sample {sample_index}"),
            ))
        }
    }
}

fn invalid_input(code: &'static str, message: impl ToString) -> JsValue {
    js_error(AuthoringFailure::new("invalid_input", code, message))
}

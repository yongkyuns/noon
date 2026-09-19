//! Thin WASM handles for shared Rust SampleSpace construction and partitions.

use crate::authoring_error::{js_error, AuthoringFailure};
use crate::{
    CanonicalAuthoringSceneContext, WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle,
    WasmAuthoringStore,
};
use noon::{Color, SampleSpace, SampleSpaceOptions};
use std::fmt::Display;
use wasm_bindgen::prelude::*;

fn failure(error: impl Display) -> JsValue {
    js_error(AuthoringFailure::new(
        "invalid_input",
        "sample_space.authoring",
        error.to_string(),
    ))
}

/// Inert construction parameters. Semantic identity is allocated only by the
/// consuming `WasmAuthoringStore.createSampleSpace` operation.
#[wasm_bindgen]
pub struct WasmSampleSpaceOptions {
    pub(crate) options: SampleSpaceOptions,
}

#[wasm_bindgen]
impl WasmSampleSpaceOptions {
    #[wasm_bindgen(constructor)]
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            options: SampleSpaceOptions {
                width,
                height,
                ..Default::default()
            },
        }
    }

    #[wasm_bindgen(js_name = setFillColor)]
    pub fn set_fill_color(&mut self, red: f64, green: f64, blue: f64) -> Result<(), JsValue> {
        self.options.fill_color = family_color(red, green, blue)?;
        Ok(())
    }

    #[wasm_bindgen(js_name = setFillOpacity)]
    pub fn set_fill_opacity(&mut self, opacity: f64) {
        self.options.fill_opacity = opacity;
    }

    #[wasm_bindgen(js_name = setStrokeColor)]
    pub fn set_stroke_color(&mut self, red: f64, green: f64, blue: f64) -> Result<(), JsValue> {
        self.options.stroke_color = family_color(red, green, blue)?;
        Ok(())
    }

    #[wasm_bindgen(js_name = setStrokeWidth)]
    pub fn set_stroke_width(&mut self, width: f64) {
        self.options.stroke_width = width;
    }
}

fn family_color(red: f64, green: f64, blue: f64) -> Result<Color, JsValue> {
    crate::authoring_mobject::family_color(true, red, green, blue, 1.0)
        .map_err(failure)?
        .ok_or_else(|| failure("SampleSpace colors cannot be disabled"))
}

#[wasm_bindgen]
pub struct WasmSampleSpaceHandle {
    pub(crate) sample_space: SampleSpace,
}

#[wasm_bindgen]
impl WasmAuthoringStore {
    #[wasm_bindgen(js_name = createSampleSpace)]
    pub fn create_sample_space(
        &self,
        options: WasmSampleSpaceOptions,
    ) -> Result<WasmSampleSpaceHandle, JsValue> {
        SampleSpace::detached(self.semantics.clone(), &options.options)
            .map(|sample_space| WasmSampleSpaceHandle { sample_space })
            .map_err(failure)
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = liveCreateSampleSpace)]
    pub fn live_create_sample_space(
        &mut self,
        options: WasmSampleSpaceOptions,
    ) -> Result<WasmSampleSpaceHandle, JsValue> {
        self.inner
            .live_create_sample_space(&options.options)
            .map(WasmSampleSpaceHandle::from_sample_space)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = liveGetHorizontalDivision)]
    pub fn live_get_horizontal_division(
        &mut self,
        sample_space: &WasmSampleSpaceHandle,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.inner
            .live_get_sample_space_division(
                &sample_space.sample_space,
                probabilities,
                &colors,
                false,
            )
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = liveGetVerticalDivision)]
    pub fn live_get_vertical_division(
        &mut self,
        sample_space: &WasmSampleSpaceHandle,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.inner
            .live_get_sample_space_division(
                &sample_space.sample_space,
                probabilities,
                &colors,
                true,
            )
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = liveDivideHorizontally)]
    pub fn live_divide_horizontally(
        &mut self,
        sample_space: &mut WasmSampleSpaceHandle,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.inner
            .live_divide_sample_space(
                &mut sample_space.sample_space,
                probabilities,
                &colors,
                false,
            )
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_error::js_error)
    }

    #[wasm_bindgen(js_name = liveDivideVertically)]
    pub fn live_divide_vertically(
        &mut self,
        sample_space: &mut WasmSampleSpaceHandle,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.inner
            .live_divide_sample_space(&mut sample_space.sample_space, probabilities, &colors, true)
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(crate::authoring_error::js_error)
    }
}

#[wasm_bindgen]
impl WasmSampleSpaceHandle {
    pub(crate) fn from_sample_space(sample_space: SampleSpace) -> Self {
        Self { sample_space }
    }

    pub(crate) fn sample_space_mut(&mut self) -> &mut SampleSpace {
        &mut self.sample_space
    }

    pub fn family(&self) -> WasmAuthoringFamilyHandle {
        WasmAuthoringFamilyHandle::from_semantic_family(self.sample_space.family().clone())
    }

    pub fn rectangle(&self) -> WasmAuthoringMobjectHandle {
        WasmAuthoringMobjectHandle::from_semantic_mobject(self.sample_space.rectangle().clone())
    }

    #[wasm_bindgen(js_name = horizontalParts)]
    pub fn horizontal_parts(&self) -> Result<Option<WasmAuthoringFamilyHandle>, JsValue> {
        self.sample_space
            .horizontal_parts()
            .map(|family| family.map(WasmAuthoringFamilyHandle::from_semantic_family))
            .map_err(failure)
    }

    #[wasm_bindgen(js_name = verticalParts)]
    pub fn vertical_parts(&self) -> Result<Option<WasmAuthoringFamilyHandle>, JsValue> {
        self.sample_space
            .vertical_parts()
            .map(|family| family.map(WasmAuthoringFamilyHandle::from_semantic_family))
            .map_err(failure)
    }

    #[wasm_bindgen(js_name = completePList)]
    pub fn complete_p_list(&self, probabilities: &[f64]) -> Result<Vec<f64>, JsValue> {
        SampleSpace::complete_p_list(probabilities.iter().copied()).map_err(failure)
    }

    #[wasm_bindgen(js_name = getHorizontalDivision)]
    pub fn get_horizontal_division(
        &self,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.sample_space
            .get_horizontal_division(probabilities.iter().copied(), &colors)
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(failure)
    }

    #[wasm_bindgen(js_name = getVerticalDivision)]
    pub fn get_vertical_division(
        &self,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.sample_space
            .get_vertical_division(probabilities.iter().copied(), &colors)
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(failure)
    }

    #[wasm_bindgen(js_name = divideHorizontally)]
    pub fn divide_horizontally(
        &mut self,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.sample_space
            .divide_horizontally_detached(probabilities.iter().copied(), &colors)
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(failure)
    }

    #[wasm_bindgen(js_name = divideVertically)]
    pub fn divide_vertically(
        &mut self,
        probabilities: &[f64],
        colors: &[f64],
    ) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let colors = colors_from_rgba(colors)?;
        self.sample_space
            .divide_vertically_detached(probabilities.iter().copied(), &colors)
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(failure)
    }
}

#[wasm_bindgen]
impl WasmAuthoringFamilyHandle {
    #[wasm_bindgen(js_name = memberIsFamily)]
    pub fn member_is_family(&self, index: usize) -> Result<bool, JsValue> {
        let family = self.semantic_family()?;
        let store = family.integration_store();
        let member = store
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .map_err(failure)?
            .get(index)
            .copied()
            .ok_or_else(|| failure("family member index is out of bounds"))?;
        Ok(store.borrow().semantic_family_checked(member).is_ok())
    }

    #[wasm_bindgen(js_name = memberFamily)]
    pub fn member_family(&self, index: usize) -> Result<WasmAuthoringFamilyHandle, JsValue> {
        let family = self.semantic_family()?;
        let store = family.integration_store();
        let member = store
            .borrow()
            .semantic_family_members_checked(family.node_id())
            .map_err(failure)?
            .get(index)
            .copied()
            .ok_or_else(|| failure("family member index is out of bounds"))?;
        noon::MobjectFamily::from_node(std::rc::Rc::clone(store), member)
            .map(WasmAuthoringFamilyHandle::from_semantic_family)
            .map_err(failure)
    }

    #[wasm_bindgen(js_name = asSampleSpace)]
    pub fn as_sample_space(&self) -> Result<WasmSampleSpaceHandle, JsValue> {
        SampleSpace::from_family(self.semantic_family()?)
            .map(WasmSampleSpaceHandle::from_sample_space)
            .map_err(failure)
    }
}

fn colors_from_rgba(values: &[f64]) -> Result<Vec<Color>, JsValue> {
    if values.is_empty() || !values.len().is_multiple_of(4) {
        return Err(failure(
            "partition colors must contain one or more RGBA values",
        ));
    }
    values
        .chunks_exact(4)
        .map(|rgba| {
            crate::authoring_mobject::family_color(true, rgba[0], rgba[1], rgba[2], rgba[3])
                .map_err(failure)?
                .ok_or_else(|| failure("partition colors cannot be disabled"))
        })
        .collect()
}

/// The direct Rust SampleSpace example also runs unchanged inside the browser's
/// single WASM execution context.
#[cfg(all(
    feature = "renderer",
    any(debug_assertions, feature = "renderer-smoke")
))]
#[wasm_bindgen(js_name = createSampleSpaceRenderer)]
pub async fn create_sample_space_renderer(
    canvas: web_sys::OffscreenCanvas,
) -> Result<crate::WasmExecutionCanvasRenderer, JsValue> {
    let session = noon::example_scenes::sample_space::session().map_err(failure)?;
    crate::WasmExecutionCanvasRenderer::create_from_execution_session(canvas, session).await
}

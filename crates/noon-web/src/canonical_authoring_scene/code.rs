use super::wasm::CanonicalAuthoringSceneContext;
use crate::authoring_code::{line_index, line_range};
use crate::{
    authoring_error::js_error, WasmAuthoringMobjectHandle, WasmCodeHandle,
    WasmCodeHighlightOptions, WasmCodeLineReplacement, WasmCodeOptions,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = liveReplaceCodeLine)]
    pub fn live_replace_code_line(
        &mut self,
        code: &WasmCodeHandle,
        index: JsValue,
        source: &str,
    ) -> Result<WasmCodeLineReplacement, JsValue> {
        let index = line_index("index", &index)?;
        if self.inner.player_ownership.is_unstarted() {
            code.code
                .replace_line(index, source)
                .map(WasmCodeLineReplacement::new)
                .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_replace_code_line(&code.code, index, source)
                .map(WasmCodeLineReplacement::new)
                .map_err(js_error)
        }
    }

    #[wasm_bindgen(js_name = liveGetHighlightedCodeLines)]
    pub fn live_get_highlighted_code_lines(
        &mut self,
        code: &WasmCodeHandle,
        start: JsValue,
        stop: JsValue,
        options: WasmCodeHighlightOptions,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        let lines = line_range(&start, &stop)?;
        if self.inner.player_ownership.is_unstarted() {
            code.code
                .get_highlighted_lines(lines, options.options)
                .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_get_highlighted_code_lines(&code.code, lines, options.options)
                .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        }
    }

    #[wasm_bindgen(js_name = liveHighlightCodeLines)]
    pub fn live_highlight_code_lines(
        &mut self,
        code: &WasmCodeHandle,
        start: JsValue,
        stop: JsValue,
        options: WasmCodeHighlightOptions,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        let lines = line_range(&start, &stop)?;
        if self.inner.player_ownership.is_unstarted() {
            code.code
                .highlight_lines(lines, options.options)
                .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_highlight_code_lines(&code.code, lines, options.options)
                .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
                .map_err(js_error)
        }
    }

    #[wasm_bindgen(js_name = liveCreateCode)]
    pub fn live_create_code(
        &mut self,
        source: &str,
        options: WasmCodeOptions,
    ) -> Result<WasmCodeHandle, JsValue> {
        if self.inner.player_ownership.is_unstarted() {
            noon::Code::new(&mut self.inner.scene, source, options.options)
                .map(WasmCodeHandle::new)
                .map_err(js_error)
        } else {
            self.inner
                .active_live_player()
                .map_err(js_error)?
                .live_create_code(source, options.options)
                .map(WasmCodeHandle::new)
                .map_err(js_error)
        }
    }
}

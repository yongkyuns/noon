from pathlib import Path

path = Path("scripts/tmp_shared_family_translation_migration.py")
text = path.read_text()

start = text.index("layout_helper_anchor =")
end = text.index("rust = replace_once(\n    rust,\n    layout_helper_anchor,", start)
block = r'''layout_helper_anchor = ''' + "'''" + r'''        fn include_bounds(&mut self, bounds: Bounds2D64) {
            if let Some(total) = &mut self.bounds {
                total.include(bounds.min_x, bounds.min_y);
                total.include(bounds.max_x, bounds.max_y);
            } else {
                self.bounds = Some(bounds);
            }
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyLayout {
''' + "'''" + r'''
layout_helper_replacement = ''' + "'''" + r'''        fn include_bounds(&mut self, bounds: Bounds2D64) {
            if let Some(total) = &mut self.bounds {
                total.include(bounds.min_x, bounds.min_y);
                total.include(bounds.max_x, bounds.max_y);
            } else {
                self.bounds = Some(bounds);
            }
        }

        fn translation(
            &self,
            delta_x: f64,
            delta_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            let translation = FrontendFamilyTranslation::from_members(
                self.expected_leaves.clone(),
                delta_x,
                delta_y,
            )
            .map_err(js_error)?;
            Ok(WasmAuthoringFamilyTranslation {
                semantics: Rc::clone(&self.semantics),
                translation,
            })
        }

        fn validate_target_mobject(
            &self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let store = member.1.as_ref().ok_or_else(|| {
                JsValue::from_str(
                    "family placement target is not attached to a shared authoring store",
                )
            })?;
            if !Rc::ptr_eq(&self.semantics, store) {
                return Err(JsValue::from_str(
                    "family placement source and target belong to different authoring stores",
                ));
            }
            if member.2.is_none() {
                return Err(JsValue::from_str(
                    "family placement target has no semantic identity",
                ));
            }
            Ok(())
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyLayout {
''' + "'''" + "\n"

text = text[:start] + block + text[end:]
text = text.replace(
    "        semantic_family_leaf_ids, Bounds2D64, FrontendFamilyTargetEditor,\\n",
    "        semantic_family_leaf_ids, semantic_xy_f64, Bounds2D64, FrontendFamilyTargetEditor,\\n",
    1,
)
path.write_text(text)

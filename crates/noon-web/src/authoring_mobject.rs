use noon::semantic_mobject::{
    authoring_render_f64 as render_f64, authoring_xy_f64 as semantic_xy_f64,
};
pub use noon::semantic_mobject::{ManimNextToArgs, Mobject};
use noon_core::{
    Bounds2D64, SemanticMutationTransaction, SemanticNodeCreation, SemanticNodeId,
    SemanticNodeKind, SemanticStore,
};

/// Shared target-family construction used by frontend Group/VGroup animation builders.
///
/// The Python/JS wrapper tree is host-language identity metadata only. This editor
/// snapshots the source family's authoritative ordered membership, validates each
/// wrapper pair against that order, and constructs the target family in the same
/// semantic store. Leaf target state is edited through `Mobject`.
#[derive(Clone, Debug)]
pub struct FrontendFamilyTargetEditor {
    source_members: Vec<SemanticNodeId>,
    target_members: Vec<SemanticNodeId>,
    next_index: usize,
}

impl FrontendFamilyTargetEditor {
    pub fn begin(store: &SemanticStore, source: SemanticNodeId) -> Result<Self, String> {
        let source_members = {
            let source_node = store
                .node(source)
                .ok_or_else(|| format!("unknown source family semantic node {source:?}"))?;
            if !matches!(source_node.kind(), SemanticNodeKind::Family) {
                return Err(format!("source semantic node {source:?} is not a family"));
            }
            source_node.members().to_vec()
        };
        Ok(Self {
            source_members,
            target_members: Vec::new(),
            next_index: 0,
        })
    }

    pub fn accept_member(
        &mut self,
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
        self.target_members.push(target_member);
        self.next_index += 1;
        Ok(())
    }

    pub fn target_members(&self) -> Result<&[SemanticNodeId], String> {
        if self.next_index != self.source_members.len() {
            return Err(format!(
                "family target editor is incomplete: accepted {} of {} members",
                self.next_index,
                self.source_members.len()
            ));
        }
        Ok(&self.target_members)
    }

    pub fn finish(&self, store: &mut SemanticStore) -> Result<SemanticNodeId, String> {
        let members = self.target_members()?;
        let mut transaction = SemanticMutationTransaction::new();
        let family = transaction.create_node(SemanticNodeCreation::family());
        for &member in members {
            transaction.add_member(family, member);
        }
        let result = transaction
            .apply(store)
            .map_err(|error| error.to_string())?;
        let node = result
            .resolve(family)
            .expect("committed target family token resolves to one semantic identity");
        Ok(node)
    }
}

/// Ordered family translation over authoritative shared semantic leaf identity.
///
/// Frontends may retain wrapper trees for language-level identity, but the shared
/// semantic family decides which leaves are mutated and in what order. The delta is
/// validated once in Rust and then applied directly to each shared leaf handle.
#[derive(Clone, Debug)]
pub struct FrontendFamilyTranslation {
    source_members: Vec<SemanticNodeId>,
    next_index: usize,
    delta: (f64, f64),
}

impl FrontendFamilyTranslation {
    pub fn begin(
        store: &SemanticStore,
        source: SemanticNodeId,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<Self, String> {
        let source_members = semantic_family_leaf_ids(store, source)?;
        Self::from_members(source_members, delta_x, delta_y)
    }

    fn from_members(
        source_members: Vec<SemanticNodeId>,
        delta_x: f64,
        delta_y: f64,
    ) -> Result<Self, String> {
        let delta = semantic_xy_f64(delta_x, delta_y)?;
        Ok(Self {
            source_members,
            next_index: 0,
            delta: (delta.x, delta.y),
        })
    }

    fn apply_with<F>(&mut self, source_member: SemanticNodeId, apply: F) -> Result<(), String>
    where
        F: FnOnce((f64, f64)) -> Result<(), String>,
    {
        let expected = self
            .source_members
            .get(self.next_index)
            .copied()
            .ok_or_else(|| "family translation has no remaining leaves".to_owned())?;
        if source_member != expected {
            return Err(format!(
                "family translation leaf mismatch at index {}: expected {expected:?}, got {source_member:?}",
                self.next_index
            ));
        }
        apply(self.delta)?;
        self.next_index += 1;
        Ok(())
    }

    pub fn apply(
        &mut self,
        source_member: SemanticNodeId,
        member: &mut Mobject,
    ) -> Result<(), String> {
        self.apply_with(source_member, |delta| member.shift(delta.0, delta.1))
    }

    pub fn finish(&self) -> Result<(), String> {
        if self.next_index != self.source_members.len() {
            return Err(format!(
                "family translation is incomplete: applied {} of {} leaves",
                self.next_index,
                self.source_members.len()
            ));
        }
        Ok(())
    }
}

/// Shared Manim family arrangement over authoritative direct-member identity.
///
/// The semantic store snapshots direct membership/order and recursively resolves the
/// leaf identities each direct member owns. Frontends only feed live shared bounds
/// for those members in the validated order; all sequencing, buffer math, optional
/// recentering, and resulting per-member translations are computed here.
#[derive(Clone, Debug)]
pub struct FrontendFamilyArrangePlan {
    members: Vec<FrontendFamilyArrangeMember>,
    next_member: usize,
}

#[derive(Clone, Debug)]
struct FrontendFamilyArrangeMember {
    id: SemanticNodeId,
    leaves: Vec<SemanticNodeId>,
    bounds: Option<Bounds2D64>,
}

impl FrontendFamilyArrangePlan {
    pub fn begin(store: &SemanticStore, source: SemanticNodeId) -> Result<Self, String> {
        let direct_members = {
            let source_node = store
                .node(source)
                .ok_or_else(|| format!("unknown family semantic node {source:?}"))?;
            if !matches!(source_node.kind(), SemanticNodeKind::Family) {
                return Err(format!("semantic node {source:?} is not a family"));
            }
            source_node.members().to_vec()
        };

        let mut members = Vec::with_capacity(direct_members.len());
        for id in direct_members {
            let node = store
                .node(id)
                .ok_or_else(|| format!("unknown family arrange member {id:?}"))?;
            let leaves = match node.kind() {
                SemanticNodeKind::AuthoringObject => vec![id],
                SemanticNodeKind::Family => semantic_family_leaf_ids(store, id)?,
                SemanticNodeKind::Object(_)
                | SemanticNodeKind::Signal(_)
                | SemanticNodeKind::Animation(_) => {
                    return Err(format!(
                        "family arrange member {id:?} is not an authoring object"
                    ));
                }
            };
            members.push(FrontendFamilyArrangeMember {
                id,
                leaves,
                bounds: None,
            });
        }
        Ok(Self {
            members,
            next_member: 0,
        })
    }

    pub fn accept_member_bounds(
        &mut self,
        member: SemanticNodeId,
        bounds: Option<Bounds2D64>,
    ) -> Result<(), String> {
        let expected = self
            .members
            .get(self.next_member)
            .ok_or_else(|| "family arrange received too many direct members".to_owned())?;
        if expected.id != member {
            return Err(format!(
                "family arrange member mismatch at index {}: expected {:?}, got {member:?}",
                self.next_member, expected.id
            ));
        }
        self.members[self.next_member].bounds = bounds;
        self.next_member += 1;
        Ok(())
    }

    pub fn ensure_complete(&self) -> Result<(), String> {
        if self.next_member != self.members.len() {
            return Err(format!(
                "family arrange is incomplete: accepted {} of {} direct members",
                self.next_member,
                self.members.len()
            ));
        }
        Ok(())
    }

    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    pub fn finish(
        &self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        center: bool,
    ) -> Result<Vec<FrontendFamilyTranslation>, String> {
        self.ensure_complete()?;
        let bounds = self
            .members
            .iter()
            .map(|member| member.bounds)
            .collect::<Vec<_>>();
        let deltas = manim_family_arrange_deltas(&bounds, direction_x, direction_y, buff, center)?;
        self.members
            .iter()
            .zip(deltas)
            .map(|(member, delta)| {
                FrontendFamilyTranslation::from_members(member.leaves.clone(), delta.0, delta.1)
            })
            .collect()
    }
}

fn semantic_family_leaf_ids(
    store: &SemanticStore,
    family: SemanticNodeId,
) -> Result<Vec<SemanticNodeId>, String> {
    fn collect(
        store: &SemanticStore,
        node_id: SemanticNodeId,
        leaves: &mut Vec<SemanticNodeId>,
    ) -> Result<(), String> {
        let node = store
            .node(node_id)
            .ok_or_else(|| format!("unknown semantic family member {node_id:?}"))?;
        match node.kind() {
            SemanticNodeKind::AuthoringObject => {
                leaves.push(node_id);
                Ok(())
            }
            SemanticNodeKind::Family => {
                for member in node.members() {
                    collect(store, member, leaves)?;
                }
                Ok(())
            }
            SemanticNodeKind::Object(_)
            | SemanticNodeKind::Signal(_)
            | SemanticNodeKind::Animation(_) => Err(format!(
                "family layout member {node_id:?} is not an authoring object"
            )),
        }
    }

    let root = store
        .node(family)
        .ok_or_else(|| format!("unknown family semantic node {family:?}"))?;
    if !matches!(root.kind(), SemanticNodeKind::Family) {
        return Err(format!("semantic node {family:?} is not a family"));
    }

    let mut leaves = Vec::new();
    collect(store, family, &mut leaves)?;
    Ok(leaves)
}

fn manim_family_arrange_deltas(
    member_bounds: &[Option<Bounds2D64>],
    direction_x: f64,
    direction_y: f64,
    buff: f64,
    center: bool,
) -> Result<Vec<(f64, f64)>, String> {
    let direction = semantic_xy_f64(direction_x, direction_y)?;
    let buff = render_f64("buffer", buff)?;
    if member_bounds.is_empty() {
        return Ok(Vec::new());
    }

    let critical = |bounds: Option<Bounds2D64>, x: f64, y: f64| -> (f64, f64) {
        let Some(bounds) = bounds else {
            return (0.0, 0.0);
        };
        let center_x = (bounds.min_x + bounds.max_x) * 0.5;
        let center_y = (bounds.min_y + bounds.max_y) * 0.5;
        (
            if x < 0.0 {
                bounds.min_x
            } else if x > 0.0 {
                bounds.max_x
            } else {
                center_x
            },
            if y < 0.0 {
                bounds.min_y
            } else if y > 0.0 {
                bounds.max_y
            } else {
                center_y
            },
        )
    };

    let mut deltas = vec![(0.0, 0.0); member_bounds.len()];
    for index in 1..member_bounds.len() {
        let source = critical(member_bounds[index], -direction.x, -direction.y);
        let previous = critical(member_bounds[index - 1], direction.x, direction.y);
        deltas[index] = (
            previous.0 + deltas[index - 1].0 - source.0 + direction.x * buff,
            previous.1 + deltas[index - 1].1 - source.1 + direction.y * buff,
        );
    }

    if center {
        let mut arranged_bounds: Option<Bounds2D64> = None;
        for (bounds, delta) in member_bounds.iter().zip(&deltas) {
            let Some(bounds) = bounds else {
                continue;
            };
            let shifted = Bounds2D64 {
                min_x: bounds.min_x + delta.0,
                min_y: bounds.min_y + delta.1,
                max_x: bounds.max_x + delta.0,
                max_y: bounds.max_y + delta.1,
            };
            if let Some(total) = &mut arranged_bounds {
                total.include(shifted.min_x, shifted.min_y);
                total.include(shifted.max_x, shifted.max_y);
            } else {
                arranged_bounds = Some(shifted);
            }
        }
        if let Some(bounds) = arranged_bounds {
            let center_x = (bounds.min_x + bounds.max_x) * 0.5;
            let center_y = (bounds.min_y + bounds.max_y) * 0.5;
            for delta in &mut deltas {
                delta.0 -= center_x;
                delta.1 -= center_y;
            }
        }
    }

    Ok(deltas)
}

#[cfg(any(target_arch = "wasm32", test))]
fn manim_family_next_to_delta(
    source: (f64, f64),
    target: (f64, f64),
    direction: (f64, f64),
    buff: f64,
    mask: (f64, f64),
) -> Result<(f64, f64), String> {
    let direction = semantic_xy_f64(direction.0, direction.1)?;
    let mask = semantic_xy_f64(mask.0, mask.1)?;
    let buff = render_f64("buffer", buff)?;
    Ok((
        (target.0 - source.0 + direction.x * buff) * mask.x,
        (target.1 - source.1 + direction.y * buff) * mask.y,
    ))
}

#[cfg(any(target_arch = "wasm32", test))]
fn manim_family_align_to_delta(
    source: (f64, f64),
    target: (f64, f64),
    axis: (f64, f64),
) -> Result<(f64, f64), String> {
    let axis = semantic_xy_f64(axis.0, axis.1)?;
    Ok((
        if axis.x != 0.0 {
            target.0 - source.0
        } else {
            0.0
        },
        if axis.y != 0.0 {
            target.1 - source.1
        } else {
            0.0
        },
    ))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::{cell::RefCell, rc::Rc};

    use wasm_bindgen::prelude::*;

    use crate::{AuthoringSemanticIdentity, WasmRetainedNativeTextAuthoringHandle};

    use super::{
        manim_family_align_to_delta, manim_family_next_to_delta, render_f64,
        semantic_family_leaf_ids, semantic_xy_f64, Bounds2D64, FrontendFamilyArrangePlan,
        FrontendFamilyTargetEditor, FrontendFamilyTranslation, ManimNextToArgs, Mobject,
        SemanticNodeId, SemanticStore,
    };

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    fn text_authoring_f32(field: &str, value: f64) -> Result<f32, JsValue> {
        let value = render_f64(field, value).map_err(js_error)?;
        let value = value as f32;
        if !value.is_finite() {
            return Err(js_error(format!("{field} is outside the supported range")));
        }
        Ok(value)
    }

    type SharedSemanticStore = Rc<RefCell<SemanticStore>>;

    fn retained_native_member_id(
        semantics: &SharedSemanticStore,
        member: &WasmAuthoringFamilyMemberHandle,
        text: &WasmRetainedNativeTextAuthoringHandle,
        context: &str,
    ) -> Result<SemanticNodeId, JsValue> {
        if !Rc::ptr_eq(semantics, &member.semantics) {
            return Err(JsValue::from_str(&format!(
                "{context} and retained member belong to different authoring stores"
            )));
        }
        let member_identity = member.authoring_identity();
        if !text
            .family_identity()
            .is_some_and(|identity| identity.matches(&member_identity))
        {
            return Err(JsValue::from_str(&format!(
                "{context} retained member identity does not match retained native Text handle"
            )));
        }
        Ok(member.id)
    }

    fn retained_native_critical_point(
        text: &WasmRetainedNativeTextAuthoringHandle,
        direction: (f64, f64),
    ) -> (f64, f64) {
        let bounds = text.family_layout_bounds();
        let center = (
            (bounds.min_x + bounds.max_x) * 0.5,
            (bounds.min_y + bounds.max_y) * 0.5,
        );
        (
            if direction.0 < 0.0 {
                bounds.min_x
            } else if direction.0 > 0.0 {
                bounds.max_x
            } else {
                center.0
            },
            if direction.1 < 0.0 {
                bounds.min_y
            } else if direction.1 > 0.0 {
                bounds.max_y
            } else {
                center.1
            },
        )
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringStore {
        semantics: SharedSemanticStore,
    }

    /// Content-agnostic semantic identity for authoring objects whose mutable
    /// presentation state is owned by another retained/resource-specific handle.
    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyMemberHandle {
        semantics: SharedSemanticStore,
        id: SemanticNodeId,
    }

    impl WasmAuthoringFamilyMemberHandle {
        pub(crate) fn authoring_identity(&self) -> AuthoringSemanticIdentity {
            AuthoringSemanticIdentity::from_shared_store(&self.semantics, self.id)
        }
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

        #[wasm_bindgen(js_name = createMobject)]
        pub fn create_mobject(
            &self,
            snapshot_json: &str,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            let snapshot = serde_json::from_str(snapshot_json)
                .map_err(|error| js_error(format!("invalid mobject snapshot: {error}")))?;
            let handle =
                noon::legacy::import_mobject_snapshot(Rc::clone(&self.semantics), snapshot)
                    .map_err(js_error)?;
            Ok(WasmAuthoringMobjectHandle { handle })
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
            let font_size = text_authoring_f32("font size", font_size)?;
            let line_spacing = text_authoring_f32("line spacing", line_spacing)?;
            if line_spacing != -1.0 && line_spacing <= -1.0 {
                return Err(js_error(
                    "line spacing must be -1 or greater than -1".to_owned(),
                ));
            }
            let text = noon::Text::new(source)
                .with_font(font_family)
                .with_font_size(font_size)
                .with_line_spacing(line_spacing);
            Mobject::from_text(Rc::clone(&self.semantics), text)
                .map(|handle| WasmAuthoringMobjectHandle { handle })
                .map_err(|error| js_error(error.to_string()))
        }

        /// Compile Typst or MathTypst into the same semantic store as geometry handles.
        #[wasm_bindgen(js_name = createManimTypst)]
        pub fn create_manim_typst(
            &self,
            source: &str,
            math: bool,
            font_size: f64,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            let font_size = text_authoring_f32("font size", font_size)?;
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
                .map_err(|error| js_error(error.to_string()))
        }

        /// Allocate stable semantic identity for a non-geometry authoring object.
        #[wasm_bindgen(js_name = createFamilyMember)]
        pub fn create_family_member(&self) -> WasmAuthoringFamilyMemberHandle {
            let id = self.semantics.borrow_mut().insert_authoring_object();
            WasmAuthoringFamilyMemberHandle {
                semantics: Rc::clone(&self.semantics),
                id,
            }
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
    impl WasmAuthoringFamilyMemberHandle {
        #[wasm_bindgen(getter, js_name = semanticSlot)]
        pub fn semantic_slot(&self) -> u32 {
            self.id.slot()
        }

        #[wasm_bindgen(getter, js_name = semanticGeneration)]
        pub fn semantic_generation(&self) -> u32 {
            self.id.generation()
        }

        #[wasm_bindgen(js_name = bindRetainedNativeText)]
        pub fn bind_retained_native_text(
            &self,
            text: &mut WasmRetainedNativeTextAuthoringHandle,
        ) -> Result<(), JsValue> {
            let identity = self.authoring_identity();
            text.bind_family_identity(&identity).map_err(js_error)
        }
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyHandle {
        semantics: SharedSemanticStore,
        id: SemanticNodeId,
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyLayout {
        semantics: SharedSemanticStore,
        family_id: SemanticNodeId,
        expected_leaves: Vec<SemanticNodeId>,
        next_leaf: usize,
        bounds: Option<Bounds2D64>,
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyTranslation {
        semantics: SharedSemanticStore,
        translation: FrontendFamilyTranslation,
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyArrange {
        semantics: SharedSemanticStore,
        plan: FrontendFamilyArrangePlan,
        direction: (f64, f64),
        buff: f64,
        center: bool,
        translations: Option<Vec<Option<FrontendFamilyTranslation>>>,
        next_translation: usize,
    }

    impl WasmAuthoringFamilyArrange {
        fn prepare(&mut self) -> Result<(), JsValue> {
            if self.translations.is_none() {
                let translations = self
                    .plan
                    .finish(self.direction.0, self.direction.1, self.buff, self.center)
                    .map_err(js_error)?;
                self.translations = Some(translations.into_iter().map(Some).collect());
            }
            Ok(())
        }

        fn mobject_member_id(
            &self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<SemanticNodeId, JsValue> {
            member.id_in_store(&self.semantics, "family arrange")
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyArrange {
        #[wasm_bindgen(js_name = includeMobject)]
        pub fn include_mobject(
            &mut self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let id = self.mobject_member_id(member)?;
            self.plan
                .accept_member_bounds(id, member.handle.layout_bounds().map_err(js_error)?)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = includeRetainedNativeText)]
        pub fn include_retained_native_text(
            &mut self,
            member: &WasmAuthoringFamilyMemberHandle,
            text: &WasmRetainedNativeTextAuthoringHandle,
        ) -> Result<(), JsValue> {
            let id = retained_native_member_id(&self.semantics, member, text, "family arrange")?;
            self.plan
                .accept_member_bounds(id, Some(text.family_layout_bounds()))
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = includeFamily)]
        pub fn include_family(
            &mut self,
            layout: &WasmAuthoringFamilyLayout,
        ) -> Result<(), JsValue> {
            layout.ensure_complete()?;
            if !Rc::ptr_eq(&self.semantics, &layout.semantics) {
                return Err(JsValue::from_str(
                    "family arrange and nested family belong to different authoring stores",
                ));
            }
            self.plan
                .accept_member_bounds(layout.family_id, layout.bounds)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextTranslation)]
        pub fn next_translation(&mut self) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.prepare()?;
            let translations = self.translations.as_mut().expect("prepared translations");
            let slot = translations
                .get_mut(self.next_translation)
                .ok_or_else(|| JsValue::from_str("family arrange has no remaining translations"))?;
            let translation = slot.take().ok_or_else(|| {
                JsValue::from_str("family arrange translation was already consumed")
            })?;
            self.next_translation += 1;
            Ok(WasmAuthoringFamilyTranslation {
                semantics: Rc::clone(&self.semantics),
                translation,
            })
        }

        pub fn finish(&self) -> Result<(), JsValue> {
            self.plan.ensure_complete().map_err(js_error)?;
            if self.next_translation != self.plan.member_count() {
                return Err(JsValue::from_str(&format!(
                    "family arrange is incomplete: emitted {} of {} translations",
                    self.next_translation,
                    self.plan.member_count()
                )));
            }
            Ok(())
        }
    }

    impl WasmAuthoringFamilyLayout {
        fn include_leaf_bounds(
            &mut self,
            id: SemanticNodeId,
            bounds: Option<Bounds2D64>,
        ) -> Result<(), JsValue> {
            let expected = self
                .expected_leaves
                .get(self.next_leaf)
                .copied()
                .ok_or_else(|| JsValue::from_str("family layout received too many leaves"))?;
            if id != expected {
                return Err(JsValue::from_str(&format!(
                    "family layout leaf mismatch at index {}: expected {expected:?}, got {id:?}",
                    self.next_leaf
                )));
            }
            if let Some(bounds) = bounds {
                self.include_bounds(bounds);
            }
            self.next_leaf += 1;
            Ok(())
        }

        fn ensure_complete(&self) -> Result<(), JsValue> {
            if self.next_leaf != self.expected_leaves.len() {
                return Err(JsValue::from_str(&format!(
                    "family layout is incomplete: accepted {} of {} leaves",
                    self.next_leaf,
                    self.expected_leaves.len()
                )));
            }
            Ok(())
        }

        fn center(&self) -> Result<(f64, f64), JsValue> {
            self.ensure_complete()?;
            Ok(self.bounds.as_ref().map_or((0.0, 0.0), |bounds| {
                (
                    (bounds.min_x + bounds.max_x) * 0.5,
                    (bounds.min_y + bounds.max_y) * 0.5,
                )
            }))
        }

        fn include_bounds(&mut self, bounds: Bounds2D64) {
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
            member
                .id_in_store(&self.semantics, "family placement")
                .map(|_| ())
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyLayout {
        #[wasm_bindgen(js_name = includeMobject)]
        pub fn include_mobject(
            &mut self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let id = member.id_in_store(&self.semantics, "family layout")?;
            self.include_leaf_bounds(id, member.handle.layout_bounds().map_err(js_error)?)
        }

        #[wasm_bindgen(js_name = includeRetainedNativeText)]
        pub fn include_retained_native_text(
            &mut self,
            member: &WasmAuthoringFamilyMemberHandle,
            text: &WasmRetainedNativeTextAuthoringHandle,
        ) -> Result<(), JsValue> {
            let id = retained_native_member_id(&self.semantics, member, text, "family layout")?;
            self.include_leaf_bounds(id, Some(text.family_layout_bounds()))
        }

        #[wasm_bindgen(getter, js_name = centerX)]
        pub fn center_x(&self) -> Result<f64, JsValue> {
            self.center().map(|center| center.0)
        }

        #[wasm_bindgen(getter, js_name = centerY)]
        pub fn center_y(&self) -> Result<f64, JsValue> {
            self.center().map(|center| center.1)
        }

        #[wasm_bindgen(getter)]
        pub fn width(&self) -> Result<f64, JsValue> {
            self.ensure_complete()?;
            Ok(self
                .bounds
                .as_ref()
                .map_or(0.0, |bounds| bounds.max_x - bounds.min_x))
        }

        #[wasm_bindgen(getter)]
        pub fn height(&self) -> Result<f64, JsValue> {
            self.ensure_complete()?;
            Ok(self
                .bounds
                .as_ref()
                .map_or(0.0, |bounds| bounds.max_y - bounds.min_y))
        }

        #[wasm_bindgen(js_name = shiftBy)]
        pub fn shift_by(
            &self,
            delta_x: f64,
            delta_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.translation(delta_x, delta_y)
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
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            let point = semantic_xy_f64(point_x, point_y).map_err(js_error)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let mask = semantic_xy_f64(mask_x, mask_y).map_err(js_error)?;
            let source_x = self.critical_x(edge.x, edge.y)?;
            let source_y = self.critical_y(edge.x, edge.y)?;
            self.translation((point.x - source_x) * mask.x, (point.y - source_y) * mask.y)
        }

        #[wasm_bindgen(js_name = moveToMobject)]
        pub fn move_to_mobject(
            &self,
            target: &WasmAuthoringMobjectHandle,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            self.validate_target_mobject(target)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let mask = semantic_xy_f64(mask_x, mask_y).map_err(js_error)?;
            let source_x = self.critical_x(edge.x, edge.y)?;
            let source_y = self.critical_y(edge.x, edge.y)?;
            let target_point = target
                .handle
                .critical_point(edge.x, edge.y)
                .map_err(js_error)?;
            self.translation(
                (target_point.0 - source_x) * mask.x,
                (target_point.1 - source_y) * mask.y,
            )
        }

        #[wasm_bindgen(js_name = moveToRetainedNativeText)]
        pub fn move_to_retained_native_text(
            &self,
            target_member: &WasmAuthoringFamilyMemberHandle,
            target: &WasmRetainedNativeTextAuthoringHandle,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            retained_native_member_id(
                &self.semantics,
                target_member,
                target,
                "family placement target",
            )?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let mask = semantic_xy_f64(mask_x, mask_y).map_err(js_error)?;
            let source_x = self.critical_x(edge.x, edge.y)?;
            let source_y = self.critical_y(edge.x, edge.y)?;
            let target_point = retained_native_critical_point(target, (edge.x, edge.y));
            self.translation(
                (target_point.0 - source_x) * mask.x,
                (target_point.1 - source_y) * mask.y,
            )
        }

        #[wasm_bindgen(js_name = moveToFamily)]
        pub fn move_to_family(
            &self,
            target: &WasmAuthoringFamilyLayout,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            target.ensure_complete()?;
            if !Rc::ptr_eq(&self.semantics, &target.semantics) {
                return Err(JsValue::from_str(
                    "family placement source and target belong to different authoring stores",
                ));
            }
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let mask = semantic_xy_f64(mask_x, mask_y).map_err(js_error)?;
            let source_x = self.critical_x(edge.x, edge.y)?;
            let source_y = self.critical_y(edge.x, edge.y)?;
            let target_x = target.critical_x(edge.x, edge.y)?;
            let target_y = target.critical_y(edge.x, edge.y)?;
            self.translation(
                (target_x - source_x) * mask.x,
                (target_y - source_y) * mask.y,
            )
        }

        #[wasm_bindgen(js_name = nextToPoint)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_point(
            &self,
            point_x: f64,
            point_y: f64,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            let point = semantic_xy_f64(point_x, point_y).map_err(js_error)?;
            let direction = semantic_xy_f64(direction_x, direction_y).map_err(js_error)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let source = (
                self.critical_x(edge.x - direction.x, edge.y - direction.y)?,
                self.critical_y(edge.x - direction.x, edge.y - direction.y)?,
            );
            let delta = manim_family_next_to_delta(
                source,
                (point.x, point.y),
                (direction.x, direction.y),
                buff,
                (mask_x, mask_y),
            )
            .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = nextToMobject)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_mobject(
            &self,
            target: &WasmAuthoringMobjectHandle,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            self.validate_target_mobject(target)?;
            let direction = semantic_xy_f64(direction_x, direction_y).map_err(js_error)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let source = (
                self.critical_x(edge.x - direction.x, edge.y - direction.y)?,
                self.critical_y(edge.x - direction.x, edge.y - direction.y)?,
            );
            let target_point = target
                .handle
                .critical_point(edge.x + direction.x, edge.y + direction.y)
                .map_err(js_error)?;
            let delta = manim_family_next_to_delta(
                source,
                target_point,
                (direction.x, direction.y),
                buff,
                (mask_x, mask_y),
            )
            .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = nextToRetainedNativeText)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_retained_native_text(
            &self,
            target_member: &WasmAuthoringFamilyMemberHandle,
            target: &WasmRetainedNativeTextAuthoringHandle,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            retained_native_member_id(
                &self.semantics,
                target_member,
                target,
                "family placement target",
            )?;
            let direction = semantic_xy_f64(direction_x, direction_y).map_err(js_error)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let source = (
                self.critical_x(edge.x - direction.x, edge.y - direction.y)?,
                self.critical_y(edge.x - direction.x, edge.y - direction.y)?,
            );
            let target_point = retained_native_critical_point(
                target,
                (edge.x + direction.x, edge.y + direction.y),
            );
            let delta = manim_family_next_to_delta(
                source,
                target_point,
                (direction.x, direction.y),
                buff,
                (mask_x, mask_y),
            )
            .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = nextToFamily)]
        #[allow(clippy::too_many_arguments)]
        pub fn next_to_family(
            &self,
            target: &WasmAuthoringFamilyLayout,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            aligned_edge_x: f64,
            aligned_edge_y: f64,
            mask_x: f64,
            mask_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            target.ensure_complete()?;
            if !Rc::ptr_eq(&self.semantics, &target.semantics) {
                return Err(JsValue::from_str(
                    "family placement source and target belong to different authoring stores",
                ));
            }
            let direction = semantic_xy_f64(direction_x, direction_y).map_err(js_error)?;
            let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y).map_err(js_error)?;
            let source = (
                self.critical_x(edge.x - direction.x, edge.y - direction.y)?,
                self.critical_y(edge.x - direction.x, edge.y - direction.y)?,
            );
            let target_point = (
                target.critical_x(edge.x + direction.x, edge.y + direction.y)?,
                target.critical_y(edge.x + direction.x, edge.y + direction.y)?,
            );
            let delta = manim_family_next_to_delta(
                source,
                target_point,
                (direction.x, direction.y),
                buff,
                (mask_x, mask_y),
            )
            .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = alignToPoint)]
        pub fn align_to_point(
            &self,
            point_x: f64,
            point_y: f64,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            let point = semantic_xy_f64(point_x, point_y).map_err(js_error)?;
            let axis = semantic_xy_f64(axis_x, axis_y).map_err(js_error)?;
            let source = (
                self.critical_x(axis.x, axis.y)?,
                self.critical_y(axis.x, axis.y)?,
            );
            let delta = manim_family_align_to_delta(source, (point.x, point.y), (axis.x, axis.y))
                .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = alignToMobject)]
        pub fn align_to_mobject(
            &self,
            target: &WasmAuthoringMobjectHandle,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            self.validate_target_mobject(target)?;
            let axis = semantic_xy_f64(axis_x, axis_y).map_err(js_error)?;
            let source = (
                self.critical_x(axis.x, axis.y)?,
                self.critical_y(axis.x, axis.y)?,
            );
            let target_point = target
                .handle
                .critical_point(axis.x, axis.y)
                .map_err(js_error)?;
            let delta = manim_family_align_to_delta(source, target_point, (axis.x, axis.y))
                .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = alignToRetainedNativeText)]
        pub fn align_to_retained_native_text(
            &self,
            target_member: &WasmAuthoringFamilyMemberHandle,
            target: &WasmRetainedNativeTextAuthoringHandle,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            retained_native_member_id(
                &self.semantics,
                target_member,
                target,
                "family placement target",
            )?;
            let axis = semantic_xy_f64(axis_x, axis_y).map_err(js_error)?;
            let source = (
                self.critical_x(axis.x, axis.y)?,
                self.critical_y(axis.x, axis.y)?,
            );
            let target_point = retained_native_critical_point(target, (axis.x, axis.y));
            let delta = manim_family_align_to_delta(source, target_point, (axis.x, axis.y))
                .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = alignToFamily)]
        pub fn align_to_family(
            &self,
            target: &WasmAuthoringFamilyLayout,
            axis_x: f64,
            axis_y: f64,
        ) -> Result<WasmAuthoringFamilyTranslation, JsValue> {
            self.ensure_complete()?;
            target.ensure_complete()?;
            if !Rc::ptr_eq(&self.semantics, &target.semantics) {
                return Err(JsValue::from_str(
                    "family placement source and target belong to different authoring stores",
                ));
            }
            let axis = semantic_xy_f64(axis_x, axis_y).map_err(js_error)?;
            let source = (
                self.critical_x(axis.x, axis.y)?,
                self.critical_y(axis.x, axis.y)?,
            );
            let target_point = (
                target.critical_x(axis.x, axis.y)?,
                target.critical_y(axis.x, axis.y)?,
            );
            let delta = manim_family_align_to_delta(source, target_point, (axis.x, axis.y))
                .map_err(js_error)?;
            self.translation(delta.0, delta.1)
        }

        #[wasm_bindgen(js_name = criticalX)]
        pub fn critical_x(&self, direction_x: f64, _direction_y: f64) -> Result<f64, JsValue> {
            self.ensure_complete()?;
            let center = self.center()?.0;
            Ok(self.bounds.as_ref().map_or(center, |bounds| {
                if direction_x < 0.0 {
                    bounds.min_x
                } else if direction_x > 0.0 {
                    bounds.max_x
                } else {
                    center
                }
            }))
        }

        #[wasm_bindgen(js_name = criticalY)]
        pub fn critical_y(&self, _direction_x: f64, direction_y: f64) -> Result<f64, JsValue> {
            self.ensure_complete()?;
            let center = self.center()?.1;
            Ok(self.bounds.as_ref().map_or(center, |bounds| {
                if direction_y < 0.0 {
                    bounds.min_y
                } else if direction_y > 0.0 {
                    bounds.max_y
                } else {
                    center
                }
            }))
        }
    }

    #[wasm_bindgen]
    pub struct WasmAuthoringFamilyTargetEditor {
        semantics: SharedSemanticStore,
        editor: FrontendFamilyTargetEditor,
    }

    impl WasmAuthoringFamilyTargetEditor {
        pub(crate) fn store(&self) -> &SharedSemanticStore {
            &self.semantics
        }

        pub(crate) fn target_member_ids(&self) -> Result<&[SemanticNodeId], JsValue> {
            self.editor.target_members().map_err(js_error)
        }

        fn mobject_member_id(
            &self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<SemanticNodeId, JsValue> {
            member.id_in_store(&self.semantics, "family target editor")
        }

        fn identity_member_id(
            &self,
            member: &WasmAuthoringFamilyMemberHandle,
        ) -> Result<SemanticNodeId, JsValue> {
            if !Rc::ptr_eq(&self.semantics, &member.semantics) {
                return Err(JsValue::from_str(
                    "family target editor and member belong to different authoring stores",
                ));
            }
            Ok(member.id)
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
                .accept_member(source_id, target_id)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = acceptMember)]
        pub fn accept_member_identity(
            &mut self,
            source: &WasmAuthoringFamilyMemberHandle,
            target: &WasmAuthoringFamilyMemberHandle,
        ) -> Result<(), JsValue> {
            let source_id = self.identity_member_id(source)?;
            let target_id = self.identity_member_id(target)?;
            self.editor
                .accept_member(source_id, target_id)
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
                .accept_member(source_id, target_id)
                .map_err(js_error)
        }

        pub fn finish(&self) -> Result<WasmAuthoringFamilyHandle, JsValue> {
            let id = self
                .editor
                .finish(&mut self.semantics.borrow_mut())
                .map_err(js_error)?;
            Ok(WasmAuthoringFamilyHandle {
                semantics: Rc::clone(&self.semantics),
                id,
            })
        }
    }

    impl WasmAuthoringFamilyHandle {
        pub(crate) fn from_semantic_family(family: noon::MobjectFamily) -> Self {
            Self {
                semantics: Rc::clone(family.store()),
                id: family.node_id(),
            }
        }

        pub(crate) fn semantic_family(&self) -> Result<noon::MobjectFamily, JsValue> {
            noon::MobjectFamily::from_node(Rc::clone(&self.semantics), self.id).map_err(js_error)
        }

        fn object_member_id(
            &self,
            member: &WasmAuthoringMobjectHandle,
        ) -> Result<SemanticNodeId, JsValue> {
            member.id_in_store(&self.semantics, "family")
        }

        fn identity_member_id(
            &self,
            member: &WasmAuthoringFamilyMemberHandle,
        ) -> Result<SemanticNodeId, JsValue> {
            if !Rc::ptr_eq(&self.semantics, &member.semantics) {
                return Err(JsValue::from_str(
                    "family and member belong to different authoring stores",
                ));
            }
            Ok(member.id)
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
    impl WasmAuthoringFamilyTranslation {
        #[wasm_bindgen(js_name = applyMobject)]
        pub fn apply_mobject(
            &mut self,
            member: &mut WasmAuthoringMobjectHandle,
        ) -> Result<(), JsValue> {
            let id = member.id_in_store(&self.semantics, "family translation")?;
            self.translation
                .apply(id, &mut member.handle)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = applyRetainedNativeText)]
        pub fn apply_retained_native_text(
            &mut self,
            member: &WasmAuthoringFamilyMemberHandle,
            text: &mut WasmRetainedNativeTextAuthoringHandle,
        ) -> Result<(), JsValue> {
            let id =
                retained_native_member_id(&self.semantics, member, text, "family translation")?;
            self.translation
                .apply_with(id, |delta| text.apply_family_translation(delta.0, delta.1))
                .map_err(js_error)
        }

        pub fn finish(&self) -> Result<(), JsValue> {
            self.translation.finish().map_err(js_error)
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringFamilyHandle {
        #[wasm_bindgen(js_name = layoutSession)]
        pub fn layout_session(&self) -> Result<WasmAuthoringFamilyLayout, JsValue> {
            let expected_leaves =
                semantic_family_leaf_ids(&self.semantics.borrow(), self.id).map_err(js_error)?;
            Ok(WasmAuthoringFamilyLayout {
                semantics: Rc::clone(&self.semantics),
                family_id: self.id,
                expected_leaves,
                next_leaf: 0,
                bounds: None,
            })
        }

        #[wasm_bindgen(js_name = arrangeSession)]
        pub fn arrange_session(
            &self,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
            center: bool,
        ) -> Result<WasmAuthoringFamilyArrange, JsValue> {
            let direction = semantic_xy_f64(direction_x, direction_y).map_err(js_error)?;
            let buff = render_f64("buffer", buff).map_err(js_error)?;
            let plan = FrontendFamilyArrangePlan::begin(&self.semantics.borrow(), self.id)
                .map_err(js_error)?;
            Ok(WasmAuthoringFamilyArrange {
                semantics: Rc::clone(&self.semantics),
                plan,
                direction: (direction.x, direction.y),
                buff,
                center,
                translations: None,
                next_translation: 0,
            })
        }

        #[wasm_bindgen(js_name = targetEditor)]
        pub fn target_editor(&self) -> Result<WasmAuthoringFamilyTargetEditor, JsValue> {
            let editor = FrontendFamilyTargetEditor::begin(&self.semantics.borrow(), self.id)
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

        #[wasm_bindgen(js_name = addMember)]
        pub fn add_member_identity(
            &mut self,
            member: &WasmAuthoringFamilyMemberHandle,
        ) -> Result<bool, JsValue> {
            let id = self.identity_member_id(member)?;
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

        #[wasm_bindgen(js_name = removeMember)]
        pub fn remove_member_identity(
            &mut self,
            member: &WasmAuthoringFamilyMemberHandle,
        ) -> Result<bool, JsValue> {
            let id = self.identity_member_id(member)?;
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
            if !Rc::ptr_eq(semantics, self.handle.store()) {
                return Err(js_error(format!(
                    "{context} and mobject belong to different authoring stores"
                )));
            }
            self.handle.validate().map_err(js_error)?;
            Ok(self.handle.node_id())
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringMobjectHandle {
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
            self.handle.validate().map_err(js_error)?;
            serde_json::to_string(
                &noon::legacy::export_mobject_snapshot(&self.handle).map_err(js_error)?,
            )
            .map_err(|error| js_error(error.to_string()))
        }

        /// Explicit #959 export access for native Text when a legacy timeline
        /// must be finalized. The source and presentation are reconstructed from
        /// the one shared semantic store, never from Python wrapper state.
        #[wasm_bindgen(js_name = textSpecJson)]
        pub fn text_spec_json(&self) -> Result<String, JsValue> {
            let state = self.handle.state().map_err(js_error)?;
            let spec = crate::canonical_text_authoring_spec(&self.handle.store().borrow(), &state)
                .map_err(js_error)?;
            serde_json::to_string(&spec).map_err(|error| js_error(error.to_string()))
        }

        #[wasm_bindgen(getter, js_name = wireTranslationX)]
        pub fn wire_translation_x(&self) -> Result<f64, JsValue> {
            Ok(self.handle.wire_translation().map_err(js_error)?.0)
        }

        #[wasm_bindgen(getter, js_name = wireTranslationY)]
        pub fn wire_translation_y(&self) -> Result<f64, JsValue> {
            Ok(self.handle.wire_translation().map_err(js_error)?.1)
        }

        #[wasm_bindgen(getter, js_name = wireScaleX)]
        pub fn wire_scale_x(&self) -> Result<f64, JsValue> {
            Ok(self.handle.wire_scale().map_err(js_error)?.0)
        }

        #[wasm_bindgen(getter, js_name = wireScaleY)]
        pub fn wire_scale_y(&self) -> Result<f64, JsValue> {
            Ok(self.handle.wire_scale().map_err(js_error)?.1)
        }

        #[wasm_bindgen(getter, js_name = wireRotation)]
        pub fn wire_rotation(&self) -> Result<f64, JsValue> {
            Ok(self.handle.wire_rotation().map_err(js_error)?)
        }

        #[wasm_bindgen(getter, js_name = wireHasFill)]
        pub fn wire_has_fill(&self) -> Result<bool, JsValue> {
            Ok(self.handle.wire_fill().map_err(js_error)?.is_some())
        }

        #[wasm_bindgen(getter, js_name = wireFillRed)]
        pub fn wire_fill_red(&self) -> Result<f64, JsValue> {
            Ok(self
                .handle
                .wire_fill()
                .map_err(js_error)?
                .map_or(0.0, |value| value.0))
        }

        #[wasm_bindgen(getter, js_name = wireFillGreen)]
        pub fn wire_fill_green(&self) -> Result<f64, JsValue> {
            Ok(self
                .handle
                .wire_fill()
                .map_err(js_error)?
                .map_or(0.0, |value| value.1))
        }

        #[wasm_bindgen(getter, js_name = wireFillBlue)]
        pub fn wire_fill_blue(&self) -> Result<f64, JsValue> {
            Ok(self
                .handle
                .wire_fill()
                .map_err(js_error)?
                .map_or(0.0, |value| value.2))
        }

        #[wasm_bindgen(getter, js_name = wireFillAlpha)]
        pub fn wire_fill_alpha(&self) -> Result<f64, JsValue> {
            Ok(self
                .handle
                .wire_fill()
                .map_err(js_error)?
                .map_or(0.0, |value| value.3))
        }

        #[wasm_bindgen(getter, js_name = wireHasStroke)]
        pub fn wire_has_stroke(&self) -> Result<bool, JsValue> {
            Ok(self.handle.wire_stroke().map_err(js_error)?.is_some())
        }

        #[wasm_bindgen(getter, js_name = wireStrokeRed)]
        pub fn wire_stroke_red(&self) -> Result<f64, JsValue> {
            Ok(self
                .handle
                .wire_stroke()
                .map_err(js_error)?
                .map_or(0.0, |value| value.0))
        }

        #[wasm_bindgen(getter, js_name = wireStrokeGreen)]
        pub fn wire_stroke_green(&self) -> Result<f64, JsValue> {
            Ok(self
                .handle
                .wire_stroke()
                .map_err(js_error)?
                .map_or(0.0, |value| value.1))
        }

        #[wasm_bindgen(getter, js_name = wireStrokeBlue)]
        pub fn wire_stroke_blue(&self) -> Result<f64, JsValue> {
            Ok(self
                .handle
                .wire_stroke()
                .map_err(js_error)?
                .map_or(0.0, |value| value.2))
        }

        #[wasm_bindgen(getter, js_name = wireStrokeAlpha)]
        pub fn wire_stroke_alpha(&self) -> Result<f64, JsValue> {
            Ok(self
                .handle
                .wire_stroke()
                .map_err(js_error)?
                .map_or(0.0, |value| value.3))
        }

        #[wasm_bindgen(getter, js_name = wireStrokeWidth)]
        pub fn wire_stroke_width(&self) -> Result<f64, JsValue> {
            Ok(self.handle.wire_stroke_width().map_err(js_error)?)
        }

        #[wasm_bindgen(getter, js_name = wireObjectOpacity)]
        pub fn wire_object_opacity(&self) -> Result<f64, JsValue> {
            Ok(self.handle.wire_object_opacity().map_err(js_error)?)
        }

        #[wasm_bindgen(js_name = replaceSnapshotJson)]
        pub fn replace_snapshot_json(&mut self, snapshot_json: &str) -> Result<(), JsValue> {
            let snapshot = serde_json::from_str(snapshot_json)
                .map_err(|error| js_error(format!("invalid mobject snapshot: {error}")))?;
            noon::legacy::replace_mobject_snapshot(&mut self.handle, snapshot).map_err(js_error)
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
            self.handle
                .manim_next_to_handle(
                    &other.handle,
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
            self.handle
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
            self.handle.scale(x, y).map_err(js_error)
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
        pub fn become_handle(&mut self, other: &WasmAuthoringMobjectHandle) -> Result<(), JsValue> {
            self.handle.become_handle(&other.handle).map_err(js_error)
        }

        #[wasm_bindgen(js_name = replaceHandle)]
        pub fn replace_handle(
            &mut self,
            other: &WasmAuthoringMobjectHandle,
            dim_to_match: u32,
            stretch: bool,
        ) -> Result<(), JsValue> {
            self.handle
                .replace_handle(&other.handle, dim_to_match, stretch)
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
            self.handle
                .next_to_handle(&other.handle, direction_x, direction_y, buff)
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
            self.handle
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
    use noon_core::{
        Color, GeometryRef, ObjectSnapshot, StrokeCap, StrokeJoin, StrokeWidthMode, Transform2D,
        Vec2, VectorPath,
    };

    use super::*;

    #[test]
    fn family_arrange_preserves_direct_order_spacing_and_recentering() {
        let bounds = [
            Some(Bounds2D64 {
                min_x: -1.0,
                min_y: -0.5,
                max_x: 1.0,
                max_y: 0.5,
            }),
            Some(Bounds2D64 {
                min_x: -0.5,
                min_y: -0.25,
                max_x: 0.5,
                max_y: 0.25,
            }),
        ];
        let deltas =
            manim_family_arrange_deltas(&bounds, 2.0, 0.0, 0.25, true).expect("arrange deltas");
        assert_eq!(deltas, vec![(-0.75, 0.0), (1.25, 0.0)]);

        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let nested = store.insert_family();
        store.add_member(nested, second).unwrap();
        let outer = store.insert_family();
        store.add_member(outer, first).unwrap();
        store.add_member(outer, nested).unwrap();

        let mut rejected = FrontendFamilyArrangePlan::begin(&store, outer).unwrap();
        assert!(rejected.accept_member_bounds(nested, bounds[1]).is_err());

        let mut plan = FrontendFamilyArrangePlan::begin(&store, outer).unwrap();
        plan.accept_member_bounds(first, bounds[0]).unwrap();
        plan.accept_member_bounds(nested, bounds[1]).unwrap();
        let translations = plan.finish(2.0, 0.0, 0.25, true).unwrap();
        assert_eq!(translations.len(), 2);
        assert_eq!(translations[0].source_members, vec![first]);
        assert_eq!(translations[0].delta, (-0.75, 0.0));
        assert_eq!(translations[1].source_members, vec![second]);
        assert_eq!(translations[1].delta, (1.25, 0.0));
    }

    #[test]
    fn family_relative_placement_preserves_manim_direction_and_axis_semantics() {
        let next =
            manim_family_next_to_delta((2.0, 3.0), (7.0, 11.0), (2.0, -3.0), 0.5, (1.0, 0.25))
                .expect("next_to delta");
        assert_eq!(next, (6.0, 1.625));

        let aligned = manim_family_align_to_delta((2.0, 3.0), (7.0, 11.0), (0.0, -1.0))
            .expect("align_to delta");
        assert_eq!(aligned, (0.0, 8.0));
    }

    fn snapshot(geometry: GeometryRef) -> ObjectSnapshot {
        ObjectSnapshot {
            geometry,
            transform: Transform2D::default(),
            style: noon_core::Style::default(),
        }
    }

    #[test]
    fn handle_mutations_keep_state_in_shared_rust_semantics() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut handle = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::circle(1.0)),
        )
        .unwrap();
        handle.shift(2.0, -1.0).unwrap();
        handle.scale(1.5, 0.5).unwrap();
        assert_eq!(handle.center().unwrap(), (2.0, -1.0));
        assert_eq!(handle.width().unwrap(), 3.0);
        assert_eq!(handle.height().unwrap(), 1.0);
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&handle)
                .unwrap()
                .transform
                .translation,
            Vec2::new(2.0, -1.0)
        );
    }

    #[test]
    fn authoring_transform_keeps_f64_precision_until_render_lowering() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut handle = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::rectangle(2.0, 1.0)),
        )
        .unwrap();
        handle.shift(0.7, 0.3).unwrap();
        assert_eq!(handle.state().unwrap().transform.translation.x, 0.7);
        assert_eq!(handle.state().unwrap().transform.translation.y, 0.3);
        assert!((handle.critical_point(-1.0, 0.0).unwrap().0 + 0.3).abs() < 1e-12);
        assert!((handle.critical_point(0.0, 1.0).unwrap().1 - 0.8).abs() < 1e-12);
        assert_ne!(
            f64::from(
                noon::legacy::export_mobject_snapshot(&handle)
                    .unwrap()
                    .transform
                    .translation
                    .x
            ),
            0.7
        );

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
        let mut handle = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::line(Vec2::ZERO, Vec2::new(1.0, 0.0))),
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
        let handle = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::path(path)),
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
        let mut ellipse = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::circle(1.0)),
        )
        .unwrap();
        ellipse.scale(2.0, 1.0).unwrap();
        ellipse.rotate(std::f64::consts::FRAC_PI_4).unwrap();
        assert!((ellipse.width().unwrap() - 10.0_f64.sqrt()).abs() < 1e-12);
        assert!((ellipse.height().unwrap() - 10.0_f64.sqrt()).abs() < 1e-12);

        let mut diagonal = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::line(Vec2::ZERO, Vec2::new(1.0, 1.0))),
        )
        .unwrap();
        diagonal.rotate(std::f64::consts::FRAC_PI_4).unwrap();
        assert!(diagonal.width().unwrap().abs() < 1e-12);
        assert!((diagonal.height().unwrap() - 2.0_f64.sqrt()).abs() < 1e-12);

        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, 0.0))
            .quadratic_to(Vec2::new(0.0, 2.0), Vec2::new(1.0, 0.0));
        let mut curve = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::path(path)),
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
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&circle)
                .unwrap()
                .geometry,
            GeometryRef::circle(1.5)
        );
        let fill = noon::legacy::export_mobject_snapshot(&circle)
            .unwrap()
            .style
            .fill
            .unwrap();
        let stroke = noon::legacy::export_mobject_snapshot(&circle)
            .unwrap()
            .style
            .stroke
            .unwrap();
        assert_eq!(fill.red, Color::RED.red);
        assert_eq!(fill.alpha, 0.0);
        assert_eq!(stroke.red, Color::RED.red);
        assert_eq!(stroke.alpha, 1.0);
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&circle)
                .unwrap()
                .style
                .stroke_width,
            0.04
        );
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&circle)
                .unwrap()
                .style
                .stroke_width_mode,
            StrokeWidthMode::ScreenSpace
        );
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&circle)
                .unwrap()
                .style
                .stroke_join,
            StrokeJoin::Miter
        );
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&circle)
                .unwrap()
                .style
                .stroke_cap,
            StrokeCap::Butt
        );

        let line = Mobject::manim_line(std::rc::Rc::clone(&authoring_store), -2.0, 1.0, 3.0, -1.0)
            .unwrap();
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&line)
                .unwrap()
                .geometry,
            GeometryRef::line(Vec2::new(-2.0, 1.0), Vec2::new(3.0, -1.0))
        );
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&line)
                .unwrap()
                .style
                .stroke
                .unwrap()
                .red,
            Color::WHITE.red
        );

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
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&square)
                .unwrap()
                .style
                .stroke_width_mode,
            StrokeWidthMode::ScaleWithObject
        );
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&square)
                .unwrap()
                .style
                .stroke_join,
            StrokeJoin::Bevel
        );
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&square)
                .unwrap()
                .style
                .stroke_cap,
            StrokeCap::Square
        );
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&square)
                .unwrap()
                .style
                .opacity,
            0.8
        );
    }

    #[test]
    fn layout_operations_are_shared_and_deterministic() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let left = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::circle(0.5)),
        )
        .unwrap();
        let mut right = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::rectangle(1.0, 1.0)),
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
        let reference = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::rectangle(2.0, 2.0)),
        )
        .unwrap();
        let mut diagonal = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::rectangle(2.0, 2.0)),
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

        let mut moved = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::rectangle(1.0, 1.0)),
        )
        .unwrap();
        moved.shift(0.0, -2.0).unwrap();
        moved
            .manim_move_to_handle(&reference, -1.0, 1.0, 1.0, 0.0)
            .unwrap();
        assert!((moved.center().unwrap().0 + 0.5).abs() < 1e-12);
        assert!((moved.center().unwrap().1 + 2.0).abs() < 1e-12);

        let mut aligned = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::rectangle(1.0, 1.0)),
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
        let mut value = snapshot(GeometryRef::circle(1.0));
        value.style.fill = Some(Color::rgba(1.0, 0.0, 0.0, 0.4));
        value.style.stroke = Some(Color::rgba(0.0, 0.0, 1.0, 0.7));
        let mut handle =
            noon::legacy::import_mobject_snapshot(std::rc::Rc::clone(&authoring_store), value)
                .unwrap();

        handle.set_fill_color(0.0, 1.0, 0.0, 1.0).unwrap();
        assert!((handle.fill_opacity().unwrap() - 0.4).abs() < 1e-6);
        handle.set_fill_opacity(0.25).unwrap();
        handle.set_stroke_width(3.5).unwrap();
        handle.set_stroke_opacity(0.6).unwrap();
        assert_eq!(handle.fill_opacity().unwrap(), 0.25);
        assert_eq!(handle.stroke_opacity().unwrap(), 0.6);
        assert!(
            (noon::legacy::export_mobject_snapshot(&handle)
                .unwrap()
                .style
                .stroke_width
                - 3.5)
                .abs()
                < 1e-6
        );
        assert!(
            (noon::legacy::export_mobject_snapshot(&handle)
                .unwrap()
                .style
                .stroke
                .unwrap()
                .alpha
                - 0.6)
                .abs()
                < 1e-6
        );

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
        let base = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::circle(1.0)),
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
        let fill = noon::legacy::export_mobject_snapshot(&target)
            .unwrap()
            .style
            .fill
            .unwrap();
        assert_eq!(fill.red, 1.0);
        assert_eq!(fill.green, 0.525);
        assert_eq!(fill.blue, 0.184);
        assert_eq!(fill.alpha, 0.5);
    }

    #[test]
    fn target_editor_clone_alias_is_independent_and_set_fill_is_transactional() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let base = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::circle(1.0)),
        )
        .unwrap();
        let mut target = base.target_editor().unwrap();
        let sibling = target.target_editor().unwrap();

        target.shift(2.0, 0.0).unwrap();
        target.set_fill(0.0, 1.0, 0.0, 0.25).unwrap();
        assert_eq!(base.center().unwrap(), (0.0, 0.0));
        assert_eq!(sibling.center().unwrap(), (0.0, 0.0));
        assert_eq!(sibling.fill_opacity().unwrap(), 1.0);
        let sibling_fill = noon::legacy::export_mobject_snapshot(&sibling)
            .unwrap()
            .style
            .fill
            .unwrap();
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
            semantic_family_leaf_ids(&store, outer).unwrap(),
            vec![first, second]
        );

        let alias = store.insert_family();
        store.add_member(alias, first).unwrap();
        let aliased_outer = store.insert_family();
        store.add_member(aliased_outer, nested).unwrap();
        store.add_member(aliased_outer, alias).unwrap();
        assert_eq!(
            semantic_family_leaf_ids(&store, aliased_outer).unwrap(),
            vec![first, first]
        );
    }

    #[test]
    fn family_translation_uses_shared_recursive_leaf_order() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut store = SemanticStore::new();
        let first = store.insert_authoring_object();
        let second = store.insert_authoring_object();
        let nested = store.insert_family();
        store.add_member(nested, first).unwrap();
        let outer = store.insert_family();
        store.add_member(outer, nested).unwrap();
        store.add_member(outer, second).unwrap();

        let mut first_handle =
            Mobject::manim_circle(std::rc::Rc::clone(&authoring_store), 1.0).unwrap();
        let mut second_handle =
            Mobject::manim_square(std::rc::Rc::clone(&authoring_store), 2.0).unwrap();
        let first_before = first_handle.center().unwrap();
        let second_before = second_handle.center().unwrap();

        let mut translation = FrontendFamilyTranslation::begin(&store, outer, 2.5, -1.25).unwrap();
        translation.apply(first, &mut first_handle).unwrap();
        translation.apply(second, &mut second_handle).unwrap();
        translation.finish().unwrap();

        assert_eq!(
            first_handle.center().unwrap(),
            (first_before.0 + 2.5, first_before.1 - 1.25)
        );
        assert_eq!(
            second_handle.center().unwrap(),
            (second_before.0 + 2.5, second_before.1 - 1.25)
        );

        let mut reordered = FrontendFamilyTranslation::begin(&store, outer, 1.0, 0.0).unwrap();
        let error = reordered.apply(second, &mut second_handle).unwrap_err();
        assert!(error.contains("mismatch at index 0"));
        assert!(reordered.finish().unwrap_err().contains("incomplete"));
    }

    #[test]
    fn family_target_editor_builds_target_from_shared_source_order() {
        let mut store = SemanticStore::new();
        let source_a = store.insert_semantic_object(noon_core::SemanticObjectState::new(
            noon_core::StoredGeometry::Circle { radius: 1.0 },
        ));
        let source_b = store.insert_semantic_object(noon_core::SemanticObjectState::new(
            noon_core::StoredGeometry::Circle { radius: 1.0 },
        ));
        let source_family = store.insert_family();
        store.add_member(source_family, source_a).unwrap();
        store.add_member(source_family, source_b).unwrap();

        let target_a = store.insert_semantic_object(noon_core::SemanticObjectState::new(
            noon_core::StoredGeometry::Circle { radius: 1.0 },
        ));
        let target_b = store.insert_semantic_object(noon_core::SemanticObjectState::new(
            noon_core::StoredGeometry::Circle { radius: 1.0 },
        ));
        let mut editor = FrontendFamilyTargetEditor::begin(&store, source_family).unwrap();

        editor.accept_member(source_a, target_a).unwrap();
        editor.accept_member(source_b, target_b).unwrap();
        let target_family = editor.finish(&mut store).unwrap();

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

        let mut editor = FrontendFamilyTargetEditor::begin(&store, source_family).unwrap();
        let error = editor.accept_member(source_b, target_a).unwrap_err();
        assert!(error.contains("mismatch at index 0"));
        assert!(editor
            .finish(&mut store)
            .unwrap_err()
            .contains("accepted 0 of 2"));
    }

    #[test]
    fn become_and_replace_keep_state_inside_shared_handle() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut source = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::circle(0.5)),
        )
        .unwrap();
        source.shift(-2.0, 0.5).unwrap();
        let mut target = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::rectangle(2.0, 1.0)),
        )
        .unwrap();
        target.shift(1.0, -0.25).unwrap();

        source.become_handle(&target).unwrap();
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&source).unwrap(),
            noon::legacy::export_mobject_snapshot(&target).unwrap()
        );

        let mut replacement = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::circle(0.25)),
        )
        .unwrap();
        replacement.replace_handle(&target, 0, false).unwrap();
        assert!((replacement.width().unwrap() - 2.0).abs() < 1e-6);
        assert!((replacement.height().unwrap() - 2.0).abs() < 1e-6);
        assert!((replacement.center().unwrap().0 - 1.0).abs() < 1e-6);
        assert!((replacement.center().unwrap().1 + 0.25).abs() < 1e-6);

        let mut stretched = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::circle(0.25)),
        )
        .unwrap();
        stretched.replace_handle(&target, 0, true).unwrap();
        assert!((stretched.width().unwrap() - 2.0).abs() < 1e-6);
        assert!((stretched.height().unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn wire_projection_matches_lowered_snapshot_after_shared_edits() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let mut value = snapshot(GeometryRef::rectangle(2.0, 1.0));
        value.style.fill = Some(Color::rgba(0.2, 0.3, 0.4, 0.5));
        value.style.stroke = Some(Color::rgba(0.6, 0.7, 0.8, 0.9));
        let mut handle =
            noon::legacy::import_mobject_snapshot(std::rc::Rc::clone(&authoring_store), value)
                .unwrap();

        handle.shift(0.7, -0.3).unwrap();
        handle.scale(1.1, 0.9).unwrap();
        handle.rotate(0.2).unwrap();
        handle.set_fill_opacity(0.25).unwrap();
        handle.set_stroke_width(3.5).unwrap();

        let snapshot = noon::legacy::export_mobject_snapshot(&handle).unwrap();
        assert_eq!(
            handle.wire_translation().unwrap(),
            (
                f64::from(snapshot.transform.translation.x),
                f64::from(snapshot.transform.translation.y),
            )
        );
        assert_eq!(
            handle.wire_scale().unwrap(),
            (
                f64::from(snapshot.transform.scale.x),
                f64::from(snapshot.transform.scale.y),
            )
        );
        assert_eq!(
            handle.wire_rotation().unwrap(),
            f64::from(snapshot.transform.rotation)
        );
        assert_eq!(handle.wire_fill().unwrap().unwrap().3, 0.25_f32 as f64);
        assert_eq!(handle.wire_stroke_width().unwrap(), 3.5_f32 as f64);
    }

    #[test]
    fn json_round_trip_preserves_wire_snapshot() {
        let authoring_store =
            std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let handle = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            snapshot(GeometryRef::rectangle(2.0, 3.0)),
        )
        .unwrap();
        let json = serde_json::to_string(&noon::legacy::export_mobject_snapshot(&handle).unwrap())
            .unwrap();
        let restored = noon::legacy::import_mobject_snapshot(
            std::rc::Rc::clone(&authoring_store),
            serde_json::from_str(&json).unwrap(),
        )
        .unwrap();
        assert_eq!(
            noon::legacy::export_mobject_snapshot(&restored).unwrap(),
            noon::legacy::export_mobject_snapshot(&handle).unwrap()
        );
    }
}

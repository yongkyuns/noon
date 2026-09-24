"""Staging-only source reconstruction, excluded from the clean product commit."""
from pathlib import Path
import subprocess

BASE = '2ccf83664b51f4bc973e01736acf1fae138d33ae'
assert subprocess.check_output(['git', 'rev-parse', f'{BASE}^{{tree}}']).decode().strip() == 'c4ca5b9f5fbd01b951172aea81217f6830539249'
changed = set()
def edit(path, old, new, count=1):
    p = Path(path)
    source = p.read_text()
    assert source.count(old) == count, (path, old, source.count(old))
    p.write_text(source.replace(old, new))
    changed.add(path)

edit('crates/noon-core/src/semantic_store.rs', 'mod object_content;', 'mod pointer_actions;\npub use pointer_actions::*;\n\nmod object_content;')
p = 'crates/noon-core/src/semantic_store/object_content.rs'
edit(p, '    signal_bindings: Vec<SemanticSignalBinding>,', '    signal_bindings: Vec<SemanticSignalBinding>,\n    pointer_click_action: Option<crate::SemanticPointerClickAction>,')
edit(p, '            signal_bindings: Vec::new(),', '            signal_bindings: Vec::new(),\n            pointer_click_action: None,')
edit(p, '            signal_bindings: self.signal_bindings.clone(),', '            signal_bindings: self.signal_bindings.clone(),\n            pointer_click_action: self.pointer_click_action,')
edit(p, '    pub fn signal_bindings(&self) -> &[SemanticSignalBinding] {', '''    pub const fn pointer_click_action(&self) -> Option<crate::SemanticPointerClickAction> {
        self.pointer_click_action
    }

    pub(crate) fn set_pointer_click_action(&mut self, action: Option<crate::SemanticPointerClickAction>) {
        self.pointer_click_action = action;
    }

    pub fn signal_bindings(&self) -> &[SemanticSignalBinding] {''')
p = 'crates/noon-core/src/semantic_store/semantic_transaction.rs'
edit(p, '    ChangeSubscription {\n        object: SemanticTransactionNodeRef,', '''    SetPointerClickAction {
        object: SemanticTransactionNodeRef,
        action: Option<crate::SemanticPointerClickAction>,
    },
    ChangeSubscription {
        object: SemanticTransactionNodeRef,''')
edit(p, '            | Self::ReplaceStyle { object, .. }', '            | Self::ReplaceStyle { object, .. }\n            | Self::SetPointerClickAction { object, .. }', 2)
edit(p, '            Self::ReplaceStyle { object, .. } => Some(SemanticMutationKey::ObjectStyle(*object)),', '''            Self::ReplaceStyle { object, .. } => Some(SemanticMutationKey::ObjectStyle(*object)),
            Self::SetPointerClickAction { object, .. } => Some(SemanticMutationKey::PointerClickAction(*object)),''')
edit(p, '    ObjectStyle(SemanticTransactionNodeRef),', '    ObjectStyle(SemanticTransactionNodeRef),\n    PointerClickAction(SemanticTransactionNodeRef),')
edit(p, '    ObjectStyle {\n        object: SemanticNodeId,\n    },', '''    ObjectStyle {
        object: SemanticNodeId,
    },
    /// Declaration-only metadata; no execution property or membership changed.
    PointerClickAction { object: SemanticNodeId },''')
edit(p, '    /// Change the authored signal driver for one object property.\n    pub fn change_subscription(', '''    /// Author or remove one object's self-targeting primary-click action.
    pub fn set_pointer_click_action(
        &mut self, object: impl Into<SemanticTransactionNodeRef>,
        action: Option<crate::SemanticPointerClickAction>,
    ) -> &mut Self {
        self.mutations.push(SemanticMutation::SetPointerClickAction { object: object.into(), action });
        self
    }

    /// Change the authored signal driver for one object property.
    pub fn change_subscription(''')
edit(p, '''                SemanticMutation::ChangeSubscription {
                    object,
                    property,
                    signal,
                } => {
                    let state = catalog.staged_object_state(''', '''                SemanticMutation::SetPointerClickAction { object, action } => {
                    let state = catalog.staged_object_state(
                        &mut staged_objects, &mut staged_object_order, *object, index,
                    )?;
                    if action.is_some_and(|action| !action.is_valid()) {
                        return Err(SemanticMutationTransactionError::InvalidPointerClickAction { index, object: *object });
                    }
                    let did_change = state.pointer_click_action() != *action;
                    state.set_pointer_click_action(*action);
                    changed.push(did_change);
                }
                SemanticMutation::ChangeSubscription {
                    object,
                    property,
                    signal,
                } => {
                    let state = catalog.staged_object_state(''')
edit(p, 'pub enum SemanticMutationTransactionError {', '''pub enum SemanticMutationTransactionError {
    InvalidPointerClickAction { index: usize, object: SemanticTransactionNodeRef },
    DuplicatePointerClickAction { index: usize, object: SemanticNodeId },''')
edit(p, '''impl std::fmt::Display for SemanticMutationTransactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {''', '''impl std::fmt::Display for SemanticMutationTransactionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPointerClickAction { index, object } => write!(formatter,
                "semantic transaction mutation {index} has invalid pointer-click action on {object:?}"),
            Self::DuplicatePointerClickAction { index, object } => write!(formatter,
                "semantic transaction mutation {index} duplicates pointer-click action on {object:?}"),''')
edit(p, 'mod graph_tests;', 'mod graph_tests;\n\n#[cfg(test)]\nmod pointer_action_tests;')
p = 'crates/noon-core/src/semantic_store/semantic_transaction/provisional.rs'
edit(p, '        | SemanticMutationKey::ObjectStyle(object)', '        | SemanticMutationKey::ObjectStyle(object)\n        | SemanticMutationKey::PointerClickAction(object)')
edit(p, '        SemanticMutationKey::ObjectStyle(SemanticTransactionNodeRef::Existing(object)) => {', '''        SemanticMutationKey::PointerClickAction(SemanticTransactionNodeRef::Existing(object)) => {
            SemanticMutationTransactionError::DuplicatePointerClickAction { index, object }
        }
        SemanticMutationKey::ObjectStyle(SemanticTransactionNodeRef::Existing(object)) => {''')
p = 'crates/noon-core/src/semantic_store/semantic_transaction/prepared.rs'
edit(p, '                SemanticMutation::ReplaceStyle { object, style } => {', '''                SemanticMutation::SetPointerClickAction { object, action } => {
                    let object = resolve_node_ref(object, &committed_nodes);
                    store.node_mut(object).expect("preflighted click target")
                        .semantic_object_state_mut().expect("preflighted object")
                        .set_pointer_click_action(action);
                    written_slots.insert(object);
                    impacts.push(SemanticMutationImpact::PointerClickAction { object });
                }
                SemanticMutation::ReplaceStyle { object, style } => {''')
for p in ['crates/noon-compile/src/semantic_lowering/projection.rs', 'crates/noon-compile/src/semantic_lowering/reachability.rs']:
    edit(p, '                | SemanticMutationImpact::ObjectStyle { .. }', '                | SemanticMutationImpact::ObjectStyle { .. }\n                | SemanticMutationImpact::PointerClickAction { .. }')
p = 'crates/noon-compile/src/semantic_lowering/publication.rs'
edit(p, '                | SemanticMutation::ReplaceStyle { .. }', '                | SemanticMutation::ReplaceStyle { .. }\n                | SemanticMutation::SetPointerClickAction { .. }')
edit(p, '            SemanticMutation::ReplaceStyle { object, .. } => {', '''            SemanticMutation::SetPointerClickAction { object, .. } => {
                // An authored declaration still publishes coherently, but does
                // not rewrite geometry, paint, content, or runtime drivers.
                if let Some(object) = object.existing() {
                    domains.entry(object).or_default();
                }
            }
            SemanticMutation::ReplaceStyle { object, .. } => {''')
p = 'crates/noon/src/execution_session/selection.rs'
edit(p, '    max_movement: Option<f32>,', '    max_movement: Option<f32>,\n    select_on_click: bool,')
edit(p, '            max_movement: self.max_movement,', '            max_movement: self.max_movement,\n            select_on_click: self.select_on_click,')
edit(p, '        self.ensure_direct_input_ingress_available()?;\n        if !max_movement.is_finite()', '''        self.configure_pointer_fill_clicks(max_movement, true)
    }

    /// Recognize the same ordered fill clicks without enabling editor selection.
    /// A language-neutral action binding may consume the accepted occurrence.
    pub fn enable_pointer_fill_clicks(&mut self, max_movement: f32) -> Result<(), ExecutionSessionInputError> {
        self.configure_pointer_fill_clicks(max_movement, false)
    }

    fn configure_pointer_fill_clicks(&mut self, max_movement: f32, select_on_click: bool) -> Result<(), ExecutionSessionInputError> {
        self.ensure_direct_input_ingress_available()?;
        if !max_movement.is_finite()''')
edit(p, '            max_movement: Some(max_movement),', '            max_movement: Some(max_movement),\n            select_on_click,')
edit(p, '''                        prepared.state.selected = target.map(|node| SelectedTarget {
                            node,
                            publication: query.publication(),
                        });''', '''                        if prepared.state.select_on_click {
                            prepared.state.selected = target.map(|node| SelectedTarget {
                                node,
                                publication: query.publication(),
                            });
                            prepared.changed = target != previous_selection;
                        }''')
edit(p, '                        prepared.changed = target != previous_selection;\n                    }', '                    }')
p = 'crates/noon/src/live_session.rs'
edit(p, 'mod property_animation;', 'mod property_animation;\nmod pointer_actions;\npub use pointer_actions::{PointerActionPublication, PointerClickActionOutcome};')
p = 'crates/noon/src/lib.rs'
edit(p, '    FadeTranslation, IndicateOptions, LiveSession, LiveSessionError, SubsetDisplayMode,', '    FadeTranslation, IndicateOptions, LiveSession, LiveSessionError, PointerActionPublication,\n    PointerClickActionOutcome, SubsetDisplayMode,')
edit(p, '    SemanticObjectState, SemanticPaint, SemanticSignalValue,', '    SemanticPointerClickAction, SemanticObjectState, SemanticPaint, SemanticSignalValue,')
p = 'crates/noon/src/live_program/property_animation.rs'
edit(p, 'impl<C: LiveContinuation> LiveProgram<C> {', '''impl<C: LiveContinuation> LiveProgram<C> {
    /// Configure click recognition without changing editor selection presentation.
    pub fn set_pointer_fill_clicks(&mut self, max_movement: Option<f32>) -> Result<(), LiveProgramError<C::Error>> {
        self.ensure_host_input_available("configure pointer clicks")?;
        let session = self.scene.owned_execution_mut();
        match max_movement {
            Some(value) => session.enable_pointer_fill_clicks(value),
            None => session.disable_pointer_fill_selection(),
        }.map_err(LiveProgramError::Input)
    }

    /// Admit input and execute its authored action through this program's live owner.
    /// Action failures are retained inside the accepted-input publication.
    pub fn submit_pointer_input_with_actions(
        &mut self,
        token: &crate::integration::NativePointerInputToken,
        input: noon_core::NativePointerInput,
    ) -> Result<crate::PointerActionPublication, LiveProgramError<C::Error>> {
        self.ensure_host_input_available("dispatch pointer input actions")?;
        let publication = self.scene.owned_live()
            .submit_pointer_input_with_actions(token, input)
            .map_err(LiveProgramError::Effect)?;
        self.refresh_pending_publication();
        Ok(publication)
    }
''')
changed.update([
    'crates/noon-core/src/semantic_store/pointer_actions.rs',
    'crates/noon-core/src/semantic_store/semantic_transaction/pointer_action_tests.rs',
    'crates/noon/src/live_session/pointer_actions.rs',
    'crates/noon/src/live_session/pointer_actions/tests.rs',
])
for path in changed:
    assert Path(path).is_file(), path
Path('/tmp/c5-click-proof').mkdir(exist_ok=True)
Path('/tmp/c5-click-proof/paths.txt').write_text(''.join(path+'\n' for path in sorted(changed)))

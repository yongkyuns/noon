from pathlib import Path

def edit(name, old, new):
    p = Path(name)
    s = p.read_text()
    assert old in s, name
    p.write_text(s.replace(old, new, 1))

edit('crates/noon-core/src/reactive/semantic_transaction.rs',
     '    staged_object_order: Vec<SemanticTransactionNodeRef>,',
     '    staged_object_order: Vec<SemanticTransactionNodeRef>,\n    staged_updaters: HashMap<SemanticTransactionNodeRef, Vec<SemanticUpdaterRegistration>>,')
edit('crates/noon-core/src/reactive/semantic_transaction.rs',
     '            staged_object_order,\n            family_edges,',
     '            staged_object_order,\n            staged_updaters,\n            family_edges,')
edit('crates/noon-core/src/reactive/semantic_transaction/prepared.rs',
     '    /// Reserved candidate revision, or the current revision when no candidates exist.',
     '''    /// Staged callback registrations computed by the semantic transaction's own
    /// preflight. Compiler preparation reads these rather than reimplementing
    /// updater insertion, occurrence identity, or interval-closing semantics.
    pub fn proposed_updater_registrations(
        &self,
        target: SemanticNodeId,
    ) -> Option<&[SemanticUpdaterRegistration]> {
        self.preflight.staged_updaters.get(&target.into()).map(Vec::as_slice)
    }

    /// Reserved candidate revision, or the current revision when no candidates exist.''')
p = Path('crates/noon-compile/src/semantic_lowering/host_callbacks.rs')
s = p.read_text().replace('use std::collections::HashSet;', 'use std::collections::{HashMap, HashSet};')
s = s.replace('    HostCallbackId, SemanticNodeId, SemanticNodeKind, SemanticStore, SemanticUpdaterRegistration,',
'''    HostCallbackId, PreparedSemanticMutationTransaction, SemanticMutation, SemanticNodeId,
    SemanticNodeKind, SemanticStore, SemanticUpdaterRegistration,''')
s = s.replace('''    pub fn is_empty(&self) -> bool {
        self.occurrences.is_empty()
    }''', '''    pub fn is_empty(&self) -> bool {
        self.occurrences.is_empty()
    }

    /// Relower only callback history, not scene geometry, at a live publication.
    /// The first live subset retains target preorder from initial lowering and
    /// therefore admits registration edits only on already indexed targets.
    pub(super) fn prepare_registration_revision(
        &self,
        prepared: &PreparedSemanticMutationTransaction<'_>,
        current_time: f64,
    ) -> Result<Option<Self>, super::SemanticPublicationLoweringError> {
        use super::SemanticPublicationLoweringError as Error;
        let mut changed = HashSet::new();
        for (index, mutation) in prepared.mutations().iter().enumerate() {
            let (target, boundary) = match mutation {
                SemanticMutation::AddUpdater { target, active_from, .. } => (*target, *active_from),
                SemanticMutation::RemoveUpdater { target, inactive_from, .. }
                | SemanticMutation::ClearUpdaters { target, inactive_from } => (*target, *inactive_from),
                _ => return Err(Error::UnsupportedMutation { index }),
            };
            if boundary < current_time {
                return Err(Error::RetroactiveUpdaterMutation { index });
            }
            // Exact no-ops must not rebuild the callback index or invalidate an
            // already accepted phase. Staged registrations are semantic-owned.
            let target = target.existing().ok_or(Error::UnsupportedMutation { index })?;
            if let Some(staged) = prepared.proposed_updater_registrations(target) {
                if staged != prepared.store().node(target).expect("validated target").host_updaters() {
                    changed.insert(target);
                }
            }
        }
        if changed.is_empty() { return Ok(None); }
        let indexed = self.occurrences.iter().map(|item| item.target).collect::<HashSet<_>>();
        for &target in &changed {
            if !indexed.contains(&target) {
                return Err(Error::UpdaterTargetNotIndexed { target });
            }
        }
        let mut replacements = HashMap::new();
        for target in changed {
            replacements.insert(target, prepared.proposed_updater_registrations(target)
                .expect("changed target has staged registrations"));
        }
        let mut emitted = HashSet::new();
        let mut occurrences = Vec::new();
        for occurrence in &self.occurrences {
            if let Some(registrations) = replacements.get(&occurrence.target) {
                if emitted.insert(occurrence.target) {
                    occurrences.extend(registrations.iter().copied().map(|activation|
                        SemanticHostCallbackOccurrence { target: occurrence.target, activation }));
                }
            } else {
                occurrences.push(*occurrence);
            }
        }
        Ok(Some(index_callback_occurrences(occurrences)))
    }''')
s = s.replace('    let mut events = Vec::with_capacity(occurrences.len().saturating_mul(2));',
'''    index_callback_occurrences(occurrences)
}

fn index_callback_occurrences(occurrences: Vec<SemanticHostCallbackOccurrence>) -> SemanticHostCallbackPlan {
    let mut events = Vec::with_capacity(occurrences.len().saturating_mul(2));''', 1)
p.write_text(s)
edit('crates/noon-compile/src/semantic_lowering/publication.rs',
     '    UnsupportedReactiveMembership {',
     '    UpdaterTargetNotIndexed { target: SemanticNodeId },\n    RetroactiveUpdaterMutation { index: usize },\n    UnsupportedReactiveMembership {')
edit('crates/noon-compile/src/semantic_lowering/publication.rs',
     '            Self::UnsupportedReactiveMembership { object } => write!(',
     '''            Self::UpdaterTargetNotIndexed { target } => write!(
                f, "live updater target {target:?} requires callback preorder enrollment before execution"
            ),
            Self::RetroactiveUpdaterMutation { index } => write!(
                f, "updater mutation {index} precedes the current live frame"
            ),
            Self::UnsupportedReactiveMembership { object } => write!(''')
edit('crates/noon-compile/src/semantic_lowering/publication.rs', 'pub fn validate_semantic_publication(',
'''/// Registration-only batches use callback-plan lowering, not the property lane.
/// Mixed structural/registration transactions remain unsupported until their
/// proposed target preorder can be preflighted together.
pub fn is_semantic_updater_publication(mutations: &[SemanticMutation]) -> bool {
    !mutations.is_empty() && mutations.iter().all(|mutation| matches!(mutation,
        SemanticMutation::AddUpdater { .. } | SemanticMutation::RemoveUpdater { .. }
        | SemanticMutation::ClearUpdaters { .. }))
}

/// Prepare the callback-plan revision and the empty geometry projection together.
/// The caller must commit both after all semantic/runtime preflight succeeds.
pub fn prepare_semantic_updater_publication(
    prepared: &PreparedSemanticMutationTransaction<'_>,
    callbacks: &super::SemanticHostCallbackPlan,
    current_time: f64,
) -> Result<(PreparedSemanticPublication, Option<super::SemanticHostCallbackPlan>), SemanticPublicationLoweringError> {
    let revised = callbacks.prepare_registration_revision(prepared, current_time)?;
    Ok((PreparedSemanticPublication {
        values: ExecutionMutationTransaction::from_mutations(Vec::new()),
        resource_additions: CompiledResources::default(),
        entries: Vec::new(), possible_exits: Vec::new(),
        stats: SemanticPublicationPreparationStats::default(),
    }, revised))
}

pub fn validate_semantic_publication(''')
p = Path('crates/noon/src/execution_session/publication.rs')
s = p.read_text().replace('    prepare_semantic_publication, prepare_semantic_publication_with_scalar_timeline,',
'''    is_semantic_updater_publication, prepare_semantic_updater_publication,
    prepare_semantic_publication, prepare_semantic_publication_with_scalar_timeline,''')
s = s.replace('''        validate_semantic_publication(&transaction)
            .map_err(ExecutionSessionPublicationError::Lowering)?;''',
'''        if !is_semantic_updater_publication(transaction.mutations()) {
            validate_semantic_publication(&transaction)
                .map_err(ExecutionSessionPublicationError::Lowering)?;
        }''')
s = s.replace('        let publication = match scalar.as_ref() {',
'''        let (publication, revised_callbacks) = if is_semantic_updater_publication(prepared.mutations()) {
            // Registration publication cannot smuggle an execution prefix or
            // completion carry into its callback-only lowering contract.
            if !execution_prefix.is_empty() || effective.is_some() || scalar.is_some()
                || purpose != SemanticPublicationPurpose::AuthoredMutation {
                return Err(ExecutionSessionPublicationError::Lowering(
                    SemanticPublicationLoweringError::UnsupportedMutation { index: 0 }));
            }
            prepare_semantic_updater_publication(
                &prepared, self.callback_schedule.plan(), self.frame().time,
            ).map_err(ExecutionSessionPublicationError::Lowering)?
        } else {
        let publication = match scalar.as_ref() {''', 1)
s = s.replace('        let preparation_stats = publication.stats();',
'''        (publication, None)
        };
        let revised_schedule = revised_callbacks.map(|plan|
            super::callback::CallbackSchedule::at_publication(plan, self.frame().time));
        let preparation_stats = publication.stats();''', 1)
s = s.replace('        apply_execution_slot_membership_changes(&mut self.slots, &exited, &entered)',
'''        if let Some(schedule) = revised_schedule {
            self.callback_schedule = schedule;
            self.last_callback_receipt = None;
        }
        apply_execution_slot_membership_changes(&mut self.slots, &exited, &entered)''', 1)
p.write_text(s)
edit('crates/noon/src/execution_session/callback.rs', '    pub(super) fn is_empty(&self) -> bool {',
'''    pub(super) fn plan(&self) -> &SemanticHostCallbackPlan { &self.plan }

    /// Install a new compiler plan at the current frame without evaluating any
    /// historical callbacks. Invalidate phase completion at this publication:
    /// an active newly registered occurrence can next run with dt=0.
    pub(super) fn at_publication(plan: SemanticHostCallbackPlan, time: f64) -> Self {
        let mut schedule = Self::new(plan);
        // Disable the next-activation barrier only while reconstructing the
        // active membership. This must consume every event through `time`.
        schedule.next_required_activation_event = None;
        let preview = schedule.preview(time, time);
        schedule.event_cursor = preview.event_cursor;
        schedule.active_occurrences = preview.active_occurrences;
        schedule.next_required_activation_event = schedule.plan.events()[schedule.event_cursor..]
            .iter().position(|event| Self::event_requires_phase(&schedule.plan, *event))
            .map(|offset| schedule.event_cursor + offset);
        schedule
    }

    pub(super) fn is_empty(&self) -> bool {''')
p = Path('crates/noon-web/src/canonical_authoring_scene.rs')
s = p.read_text().replace('self.require_pre_execution_updater_target(handle)?;', 'self.require_updater_target(handle)?;')
old = '''        transaction
            .apply(&mut self.scene.store().borrow_mut())
            .map(|_| ())
            .map_err(|error| error.to_string())'''
a = s.index('    fn add_updater(')
b = s.index('    fn live_player(', a)
chunk = s[a:b]
assert chunk.count(old) == 3
chunk = chunk.replace(old, '        self.publish_updater_edit(transaction)')
chunk = chunk.replace('#[cfg(target_arch = "wasm32")]', '#[cfg(any(target_arch = "wasm32", test))]')
chunk = chunk.replace('    fn require_pre_execution_updater_target', '    fn require_updater_target')
chunk = chunk.replace('''        if self.live_player.is_some() || self.live_player_transferred {
            return Err(
                "callback registrations must be authored before canonical execution begins".into(),
            );
        }
''', '')
chunk = chunk.replace('authored time before the canonical execution session exists.', 'authored time through the owning live session when execution has begun.')
s = s[:a] + chunk + s[b:]
needle = '    #[cfg(any(target_arch = "wasm32", test))]\n    fn require_updater_target'
s = s.replace(needle, '''    #[cfg(any(target_arch = "wasm32", test))]
    fn publish_updater_edit(&mut self, transaction: SemanticMutationTransaction) -> Result<(), String> {
        if self.live_player_transferred {
            return Err("return the active execution player before editing updaters".into());
        }
        if let Some(player) = self.live_player.as_mut() {
            return player.live_edit_updaters(transaction);
        }
        transaction.apply(&mut self.scene.store().borrow_mut())
            .map(|_| ()).map_err(|error| error.to_string())
    }

''' + needle, 1)
p.write_text(s)
edit('crates/noon-web/src/semantic_execution_player.rs',
     '    #[cfg(any(target_arch = "wasm32", test))]\n    fn require_completed_live_segment(',
'''    /// Route callback declarations through the same shared semantic publication
    /// as native authoring. The browser host owns no callback schedule mirror.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn live_edit_updaters(
        &mut self, transaction: noon_core::SemanticMutationTransaction,
    ) -> Result<(), String> {
        self.require_completed_live_segment()?;
        let semantics = self.semantics.clone().ok_or("execution player has no live semantic store")?;
        noon::LiveSession::new(
            &semantics, self.semantic_root.expect("live semantic store has one scene root"),
            &mut self.session,
        ).apply(transaction).map(|_| ()).map_err(|error| error.to_string())
    }

    #[cfg(any(target_arch = "wasm32", test))]
    fn require_completed_live_segment(''')

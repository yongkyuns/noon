use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use noon_compile::{
    CompiledNumericTextDriver, CompiledScene, ExecutionMutationTransaction, ExecutionPatch,
};
use noon_core::{
    compose_numeric_text_resource, numeric_text_layout, NumericTextResourceError, ObjectId,
    ReactiveValue, SignalId, TextResource, TextResourceArena, TextResourceHandle,
    TextResourceLookup,
};

use crate::{PreparedReactiveRuntimeUpdate, SceneInstance};

#[derive(Clone, Debug)]
struct NumericTextDriverState {
    declaration: CompiledNumericTextDriver,
    tokens: BTreeMap<Arc<str>, TextResourceHandle>,
    current: Option<TextResourceHandle>,
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedNumericTextUpdate {
    pub(crate) driver_index: usize,
    pub(crate) object_index: usize,
    pub(crate) resource: TextResource,
}

/// One compiler-owned declaration change addressed by stable execution identity.
/// `None` removes the driver and its effective resource slot.
#[derive(Clone, Debug)]
pub struct NumericTextDriverRevisionEntry {
    pub object: ObjectId,
    pub declaration: Option<CompiledNumericTextDriver>,
}

#[derive(Clone, Debug)]
struct PreparedNumericTextDriverRevisionEntry {
    object: ObjectId,
    object_index: usize,
    declaration: Option<CompiledNumericTextDriver>,
    resource: Option<TextResource>,
}

/// Fully validated sparse numeric-driver revision committed after its matching
/// authored execution transaction crosses the semantic point of no return.
#[derive(Clone, Debug, Default)]
pub struct PreparedNumericTextDriverRevision {
    entries: Vec<PreparedNumericTextDriverRevisionEntry>,
}

/// Sparse retained state for tracker-driven numeric text.
///
/// Each declaration owns at most one replace-in-place effective resource slot.
/// The signal index keeps ordinary updates proportional to the affected numeric
/// objects rather than to the scene size.
#[derive(Clone, Debug, Default)]
pub(crate) struct NumericTextRuntime {
    resources: TextResourceArena,
    drivers: Vec<Option<NumericTextDriverState>>,
    free_driver_slots: Vec<usize>,
    drivers_by_signal: BTreeMap<SignalId, BTreeSet<usize>>,
    driver_by_object: BTreeMap<usize, usize>,
}

impl NumericTextRuntime {
    pub(crate) fn new(compiled: &CompiledScene) -> Self {
        let mut drivers_by_signal = BTreeMap::<SignalId, BTreeSet<usize>>::new();
        let mut driver_by_object = BTreeMap::new();
        let drivers = compiled
            .numeric_text_drivers()
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, declaration)| {
                let previous = driver_by_object.insert(declaration.object_index as usize, index);
                debug_assert!(previous.is_none(), "one numeric driver per object");
                drivers_by_signal
                    .entry(declaration.signal)
                    .or_default()
                    .insert(index);
                let tokens = declaration.token_resources.iter().cloned().collect();
                Some(NumericTextDriverState {
                    declaration,
                    tokens,
                    current: None,
                })
            })
            .collect();
        Self {
            resources: TextResourceArena::new(),
            drivers,
            free_driver_slots: Vec::new(),
            drivers_by_signal,
            driver_by_object,
        }
    }

    pub(crate) fn driver_indices(&self, signal: SignalId) -> impl Iterator<Item = usize> + '_ {
        self.drivers_by_signal
            .get(&signal)
            .into_iter()
            .flatten()
            .copied()
    }

    pub(crate) fn all_driver_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.driver_by_object.values().copied()
    }

    pub(crate) fn object_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.driver_by_object.keys().copied()
    }

    pub(crate) fn signal(&self, driver_index: usize) -> SignalId {
        self.driver(driver_index).declaration.signal
    }

    pub(crate) fn object_index(&self, driver_index: usize) -> usize {
        self.driver(driver_index).declaration.object_index as usize
    }

    pub(crate) fn prepare(
        &self,
        driver_index: usize,
        value: &ReactiveValue,
        authored: &dyn TextResourceLookup,
    ) -> Result<PreparedNumericTextUpdate, NumericTextResourceError> {
        let driver = self.driver(driver_index);
        let resource = prepare_driver_resource(driver, value, authored)?;
        Ok(PreparedNumericTextUpdate {
            driver_index,
            object_index: driver.declaration.object_index as usize,
            resource,
        })
    }

    pub(crate) fn commit(&mut self, update: PreparedNumericTextUpdate) -> TextResourceHandle {
        let driver = self.drivers[update.driver_index]
            .as_mut()
            .expect("prepared numeric driver remains enrolled");
        let handle = match driver.current {
            Some(current) => self
                .resources
                .replace(current.id, update.resource)
                .expect("prepared numeric resources remain valid and versions are not exhausted"),
            None => self
                .resources
                .insert(update.resource)
                .expect("prepared numeric resources remain valid"),
        };
        driver.current = Some(handle);
        handle
    }

    pub(crate) fn current_for_object(&self, object_index: usize) -> Option<TextResourceHandle> {
        self.driver_by_object
            .get(&object_index)
            .and_then(|&index| self.driver(index).current)
    }

    pub(crate) fn resources(&self) -> &TextResourceArena {
        &self.resources
    }

    fn driver(&self, index: usize) -> &NumericTextDriverState {
        self.drivers[index]
            .as_ref()
            .expect("numeric driver index remains enrolled")
    }

    fn remove_for_object(&mut self, object_index: usize) {
        let Some(index) = self.driver_by_object.remove(&object_index) else {
            return;
        };
        let driver = self.drivers[index]
            .take()
            .expect("object index points at an enrolled numeric driver");
        let remove_signal_entry =
            if let Some(indices) = self.drivers_by_signal.get_mut(&driver.declaration.signal) {
                indices.remove(&index);
                indices.is_empty()
            } else {
                false
            };
        if remove_signal_entry {
            self.drivers_by_signal.remove(&driver.declaration.signal);
        }
        if let Some(current) = driver.current {
            self.resources
                .remove(current.id)
                .expect("current numeric resource slot remains owned by its driver");
        }
        self.free_driver_slots.push(index);
    }

    fn enroll(&mut self, declaration: CompiledNumericTextDriver) -> usize {
        let object_index = declaration.object_index as usize;
        self.remove_for_object(object_index);
        let tokens = declaration.token_resources.iter().cloned().collect();
        let signal = declaration.signal;
        let state = NumericTextDriverState {
            declaration,
            tokens,
            current: None,
        };
        let index = match self.free_driver_slots.pop() {
            Some(index) => {
                self.drivers[index] = Some(state);
                index
            }
            None => {
                let index = self.drivers.len();
                self.drivers.push(Some(state));
                index
            }
        };
        self.driver_by_object.insert(object_index, index);
        self.drivers_by_signal
            .entry(signal)
            .or_default()
            .insert(index);
        index
    }
}

impl SceneInstance {
    pub fn prepare_numeric_text_driver_revision(
        &self,
        transaction: &ExecutionMutationTransaction,
        entries: Vec<NumericTextDriverRevisionEntry>,
        authored: &dyn TextResourceLookup,
        pending_signals: &BTreeMap<SignalId, ReactiveValue>,
    ) -> Result<PreparedNumericTextDriverRevision, NumericTextResourceError> {
        let mut appended = BTreeMap::new();
        let mut next_index = self.compiled.objects().len();
        for patch in transaction.mutations() {
            if let ExecutionPatch::CreateObject(object) = patch {
                if !appended.contains_key(&object.id) {
                    let index = self
                        .compiled
                        .retained_object_index(object.id)
                        .map(|index| index as usize)
                        .unwrap_or_else(|| {
                            let index = next_index;
                            next_index += 1;
                            index
                        });
                    appended.insert(object.id, index);
                }
            }
        }

        let mut prepared = Vec::with_capacity(entries.len());
        for mut entry in entries {
            let object_index = self
                .compiled
                .object_index(entry.object)
                .map(|index| index as usize)
                .or_else(|| appended.get(&entry.object).copied())
                .ok_or(NumericTextResourceError::InvalidSourceSpan)?;
            let resource = if let Some(declaration) = entry.declaration.as_mut() {
                declaration.object_index = u32::try_from(object_index)
                    .map_err(|_| NumericTextResourceError::InvalidSourceSpan)?;
                let reactive = self
                    .reactive
                    .as_ref()
                    .ok_or(NumericTextResourceError::InvalidSourceSpan)?;
                let value = reactive
                    .state_value(declaration.signal)
                    .or_else(|| pending_signals.get(&declaration.signal))
                    .ok_or(NumericTextResourceError::InvalidSourceSpan)?;
                let temporary = NumericTextDriverState {
                    tokens: declaration.token_resources.iter().cloned().collect(),
                    declaration: declaration.clone(),
                    current: None,
                };
                Some(prepare_driver_resource(&temporary, value, authored)?)
            } else {
                None
            };
            prepared.push(PreparedNumericTextDriverRevisionEntry {
                object: entry.object,
                object_index,
                declaration: entry.declaration,
                resource,
            });
        }
        Ok(PreparedNumericTextDriverRevision { entries: prepared })
    }

    pub fn commit_numeric_text_driver_revision(
        &mut self,
        revision: PreparedNumericTextDriverRevision,
    ) {
        for entry in revision.entries {
            self.numeric_text.remove_for_object(entry.object_index);
            self.effective_driver_rows.remove(&entry.object_index);
            let Some(declaration) = entry.declaration else {
                continue;
            };
            let driver_index = self.numeric_text.enroll(declaration);
            let resource = entry
                .resource
                .expect("prepared numeric enrollment owns an initial resource");
            self.commit_numeric_text_updates(
                vec![PreparedNumericTextUpdate {
                    driver_index,
                    object_index: entry.object_index,
                    resource,
                }],
                true,
            );
        }
    }

    pub(crate) fn prepare_all_numeric_text(
        &self,
    ) -> Result<Vec<PreparedNumericTextUpdate>, NumericTextResourceError> {
        let Some(reactive) = self.reactive.as_ref() else {
            return Ok(Vec::new());
        };
        self.numeric_text
            .all_driver_indices()
            .map(|driver_index| {
                let signal = self.numeric_text.signal(driver_index);
                let value = reactive
                    .state_value(signal)
                    .ok_or(NumericTextResourceError::InvalidSourceSpan)?;
                self.numeric_text
                    .prepare(driver_index, value, self.compiled.text_resources())
            })
            .collect()
    }

    pub(crate) fn prepare_changed_numeric_text(
        &self,
        prepared: &PreparedReactiveRuntimeUpdate,
    ) -> Result<Vec<PreparedNumericTextUpdate>, NumericTextResourceError> {
        let Some(reactive) = self.reactive.as_ref() else {
            return Ok(Vec::new());
        };
        let mut updates = Vec::new();
        for change in prepared.signal_changes() {
            for driver_index in self.numeric_text.driver_indices(change.signal) {
                let value = reactive
                    .prepared_value(prepared, self.numeric_text.signal(driver_index))
                    .ok_or(NumericTextResourceError::InvalidSourceSpan)?;
                updates.push(self.numeric_text.prepare(
                    driver_index,
                    &value,
                    self.compiled.text_resources(),
                )?);
            }
        }
        Ok(updates)
    }

    pub(crate) fn commit_numeric_text_updates(
        &mut self,
        updates: Vec<PreparedNumericTextUpdate>,
        mark_changes: bool,
    ) {
        for update in updates {
            let object_index = update.object_index;
            let bounds = update.resource.bounds;
            let handle = self.numeric_text.commit(update);
            if !self.object_slot_is_live(object_index) {
                continue;
            }
            let changed = self.frame.objects[object_index].content.text() != Some(handle)
                || self.frame.objects[object_index].text_bounds != Some(bounds);
            self.frame.objects[object_index].content = handle.into();
            self.frame.objects[object_index].text_bounds = Some(bounds);
            self.effective_driver_rows.insert(object_index);
            if mark_changes && changed {
                self.mark_changed(object_index);
            }
        }
    }

    pub(crate) fn reapply_numeric_text(&mut self) {
        let object_indices = self
            .numeric_text
            .all_driver_indices()
            .map(|index| self.numeric_text.object_index(index))
            .collect::<Vec<_>>();
        for object_index in object_indices {
            if !self.object_slot_is_live(object_index) {
                continue;
            }
            let Some(handle) = self.numeric_text.current_for_object(object_index) else {
                continue;
            };
            let bounds = self
                .numeric_text
                .resources()
                .get(handle)
                .expect("current numeric resource handle resolves")
                .bounds;
            self.frame.objects[object_index].content = handle.into();
            self.frame.objects[object_index].text_bounds = Some(bounds);
            self.effective_driver_rows.insert(object_index);
        }
    }

    pub(crate) fn reapply_numeric_text_for_object(&mut self, object_index: usize) {
        if !self.object_slot_is_live(object_index) {
            self.effective_driver_rows.remove(&object_index);
            return;
        }
        let Some(handle) = self.numeric_text.current_for_object(object_index) else {
            return;
        };
        let bounds = self
            .numeric_text
            .resources()
            .get(handle)
            .expect("current numeric resource handle resolves")
            .bounds;
        self.frame.objects[object_index].content = handle.into();
        self.frame.objects[object_index].text_bounds = Some(bounds);
        self.effective_driver_rows.insert(object_index);
    }
}

impl PreparedNumericTextDriverRevision {
    /// Retain the exact net membership chosen by semantic commit from the
    /// conservative candidate set that was fully preflighted beforehand.
    pub fn retain_exact(mut self, exact: &[NumericTextDriverRevisionEntry]) -> Self {
        let exact = exact
            .iter()
            .map(|entry| (entry.object, entry.declaration.is_some()))
            .collect::<BTreeMap<_, _>>();
        let mut retained = BTreeMap::new();
        for entry in self.entries {
            if exact
                .get(&entry.object)
                .is_some_and(|bound| *bound == entry.declaration.is_some())
            {
                retained.insert(entry.object, entry);
            }
        }
        self.entries = retained.into_values().collect();
        self
    }
}

fn prepare_driver_resource(
    driver: &NumericTextDriverState,
    value: &ReactiveValue,
    authored: &dyn TextResourceLookup,
) -> Result<TextResource, NumericTextResourceError> {
    let value = match value {
        ReactiveValue::Scalar(value) => f64::from(*value),
        _ => return Err(NumericTextResourceError::InvalidSourceSpan),
    };
    let (source, layout) = numeric_text_layout(value, &driver.declaration.format)?;
    let mut children = Vec::with_capacity(layout.len());
    for token in &layout {
        let handle = driver
            .tokens
            .get(token.tex())
            .copied()
            .ok_or(NumericTextResourceError::TokenCountMismatch)?;
        children.push(
            authored
                .get(handle)
                .ok_or(NumericTextResourceError::TokenCountMismatch)?,
        );
    }
    compose_numeric_text_resource(
        source,
        &layout,
        &children,
        driver.declaration.font_size,
        driver.declaration.point_to_scene_scale,
    )
}

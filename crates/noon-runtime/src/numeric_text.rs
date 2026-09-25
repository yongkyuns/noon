use std::{collections::BTreeMap, sync::Arc};

use noon_compile::{CompiledNumericTextDriver, CompiledScene};
use noon_core::{
    compose_numeric_text_resource, numeric_text_layout, NumericTextResourceError, ReactiveValue,
    SignalId, TextResource, TextResourceArena, TextResourceHandle, TextResourceLookup,
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

/// Sparse retained state for tracker-driven numeric text.
///
/// Each declaration owns at most one replace-in-place effective resource slot.
/// The signal index keeps ordinary updates proportional to the affected numeric
/// objects rather than to the scene size.
#[derive(Clone, Debug, Default)]
pub(crate) struct NumericTextRuntime {
    resources: TextResourceArena,
    drivers: Vec<NumericTextDriverState>,
    drivers_by_signal: BTreeMap<SignalId, Vec<usize>>,
}

impl NumericTextRuntime {
    pub(crate) fn new(compiled: &CompiledScene) -> Self {
        let mut drivers_by_signal = BTreeMap::<SignalId, Vec<usize>>::new();
        let drivers = compiled
            .numeric_text_drivers()
            .iter()
            .cloned()
            .enumerate()
            .map(|(index, declaration)| {
                drivers_by_signal
                    .entry(declaration.signal)
                    .or_default()
                    .push(index);
                let tokens = declaration.token_resources.iter().cloned().collect();
                NumericTextDriverState {
                    declaration,
                    tokens,
                    current: None,
                }
            })
            .collect();
        Self {
            resources: TextResourceArena::new(),
            drivers,
            drivers_by_signal,
        }
    }

    pub(crate) fn driver_indices(&self, signal: SignalId) -> &[usize] {
        self.drivers_by_signal
            .get(&signal)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub(crate) fn all_driver_indices(&self) -> impl Iterator<Item = usize> + '_ {
        0..self.drivers.len()
    }

    pub(crate) fn object_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.drivers
            .iter()
            .map(|driver| driver.declaration.object_index as usize)
    }

    pub(crate) fn signal(&self, driver_index: usize) -> SignalId {
        self.drivers[driver_index].declaration.signal
    }

    pub(crate) fn object_index(&self, driver_index: usize) -> usize {
        self.drivers[driver_index].declaration.object_index as usize
    }

    pub(crate) fn prepare(
        &self,
        driver_index: usize,
        value: &ReactiveValue,
        authored: &dyn TextResourceLookup,
    ) -> Result<PreparedNumericTextUpdate, NumericTextResourceError> {
        let driver = &self.drivers[driver_index];
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
        let resource = compose_numeric_text_resource(
            source,
            &layout,
            &children,
            driver.declaration.font_size,
            driver.declaration.point_to_scene_scale,
        )?;
        Ok(PreparedNumericTextUpdate {
            driver_index,
            object_index: driver.declaration.object_index as usize,
            resource,
        })
    }

    pub(crate) fn commit(&mut self, update: PreparedNumericTextUpdate) -> TextResourceHandle {
        let driver = &mut self.drivers[update.driver_index];
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
        self.drivers
            .iter()
            .find(|driver| driver.declaration.object_index as usize == object_index)
            .and_then(|driver| driver.current)
    }

    pub(crate) fn resources(&self) -> &TextResourceArena {
        &self.resources
    }
}

impl SceneInstance {
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
            for &driver_index in self.numeric_text.driver_indices(change.signal) {
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

//! Immutable capture plans at the genuine execution/render worker boundary.
//! Identity comes from the existing owner and successful envelope sequence.
use super::*;
use noon_core::{ComposedSource, ObjectId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportComposedSourceHandle {
    pub session: u32,
    pub owner: ObjectId,
    pub version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TransportComposedSourceEntry {
    pub handle: TransportComposedSourceHandle,
    members: Vec<ObjectId>,
    parent_scope: Option<ObjectId>,
}

pub(super) type ComposedSources = HashMap<TransportComposedSourceHandle, Arc<ComposedSource>>;

impl TransportComposedSourceEntry {
    fn install(
        self,
    ) -> Result<(TransportComposedSourceHandle, Arc<ComposedSource>), RetainedResourceTransportError>
    {
        if self.members.contains(&self.handle.owner) || self.parent_scope == Some(self.handle.owner)
        {
            return Err(RetainedResourceTransportError::InvalidComposedSource {
                handle: self.handle,
                reason: "source owner cannot be its own input or parent".into(),
            });
        }
        let source = ComposedSource::new(self.members, self.parent_scope).map_err(|error| {
            RetainedResourceTransportError::InvalidComposedSource {
                handle: self.handle,
                reason: error.to_string(),
            }
        })?;
        Ok((self.handle, Arc::new(source)))
    }
}

pub(super) fn install_composed_sources(
    entries: Vec<TransportComposedSourceEntry>,
) -> Result<ComposedSources, RetainedResourceTransportError> {
    let mut installed = HashMap::with_capacity(entries.len());
    for entry in entries {
        if installed.contains_key(&entry.handle) {
            return Err(RetainedResourceTransportError::DuplicateComposedSource(
                entry.handle,
            ));
        }
        let (handle, source) = entry.install()?;
        installed.insert(handle, source);
    }
    Ok(installed)
}

impl RetainedResourceBundle {
    /// Install each changed immutable source once. Effective parameter rows use
    /// its checked handle; they do not resend member lists. Full graph admission
    /// belongs to the shared execution validator after all rows are resolved.
    pub fn with_composed_sources<'a>(
        mut self,
        sources: impl IntoIterator<Item = (TransportComposedSourceHandle, &'a ComposedSource)>,
    ) -> Result<Self, RetainedResourceTransportError> {
        let mut seen = self
            .composed_sources
            .iter()
            .map(|entry| entry.handle)
            .collect::<HashSet<_>>();
        let mut additions = Vec::new();
        for (handle, source) in sources {
            if !seen.insert(handle) {
                return Err(RetainedResourceTransportError::DuplicateComposedSource(
                    handle,
                ));
            }
            if source.members().contains(&handle.owner)
                || source.parent_scope() == Some(handle.owner)
            {
                return Err(RetainedResourceTransportError::InvalidComposedSource {
                    handle,
                    reason: "source owner cannot be its own input or parent".into(),
                });
            }
            additions.push(TransportComposedSourceEntry {
                handle,
                members: source.members().to_vec(),
                parent_scope: source.parent_scope(),
            });
        }
        self.composed_sources.extend(additions);
        Ok(self)
    }
    pub fn composed_source_count(&self) -> usize {
        self.composed_sources.len()
    }
}

impl InstalledRetainedResources {
    pub(crate) fn composed_source_remap(&self) -> ComposedSources {
        self.composed_sources.clone()
    }
    pub fn composed_source_count(&self) -> usize {
        self.composed_sources.len()
    }
    pub fn composed_source(
        &self,
        handle: TransportComposedSourceHandle,
    ) -> Option<&Arc<ComposedSource>> {
        self.composed_sources.get(&handle)
    }
}

impl PreparedRetainedResourceAdditions {
    pub(crate) fn composed_source_remap(&self) -> ComposedSources {
        self.installed.composed_source_remap()
    }
}

#[cfg(test)]
mod tests;

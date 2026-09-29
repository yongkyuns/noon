use std::{collections::HashSet, sync::Arc};

use noon_core::GeometryRef;

use super::{
    InstalledRetainedResources, InstalledTextResourceOverlay, PreparedRetainedResourceAdditions,
    RenderGeometryPreparation, RenderGeometrySlot, RetainedResourceBundle,
    RetainedResourceTransportError,
};

#[cfg(target_arch = "wasm32")]
pub(crate) struct RenderGeometryAdditionView<'a> {
    pub(crate) session: u32,
    pub(crate) geometries: std::collections::HashMap<u32, &'a GeometryRef>,
    pub(crate) preparations: &'a [RenderGeometryPreparation],
}

/// A validated resource transaction. Only touched arena slots are copied or
/// changed; the installed table remains intact until the execution delta lands.
pub(crate) struct PreparedRetainedResourceAdditionsWithRender {
    ordinary: PreparedRetainedResourceAdditions,
    render_geometry_session: Option<u32>,
    render_geometry_updates: Vec<(u32, RenderGeometrySlot)>,
    render_geometry_preparations: Vec<RenderGeometryPreparation>,
}

impl RetainedResourceBundle {
    pub(crate) fn render_geometry_count(&self) -> usize {
        self.render_geometry_resources
            .as_ref()
            .map_or(0, |resources| {
                resources
                    .updates
                    .iter()
                    .filter(|update| update.geometry.is_some())
                    .count()
            })
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn render_geometry_addition(
        &self,
    ) -> Result<Option<RenderGeometryAdditionView<'_>>, RetainedResourceTransportError> {
        self.validate_protocol()?;
        let Some(resources) = &self.render_geometry_resources else {
            return Ok(None);
        };
        validate_render_geometry_resources(resources)?;
        Ok(Some(RenderGeometryAdditionView {
            session: resources.session,
            geometries: resources
                .updates
                .iter()
                .filter_map(|update| {
                    update
                        .geometry
                        .as_ref()
                        .map(|geometry| (update.slot, geometry))
                })
                .collect(),
            preparations: &resources.preparations,
        }))
    }
}

impl InstalledRetainedResources {
    pub(crate) fn prepare_additions_with_render(
        &self,
        mut bundle: RetainedResourceBundle,
    ) -> Result<PreparedRetainedResourceAdditionsWithRender, RetainedResourceTransportError> {
        bundle.validate_protocol()?;
        let render_resources = bundle.render_geometry_resources.take();
        if let Some(resources) = &render_resources {
            validate_render_geometry_resources(resources)?;
            if self
                .render_geometry_session
                .is_some_and(|installed| installed != resources.session)
            {
                return Err(RetainedResourceTransportError::Decode(format!(
                    "retained render geometry session mismatch: installed {:?}, addition {}",
                    self.render_geometry_session, resources.session
                )));
            }
        }

        let ordinary = self.prepare_additions(bundle)?;
        let Some(resources) = render_resources else {
            return Ok(PreparedRetainedResourceAdditionsWithRender {
                ordinary,
                render_geometry_session: None,
                render_geometry_updates: Vec::new(),
                render_geometry_preparations: Vec::new(),
            });
        };

        let mut updates = Vec::with_capacity(resources.updates.len());
        let mut expected_len = self.render_geometries.len();
        for update in resources.updates {
            let index = update.slot as usize;
            let valid = match self.render_geometries.get(index) {
                Some(previous) if previous.geometry.is_some() => {
                    previous.generation.checked_add(1) == Some(update.generation)
                }
                Some(previous) => {
                    previous.generation == update.generation && update.geometry.is_some()
                }
                None => {
                    index == expected_len && update.generation == 0 && update.geometry.is_some()
                }
            };
            if !valid {
                return Err(RetainedResourceTransportError::InvalidRenderGeometry(index));
            }
            if index == expected_len {
                expected_len += 1;
            }
            updates.push((
                update.slot,
                RenderGeometrySlot {
                    generation: update.generation,
                    geometry: update.geometry.map(Arc::new),
                },
            ));
        }
        Ok(PreparedRetainedResourceAdditionsWithRender {
            ordinary,
            render_geometry_session: Some(resources.session),
            render_geometry_updates: updates,
            render_geometry_preparations: resources.preparations,
        })
    }

    pub(crate) fn commit_additions_with_render(
        &mut self,
        additions: PreparedRetainedResourceAdditionsWithRender,
    ) {
        let PreparedRetainedResourceAdditionsWithRender {
            ordinary,
            render_geometry_session,
            render_geometry_updates,
            render_geometry_preparations,
        } = additions;
        self.commit_additions(ordinary);
        if let Some(session) = render_geometry_session {
            self.render_geometry_session = Some(session);
            for (slot, update) in render_geometry_updates {
                if let Some(previous) = self.render_geometry_preparations.remove(&slot) {
                    self.render_geometry_preparation_count -= previous.len();
                }
                if slot as usize == self.render_geometries.len() {
                    self.render_geometries.push(update);
                } else {
                    self.render_geometries[slot as usize] = update;
                }
            }
            self.render_geometry_preparation_count += render_geometry_preparations.len();
            for preparation in render_geometry_preparations {
                self.render_geometry_preparations
                    .entry(preparation.resource)
                    .or_default()
                    .push(preparation);
            }
        }
    }
}

impl PreparedRetainedResourceAdditionsWithRender {
    pub(crate) fn image_handle_remap(&self) -> super::images::ImageHandles {
        self.ordinary.image_handle_remap()
    }

    pub(crate) fn text_handle_remap(
        &self,
    ) -> std::collections::HashMap<crate::TransportTextResourceHandle, noon_core::TextResourceHandle>
    {
        self.ordinary.text_handle_remap()
    }

    pub(crate) fn superseded_text_handles(&self) -> &[crate::TransportTextResourceHandle] {
        self.ordinary.superseded_text_handles()
    }

    pub(crate) fn text_lookup<'a>(
        &'a self,
        existing: &'a InstalledRetainedResources,
    ) -> InstalledTextResourceOverlay<'a> {
        self.ordinary.text_lookup(existing)
    }

    pub(crate) fn render_geometry_session(&self) -> Option<u32> {
        self.render_geometry_session
    }

    pub(crate) fn render_geometry_updates(&self) -> &[(u32, RenderGeometrySlot)] {
        &self.render_geometry_updates
    }
}

fn validate_render_geometry_resources(
    resources: &super::TransportRenderGeometryResources,
) -> Result<(), RetainedResourceTransportError> {
    let mut seen = HashSet::new();
    let mut live_updates = HashSet::new();
    for (index, update) in resources.updates.iter().enumerate() {
        if !seen.insert(update.slot)
            || update.geometry.as_ref().is_some_and(|geometry| {
                !matches!(geometry, GeometryRef::VectorPath(_)) || !geometry.is_finite()
            })
        {
            return Err(RetainedResourceTransportError::InvalidRenderGeometry(index));
        }
        if update.geometry.is_some() {
            live_updates.insert(update.slot);
        }
    }
    for (index, preparation) in resources.preparations.iter().enumerate() {
        if !preparation.is_finite() || !live_updates.contains(&preparation.resource) {
            return Err(RetainedResourceTransportError::InvalidRenderPreparation(
                index,
            ));
        }
    }
    Ok(())
}

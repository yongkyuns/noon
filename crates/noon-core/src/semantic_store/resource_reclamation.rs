//! Local semantic ownership accounting for immutable resource arena entries.
//!
//! Resource handles are versioned lookup capabilities, not owning leases.  This
//! index records only durable SemanticStore references and is updated at the
//! same object-content mutation points as the authored scene.  It deliberately
//! does not inspect runtime or compiled-plan resources: those own their own Arc
//! dependency closures until their revision is retired.

use std::collections::{HashMap, HashSet};

use crate::{
    FontResourceHandle, GeometryResourceHandle, RasterImageResourceHandle, SemanticObjectContent,
    SemanticObjectState, StoredGeometry, TextResourceHandle,
};

use super::SemanticStore;

#[derive(Clone, Debug, Default)]
pub(super) struct SemanticResourceReferences {
    geometries: HashMap<GeometryResourceHandle, usize>,
    texts: HashMap<TextResourceHandle, usize>,
    registered_texts: HashSet<TextResourceHandle>,
    images: HashMap<RasterImageResourceHandle, usize>,
    fonts: HashMap<FontResourceHandle, usize>,
}

#[derive(Debug, Default)]
pub(super) struct SemanticResourceRetirementCandidates {
    geometries: Vec<GeometryResourceHandle>,
    texts: Vec<TextResourceHandle>,
    images: Vec<RasterImageResourceHandle>,
    fonts: Vec<FontResourceHandle>,
}

impl SemanticResourceReferences {
    fn retain<T: std::hash::Hash + Eq + Copy>(references: &mut HashMap<T, usize>, handle: T) {
        *references.entry(handle).or_default() += 1;
    }

    fn release<T: std::hash::Hash + Eq + Copy>(
        references: &mut HashMap<T, usize>,
        handle: T,
    ) -> bool {
        let count = references
            .get_mut(&handle)
            .expect("semantic resource reference was retained before release");
        *count -= 1;
        if *count == 0 {
            references.remove(&handle);
            true
        } else {
            false
        }
    }
}

impl SemanticStore {
    pub(super) fn retain_semantic_object_resources(&mut self, state: &SemanticObjectState) {
        self.retain_semantic_content_resources(state.content);
        self.retain_semantic_numeric_text_resources(state);
    }

    pub(super) fn release_semantic_object_resources(&mut self, state: &SemanticObjectState) {
        self.release_semantic_content_resources(state.content);
        self.release_semantic_numeric_text_resources(state);
    }

    pub(super) fn replace_semantic_object_content(
        &mut self,
        object: crate::SemanticNodeId,
        content: SemanticObjectContent,
    ) {
        let previous = self
            .node(object)
            .and_then(|node| node.semantic_object_state())
            .expect("preflighted semantic object remains valid while transaction owns the store")
            .content;
        if previous == content {
            return;
        }
        self.retain_semantic_content_resources(content);
        self.node_mut(object)
            .and_then(|node| node.semantic_object_state_mut())
            .expect("preflighted semantic object remains valid while transaction owns the store")
            .content = content;
        self.release_semantic_content_resources(previous);
    }

    pub(super) fn replace_semantic_decimal_number(
        &mut self,
        object: crate::SemanticNodeId,
        number: crate::SemanticDecimalNumber,
    ) {
        let previous = self
            .node(object)
            .and_then(|node| node.semantic_object_state())
            .and_then(SemanticObjectState::decimal_number)
            .cloned();
        self.retain_semantic_numeric_text_resources_for(number.binding());
        self.node_mut(object)
            .and_then(|node| node.semantic_object_state_mut())
            .expect("preflighted semantic object remains valid while transaction owns the store")
            .set_decimal_number(Some(number));
        self.release_semantic_numeric_text_resources_for(
            previous
                .as_ref()
                .and_then(crate::SemanticDecimalNumber::binding),
        );
    }

    fn retain_semantic_content_resources(&mut self, content: SemanticObjectContent) {
        match content {
            SemanticObjectContent::Geometry(StoredGeometry::Resource(handle)) => {
                if self.geometry_resources.get(handle).is_some() {
                    SemanticResourceReferences::retain(
                        &mut self.resource_references.geometries,
                        handle,
                    );
                }
            }
            SemanticObjectContent::Text(handle) if self.text_resources.get(handle).is_some() => {
                self.retain_semantic_text_resource(handle)
            }
            SemanticObjectContent::Image(content) => {
                if self
                    .raster_image_resources
                    .get(content.resource())
                    .is_some()
                {
                    SemanticResourceReferences::retain(
                        &mut self.resource_references.images,
                        content.resource(),
                    );
                }
            }
            SemanticObjectContent::Geometry(_) | SemanticObjectContent::Text(_) => {}
        }
    }

    fn release_semantic_content_resources(&mut self, content: SemanticObjectContent) {
        match content {
            SemanticObjectContent::Geometry(StoredGeometry::Resource(handle)) => {
                if self.resource_references.geometries.contains_key(&handle)
                    && SemanticResourceReferences::release(
                        &mut self.resource_references.geometries,
                        handle,
                    )
                {
                    self.resource_retirement_candidates.geometries.push(handle);
                }
            }
            SemanticObjectContent::Text(handle)
                if self.resource_references.texts.contains_key(&handle) =>
            {
                self.release_semantic_text_resource(handle)
            }
            SemanticObjectContent::Image(content) => {
                let handle = content.resource();
                if self.resource_references.images.contains_key(&handle)
                    && SemanticResourceReferences::release(
                        &mut self.resource_references.images,
                        handle,
                    )
                {
                    self.resource_retirement_candidates.images.push(handle);
                }
            }
            SemanticObjectContent::Geometry(_) | SemanticObjectContent::Text(_) => {}
        }
    }

    fn retain_semantic_numeric_text_resources(&mut self, state: &SemanticObjectState) {
        self.retain_semantic_numeric_text_resources_for(
            state
                .decimal_number()
                .and_then(crate::SemanticDecimalNumber::binding),
        );
    }

    fn retain_semantic_numeric_text_resources_for(
        &mut self,
        binding: Option<&crate::SemanticNumericTextBinding>,
    ) {
        if let Some(binding) = binding {
            for (_, handle) in binding.token_resources() {
                if self.text_resources.get(*handle).is_some() {
                    self.retain_semantic_text_resource(*handle);
                }
            }
        }
    }

    fn release_semantic_numeric_text_resources(&mut self, state: &SemanticObjectState) {
        self.release_semantic_numeric_text_resources_for(
            state
                .decimal_number()
                .and_then(crate::SemanticDecimalNumber::binding),
        );
    }

    fn release_semantic_numeric_text_resources_for(
        &mut self,
        binding: Option<&crate::SemanticNumericTextBinding>,
    ) {
        if let Some(binding) = binding {
            for (_, handle) in binding.token_resources() {
                if self.resource_references.texts.contains_key(handle) {
                    self.release_semantic_text_resource(*handle);
                }
            }
        }
    }

    fn retain_semantic_text_resource(&mut self, handle: TextResourceHandle) {
        SemanticResourceReferences::retain(&mut self.resource_references.texts, handle);
    }

    /// Keep a compiler-cache entry alive until that bounded cache evicts its
    /// complete compilation identity. This is deliberately the same direct
    /// reference accounting used by authored objects: a cache entry is a real
    /// owner of its helper text, rather than a hint that leaves payloads behind.
    pub(crate) fn retain_compiled_text_resource(&mut self, handle: TextResourceHandle) {
        if self.text_resources.get(handle).is_some() {
            self.retain_semantic_text_resource(handle);
        }
    }

    pub(crate) fn release_compiled_text_resource(&mut self, handle: TextResourceHandle) {
        if self.resource_references.texts.contains_key(&handle) {
            self.release_semantic_text_resource(handle);
        }
    }

    /// Text dependencies belong to the text arena entry itself, rather than only
    /// to an object that currently presents it. This keeps fonts and vector paths
    /// alive for inert compiled text awaiting attachment.
    pub(super) fn register_semantic_text_resource_dependencies(
        &mut self,
        handle: TextResourceHandle,
    ) {
        if !self.resource_references.registered_texts.insert(handle) {
            return;
        }
        let (geometries, fonts) = self.semantic_text_dependencies(handle);
        for geometry in geometries {
            SemanticResourceReferences::retain(&mut self.resource_references.geometries, geometry);
        }
        for font in fonts {
            SemanticResourceReferences::retain(&mut self.resource_references.fonts, font);
        }
    }

    fn release_semantic_text_resource(&mut self, handle: TextResourceHandle) {
        if SemanticResourceReferences::release(&mut self.resource_references.texts, handle) {
            self.resource_retirement_candidates.texts.push(handle);
        }
    }

    pub(crate) fn unregister_semantic_text_resource_dependencies(
        &mut self,
        handle: TextResourceHandle,
    ) {
        self.unregister_semantic_text_resource_dependencies_inner(handle, true);
    }

    /// Undo provisional admission without turning pre-existing raw resources
    /// into retirement candidates. The caller removes only the fresh text
    /// entry, leaving the arena exactly as it was before the failed callback.
    pub(crate) fn abort_semantic_text_resource_dependencies(&mut self, handle: TextResourceHandle) {
        self.unregister_semantic_text_resource_dependencies_inner(handle, false);
    }

    fn unregister_semantic_text_resource_dependencies_inner(
        &mut self,
        handle: TextResourceHandle,
        retire_dependencies: bool,
    ) {
        if !self.resource_references.registered_texts.remove(&handle) {
            return;
        }
        let (geometries, fonts) = self.semantic_text_dependencies(handle);
        for geometry in geometries {
            if SemanticResourceReferences::release(
                &mut self.resource_references.geometries,
                geometry,
            ) && retire_dependencies
            {
                self.resource_retirement_candidates
                    .geometries
                    .push(geometry);
            }
        }
        for font in fonts {
            if SemanticResourceReferences::release(&mut self.resource_references.fonts, font)
                && retire_dependencies
            {
                self.resource_retirement_candidates.fonts.push(font);
            }
        }
    }

    fn semantic_text_dependencies(
        &self,
        handle: TextResourceHandle,
    ) -> (Vec<GeometryResourceHandle>, Vec<FontResourceHandle>) {
        let resource = self
            .text_resources
            .get(handle)
            .expect("semantic text reference was validated before retention");
        let geometries = resource
            .vector_items
            .iter()
            .map(|item| item.geometry)
            .collect();
        let fonts = resource
            .runs
            .iter()
            .map(|run| {
                self.font_resources
                    .handle_for_face(&run.font)
                    .expect("semantic text font dependency was imported with its text resource")
            })
            .collect();
        (geometries, fonts)
    }

    pub(super) fn reclaim_semantic_resource_candidates(&mut self) {
        if self.resource_reclamation_defer_depth != 0 {
            return;
        }
        while !self.resource_retirement_candidates.texts.is_empty()
            || !self.resource_retirement_candidates.images.is_empty()
            || !self.resource_retirement_candidates.geometries.is_empty()
            || !self.resource_retirement_candidates.fonts.is_empty()
        {
            let candidates = std::mem::take(&mut self.resource_retirement_candidates);
            for handle in candidates.texts {
                if !self.resource_references.texts.contains_key(&handle)
                    && self.text_resources.current_handle(handle.id) == Some(handle)
                {
                    self.forget_compiled_text_resources_for(handle);
                    self.unregister_semantic_text_resource_dependencies(handle);
                    let _ = self.text_resources.remove(handle.id);
                }
            }
            for handle in candidates.images {
                if !self.resource_references.images.contains_key(&handle) {
                    self.raster_image_resources.remove(handle);
                }
            }
            for handle in candidates.geometries {
                if !self.resource_references.geometries.contains_key(&handle)
                    && self.geometry_resources.current_handle(handle.id) == Some(handle)
                {
                    let _ = self.geometry_resources.remove(handle.id);
                }
            }
            for handle in candidates.fonts {
                if !self.resource_references.fonts.contains_key(&handle) {
                    self.font_resources.remove(handle);
                }
            }
        }
    }

    pub(super) fn begin_semantic_resource_reclamation_defer(&mut self) {
        self.resource_reclamation_defer_depth += 1;
    }

    pub(super) fn end_semantic_resource_reclamation_defer(&mut self) {
        self.resource_reclamation_defer_depth = self
            .resource_reclamation_defer_depth
            .checked_sub(1)
            .expect("semantic resource reclamation defer depth is balanced");
        self.reclaim_semantic_resource_candidates();
    }

    pub(crate) fn rebuild_semantic_resource_references(&mut self) {
        self.resource_references = SemanticResourceReferences::default();
        let text_handles = self.text_resources.handles().collect::<Vec<_>>();
        for handle in text_handles {
            self.register_semantic_text_resource_dependencies(handle);
        }
        let compiled_handles = self
            .compiled_text_resources
            .values()
            .copied()
            .collect::<Vec<_>>();
        for handle in compiled_handles {
            self.retain_compiled_text_resource(handle);
        }
        let states = self
            .slots
            .iter()
            .filter_map(|slot| slot.node.as_ref())
            .filter_map(|node| node.semantic_object_state())
            .cloned()
            .collect::<Vec<_>>();
        for state in &states {
            self.retain_semantic_object_resources(state);
        }
    }
}

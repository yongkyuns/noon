//! Cold and live glyph-text admission coupled to one semantic publication.
use super::{SemanticStore, SemanticTextImportError};
use crate::{
    FontFaceIdentity, FontResourceKey, SemanticMutationTransaction,
    SemanticMutationTransactionError, SemanticMutationTransactionResult,
};
use crate::{
    FontResourceArena, GeometryResource, GeometryResourceArena, TextResource, TextResourceHandle,
};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

type PreparedGlyphFonts = BTreeMap<FontResourceKey, (FontFaceIdentity, Arc<[u8]>)>;

impl SemanticStore {
    /// Publish derived text views with their compiler font dependencies in one
    /// semantic transaction. Fonts can still be staged by an enclosing resource
    /// admission; a rejection retires every provisional view without publishing
    /// those fonts.
    pub fn with_derived_text_resources<T, E>(
        &mut self,
        resources: Vec<TextResource>,
        fonts: &FontResourceArena,
        publish: impl FnOnce(&mut Self, &[TextResourceHandle]) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError> + From<std::collections::TryReserveError>,
    {
        for resource in &resources {
            resource
                .validate()
                .map_err(SemanticTextImportError::Validation)?;
            for vector in resource.vector_items.iter() {
                let crate::GeometryResource::VectorPath(path) = self
                    .geometry_resources
                    .get(vector.geometry)
                    .ok_or(SemanticTextImportError::MissingGeometry(vector.geometry))?;
                if !path.is_finite() {
                    return Err(SemanticTextImportError::NonFiniteGeometry(vector.geometry).into());
                }
            }
        }

        let staged_fonts =
            self.stage_text_fonts(resources.iter().map(|resource| (resource, fonts)))?;
        self.with_preflighted_text_resources(
            resources
                .into_iter()
                .map(|resource| (resource, FontResourceArena::new()))
                .collect(),
            staged_fonts,
            publish,
        )
    }

    /// Cold/live admission for a compiler-identified glyph resource. A live
    /// cached handle is reused only after its resource liveness has been checked;
    /// a failed first publication never installs the identity.
    pub fn publish_compiled_glyph_detached_text<T, E>(
        &mut self,
        identity: crate::TextCompilationIdentity,
        resource: TextResource,
        fonts: FontResourceArena,
        build_state: impl FnOnce(TextResourceHandle) -> crate::SemanticObjectState,
        publish: impl FnOnce(&mut Self, SemanticMutationTransaction) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError> + From<std::collections::TryReserveError>,
    {
        if let Some(handle) = self.compiled_text_resources.get(&identity).copied() {
            if self.text_resources.get(handle).is_some() {
                let mut transaction = SemanticMutationTransaction::new();
                transaction.add_node(crate::SemanticNodeCreation::object(build_state(handle)));
                return publish(self, transaction);
            }
            self.forget_compiled_text_resource(&identity);
        }

        let installed = std::cell::Cell::new(None);
        let result = self.publish_glyph_detached_text(
            resource,
            fonts,
            |handle| {
                installed.set(Some(handle));
                build_state(handle)
            },
            publish,
        );
        if result.is_ok() {
            self.remember_compiled_text_resource(
                identity,
                installed
                    .get()
                    .expect("successful text admission has one handle"),
            );
        }
        result
    }

    /// Atomically admit one compiler-identified text resource with glyph fonts
    /// and imported vector paths. The callback is the sole cold/live semantic
    /// publication boundary; neither dependencies nor the identity cache survive
    /// a rejected publication.
    pub fn publish_compiled_detached_text<T, E>(
        &mut self,
        identity: crate::TextCompilationIdentity,
        resource: TextResource,
        fonts: FontResourceArena,
        geometry: &GeometryResourceArena,
        build_state: impl FnOnce(TextResourceHandle) -> crate::SemanticObjectState,
        publish: impl FnOnce(&mut Self, SemanticMutationTransaction) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError>
            + From<std::collections::TryReserveError>
            + From<crate::GeometryResourceError>,
    {
        self.publish_compiled_text_resource(identity, resource, fonts, geometry, |store, handle| {
            let mut transaction = SemanticMutationTransaction::new();
            transaction.add_node(crate::SemanticNodeCreation::object(build_state(handle)));
            publish(store, transaction)
        })
    }

    /// Admit or reuse one compiler-identified retained resource, then let the
    /// caller publish any atomic semantic structure that references it.
    pub fn publish_compiled_text_resource<T, E>(
        &mut self,
        identity: crate::TextCompilationIdentity,
        resource: TextResource,
        fonts: FontResourceArena,
        geometry: &GeometryResourceArena,
        publish: impl FnOnce(&mut Self, TextResourceHandle) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError>
            + From<std::collections::TryReserveError>
            + From<crate::GeometryResourceError>,
    {
        if let Some(handle) = self.compiled_text_resources.get(&identity).copied() {
            if self.text_resources.get(handle).is_some() {
                return publish(self, handle);
            }
            self.forget_compiled_text_resource(&identity);
        }

        let installed = std::cell::Cell::new(None);
        let result =
            self.with_compiled_text_resource(resource, fonts, geometry, |store, handle| {
                installed.set(Some(handle));
                publish(store, handle)
            });
        if result.is_ok() {
            self.remember_compiled_text_resource(
                identity,
                installed
                    .get()
                    .expect("successful text admission has one handle"),
            );
        }
        result
    }

    /// Publish exactly one detached glyph-text object through a caller-owned
    /// cold or live semantic publication route.
    ///
    /// Every text and font dependency is validated before a resource is inserted.
    /// The fresh text handle is usable only to build this helper's one detached
    /// object; it cannot be attached or applied to an existing object. On rejection
    /// the handle is removed generationally, while fonts remain staged. Fonts enter
    /// the append-only arena only after success.
    ///
    /// Vector text requires the geometry-aware import path instead.
    pub fn publish_glyph_detached_text<T, E>(
        &mut self,
        resource: TextResource,
        fonts: FontResourceArena,
        build_state: impl FnOnce(TextResourceHandle) -> crate::SemanticObjectState,
        publish: impl FnOnce(&mut Self, SemanticMutationTransaction) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError> + From<std::collections::TryReserveError>,
    {
        let mut inputs = Vec::new();
        inputs.try_reserve_exact(1)?;
        inputs.push((resource, fonts));
        self.with_glyph_text_resources(inputs, |store, handles| {
            let mut transaction = SemanticMutationTransaction::new();
            transaction.add_node(crate::SemanticNodeCreation::object(build_state(handles[0])));
            publish(store, transaction)
        })
    }

    /// Admit one compiled text resource and publish through the caller's cold or
    /// live transaction owner. Fonts, vector paths and the text resource share
    /// one rollback scope; a rejected publication leaves no imported resources.
    pub fn with_compiled_text_resource<E, T>(
        &mut self,
        resource: TextResource,
        fonts: FontResourceArena,
        geometry: &GeometryResourceArena,
        publish: impl FnOnce(&mut Self, TextResourceHandle) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError>
            + From<std::collections::TryReserveError>
            + From<crate::GeometryResourceError>,
    {
        let staged_fonts = self.preflight_text_fonts(&[(resource.clone(), fonts.clone())])?;
        self.with_preflighted_compiled_text_resource(resource, geometry, staged_fonts, publish)
    }

    /// Import one preflighted compiled dependency while leaving its fonts staged
    /// for the aggregate publication. This keeps every dependency rollback-safe
    /// until the composed resource has been accepted.
    fn with_preflighted_compiled_text_resource<E, T>(
        &mut self,
        mut resource: TextResource,
        geometry: &GeometryResourceArena,
        staged_fonts: PreparedGlyphFonts,
        publish: impl FnOnce(&mut Self, TextResourceHandle) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError>
            + From<std::collections::TryReserveError>
            + From<crate::GeometryResourceError>,
    {
        resource
            .validate()
            .map_err(SemanticTextImportError::Validation)?;
        let mut sources = Vec::new();
        let mut indices = HashMap::new();
        for vector in resource.vector_items.iter() {
            if let std::collections::hash_map::Entry::Vacant(entry) = indices.entry(vector.geometry)
            {
                let GeometryResource::VectorPath(path) = geometry
                    .get(vector.geometry)
                    .ok_or(SemanticTextImportError::MissingGeometry(vector.geometry))?;
                if !path.is_finite() {
                    return Err(SemanticTextImportError::NonFiniteGeometry(vector.geometry).into());
                }
                let index = sources.len();
                sources.push(path.as_ref().clone());
                entry.insert(index);
            }
        }
        self.with_geometry_paths(sources, |store, geometry_handles| {
            for vector in std::sync::Arc::make_mut(&mut resource.vector_items) {
                vector.geometry = geometry_handles[indices[&vector.geometry]];
            }
            store.with_preflighted_text_resources(
                vec![(resource, FontResourceArena::new())],
                staged_fonts,
                |store, text_handles| publish(store, text_handles[0]),
            )
        })
    }

    /// Admit compiler-identified immutable dependencies for one composed text
    /// publication. Dependencies and the derived resource share the caller's
    /// rollback scope, so live and cold authors can use the same admission path.
    pub fn with_compiled_text_dependencies<E, T>(
        &mut self,
        dependencies: Vec<(
            crate::TextCompilationIdentity,
            TextResource,
            FontResourceArena,
            GeometryResourceArena,
        )>,
        compose: impl FnOnce(&Self, &[TextResourceHandle]) -> Result<TextResource, E>,
        publish: impl FnOnce(&mut Self, TextResourceHandle) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError>
            + From<std::collections::TryReserveError>
            + From<crate::GeometryResourceError>,
    {
        self.with_compiled_text_dependency_batch(
            dependencies,
            |store, handles| compose(store, handles).map(|resource| vec![resource]),
            |store, handles| publish(store, handles[0]),
        )
    }

    /// Admit a batch of derived resources using shared compiler dependencies.
    /// Duplicate identities resolve once and all provisional handles disappear
    /// if composition or semantic publication is rejected.
    pub fn with_compiled_text_dependency_batch<E, T>(
        &mut self,
        dependencies: Vec<(
            crate::TextCompilationIdentity,
            TextResource,
            FontResourceArena,
            GeometryResourceArena,
        )>,
        compose: impl FnOnce(&Self, &[TextResourceHandle]) -> Result<Vec<TextResource>, E>,
        publish: impl FnOnce(&mut Self, &[TextResourceHandle]) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError>
            + From<std::collections::TryReserveError>
            + From<crate::GeometryResourceError>,
    {
        let staged_fonts = self.preflight_compiled_dependency_fonts(&dependencies)?;
        let mut resolved = HashMap::new();
        let mut handles = Vec::new();
        let mut installed = Vec::new();
        resolved.try_reserve(dependencies.len())?;
        handles.try_reserve_exact(dependencies.len())?;
        installed.try_reserve_exact(dependencies.len())?;

        self.begin_semantic_resource_reclamation_defer();
        let newly_interned_fonts = self.intern_prepared_fonts(&staged_fonts);
        let result = (|| {
            for (identity, resource, _fonts, geometry) in dependencies {
                let cached = resolved.get(&identity).copied().or_else(|| {
                    self.compiled_text_resources
                        .get(&identity)
                        .copied()
                        .filter(|handle| self.text_resources.get(*handle).is_some())
                });
                let handle = if let Some(handle) = cached {
                    handle
                } else {
                    self.with_preflighted_compiled_text_resource(
                        resource,
                        &geometry,
                        BTreeMap::new(),
                        |_, handle| Ok::<_, E>(handle),
                    )?
                };
                if cached.is_none() {
                    installed.push((identity.clone(), handle));
                }
                resolved.insert(identity, handle);
                handles.push(handle);
            }
            let resources = compose(self, &handles)?;
            for resource in &resources {
                self.validate_existing_text_dependencies(resource, &staged_fonts)?;
            }
            self.with_preflighted_text_resources(
                resources
                    .into_iter()
                    .map(|resource| (resource, FontResourceArena::new()))
                    .collect(),
                staged_fonts,
                publish,
            )
        })();
        if result.is_ok() {
            for (identity, handle) in installed {
                self.remember_compiled_text_resource(identity, handle);
            }
        } else {
            for (_, handle) in installed {
                self.unregister_semantic_text_resource_dependencies(handle);
                let resource = self
                    .text_resources
                    .remove(handle.id)
                    .expect("unpublished dependency remains removable");
                let paths = resource
                    .vector_items
                    .iter()
                    .map(|item| item.geometry)
                    .collect::<std::collections::HashSet<_>>();
                for path in paths {
                    self.geometry_resources
                        .remove(path.id)
                        .expect("unpublished dependency path remains removable");
                }
            }
            for handle in newly_interned_fonts {
                self.font_resources.remove(handle);
            }
        }
        self.end_semantic_resource_reclamation_defer();
        result
    }

    fn with_glyph_text_resources<T, E>(
        &mut self,
        inputs: Vec<(TextResource, FontResourceArena)>,
        publish: impl FnOnce(&mut Self, &[TextResourceHandle]) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError> + From<std::collections::TryReserveError>,
    {
        let fonts = self.preflight_glyph_text_resources(&inputs)?;
        self.with_preflighted_text_resources(inputs, fonts, publish)
    }

    fn with_preflighted_text_resources<T, E>(
        &mut self,
        inputs: Vec<(TextResource, FontResourceArena)>,
        fonts: PreparedGlyphFonts,
        publish: impl FnOnce(&mut Self, &[TextResourceHandle]) -> Result<T, E>,
    ) -> Result<T, E>
    where
        E: From<SemanticTextImportError> + From<std::collections::TryReserveError>,
    {
        let mut handles = Vec::new();
        handles.try_reserve_exact(inputs.len())?;
        // The callback may replace the final current owner of a font or vector
        // dependency with one of these fresh text handles. Keep reclamation
        // deferred while their dependency closure is made visible, so the
        // nested semantic transaction cannot retire that shared dependency in
        // the gap before this scope finishes publication.
        self.begin_semantic_resource_reclamation_defer();
        let newly_interned_fonts = self.intern_prepared_fonts(&fonts);
        for (resource, _) in inputs {
            let handle = self
                .text_resources
                .insert(resource)
                .expect("text batch preflighted");
            self.register_semantic_text_resource_dependencies(handle);
            handles.push(handle);
        }

        let result = publish(self, &handles);
        if result.is_err() {
            for handle in handles {
                self.unregister_semantic_text_resource_dependencies(handle);
                self.text_resources
                    .remove(handle.id)
                    .expect("fresh unpublished text is removable");
            }
            for handle in newly_interned_fonts {
                self.font_resources.remove(handle);
            }
        }
        self.end_semantic_resource_reclamation_defer();
        result
    }

    fn intern_prepared_fonts(
        &mut self,
        fonts: &PreparedGlyphFonts,
    ) -> Vec<crate::FontResourceHandle> {
        fonts
            .values()
            .filter_map(|(face, data)| {
                let was_present = self.font_resources.get_for_face(face).is_some();
                let handle = self
                    .font_resources
                    .intern_face(face, data.clone())
                    .expect("font batch preflighted");
                (!was_present).then_some(handle)
            })
            .collect()
    }

    /// Publish glyph-only text resources and one cold semantic transaction.
    ///
    /// This is deliberately NOT a live-execution publication callback. Live
    /// detached construction uses [`Self::publish_glyph_detached_text`].
    pub fn apply_glyph_text_transaction<E>(
        &mut self,
        inputs: Vec<(TextResource, FontResourceArena)>,
        build: impl FnOnce(&[TextResourceHandle]) -> Result<SemanticMutationTransaction, E>,
    ) -> Result<SemanticMutationTransactionResult, E>
    where
        E: From<SemanticTextImportError>
            + From<SemanticMutationTransactionError>
            + From<std::collections::TryReserveError>,
    {
        self.with_glyph_text_resources(inputs, |store, handles| {
            build(handles).and_then(|transaction| transaction.apply(store).map_err(E::from))
        })
    }

    fn preflight_glyph_text_resources(
        &self,
        inputs: &[(TextResource, FontResourceArena)],
    ) -> Result<PreparedGlyphFonts, SemanticTextImportError> {
        self.validate_text_resources(inputs)?;
        for (resource, _) in inputs {
            if let Some(vector) = resource.vector_items.first() {
                return Err(SemanticTextImportError::MissingGeometry(vector.geometry));
            }
        }
        self.stage_text_fonts(inputs.iter().map(|(resource, fonts)| (resource, fonts)))
    }

    fn preflight_compiled_dependency_fonts(
        &self,
        dependencies: &[(
            crate::TextCompilationIdentity,
            TextResource,
            FontResourceArena,
            GeometryResourceArena,
        )],
    ) -> Result<PreparedGlyphFonts, SemanticTextImportError> {
        let mut fonts = BTreeMap::new();
        for (_, resource, source, _) in dependencies {
            resource
                .validate()
                .map_err(SemanticTextImportError::Validation)?;
            self.stage_one_text_resource_fonts(resource, source, &mut fonts)?;
        }
        Ok(fonts)
    }

    fn validate_existing_text_dependencies(
        &self,
        resource: &TextResource,
        staged_fonts: &PreparedGlyphFonts,
    ) -> Result<(), SemanticTextImportError> {
        resource
            .validate()
            .map_err(SemanticTextImportError::Validation)?;
        for run in resource.runs.iter() {
            let key = FontResourceKey::from_face(&run.font);
            if self.font_resources.get_for_face(&run.font).is_none()
                && !staged_fonts.contains_key(&key)
            {
                return Err(SemanticTextImportError::MissingFont(key));
            }
        }
        for vector in resource.vector_items.iter() {
            let crate::GeometryResource::VectorPath(path) = self
                .geometry_resources
                .get(vector.geometry)
                .ok_or(SemanticTextImportError::MissingGeometry(vector.geometry))?;
            if !path.is_finite() {
                return Err(SemanticTextImportError::NonFiniteGeometry(vector.geometry));
            }
        }
        Ok(())
    }

    fn preflight_text_fonts(
        &self,
        inputs: &[(TextResource, FontResourceArena)],
    ) -> Result<PreparedGlyphFonts, SemanticTextImportError> {
        self.validate_text_resources(inputs)?;
        self.stage_text_fonts(inputs.iter().map(|(resource, fonts)| (resource, fonts)))
    }

    fn validate_text_resources(
        &self,
        inputs: &[(TextResource, FontResourceArena)],
    ) -> Result<(), SemanticTextImportError> {
        for (resource, _) in inputs {
            resource
                .validate()
                .map_err(SemanticTextImportError::Validation)?;
        }
        Ok(())
    }

    fn stage_text_fonts<'a>(
        &self,
        inputs: impl IntoIterator<Item = (&'a TextResource, &'a FontResourceArena)>,
    ) -> Result<PreparedGlyphFonts, SemanticTextImportError> {
        let mut fonts = BTreeMap::new();
        for (resource, source) in inputs {
            self.stage_one_text_resource_fonts(resource, source, &mut fonts)?;
        }
        Ok(fonts)
    }

    fn stage_one_text_resource_fonts(
        &self,
        resource: &TextResource,
        source: &FontResourceArena,
        fonts: &mut PreparedGlyphFonts,
    ) -> Result<(), SemanticTextImportError> {
        for run in resource.runs.iter() {
            let key = FontResourceKey::from_face(&run.font);
            let incoming = source
                .get_for_face(&run.font)
                .ok_or_else(|| SemanticTextImportError::MissingFont(key.clone()))?;
            let existing = self
                .font_resources
                .get_for_face(&run.font)
                .map(|font| &font.data)
                .or_else(|| fonts.get(&key).map(|(_, data)| data));
            if let Some(existing) = existing {
                if !Arc::ptr_eq(existing, &incoming.data) && *existing != incoming.data {
                    return Err(SemanticTextImportError::Font(
                        crate::FontResourceError::ConflictingResource(key),
                    ));
                }
            }
            fonts
                .entry(key)
                .or_insert_with(|| (run.font.clone(), incoming.data.clone()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, sync::Arc};

    use super::*;
    use crate::{
        Color, FontFaceIdentity, GlyphRun, PositionedGlyph, Rect, SemanticNodeCreation,
        SemanticObjectState, SemanticStyle, TextAffineTransform, TextClusterIdentity,
        TextDirection, TextPart, TextRenderItem, TextSourceKind, TextSourceSpan, Vec2,
    };

    #[derive(Debug)]
    enum Error {
        Import(SemanticTextImportError),
        Transaction(SemanticMutationTransactionError),
        Allocation(std::collections::TryReserveError),
        Geometry(crate::GeometryResourceError),
    }

    impl std::fmt::Display for Error {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Import(error) => error.fmt(formatter),
                Self::Transaction(error) => error.fmt(formatter),
                Self::Allocation(error) => error.fmt(formatter),
                Self::Geometry(error) => error.fmt(formatter),
            }
        }
    }

    impl std::error::Error for Error {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match self {
                Self::Import(error) => Some(error),
                Self::Transaction(error) => Some(error),
                Self::Allocation(error) => Some(error),
                Self::Geometry(error) => Some(error),
            }
        }
    }

    impl From<SemanticTextImportError> for Error {
        fn from(value: SemanticTextImportError) -> Self {
            Self::Import(value)
        }
    }

    impl From<SemanticMutationTransactionError> for Error {
        fn from(value: SemanticMutationTransactionError) -> Self {
            Self::Transaction(value)
        }
    }

    impl From<std::collections::TryReserveError> for Error {
        fn from(value: std::collections::TryReserveError) -> Self {
            Self::Allocation(value)
        }
    }

    impl From<crate::GeometryResourceError> for Error {
        fn from(value: crate::GeometryResourceError) -> Self {
            Self::Geometry(value)
        }
    }

    fn glyph_resource() -> (TextResource, FontResourceArena) {
        let font = FontFaceIdentity {
            family: Arc::from("test"),
            face_key: Arc::from("test-face"),
            face_index: 0,
            variation_key: Arc::from(""),
        };
        let mut fonts = FontResourceArena::new();
        fonts
            .intern_face(&font, Arc::<[u8]>::from([1, 2, 3]))
            .unwrap();
        let span = TextSourceSpan::new(0, 1);
        let glyph = PositionedGlyph {
            glyph_id: 1,
            cluster: TextClusterIdentity {
                source_span: span,
                cluster_ordinal: 0,
                semantic_key: None,
            },
            origin: Vec2::ZERO,
            advance: Vec2::new(1.0, 0.0),
            bounds: Rect::new(Vec2::ZERO, Vec2::new(1.0, 1.0)),
        };
        (
            TextResource {
                source: Arc::from("x"),
                kind: TextSourceKind::Plain,
                runs: Arc::from([GlyphRun {
                    font,
                    variations: Arc::from([]),
                    font_size: 1.0,
                    direction: TextDirection::LeftToRight,
                    fill: Some(Color::WHITE),
                    stroke: None,
                    transform: TextAffineTransform::IDENTITY,
                    glyphs: Arc::from([glyph]),
                }]),
                vector_items: Arc::from([]),
                render_items: Arc::from([TextRenderItem::GlyphRun(0)]),
                parts: Arc::from([TextPart {
                    source_span: span,
                    first_cluster: 0,
                    cluster_count: 1,
                    first_vector: 0,
                    vector_count: 0,
                    semantic_key: None,
                }]),
                bounds: Rect::new(Vec2::ZERO, Vec2::new(1.0, 1.0)),
                baseline: 0.0,
                layout_artifact: None,
            },
            fonts,
        )
    }

    fn glyph_vector_resource() -> (TextResource, FontResourceArena, GeometryResourceArena) {
        let (mut resource, fonts) = glyph_resource();
        let mut geometry = GeometryResourceArena::new();
        geometry.insert_path(
            crate::VectorPath::new()
                .move_to(Vec2::new(-1.0, -1.0))
                .line_to(Vec2::ZERO),
        );
        let handle = geometry.insert_path(
            crate::VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1.0, 1.0)),
        );
        resource.vector_items = Arc::from([crate::TextVectorItem {
            geometry: handle,
            transform: TextAffineTransform::IDENTITY,
            style: crate::TextVectorStyle::default(),
            source_span: None,
            semantic_key: None,
        }]);
        resource.render_items = Arc::from([TextRenderItem::GlyphRun(0), TextRenderItem::Vector(0)]);
        let mut parts = resource.parts.to_vec();
        parts[0].vector_count = 1;
        resource.parts = Arc::from(parts);
        (resource, fonts, geometry)
    }

    #[test]
    fn text_dependencies_survive_inert_admission_and_replacement() {
        for replace_during_admission in [false, true] {
            let mut store = SemanticStore::new();
            let geometry = store
                .insert_geometry_path(
                    crate::VectorPath::new()
                        .move_to(Vec2::ZERO)
                        .line_to(Vec2::new(2.0, 1.0)),
                )
                .unwrap();
            let (mut first, fonts) = glyph_resource();
            first.vector_items = Arc::from([crate::TextVectorItem {
                geometry,
                transform: TextAffineTransform::IDENTITY,
                style: crate::TextVectorStyle::default(),
                source_span: None,
                semantic_key: None,
            }]);
            first.render_items =
                Arc::from([TextRenderItem::GlyphRun(0), TextRenderItem::Vector(0)]);
            let mut parts = first.parts.to_vec();
            parts[0].vector_count = 1;
            first.parts = Arc::from(parts);
            let owner = Cell::new(None);

            store
                .with_derived_text_resources(vec![first.clone()], &fonts, |store, handles| {
                    let mut transaction = SemanticMutationTransaction::new();
                    let token = transaction.create_node(SemanticNodeCreation::object(
                        SemanticObjectState::new(handles[0]),
                    ));
                    let result = transaction.apply(store).map_err(Error::from)?;
                    owner.set(result.resolve(token));
                    Ok::<_, Error>(result)
                })
                .unwrap();
            let second = Cell::new(None);
            store
                .with_derived_text_resources(vec![first], &fonts, |store, handles| {
                    second.set(Some(handles[0]));
                    if replace_during_admission {
                        let mut replace = SemanticMutationTransaction::new();
                        replace.replace_content(owner.get().unwrap(), handles[0]);
                        replace.apply(store).map_err(Error::from)?;
                    }
                    Ok::<_, Error>(())
                })
                .unwrap();
            let second = second.get().unwrap();
            if !replace_during_admission {
                let mut replace = SemanticMutationTransaction::new();
                replace.replace_content(owner.get().unwrap(), second);
                replace.apply(&mut store).unwrap();
            }

            assert!(store.text_resources().get(second).is_some());
            assert!(store.geometry_resources().get(geometry).is_some());
            assert_eq!(store.font_resources().len(), 1);

            assert_eq!(
                store
                    .semantic_object_state_checked(owner.get().unwrap())
                    .unwrap()
                    .content,
                crate::SemanticObjectContent::Text(second)
            );
        }
    }

    fn identity(label: &str) -> crate::TextCompilationIdentity {
        crate::TextCompilationIdentity {
            descriptor: Arc::from(label.as_bytes()),
            font_contents: Arc::from([Arc::<[u8]>::from([1, 2, 3])]),
        }
    }

    #[test]
    fn compiled_glyph_dependency_batch_keeps_fonts_through_admission_and_rollback() {
        for reject in [false, true] {
            let (dependency, fonts) = glyph_resource();
            let composed = dependency.clone();
            let mut store = SemanticStore::new();
            let before = (
                store.text_resources().stats(),
                store.font_resources().stats(),
                store.geometry_resources().stats(),
            );
            let result = store.with_compiled_text_dependency_batch(
                vec![(
                    identity("glyph-dependency-batch"),
                    dependency,
                    fonts,
                    GeometryResourceArena::new(),
                )],
                |_, _| Ok::<_, Error>(vec![composed]),
                |store, handles| {
                    let mut transaction = SemanticMutationTransaction::new();
                    let mut state = SemanticObjectState::new(handles[1]);
                    if reject {
                        state.style.object_opacity = f64::NAN;
                    }
                    transaction.add_node(SemanticNodeCreation::object(state));
                    transaction.apply(store).map_err(Error::from)
                },
            );
            if reject {
                assert!(matches!(result, Err(Error::Transaction(_))));
                assert_eq!(
                    (
                        store.text_resources().stats(),
                        store.font_resources().stats(),
                        store.geometry_resources().stats(),
                    ),
                    before
                );
                assert!(store.compiled_text_resources.is_empty());
            } else {
                assert!(result.is_ok());
                assert_eq!(store.text_resources().len(), 2);
                assert_eq!(store.font_resources().len(), 1);
            }
        }
    }

    #[test]
    fn derived_text_parts_share_staged_fonts_and_parent_rollback() {
        for reject in [true, false] {
            let (resource, fonts, geometry) = glyph_vector_resource();
            let part_fonts = fonts.clone();
            let mut store = SemanticStore::new();
            let before = (
                store.text_resources().stats(),
                store.font_resources().stats(),
                store.geometry_resources().stats(),
                store.scene_revision(),
            );
            let result = store.publish_compiled_text_resource(
                identity("derived-parts"),
                resource,
                fonts,
                &geometry,
                |store, base| {
                    // The enclosing dependency scope installs fonts before the
                    // first text can retain its dependency closure.
                    assert_eq!(store.font_resources().len(), 1);
                    let resource = store.text_resources().get(base).unwrap();
                    let part = resource
                        .projected_part(&resource.parts[0], store.geometry_resources())
                        .unwrap();
                    store.with_derived_text_resources(vec![part], &part_fonts, |store, handles| {
                        assert_eq!(store.font_resources().len(), 1);
                        let mut transaction = SemanticMutationTransaction::new();
                        let mut state = SemanticObjectState::new(handles[0]);
                        if reject {
                            state.style.object_opacity = f64::NAN;
                        }
                        transaction.add_node(SemanticNodeCreation::object(state));
                        transaction.apply(store).map_err(Error::Transaction)
                    })
                },
            );
            if reject {
                assert!(matches!(result, Err(Error::Transaction(_))));
                assert_eq!(
                    (
                        store.text_resources().stats(),
                        store.font_resources().stats(),
                        store.geometry_resources().stats(),
                        store.scene_revision(),
                    ),
                    before
                );
                assert!(store.compiled_text_resources.is_empty());
            } else {
                let result = result.unwrap();
                assert_eq!(result.impacts().len(), 1);
                assert_eq!(store.font_resources().len(), 1);
                assert_eq!(store.text_resources().len(), 2);
                assert_eq!(store.geometry_resources().len(), 1);
            }
        }
    }

    #[test]
    fn rejected_detached_publication_releases_provisional_text_and_stages_fonts() {
        let (resource, fonts) = glyph_resource();
        let mut store = SemanticStore::new();
        let before_text = store.text_resources().stats();
        let before_fonts = store.font_resources().stats();
        let provisional = Cell::new(None);

        let error = store
            .publish_glyph_detached_text(
                resource,
                fonts,
                |handle| {
                    provisional.set(Some(handle));
                    let mut state = SemanticObjectState::new(handle);
                    state.style = SemanticStyle {
                        object_opacity: f64::NAN,
                        ..Default::default()
                    };
                    state
                },
                |store, transaction| {
                    let handle = provisional
                        .get()
                        .expect("state builder observed an inserted text resource");
                    assert!(store.text_resources().get(handle).is_some());
                    transaction.apply(store).map_err(Error::Transaction)
                },
            )
            .unwrap_err();
        assert!(matches!(error, Error::Transaction(_)), "{error:?}");
        let handle = provisional
            .get()
            .expect("publisher received fresh text handle");
        assert!(store.text_resources().get(handle).is_none());
        assert_eq!(store.text_resources().stats(), before_text);
        assert_eq!(store.font_resources().stats(), before_fonts);
    }

    #[test]
    fn compiled_vector_glyph_publication_rolls_back_every_dependency_on_rejection() {
        let (resource, fonts, geometry) = glyph_vector_resource();
        assert!(geometry.get(resource.vector_items[0].geometry).is_some());
        let mut store = SemanticStore::new();
        let before = (
            store.text_resources().stats(),
            store.font_resources().stats(),
            store.geometry_resources().stats(),
        );
        let error = store
            .publish_compiled_detached_text(
                identity("reject"),
                resource,
                fonts,
                &geometry,
                |handle| {
                    let mut state = SemanticObjectState::new(handle);
                    state.style.object_opacity = f64::NAN;
                    state
                },
                |store, transaction| transaction.apply(store).map_err(Error::Transaction),
            )
            .unwrap_err();
        assert!(matches!(error, Error::Transaction(_)));
        assert_eq!(
            (
                store.text_resources().stats(),
                store.font_resources().stats(),
                store.geometry_resources().stats()
            ),
            before
        );
        assert!(store.compiled_text_resources.is_empty());
    }

    #[test]
    fn compiled_vector_admission_rejects_missing_geometry_or_font_without_mutation() {
        let (resource, fonts, geometry) = glyph_vector_resource();
        let mut store = SemanticStore::new();
        let before = (
            store.text_resources().stats(),
            store.font_resources().stats(),
            store.geometry_resources().stats(),
        );
        let missing_geometry = GeometryResourceArena::new();
        assert!(matches!(
            store.publish_compiled_detached_text(
                identity("missing-geometry"),
                resource.clone(),
                fonts.clone(),
                &missing_geometry,
                SemanticObjectState::new,
                |store, transaction| transaction.apply(store).map_err(Error::Transaction),
            ),
            Err(Error::Import(SemanticTextImportError::MissingGeometry(_)))
        ));
        let missing_fonts = FontResourceArena::new();
        assert!(matches!(
            store.publish_compiled_detached_text(
                identity("missing-font"),
                resource,
                missing_fonts,
                &geometry,
                SemanticObjectState::new,
                |store, transaction| transaction.apply(store).map_err(Error::Transaction),
            ),
            Err(Error::Import(SemanticTextImportError::MissingFont(_)))
        ));
        assert_eq!(
            (
                store.text_resources().stats(),
                store.font_resources().stats(),
                store.geometry_resources().stats(),
            ),
            before
        );
        assert!(store.compiled_text_resources.is_empty());
    }

    #[test]
    fn glyph_only_admission_still_rejects_vector_dependencies() {
        let (resource, fonts, _) = glyph_vector_resource();
        let mut store = SemanticStore::new();
        assert!(matches!(
            store.publish_glyph_detached_text(
                resource,
                fonts,
                SemanticObjectState::new,
                |store, transaction| transaction.apply(store).map_err(Error::Transaction),
            ),
            Err(Error::Import(SemanticTextImportError::MissingGeometry(_)))
        ));
        assert_eq!(store.text_resources().len(), 0);
        assert_eq!(store.geometry_resources().len(), 0);
    }

    #[test]
    fn compiled_vector_glyph_success_remaps_once_and_cache_hit_reuses_handle() {
        let (resource, fonts, geometry) = glyph_vector_resource();
        assert!(geometry.get(resource.vector_items[0].geometry).is_some());
        let key = identity("success");
        let mut store = SemanticStore::new();
        let publish = |store: &mut SemanticStore, transaction: SemanticMutationTransaction| {
            transaction.apply(store).map_err(Error::Transaction)
        };
        let first = store
            .publish_compiled_detached_text(
                key.clone(),
                resource.clone(),
                fonts.clone(),
                &geometry,
                SemanticObjectState::new,
                publish,
            )
            .unwrap();
        let first_handle = match store
            .semantic_object_state_checked(*match first.impacts() {
                [crate::SemanticMutationImpact::NodeAdded { node }] => node,
                _ => panic!("one node"),
            })
            .unwrap()
            .content
        {
            crate::SemanticObjectContent::Text(handle) => handle,
            _ => panic!("text state"),
        };
        let mapped_geometry = store
            .text_resources()
            .get(first_handle)
            .expect("fresh text resource")
            .vector_items[0]
            .geometry;
        assert_ne!(mapped_geometry, resource.vector_items[0].geometry);
        assert!(store.geometry_resources().get(mapped_geometry).is_some());
        let counts = (
            store.text_resources().len(),
            store.font_resources().len(),
            store.geometry_resources().len(),
        );
        let second = store
            .publish_compiled_detached_text(
                key.clone(),
                resource,
                fonts,
                &geometry,
                SemanticObjectState::new,
                publish,
            )
            .unwrap();
        let second_handle = match store
            .semantic_object_state_checked(*match second.impacts() {
                [crate::SemanticMutationImpact::NodeAdded { node }] => node,
                _ => panic!("one node"),
            })
            .unwrap()
            .content
        {
            crate::SemanticObjectContent::Text(handle) => handle,
            _ => panic!("text state"),
        };
        assert_eq!(first_handle, second_handle);
        assert_eq!(
            (
                store.text_resources().len(),
                store.font_resources().len(),
                store.geometry_resources().len()
            ),
            counts
        );
        store.text_resources.remove(first_handle.id).unwrap();
        let (replacement_resource, replacement_fonts, replacement_geometry) =
            glyph_vector_resource();
        let replacement = store
            .publish_compiled_detached_text(
                key.clone(),
                replacement_resource,
                replacement_fonts,
                &replacement_geometry,
                SemanticObjectState::new,
                publish,
            )
            .unwrap();
        let replacement_handle = match store
            .semantic_object_state_checked(*match replacement.impacts() {
                [crate::SemanticMutationImpact::NodeAdded { node }] => node,
                _ => panic!("one node"),
            })
            .unwrap()
            .content
        {
            crate::SemanticObjectContent::Text(handle) => handle,
            _ => panic!("text state"),
        };
        assert_ne!(replacement_handle, first_handle);
        assert!(store.text_resources().get(replacement_handle).is_some());
        assert_eq!(
            store.compiled_text_resources.get(&key),
            Some(&replacement_handle)
        );
    }

    #[test]
    fn compiled_glyph_republication_replaces_stale_cache_accounting() {
        let mut store = SemanticStore::new();
        let identity = identity("glyph-republication");
        let expected_bytes = identity.descriptor.len()
            + identity
                .font_contents
                .iter()
                .map(|font| font.len())
                .sum::<usize>();
        let publish = |store: &mut SemanticStore| {
            let (resource, fonts) = glyph_resource();
            let result = store
                .publish_compiled_glyph_detached_text(
                    identity.clone(),
                    resource,
                    fonts,
                    SemanticObjectState::new,
                    |store, transaction| transaction.apply(store).map_err(Error::Transaction),
                )
                .unwrap();
            let node = match result.impacts() {
                [crate::SemanticMutationImpact::NodeAdded { node }] => *node,
                _ => panic!("one detached text node"),
            };
            store
                .semantic_object_state_checked(node)
                .unwrap()
                .content
                .text()
                .unwrap()
        };

        let mut handle = publish(&mut store);
        for _ in 0..3 {
            store.text_resources.remove(handle.id).unwrap();
            handle = publish(&mut store);
            assert_eq!(store.compiled_text_resources.get(&identity), Some(&handle));
            assert_eq!(store.compiled_text_resource_order.len(), 1);
            assert_eq!(store.compiled_text_resource_retained_bytes, expected_bytes);
        }
    }
}

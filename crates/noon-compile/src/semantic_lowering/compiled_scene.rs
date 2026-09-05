use std::collections::BTreeMap;

use noon_core::{
    GeometryRef, GeometryResource, GeometryResourceArena, GeometryResourceHandle, SemanticNodeId,
    SemanticObjectContent, StoredGeometry, TextResourceHandle,
};

use crate::{CompiledObject, CompiledScene, DynamicProperties};

use super::SemanticExecutionProjection;

/// Failure while materializing the object-value projection into the current
/// geometry-backed compiled execution representation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SemanticCompiledSceneError {
    TooManyObjects(usize),
    UnsupportedSignalBindings {
        node: SemanticNodeId,
        count: usize,
    },
    InvalidAnalyticGeometry {
        node: SemanticNodeId,
    },
    UnsupportedGeometryResource {
        node: SemanticNodeId,
        resource: GeometryResourceHandle,
    },
    UnsupportedText {
        node: SemanticNodeId,
        text: TextResourceHandle,
    },
}

impl std::fmt::Display for SemanticCompiledSceneError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooManyObjects(count) => {
                write!(formatter, "semantic projection contains too many objects: {count}")
            }
            Self::UnsupportedSignalBindings { node, count } => write!(
                formatter,
                "semantic object {}:{} carries {count} native-reactive signal binding(s) before compiled execution slots consume semantic bindings",
                node.slot(),
                node.generation()
            ),
            Self::InvalidAnalyticGeometry { node } => write!(
                formatter,
                "semantic object {}:{} contains non-finite analytic geometry",
                node.slot(),
                node.generation()
            ),
            Self::UnsupportedGeometryResource { node, resource } => write!(
                formatter,
                "semantic object {}:{} uses unresolved geometry resource {}@{}",
                node.slot(),
                node.generation(),
                resource.id.get(),
                resource.version
            ),
            Self::UnsupportedText { node, text } => write!(
                formatter,
                "semantic object {}:{} uses text resource {}@{} before text payloads enter the stable compiled slot path",
                node.slot(),
                node.generation(),
                text.id.get(),
                text.version
            ),
        }
    }
}

impl std::error::Error for SemanticCompiledSceneError {}

impl CompiledScene {
    /// Materialize the already value-lowered semantic object projection into the
    /// existing stable compiled object representation.
    ///
    /// This standalone compatibility entry remains fail-closed when native-reactive
    /// bindings are present. Callers that need the complete semantic execution
    /// boundary must use `lower_semantic_execution`, which lowers those bindings
    /// through the native reactive graph before object materialization.
    pub fn from_semantic_projection(
        projection: &SemanticExecutionProjection,
    ) -> Result<Self, SemanticCompiledSceneError> {
        materialize_semantic_projection(projection, false, None)
    }

    /// Materialize object values after the canonical A1.6 entry point has
    /// successfully consumed every semantic signal binding into the native reactive
    /// projection. Kept crate-private so standalone callers cannot silently drop
    /// bindings by opting into this path directly.
    pub(crate) fn from_semantic_projection_after_reactive_lowering(
        projection: &SemanticExecutionProjection,
        resources: &GeometryResourceArena,
    ) -> Result<Self, SemanticCompiledSceneError> {
        materialize_semantic_projection(projection, true, Some(resources))
    }
}

fn materialize_semantic_projection(
    projection: &SemanticExecutionProjection,
    reactive_bindings_lowered: bool,
    resources: Option<&GeometryResourceArena>,
) -> Result<CompiledScene, SemanticCompiledSceneError> {
    let mut ordered = projection.objects().iter().collect::<Vec<_>>();
    ordered.sort_by_key(|object| object.presentation.order_key());

    let count = ordered.len();
    if u32::try_from(count).is_err() {
        return Err(SemanticCompiledSceneError::TooManyObjects(count));
    }

    let mut objects = Vec::with_capacity(count);
    let mut object_indices = BTreeMap::new();

    for (index, object) in ordered.into_iter().enumerate() {
        if !reactive_bindings_lowered && !object.signal_bindings.is_empty() {
            return Err(SemanticCompiledSceneError::UnsupportedSignalBindings {
                node: object.semantic_id,
                count: object.signal_bindings.len(),
            });
        }

        let object_index =
            u32::try_from(index).map_err(|_| SemanticCompiledSceneError::TooManyObjects(count))?;
        let geometry = lower_geometry(object.semantic_id, object.content, resources)?;
        objects.push(CompiledObject {
            id: object.execution_id,
            geometry,
            base_transform: object.base_transform,
            base_style: object.base_style,
            dynamic: DynamicProperties::default(),
            live: true,
        });
        object_indices.insert(object.execution_id, object_index);
    }

    Ok(CompiledScene {
        live_object_count: objects.len(),
        objects,
        tracks: BTreeMap::new(),
        track_count: 0,
        object_indices,
        track_locators: BTreeMap::new(),
    })
}

fn lower_geometry(
    node: SemanticNodeId,
    content: SemanticObjectContent,
    resources: Option<&GeometryResourceArena>,
) -> Result<GeometryRef, SemanticCompiledSceneError> {
    match content {
        SemanticObjectContent::Geometry(StoredGeometry::Circle { radius }) => {
            if !radius.is_finite() {
                return Err(SemanticCompiledSceneError::InvalidAnalyticGeometry { node });
            }
            Ok(GeometryRef::circle(radius))
        }
        SemanticObjectContent::Geometry(StoredGeometry::Rectangle { size }) => {
            if !size.x.is_finite() || !size.y.is_finite() {
                return Err(SemanticCompiledSceneError::InvalidAnalyticGeometry { node });
            }
            Ok(GeometryRef::Rectangle { size })
        }
        SemanticObjectContent::Geometry(StoredGeometry::Line { start, end }) => {
            if !start.x.is_finite()
                || !start.y.is_finite()
                || !end.x.is_finite()
                || !end.y.is_finite()
            {
                return Err(SemanticCompiledSceneError::InvalidAnalyticGeometry { node });
            }
            Ok(GeometryRef::line(start, end))
        }
        SemanticObjectContent::Geometry(StoredGeometry::Resource(resource)) => {
            match resources.and_then(|arena| arena.get(resource)) {
                Some(GeometryResource::VectorPath(path)) => {
                    Ok(GeometryRef::path(path.as_ref().clone()))
                }
                None => {
                    Err(SemanticCompiledSceneError::UnsupportedGeometryResource { node, resource })
                }
            }
        }
        SemanticObjectContent::Text(text) => {
            Err(SemanticCompiledSceneError::UnsupportedText { node, text })
        }
    }
}

#[cfg(test)]
mod tests {
    use noon_core::{
        Color, GeometryId, SemanticObjectProperty, SemanticObjectState, SemanticPaint,
        SemanticStore, SemanticVec3, TextResourceId, Vec2,
    };

    use super::*;
    use crate::SemanticExecutionIndex;

    #[test]
    fn canonical_lowering_resolves_only_the_owning_stores_path() {
        use noon_core::VectorPath;
        let mut store = SemanticStore::new();
        let path = VectorPath::new().move_to(Vec2::ZERO).line_to(Vec2::ONE);
        let resource = store.insert_geometry_path(path.clone()).unwrap();
        let node = store
            .insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(resource)));
        store.attach_to_scene(node).unwrap();
        let mut index = SemanticExecutionIndex::new();
        let lowered = crate::lower_semantic_execution(&store, &mut index).unwrap();
        assert_eq!(
            lowered.compiled().objects()[0].geometry,
            GeometryRef::path(path)
        );
        let mut foreign = SemanticStore::new();
        foreign.insert_geometry_path(VectorPath::new()).unwrap();
        let node = foreign
            .insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(resource)));
        foreign.attach_to_scene(node).unwrap();
        assert!(
            crate::lower_semantic_execution(&foreign, &mut SemanticExecutionIndex::new()).is_err()
        );
    }

    fn circle(radius: f32) -> SemanticObjectState {
        SemanticObjectState::new(StoredGeometry::Circle { radius })
    }

    #[test]
    fn analytic_projection_materializes_values_and_semantic_painter_order() {
        let mut store = SemanticStore::new();

        let mut first_state = circle(1.0);
        first_state.set_z_index(5);
        first_state.transform.translation = SemanticVec3::new(2.0, 3.0, 4.0);
        first_state.style.fill = Some(SemanticPaint::Solid(Color::rgba(0.2, 0.3, 0.4, 0.5)));
        let first = store.insert_semantic_object(first_state);

        let mut second_state = circle(2.0);
        second_state.set_z_index(-1);
        let second = store.insert_semantic_object(second_state);

        let mut third_state = circle(3.0);
        third_state.set_z_index(5);
        let third = store.insert_semantic_object(third_state);

        let family = store.insert_family();
        for member in [third, first, second] {
            store.add_member(family, member).unwrap();
        }
        store.attach_to_scene(family).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let projection = index.lower_scene(&store).unwrap();
        assert_eq!(
            projection
                .objects()
                .iter()
                .map(|object| object.semantic_id)
                .collect::<Vec<_>>(),
            vec![third, first, second]
        );

        let compiled = CompiledScene::from_semantic_projection(&projection).unwrap();
        assert_eq!(
            compiled
                .objects()
                .iter()
                .map(|object| object.id)
                .collect::<Vec<_>>(),
            vec![
                index.execution_object_id(second).unwrap(),
                index.execution_object_id(first).unwrap(),
                index.execution_object_id(third).unwrap(),
            ]
        );
        assert_eq!(compiled.objects()[1].base_transform.translation.x, 2.0);
        assert_eq!(compiled.objects()[1].base_transform.translation.y, 3.0);
        assert_eq!(compiled.objects()[1].base_style.fill.unwrap().alpha, 0.5);
        assert!(compiled.tracks().is_empty());
    }

    #[test]
    fn non_finite_analytic_geometry_is_rejected_before_compiled_scene_creation() {
        let cases = [
            StoredGeometry::Circle { radius: f32::NAN },
            StoredGeometry::Rectangle {
                size: Vec2::new(f32::INFINITY, 1.0),
            },
            StoredGeometry::Line {
                start: Vec2::new(0.0, f32::NEG_INFINITY),
                end: Vec2::ZERO,
            },
        ];

        for geometry in cases {
            let mut store = SemanticStore::new();
            let node = store.insert_semantic_object(SemanticObjectState::new(geometry));
            store.attach_to_scene(node).unwrap();

            let mut index = SemanticExecutionIndex::new();
            let projection = index.lower_scene(&store).unwrap();
            assert_eq!(
                CompiledScene::from_semantic_projection(&projection).unwrap_err(),
                SemanticCompiledSceneError::InvalidAnalyticGeometry { node }
            );
        }
    }

    #[test]
    fn native_reactive_bindings_are_not_silently_dropped() {
        let mut store = SemanticStore::new();
        let signal = store.insert_semantic_input_signal(0.4_f64).unwrap();
        let node = store.insert_semantic_object(circle(1.0));
        store.attach_to_scene(node).unwrap();
        store
            .bind_semantic_signal(signal, node, SemanticObjectProperty::ObjectOpacity)
            .unwrap();

        let mut index = SemanticExecutionIndex::new();
        let projection = index.lower_scene(&store).unwrap();
        assert_eq!(
            CompiledScene::from_semantic_projection(&projection).unwrap_err(),
            SemanticCompiledSceneError::UnsupportedSignalBindings { node, count: 1 }
        );
    }

    #[test]
    fn versioned_geometry_resource_is_not_degraded_to_an_unversioned_key() {
        let mut store = SemanticStore::new();
        let resource = GeometryResourceHandle {
            arena: 0,
            id: GeometryId::new(7),
            version: 3,
        };
        let node = store
            .insert_semantic_object(SemanticObjectState::new(StoredGeometry::Resource(resource)));
        store.attach_to_scene(node).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let projection = index.lower_scene(&store).unwrap();
        assert_eq!(
            CompiledScene::from_semantic_projection(&projection).unwrap_err(),
            SemanticCompiledSceneError::UnsupportedGeometryResource { node, resource }
        );
    }

    #[test]
    fn text_waits_for_the_stable_compiled_text_payload_path() {
        let mut store = SemanticStore::new();
        let text = TextResourceHandle {
            id: TextResourceId::new(11),
            version: 4,
        };
        let node = store.insert_semantic_object(SemanticObjectState::new(text));
        store.attach_to_scene(node).unwrap();

        let mut index = SemanticExecutionIndex::new();
        let projection = index.lower_scene(&store).unwrap();
        assert_eq!(
            CompiledScene::from_semantic_projection(&projection).unwrap_err(),
            SemanticCompiledSceneError::UnsupportedText { node, text }
        );
    }
}

use std::collections::BTreeMap;

use noon_core::{
    GeometryRef, GeometryResourceHandle, SemanticNodeId, SemanticObjectContent, StoredGeometry,
    TextResourceHandle,
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
                "semantic object {}:{} uses versioned geometry resource {}@{} before the compiled resource key carries version identity",
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
    /// Frame-row order is the semantic presentation order. Execution identity is
    /// preserved through each object's derived execution key, so ordering does not
    /// manufacture or rewrite identity. Timeline/reactive declarations are outside
    /// this object-value projection and are lowered through their dedicated A1.6
    /// channels rather than being synthesized here.
    pub fn from_semantic_projection(
        projection: &SemanticExecutionProjection,
    ) -> Result<Self, SemanticCompiledSceneError> {
        let mut ordered = projection.objects().iter().collect::<Vec<_>>();
        ordered.sort_by_key(|object| object.presentation.order_key());

        let count = ordered.len();
        if u32::try_from(count).is_err() {
            return Err(SemanticCompiledSceneError::TooManyObjects(count));
        }

        let mut objects = Vec::with_capacity(count);
        let mut object_indices = BTreeMap::new();

        for (index, object) in ordered.into_iter().enumerate() {
            if !object.signal_bindings.is_empty() {
                return Err(SemanticCompiledSceneError::UnsupportedSignalBindings {
                    node: object.semantic_id,
                    count: object.signal_bindings.len(),
                });
            }

            let object_index = u32::try_from(index)
                .map_err(|_| SemanticCompiledSceneError::TooManyObjects(count))?;
            let geometry = lower_geometry(object.semantic_id, object.content)?;
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

        Ok(Self {
            live_object_count: objects.len(),
            objects,
            tracks: BTreeMap::new(),
            track_count: 0,
            object_indices,
            track_locators: BTreeMap::new(),
        })
    }
}

fn lower_geometry(
    node: SemanticNodeId,
    content: SemanticObjectContent,
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
            Err(SemanticCompiledSceneError::UnsupportedGeometryResource { node, resource })
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

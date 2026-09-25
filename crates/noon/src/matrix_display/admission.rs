use super::*;
use noon_core::{
    Bounds2D64, SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectProperty,
    SemanticObjectState, SemanticTransform2_5D, TextResource,
};

const BRACKET_HEIGHT: f64 = 0.5977;
type Dependency = (
    noon_core::TextCompilationIdentity,
    TextResource,
    noon_core::FontResourceArena,
    noon_core::GeometryResourceArena,
);

#[derive(Clone, Copy)]
pub(super) struct MatrixShape {
    rows: usize,
    columns: usize,
}
pub(super) fn matrix_shape<T>(rows: &[Vec<T>]) -> Result<MatrixShape, MatrixAuthoringError> {
    let Some(first) = rows.first().filter(|row| !row.is_empty()) else {
        return Err(MatrixAuthoringError::EmptyMatrix);
    };
    for row in rows {
        if row.len() != first.len() {
            return Err(MatrixAuthoringError::RaggedRows {
                expected: first.len(),
                actual: row.len(),
            });
        }
    }
    Ok(MatrixShape {
        rows: rows.len(),
        columns: first.len(),
    })
}

pub(super) enum MatrixPublisher<'a, 'session> {
    Store(Rc<RefCell<noon_core::SemanticStore>>),
    Scene(&'a mut Scene),
    Live(&'a mut crate::LiveSession<'session>),
}

impl MatrixPublisher<'_, '_> {
    fn store(&self) -> Rc<RefCell<noon_core::SemanticStore>> {
        match self {
            Self::Store(store) => Rc::clone(store),
            Self::Scene(scene) => Rc::clone(scene.integration_store()),
            Self::Live(live) => Rc::clone(live.integration_store()),
        }
    }
    fn entry_state(&self, object: &Mobject) -> Result<SemanticObjectState, AuthoringError> {
        match self {
            Self::Store(_) => object.state(),
            Self::Scene(scene) => scene.composite_entry_state(object),
            Self::Live(live) => live.composite_entry_state(object),
        }
    }
    fn composite_entries(
        &self,
        entries: &[MobjectTarget<'_>],
    ) -> Result<Vec<crate::composite_entry::CompositeEntry>, MatrixAuthoringError> {
        let store = self.store();
        crate::composite_entry::capture_entries_with(&store, entries, |object| {
            self.entry_state(object)
        })
        .map_err(Into::into)
    }
    fn with_publication<T>(
        self,
        operation: impl FnOnce(
            &mut noon_core::SemanticStore,
            &mut dyn FnMut(
                &mut noon_core::SemanticStore,
                SemanticMutationTransaction,
            )
                -> Result<noon_core::SemanticMutationTransactionResult, AuthoringError>,
        ) -> Result<T, TextAuthoringError>,
    ) -> Result<T, MatrixAuthoringError> {
        match self {
            Self::Store(store) => {
                let mut store = store.borrow_mut();
                let mut publish =
                    |store: &mut noon_core::SemanticStore,
                     transaction: SemanticMutationTransaction| {
                        transaction.apply(store).map_err(AuthoringError::from)
                    };
                operation(&mut store, &mut publish).map_err(Into::into)
            }
            Self::Scene(scene) => scene
                .with_semantic_publication(|store, publish| {
                    operation(store, publish).map_err(text_publication_error)
                })
                .map_err(Into::into),
            Self::Live(live) => live
                .with_semantic_publication(|store, publish| {
                    operation(store, publish).map_err(text_publication_error)
                })
                .map_err(|error| match error {
                    crate::LiveSessionError::Authoring(error) => {
                        MatrixAuthoringError::Semantic(error)
                    }
                    crate::LiveSessionError::Publication(error) => {
                        MatrixAuthoringError::Semantic(AuthoringError::ExecutionPublication(error))
                    }
                    other => MatrixAuthoringError::LiveSession(other),
                }),
        }
    }
}
fn text_publication_error(error: TextAuthoringError) -> AuthoringError {
    match error {
        TextAuthoringError::Semantic(error) => error,
        other => AuthoringError::InvalidRenderNumber {
            name: other.to_string(),
            value: f64::NAN,
        },
    }
}

struct PreparedText {
    dependency: Dependency,
    transform: SemanticTransform2_5D,
    style: noon_core::SemanticStyle,
    font_size: f64,
}
fn prepare_text(
    backend: &mut impl LatexBackend,
    source: String,
) -> Result<PreparedText, MatrixAuthoringError> {
    let (identity, resource, fonts, geometry, transform, style, font_size) =
        crate::latex_authoring::prepare_math_tex(crate::MathTex::from_strings([source])?, backend)?
            .into_compiled_resource_parts_with_presentation();
    Ok(PreparedText {
        dependency: (identity, resource, fonts, geometry),
        transform,
        style,
        font_size,
    })
}

fn text_state(
    handle: noon_core::TextResourceHandle,
    item: &PreparedText,
) -> Result<SemanticObjectState, TextAuthoringError> {
    let mut state = SemanticObjectState::new(handle);
    state.transform = item.transform;
    state.style = item.style.clone();
    state.set_text_presentation_baseline(crate::latex_authoring::latex_presentation_baseline(
        &item.dependency.1,
        item.transform,
        item.font_size,
    )?);
    Ok(state)
}
fn brackets(
    backend: &mut impl LatexBackend,
    rows: usize,
) -> Result<[PreparedText; 2], MatrixAuthoringError> {
    let empty = format!(
        r"\begin{{array}}{{c}}{}\end{{array}}",
        r"\quad \\".repeat(rows)
    );
    Ok([
        prepare_text(backend, format!("\\left[{}\\right.", empty))?,
        prepare_text(backend, format!("\\left.{}\\right]", empty))?,
    ])
}
fn text_bounds(resource: &TextResource, transform: SemanticTransform2_5D) -> Bounds2D64 {
    let mut bounds = Bounds2D64::point(0.0, 0.0);
    for (index, point) in [
        resource.bounds.min,
        noon_core::Vec2::new(resource.bounds.min.x, resource.bounds.max.y),
        noon_core::Vec2::new(resource.bounds.max.x, resource.bounds.min.y),
        resource.bounds.max,
    ]
    .into_iter()
    .enumerate()
    {
        let x = f64::from(point.x) * transform.scale.x;
        let y = f64::from(point.y) * transform.scale.y;
        let (sin, cos) = transform.rotation_z.sin_cos();
        let (x, y) = (
            x * cos - y * sin + transform.translation.x,
            x * sin + y * cos + transform.translation.y,
        );
        if index == 0 {
            bounds = Bounds2D64::point(x, y);
        } else {
            bounds.include(x, y);
        }
    }
    bounds
}
fn include(bounds: &mut Option<Bounds2D64>, next: Bounds2D64) {
    if let Some(bounds) = bounds {
        bounds.include(next.min_x, next.min_y);
        bounds.include(next.max_x, next.max_y);
    } else {
        *bounds = Some(next);
    }
}
fn layout_text(
    resources: &[&TextResource],
    transforms: &mut [SemanticTransform2_5D],
    shape: MatrixShape,
    options: MatrixOptions,
) -> Result<(Bounds2D64, usize), MatrixAuthoringError> {
    let mut bounds = None;
    for (index, (resource, transform)) in resources.iter().zip(transforms.iter_mut()).enumerate() {
        let raw = text_bounds(resource, *transform);
        transform.translation.x += (index % shape.columns) as f64 * options.h_buff - raw.max_x;
        transform.translation.y += -((index / shape.columns) as f64) * options.v_buff - raw.min_y;
        include(&mut bounds, text_bounds(resource, *transform));
    }
    let bounds = bounds.ok_or(MatrixAuthoringError::EmptyMatrix)?;
    Ok((
        bounds,
        (bounds.height() / BRACKET_HEIGHT).floor() as usize + 1,
    ))
}
fn place_brackets(
    entry_resources: &[&TextResource],
    entry_transforms: &mut [SemanticTransform2_5D],
    bracket: &mut [PreparedText; 2],
    entry_bounds: Bounds2D64,
    options: MatrixOptions,
) {
    let target_height = entry_bounds.height() + 2.0 * options.bracket_v_buff;
    for (index, item) in bracket.iter_mut().enumerate() {
        let natural = text_bounds(&item.dependency.1, item.transform);
        if options.stretch_brackets && natural.height() > 0.0 {
            item.transform.scale.y *= target_height / natural.height();
        }
        let placed = text_bounds(&item.dependency.1, item.transform);
        item.transform.translation.y +=
            (entry_bounds.min_y + entry_bounds.max_y - placed.min_y - placed.max_y) * 0.5;
        item.transform.translation.x += if index == 0 {
            entry_bounds.min_x - options.bracket_h_buff - placed.max_x
        } else {
            entry_bounds.max_x + options.bracket_h_buff - placed.min_x
        };
    }
    let mut all = None;
    for (resource, transform) in entry_resources.iter().zip(entry_transforms.iter()) {
        include(&mut all, text_bounds(resource, *transform));
    }
    for item in bracket.iter() {
        include(&mut all, text_bounds(&item.dependency.1, item.transform));
    }
    let all = all.expect("matrix has entries and brackets");
    let (x, y) = ((all.min_x + all.max_x) * 0.5, (all.min_y + all.max_y) * 0.5);
    for transform in entry_transforms {
        transform.translation.x -= x;
        transform.translation.y -= y;
    }
    for item in bracket {
        item.transform.translation.x -= x;
        item.transform.translation.y -= y;
    }
}

struct Published {
    result: noon_core::SemanticMutationTransactionResult,
    root: noon_core::SemanticLocalNodeToken,
    entries: noon_core::SemanticLocalNodeToken,
    left: noon_core::SemanticLocalNodeToken,
    right: noon_core::SemanticLocalNodeToken,
}
fn matrix_from_publication(
    store: Rc<RefCell<noon_core::SemanticStore>>,
    value: Published,
) -> Result<Matrix, MatrixAuthoringError> {
    let resolve = |token| {
        value
            .result
            .resolve(token)
            .ok_or(MatrixAuthoringError::InvalidStructure)
    };
    Ok(Matrix {
        family: MobjectFamily::from_node(Rc::clone(&store), resolve(value.root)?)?,
        entry_family: MobjectFamily::from_node(Rc::clone(&store), resolve(value.entries)?)?,
        left_bracket: Mobject::from_node(Rc::clone(&store), resolve(value.left)?)?,
        right_bracket: Mobject::from_node(store, resolve(value.right)?)?,
    })
}
fn stage_matrix(
    transaction: &mut SemanticMutationTransaction,
    entries: impl IntoIterator<Item = noon_core::SemanticTransactionNodeRef>,
    shape: MatrixShape,
    left: noon_core::SemanticLocalNodeToken,
    right: noon_core::SemanticLocalNodeToken,
) -> (
    noon_core::SemanticLocalNodeToken,
    noon_core::SemanticLocalNodeToken,
) {
    let leaves: Vec<_> = entries.into_iter().collect();
    let entry_family = transaction.create_node(SemanticNodeCreation::family());
    for row in leaves.chunks(shape.columns) {
        let row_family = transaction.create_node(SemanticNodeCreation::family());
        for leaf in row {
            transaction.add_member(row_family, *leaf);
        }
        transaction.add_member(entry_family, row_family);
    }
    let root = transaction.create_node(SemanticNodeCreation::family());
    transaction.add_member(root, entry_family);
    transaction.add_member(root, left);
    transaction.add_member(root, right);
    (root, entry_family)
}

pub(super) fn publish_text_matrix(
    publisher: MatrixPublisher<'_, '_>,
    backend: &mut impl LatexBackend,
    values: Vec<String>,
    shape: MatrixShape,
    options: MatrixOptions,
) -> Result<Matrix, MatrixAuthoringError> {
    let options = options.validate()?;
    let mut entries: Vec<_> = values
        .into_iter()
        .map(|value| prepare_text(backend, value))
        .collect::<Result<_, _>>()?;
    let mut transforms: Vec<_> = entries.iter().map(|item| item.transform).collect();
    let resources: Vec<_> = entries.iter().map(|item| &item.dependency.1).collect();
    let (bounds, rows) = layout_text(&resources, &mut transforms, shape, options)?;
    let mut brackets = brackets(backend, rows)?;
    place_brackets(&resources, &mut transforms, &mut brackets, bounds, options);
    for (item, transform) in entries.iter_mut().zip(transforms) {
        item.transform = transform;
    }
    let mut dependencies: Vec<Dependency> =
        entries.iter().map(|item| item.dependency.clone()).collect();
    dependencies.extend(brackets.iter().map(|item| item.dependency.clone()));
    let store = match &publisher {
        MatrixPublisher::Store(store) => Rc::clone(store),
        MatrixPublisher::Scene(scene) => Rc::clone(scene.integration_store()),
        MatrixPublisher::Live(live) => Rc::clone(live.integration_store()),
    };
    let count = entries.len();
    let publication = publisher.with_publication(move |semantic, publish| {
        semantic.with_compiled_text_dependency_batch::<TextAuthoringError, _>(
            dependencies,
            |semantic, handles| {
                handles
                    .iter()
                    .map(|handle| {
                        semantic
                            .text_resources()
                            .get(*handle)
                            .cloned()
                            .ok_or(TextAuthoringError::MissingGeometryResource)
                    })
                    .collect()
            },
            |semantic, handles| {
                let mut transaction = SemanticMutationTransaction::new();
                let leaves = entries
                    .iter()
                    .zip(&handles[..count])
                    .map(|(item, handle)| {
                        text_state(*handle, item).map(|state| {
                            transaction.create_node(SemanticNodeCreation::object(state))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let mut states = brackets
                    .iter()
                    .zip(&handles[count..])
                    .map(|(item, handle)| {
                        text_state(*handle, item).map(|state| {
                            transaction.create_node(SemanticNodeCreation::object(state))
                        })
                    });
                let left = states.next().expect("two brackets")?;
                let right = states.next().expect("two brackets")?;
                let (root, entries) = stage_matrix(
                    &mut transaction,
                    leaves.into_iter().map(Into::into),
                    shape,
                    left,
                    right,
                );
                let result =
                    publish(semantic, transaction).map_err(TextAuthoringError::Semantic)?;
                Ok(Published {
                    result,
                    root,
                    entries,
                    left,
                    right,
                })
            },
        )
    })?;
    matrix_from_publication(store, publication)
}

pub(super) fn publish_numeric_matrix(
    publisher: MatrixPublisher<'_, '_>,
    backend: &mut impl LatexBackend,
    values: Vec<f64>,
    shape: MatrixShape,
    format: DecimalFormat,
    options: MatrixOptions,
) -> Result<Matrix, MatrixAuthoringError> {
    let options = options.validate()?;
    let entries = values
        .into_iter()
        .map(|value| {
            crate::numeric_authoring::PreparedDecimalValue::prepare(
                backend,
                value,
                format.clone(),
                48.0,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let brackets = brackets(backend, shape.rows)?;
    let mut dependency: Vec<Dependency> = Vec::new();
    let mut ranges = Vec::new();
    for item in &entries {
        let start = dependency.len();
        dependency.extend(item.dependencies().iter().cloned());
        ranges.push(start..dependency.len());
    }
    dependency.extend(brackets.iter().map(|item| item.dependency.clone()));
    let store = match &publisher {
        MatrixPublisher::Store(store) => Rc::clone(store),
        MatrixPublisher::Scene(scene) => Rc::clone(scene.integration_store()),
        MatrixPublisher::Live(live) => Rc::clone(live.integration_store()),
    };
    let count = entries.len();
    let publication = publisher.with_publication(move |semantic, publish| {
        semantic.with_compiled_text_dependency_batch::<TextAuthoringError, _>(
            dependency,
            |semantic, handles| {
                let mut resources: Vec<TextResource> = entries
                    .iter()
                    .zip(&ranges)
                    .map(|(item, range)| item.compose_resource(semantic, &handles[range.clone()]))
                    .collect::<Result<_, _>>()?;
                resources.extend(
                    handles[handles.len() - 2..]
                        .iter()
                        .map(|handle| {
                            semantic
                                .text_resources()
                                .get(*handle)
                                .cloned()
                                .ok_or(TextAuthoringError::MissingGeometryResource)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                );
                Ok(resources)
            },
            |semantic, handles| {
                let resources = handles[..count]
                    .iter()
                    .map(|handle| {
                        semantic
                            .text_resources()
                            .get(*handle)
                            .ok_or(TextAuthoringError::MissingGeometryResource)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let mut transforms = vec![SemanticTransform2_5D::default(); count];
                let (bounds, _) = layout_text(&resources, &mut transforms, shape, options)
                    .map_err(|error| {
                        TextAuthoringError::Semantic(AuthoringError::InvalidRenderNumber {
                            name: error.to_string(),
                            value: f64::NAN,
                        })
                    })?;
                let mut brackets = brackets;
                place_brackets(&resources, &mut transforms, &mut brackets, bounds, options);
                let mut transaction = SemanticMutationTransaction::new();
                let leaves = entries
                    .iter()
                    .zip(&handles[..count])
                    .zip(&transforms)
                    .map(|((item, handle), transform)| {
                        item.decimal_state(semantic, *handle, *transform)
                            .map(|state| {
                                transaction.create_node(SemanticNodeCreation::object(state))
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let mut bracket_nodes =
                    brackets
                        .iter()
                        .zip(&handles[count..])
                        .map(|(item, handle)| {
                            text_state(*handle, item).map(|state| {
                                transaction.create_node(SemanticNodeCreation::object(state))
                            })
                        });
                let left = bracket_nodes.next().expect("two brackets")?;
                let right = bracket_nodes.next().expect("two brackets")?;
                let (root, entries) = stage_matrix(
                    &mut transaction,
                    leaves.into_iter().map(Into::into),
                    shape,
                    left,
                    right,
                );
                let result =
                    publish(semantic, transaction).map_err(TextAuthoringError::Semantic)?;
                Ok(Published {
                    result,
                    root,
                    entries,
                    left,
                    right,
                })
            },
        )
    })?;
    matrix_from_publication(store, publication)
}

pub(super) fn publish_existing_mobject_matrix(
    publisher: MatrixPublisher<'_, '_>,
    backend: &mut impl LatexBackend,
    entries: Vec<Mobject>,
    shape: MatrixShape,
    options: MatrixOptions,
) -> Result<Matrix, MatrixAuthoringError> {
    let targets = entries.iter().map(Into::into).collect();
    publish_target_mobject_matrix(publisher, backend, targets, shape, options)
}

pub(super) fn publish_target_mobject_matrix(
    publisher: MatrixPublisher<'_, '_>,
    backend: &mut impl LatexBackend,
    entries: Vec<MobjectTarget<'_>>,
    shape: MatrixShape,
    options: MatrixOptions,
) -> Result<Matrix, MatrixAuthoringError> {
    let options = options.validate()?;
    let entries = publisher.composite_entries(&entries)?;
    let mut bounds = None;
    let mut translations = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let item = entry
            .bounds()?
            .ok_or(MatrixAuthoringError::InvalidStructure)?;
        let dx = (index % shape.columns) as f64 * options.h_buff - item.max_x;
        let dy = -((index / shape.columns) as f64) * options.v_buff - item.min_y;
        let mut moved = item;
        moved.min_x += dx;
        moved.max_x += dx;
        moved.min_y += dy;
        moved.max_y += dy;
        include(&mut bounds, moved);
        for (leaf, state) in entry.leaves() {
            let mut translation = state.transform.translation;
            translation.x += dx;
            translation.y += dy;
            translations.push((leaf.node_id(), translation));
        }
    }
    let bounds = bounds.ok_or(MatrixAuthoringError::EmptyMatrix)?;
    let mut bracket = brackets(
        backend,
        (bounds.height() / BRACKET_HEIGHT).floor() as usize + 1,
    )?;
    let target_height = bounds.height() + 2.0 * options.bracket_v_buff;
    for (index, item) in bracket.iter_mut().enumerate() {
        let natural = text_bounds(&item.dependency.1, item.transform);
        if options.stretch_brackets && natural.height() > 0.0 {
            item.transform.scale.y *= target_height / natural.height();
        }
        let placed = text_bounds(&item.dependency.1, item.transform);
        item.transform.translation.y +=
            (bounds.min_y + bounds.max_y - placed.min_y - placed.max_y) * 0.5;
        item.transform.translation.x += if index == 0 {
            bounds.min_x - options.bracket_h_buff - placed.max_x
        } else {
            bounds.max_x + options.bracket_h_buff - placed.min_x
        };
    }
    let mut all = Some(bounds);
    for item in &bracket {
        include(&mut all, text_bounds(&item.dependency.1, item.transform));
    }
    let all = all.expect("existing Matrix has bounds");
    let center = ((all.min_x + all.max_x) * 0.5, (all.min_y + all.max_y) * 0.5);
    for translation in &mut translations {
        translation.x -= center.0;
        translation.y -= center.1;
    }
    for item in &mut bracket {
        item.transform.translation.x -= center.0;
        item.transform.translation.y -= center.1;
    }
    let store = match &publisher {
        MatrixPublisher::Store(store) => Rc::clone(store),
        MatrixPublisher::Scene(scene) => Rc::clone(scene.integration_store()),
        MatrixPublisher::Live(live) => Rc::clone(live.integration_store()),
    };
    let dependencies = bracket.iter().map(|item| item.dependency.clone()).collect();
    let publication = publisher.with_publication(move |semantic, publish| {
        semantic.with_compiled_text_dependency_batch::<TextAuthoringError, _>(
            dependencies,
            |semantic, handles| {
                handles
                    .iter()
                    .map(|handle| {
                        semantic
                            .text_resources()
                            .get(*handle)
                            .cloned()
                            .ok_or(TextAuthoringError::MissingGeometryResource)
                    })
                    .collect()
            },
            |semantic, handles| {
                let mut transaction = SemanticMutationTransaction::new();
                for (entry, translation) in translations {
                    transaction.set_property(
                        entry,
                        SemanticObjectProperty::Translation,
                        translation,
                    );
                }
                let mut nodes = bracket.iter().zip(handles).map(|(item, handle)| {
                    text_state(*handle, item)
                        .map(|state| transaction.create_node(SemanticNodeCreation::object(state)))
                });
                let left = nodes.next().expect("two brackets")?;
                let right = nodes.next().expect("two brackets")?;
                let roots = entries
                    .iter()
                    .map(|entry| entry.root().into())
                    .collect::<Vec<_>>();
                let (root, entry_family) =
                    stage_matrix(&mut transaction, roots, shape, left, right);
                let result =
                    publish(semantic, transaction).map_err(TextAuthoringError::Semantic)?;
                Ok(Published {
                    result,
                    root,
                    entries: entry_family,
                    left,
                    right,
                })
            },
        )
    })?;
    matrix_from_publication(store, publication)
}

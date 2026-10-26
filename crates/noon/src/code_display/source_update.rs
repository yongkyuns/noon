//! Bounded source edits preserve the listing's allocated frame and use ordinary
//! text resources, membership and atomic semantic publication.

use super::*;
use admission::{PreparedLine, Publisher};
use noon_core::{
    SemanticCodeSourceRecipe, SemanticMutationTransaction, SemanticNodeCreation,
    SemanticObjectState, SemanticTransform2_5D, TextResource,
};

impl Code {
    /// Replace one source row without reflowing or refitting the listing.
    ///
    /// The new row must fit the original/reserved layout. Its typography,
    /// contextual syntax and logical origin come from shared admission data.
    /// The requested row receives a fresh identity even for identical source;
    /// the previous row leaves this listing but retains any other memberships.
    /// Add that previous row to the scene explicitly before FadeOut/Write.
    /// Unchanged rows keep their identities and resources. Contextually changed
    /// syntax on other rows is republished in place in the same transaction.
    pub fn replace_line(
        &self,
        index: usize,
        source: &str,
    ) -> Result<CodeLineReplacement, CodeAuthoringError> {
        self.publish_line_replacement(None, index, source)
    }

    /// Live counterpart observing the same coherent effective Code frame.
    pub fn replace_line_in_live_session(
        &self,
        live: &mut LiveSession<'_>,
        index: usize,
        source: &str,
    ) -> Result<CodeLineReplacement, CodeAuthoringError> {
        self.publish_line_replacement(Some(live), index, source)
    }

    fn publish_line_replacement(
        &self,
        live: Option<&mut LiveSession<'_>>,
        index: usize,
        source: &str,
    ) -> Result<CodeLineReplacement, CodeAuthoringError> {
        let (layout, frame, _) =
            self.layout_frame(live.as_deref())
                .map_err(|error| match error {
                    CodeAuthoringError::UnsupportedLineRegion(reason) => {
                        CodeAuthoringError::UnsupportedSourceUpdate(reason)
                    }
                    other => other,
                })?;
        let metrics = layout.metrics();
        if index >= metrics.line_count {
            return Err(CodeAuthoringError::LineOutOfRange {
                index,
                count: metrics.line_count,
            });
        }
        if live.is_none() {
            let store = self.family.integration_store().borrow();
            if !store
                .semantic_object_state_checked(layout.nodes().panel)
                .map_err(AuthoringError::from)?
                .signal_bindings()
                .is_empty()
            {
                return Err(CodeAuthoringError::UnsupportedSourceUpdate(
                    "a reactive panel requires a coherent live session",
                ));
            }
        }
        let recipe = layout
            .source_recipe()
            .ok_or(CodeAuthoringError::UnsupportedSourceUpdate(
                "the listing has no retained source recipe",
            ))?;
        let replacement = normalize_source(source, recipe.tab_width)?;
        if replacement.contains('\n') {
            return Err(invalid("replacement source", "expected exactly one row"));
        }
        let store = Rc::clone(self.family.integration_store());
        let originals: Vec<_> = {
            let semantic = store.borrow();
            semantic
                .semantic_family_checked(self.code_lines.node_id())
                .map_err(AuthoringError::from)?
                .members_iter()
                .map(|node| {
                    let state = semantic
                        .semantic_object_state_checked(node)
                        .map_err(AuthoringError::from)?;
                    let handle = state
                        .content
                        .text()
                        .ok_or(CodeAuthoringError::InvalidStructure)?;
                    let resource = semantic
                        .text_resources()
                        .get(handle)
                        .ok_or(AuthoringError::MissingTextResource(handle))?;
                    if resource.source.contains(['\r', '\n']) {
                        return Err(CodeAuthoringError::InvalidStructure);
                    }
                    Ok((node, resource.clone()))
                })
                .collect::<Result<_, CodeAuthoringError>>()?
        };
        let source = originals
            .iter()
            .enumerate()
            .map(|(row, (_, resource))| {
                if row == index {
                    replacement.as_str()
                } else {
                    resource.source.as_ref()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        let highlighted = highlight::highlight(
            &source,
            recipe.language.as_deref(),
            &recipe.formatter_style,
            &recipe.syntax_colors,
            &recipe.token_colors,
        )
        .map_err(|error| CodeAuthoringError::Highlight(error.to_string()))?;
        let paragraph = paragraph_from_recipe(recipe)?;
        let mut edits = Vec::new();
        for (row, (highlighted_line, (node, previous_resource))) in
            highlighted.lines.into_iter().zip(&originals).enumerate()
        {
            let content = if row == index {
                let mut line = PreparedLine::new(
                    highlighted_line.source,
                    highlighted_line.fills,
                    highlighted.foreground,
                    &paragraph,
                )?;
                let (left, right) = line.ink_horizontal_bounds();
                let (bottom, top) = line.vertical_bounds();
                // Both original and reserved text were measured by this same path.
                // Permit only floating-point cancellation at the retained origin.
                let tolerance = 1.0e-9
                    * metrics
                        .body_right
                        .abs()
                        .max(metrics.body_left.abs())
                        .max(1.0);
                if recipe.logical_source_x + left < metrics.body_left - tolerance
                    || recipe.logical_source_x + right > metrics.body_right + tolerance
                    || bottom < metrics.row_bottom - tolerance
                    || top > metrics.row_top + tolerance
                {
                    return Err(CodeAuthoringError::SourceDoesNotFit { index: row });
                }
                line.position(
                    recipe.logical_source_x,
                    metrics.first_baseline - row as f64 * metrics.baseline_step,
                );
                line.transform = compose_frame(frame, line.transform)?;
                PreparedRowContent::Source(Box::new(line))
            } else {
                // Context changes paint, not source or typography. Reuse retained
                // glyphs and layout; untouched rows do not enter text shaping.
                let styled = previous_resource
                    .with_source_fills(&highlighted_line.fills)
                    .map_err(TextAuthoringError::TextSourceStyle)?;
                if &styled == previous_resource {
                    continue;
                }
                PreparedRowContent::Style(Box::new(styled))
            };
            let object = Mobject::from_node(Rc::clone(&store), *node)?;
            let previous = if let Some(live) = live.as_deref() {
                // Guard leases and derived content before capturing ordinary style.
                live.effective_content_placement(&object)
                    .map_err(CodeAuthoringError::LiveSession)?;
                live.capture_mobject_state(&object)
                    .map_err(CodeAuthoringError::LiveSession)?
            } else {
                let state = object.state()?;
                if !state.signal_bindings().is_empty() || state.transform.as_planar().is_none() {
                    return Err(CodeAuthoringError::UnsupportedSourceUpdate(
                        "the edited row has a reactive or spatial owner",
                    ));
                }
                state
            };
            edits.push(PreparedRowEdit {
                object,
                previous,
                content,
            });
        }
        let previous = Mobject::from_node(Rc::clone(&store), originals[index].0)?;
        let before = originals.get(index + 1).map(|(node, _)| *node);
        let family = self.code_lines.node_id();
        let publisher = live.map_or_else(|| Publisher::Store(Rc::clone(&store)), Publisher::Live);
        let current = publisher.publish_text(|semantic, publish| {
            let dependencies = edits
                .iter()
                .filter_map(|edit| match &edit.content {
                    PreparedRowContent::Source(line) => Some(line.dependency.clone()),
                    PreparedRowContent::Style(_) => None,
                })
                .collect();
            let style_resources: Vec<_> = edits
                .iter()
                .map(|edit| match &edit.content {
                    PreparedRowContent::Source(_) => None,
                    PreparedRowContent::Style(resource) => Some(resource.as_ref().clone()),
                })
                .collect();
            semantic.with_compiled_text_dependency_batch::<TextAuthoringError, _>(
                dependencies,
                |semantic, handles| {
                    let mut handles = handles.iter();
                    style_resources
                        .into_iter()
                        .map(|resource| match resource {
                            None => {
                                let handle =
                                    *handles.next().expect("one handle per prepared source row");
                                semantic
                                    .text_resources()
                                    .get(handle)
                                    .cloned()
                                    .ok_or(TextAuthoringError::MissingGeometryResource)
                            }
                            Some(resource) => Ok(resource),
                        })
                        .collect()
                },
                |semantic, handles| {
                    let mut transaction = SemanticMutationTransaction::new();
                    let mut replacement_node = None;
                    for (edit, handle) in edits.into_iter().zip(handles) {
                        let node = edit.object.node_id();
                        if let PreparedRowContent::Source(line) = edit.content {
                            // Preserve the removed row's currently observed appearance.
                            transaction.set_object_transform(node, edit.previous.transform);
                            transaction.replace_style(node, edit.previous.style.clone());
                            transaction.set_z_index(node, edit.previous.z_index());
                            let mut state = SemanticObjectState::new(*handle);
                            state.transform = line.transform.into();
                            let z_index = edit.previous.z_index();
                            state.style = edit.previous.style;
                            state.set_z_index(z_index);
                            let replacement =
                                transaction.create_node(SemanticNodeCreation::object(state));
                            transaction.remove_member(family, node);
                            transaction.add_member(family, replacement);
                            transaction.reorder_member(family, replacement, before);
                            replacement_node = Some(replacement);
                        } else {
                            transaction.replace_content(node, *handle);
                        }
                    }
                    let replacement =
                        replacement_node.expect("the requested row is always replaced");
                    let result =
                        publish(semantic, transaction).map_err(TextAuthoringError::Semantic)?;
                    result
                        .resolve(replacement)
                        .ok_or(TextAuthoringError::Semantic(
                            AuthoringError::UnresolvedCreatedNode(replacement),
                        ))
                },
            )
        })?;
        Ok(CodeLineReplacement {
            previous,
            current: Mobject::from_node(store, current)?,
        })
    }
}

struct PreparedRowEdit {
    object: Mobject,
    previous: SemanticObjectState,
    content: PreparedRowContent,
}

enum PreparedRowContent {
    Source(Box<PreparedLine>),
    Style(Box<TextResource>),
}

fn paragraph_from_recipe(
    recipe: &SemanticCodeSourceRecipe,
) -> Result<CodeParagraphConfig, CodeAuthoringError> {
    let font = NativeFontFace::new(
        Arc::clone(&recipe.font_family),
        Arc::clone(&recipe.font_data),
        recipe.font_face_index,
    )
    .and_then(|face| face.with_metrics_profile(recipe.font_metrics_profile))
    .map_err(TextAuthoringError::NativeText)?;
    Ok(CodeParagraphConfig {
        font: recipe.font_family.to_string(),
        font_face: Some(font),
        font_size: recipe.font_size,
        line_spacing: recipe.line_spacing,
        disable_ligatures: recipe.disable_ligatures,
    })
}

fn compose_frame(
    frame: SemanticTransform2_5D,
    local: SemanticTransform2_5D,
) -> Result<SemanticTransform2_5D, CodeAuthoringError> {
    let position = frame.transform_xyz(local.translation);
    let checked = crate::semantic_mobject::authoring_render_f64;
    Ok(SemanticTransform2_5D {
        translation: noon_core::SemanticVec3::new(
            checked("Code source x", position.x)?,
            checked("Code source y", position.y)?,
            checked("Code source z", position.z)?,
        ),
        scale: noon_core::SemanticVec3::new(
            checked("Code source scale.x", frame.scale.x * local.scale.x)?,
            checked("Code source scale.y", frame.scale.y * local.scale.y)?,
            checked("Code source scale.z", frame.scale.z * local.scale.z)?,
        ),
        rotation_z: checked("Code source rotation", frame.rotation_z)?,
    })
}

#[cfg(all(test, feature = "bundled-fonts"))]
mod tests;

//! Allocated Code rows become ordinary retained rectangles through shared publication.
use super::*;
use noon_core::{
    SemanticCodeLayout, SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectState,
    SemanticPaint, SemanticStyle, StoredGeometry,
};
use std::ops::Range;

impl CodeHighlightOptions {
    fn validate(self) -> Result<(), CodeAuthoringError> {
        if !self.padding.is_finite() || self.padding < 0.0 {
            return Err(invalid("highlight padding", self.padding));
        }
        if !self.fill_opacity.is_finite() || !(0.0..=1.0).contains(&self.fill_opacity) {
            return Err(invalid("highlight fill_opacity", self.fill_opacity));
        }
        if [
            self.color.red,
            self.color.green,
            self.color.blue,
            self.color.alpha,
        ]
        .into_iter()
        .any(|channel| !channel.is_finite() || !(0.0..=1.0).contains(&channel))
        {
            return Err(invalid("highlight color", format!("{:?}", self.color)));
        }
        Ok(())
    }
}

impl Code {
    /// Create a detached rectangle over a nonempty, zero-based half-open range.
    ///
    /// Regions describe the allocated source-row grid, including blank lines
    /// and indentation, rather than the selected glyphs' ink. Moving or editing
    /// individual text lines does not reflow these slots. The rectangle uses the
    /// panel's current pose, so rotation, reflection and representable scaling
    /// preserve its orientation. Padding is local to that pose.
    pub fn get_highlighted_lines(
        &self,
        lines: Range<usize>,
        options: CodeHighlightOptions,
    ) -> Result<Mobject, CodeAuthoringError> {
        self.publish_highlight(None, lines, options, false)
    }

    /// Create a rectangle and attach it after the panel and before the text.
    /// Later whole-Code transforms include it through ordinary family semantics.
    pub fn highlight_lines(
        &self,
        lines: Range<usize>,
        options: CodeHighlightOptions,
    ) -> Result<Mobject, CodeAuthoringError> {
        self.publish_highlight(None, lines, options, true)
    }

    /// Detached counterpart observing the live panel's coherent effective pose.
    pub fn get_highlighted_lines_in_live_session(
        &self,
        live: &mut LiveSession<'_>,
        lines: Range<usize>,
        options: CodeHighlightOptions,
    ) -> Result<Mobject, CodeAuthoringError> {
        self.publish_highlight(Some(live), lines, options, false)
    }

    /// Attach a highlight through the normal live semantic transaction route.
    pub fn highlight_lines_in_live_session(
        &self,
        live: &mut LiveSession<'_>,
        lines: Range<usize>,
        options: CodeHighlightOptions,
    ) -> Result<Mobject, CodeAuthoringError> {
        self.publish_highlight(Some(live), lines, options, true)
    }

    /// Resolve the sparse declaration and validate its small fixed topology.
    /// Row membership remains authoritative on the ordinary code-line family.
    pub(super) fn layout(&self) -> Result<SemanticCodeLayout, CodeAuthoringError> {
        self.family.validate()?;
        let store = self.family.integration_store().borrow();
        let layout = store
            .semantic_code_layout(self.family.node_id())
            .map_err(AuthoringError::from)?
            .ok_or(CodeAuthoringError::InvalidStructure)?;
        let nodes = layout.nodes();
        if nodes.background != self.background.node_id()
            || nodes.highlights != self.highlights.node_id()
            || nodes.code_lines != self.code_lines.node_id()
            || nodes.line_numbers != self.line_numbers.as_ref().map(MobjectFamily::node_id)
        {
            return Err(CodeAuthoringError::InvalidStructure);
        }
        let expected = [nodes.background, nodes.highlights]
            .into_iter()
            .chain(nodes.line_numbers)
            .chain([nodes.code_lines]);
        let root = store
            .semantic_family_checked(self.family.node_id())
            .map_err(AuthoringError::from)?;
        if !root.members_iter().eq(expected)
            || store
                .semantic_family_member_at_checked(nodes.background, 0)
                .map_err(AuthoringError::from)?
                != Some(nodes.panel)
            || store
                .semantic_family_checked(nodes.code_lines)
                .map_err(AuthoringError::from)?
                .member_count()
                != layout.metrics().line_count
        {
            return Err(CodeAuthoringError::InvalidStructure);
        }
        Ok(layout)
    }

    /// Prepare geometry without allocation; also used by live publication tests.
    pub(super) fn highlighted_state(
        &self,
        live: Option<&LiveSession<'_>>,
        lines: Range<usize>,
        options: CodeHighlightOptions,
    ) -> Result<SemanticObjectState, CodeAuthoringError> {
        options.validate()?;
        let layout = self.layout()?;
        let metrics = layout.metrics();
        if lines.start >= lines.end || lines.end > metrics.line_count {
            return Err(CodeAuthoringError::InvalidLineRange {
                start: lines.start,
                end: lines.end,
                count: metrics.line_count,
            });
        }
        let store = self.family.integration_store();
        let panel = Mobject::from_node(Rc::clone(store), layout.nodes().panel)?;
        let panel_state = panel.state()?;
        if panel_state.content.geometry() != Some(StoredGeometry::Resource(layout.panel_geometry()))
        {
            return Err(CodeAuthoringError::UnsupportedLineRegion(
                "the original panel geometry was replaced or baked",
            ));
        }
        let (transform, z_index) = if let Some(live) = live {
            live.effective_content_placement(&panel)
                .map_err(|error| match error {
                    LiveSessionError::Authoring(AuthoringError::Unsupported(
                        crate::UnsupportedAuthoringOperation::EffectiveLayoutRenderOverride,
                    )) => CodeAuthoringError::UnsupportedLineRegion(
                        "the panel has an unsupported effective content or spatial override",
                    ),
                    other => CodeAuthoringError::LiveSession(other),
                })?
        } else {
            (
                panel_state.transform.as_planar().ok_or(
                    CodeAuthoringError::UnsupportedLineRegion("the panel pose is not planar"),
                )?,
                panel_state.z_index(),
            )
        };
        let left = if options.include_line_numbers {
            metrics.gutter_left
        } else {
            metrics.body_left
        } - options.padding;
        let right = metrics.body_right + options.padding;
        let top = metrics.first_baseline - lines.start as f64 * metrics.baseline_step
            + metrics.row_top
            + options.padding;
        let bottom = metrics.first_baseline - (lines.end - 1) as f64 * metrics.baseline_step
            + metrics.row_bottom
            - options.padding;
        let width =
            crate::semantic_mobject::authoring_render_f64("Code highlight width", right - left)?;
        let height =
            crate::semantic_mobject::authoring_render_f64("Code highlight height", top - bottom)?;
        let (x, y) = transform.transform_xy((left + right) * 0.5, (bottom + top) * 0.5);
        let mut pose = transform;
        pose.translation.x = crate::semantic_mobject::authoring_render_f64("Code highlight x", x)?;
        pose.translation.y = crate::semantic_mobject::authoring_render_f64("Code highlight y", y)?;
        let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
            size: noon_core::Vec2::new(width as f32, height as f32),
        });
        state.transform = pose.into();
        state.set_z_index(z_index);
        state.style = SemanticStyle {
            fill: Some(SemanticPaint::Solid(options.color)),
            fill_opacity: options.fill_opacity,
            stroke: None,
            stroke_width: 0.0,
            ..Default::default()
        };
        Ok(state)
    }

    fn publish_highlight(
        &self,
        live: Option<&mut LiveSession<'_>>,
        lines: Range<usize>,
        options: CodeHighlightOptions,
        attached: bool,
    ) -> Result<Mobject, CodeAuthoringError> {
        let state = self.highlighted_state(live.as_deref(), lines, options)?;
        let store = Rc::clone(self.family.integration_store());
        let mut transaction = SemanticMutationTransaction::new();
        let highlight = transaction.create_node(SemanticNodeCreation::object(state));
        if attached {
            transaction.add_member(self.highlights.node_id(), highlight);
        }
        let publisher = match live {
            Some(live) => admission::Publisher::Live(live),
            None => admission::Publisher::Store(Rc::clone(&store)),
        };
        let result = publisher.publish_transaction(transaction)?;
        let node = result
            .resolve(highlight)
            .ok_or(AuthoringError::UnresolvedCreatedNode(highlight))?;
        Mobject::from_node(store, node).map_err(Into::into)
    }
}

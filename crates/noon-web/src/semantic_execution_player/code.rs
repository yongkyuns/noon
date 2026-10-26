//! Code construction uses the same atomic live publication as other text.
use super::SemanticExecutionPlayer;
use crate::authoring_error::AuthoringFailure;
use std::ops::Range;

impl SemanticExecutionPlayer {
    pub(crate) fn live_get_highlighted_code_lines(
        &mut self,
        code: &noon::Code,
        lines: Range<usize>,
        options: noon::CodeHighlightOptions,
    ) -> Result<noon::Mobject, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(code.get_highlighted_lines_in_live_session(live, lines, options))
        })?
        .map_err(AuthoringFailure::from)
    }

    pub(crate) fn live_highlight_code_lines(
        &mut self,
        code: &noon::Code,
        lines: Range<usize>,
        options: noon::CodeHighlightOptions,
    ) -> Result<noon::Mobject, AuthoringFailure> {
        self.with_live_session(|live| {
            Ok(code.highlight_lines_in_live_session(live, lines, options))
        })?
        .map_err(AuthoringFailure::from)
    }

    pub(crate) fn live_create_code(
        &mut self,
        source: &str,
        options: noon::CodeOptions,
    ) -> Result<noon::Code, AuthoringFailure> {
        self.with_live_session(|live| Ok(noon::Code::in_live_session(live, source, options)))?
            .map_err(AuthoringFailure::from)
    }
}

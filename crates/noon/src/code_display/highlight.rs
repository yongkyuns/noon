//! Native authoring-time syntax highlighting. The bundled state machines run
//! identically in native Rust and WASM; no Python lexer or HTML renderer is used.
//!
//! The four named palettes adapt Pygments 2.19.2 foreground/background colors.
//! Scope classification comes from Syntect's grammars, so this is a documented
//! syntax/color profile rather than an assertion of identical Pygments tokens.

use std::{collections::BTreeMap, fmt, ops::Range, sync::OnceLock};

use noon_core::{
    is_valid_code_color, is_valid_code_token_selector, CodeSyntaxColors, Color, TextSourceFill,
    TextSourceSpan,
};
use syntect::{
    easy::ScopeRangeIterator,
    highlighting::{
        Color as SyntaxColor, HighlightState, Highlighter, RangedHighlightIterator, StyleModifier,
        Theme, ThemeItem, ThemeSettings,
    },
    parsing::{ParseState, Scope, ScopeStack, SyntaxReference, SyntaxSet},
};

#[derive(Debug)]
pub(super) struct HighlightedCode {
    pub source: String,
    /// The selected grammar/profile, including admission-time autodetection.
    pub resolved_language: String,
    pub lines: Vec<HighlightedLine>,
    pub background: Color,
    pub foreground: Color,
    pub line_number: Color,
}

#[derive(Debug)]
pub(super) struct HighlightedLine {
    pub source: String,
    pub fills: Vec<TextSourceFill>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum HighlightError {
    UnknownLanguage(String),
    UnknownStyle(String),
    InvalidColors,
    InvalidToken(String),
    SourceTooLong,
    Parse { line: usize, message: String },
}

impl fmt::Display for HighlightError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownLanguage(language) => write!(
                f,
                "unsupported Code language {language:?}; use Code::get_languages_list()"
            ),
            Self::UnknownStyle(style) => write!(
                f,
                "unsupported Code formatter style {style:?}; use Code::get_styles_list()"
            ),
            Self::InvalidColors => write!(f, "Code syntax/token colors require finite RGBA channels in 0..=1"),
            Self::InvalidToken(token) => write!(f, "invalid Code token selector {token:?}; expected a whole identifier or dotted identifier path"),
            Self::SourceTooLong => write!(f, "Code source exceeds Noon's UTF-8 span space"),
            Self::Parse { line, message } => {
                write!(f, "Code highlighting failed on line {line}: {message}")
            }
        }
    }
}

impl std::error::Error for HighlightError {}

fn syntaxes() -> &'static SyntaxSet {
    static SYNTAXES: OnceLock<SyntaxSet> = OnceLock::new();
    SYNTAXES.get_or_init(two_face::syntax::extra_newlines)
}

pub(super) fn style_names() -> Vec<String> {
    ["emacs", "friendly", "monokai", "vim"]
        .map(str::to_owned)
        .into()
}

pub(super) fn language_names() -> Vec<String> {
    let mut names = syntaxes()
        .syntaxes()
        .iter()
        .filter(|syntax| !syntax.hidden)
        .map(|syntax| syntax.name.clone())
        .collect::<Vec<_>>();
    names.push("CUDA C++".into());
    names.sort_unstable();
    names.dedup();
    names
}

fn cuda_alias(language: &str) -> bool {
    matches!(
        language.to_ascii_lowercase().as_str(),
        "cuda" | "cuda c++" | "cuda-c++" | "cu" | "cuh"
    )
}

fn syntax_for_language(language: &str) -> Result<&'static SyntaxReference, HighlightError> {
    // Prefer exact names: a C source must not select the C++ `.C` extension.
    if let Some(syntax) = syntaxes().find_syntax_by_name(language) {
        return Ok(syntax);
    }
    let lower = language.to_ascii_lowercase();
    let token = match lower.as_str() {
        "plain" | "plaintext" | "plain text" | "text" => "txt",
        "python" | "python3" | "py3" => "py",
        "rust" => "rs",
        "c++" | "cxx" => "cpp",
        "javascript" => "js",
        "typescript" => "ts",
        "bash" | "shell" | "zsh" => "sh",
        _ if cuda_alias(language) => "cpp",
        _ => &lower,
    };
    syntaxes()
        .find_syntax_by_token(token)
        .ok_or_else(|| HighlightError::UnknownLanguage(language.into()))
}

/// Infer from a supplied name only; file access belongs to the native/Python
/// input boundary, not the highlighter. Compound extensions remain significant.
pub(super) fn language_for_file_name(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next()?;
    if name.rsplit('.').next().is_some_and(cuda_alias) {
        return Some("CUDA C++".into());
    }
    let mut suffix = name;
    loop {
        if let Some(syntax) = syntaxes().find_syntax_by_extension(suffix) {
            return Some(syntax.name.clone());
        }
        suffix = suffix.split_once('.')?.1;
    }
}

pub(super) fn highlight(
    source: &str,
    language: Option<&str>,
    style: &str,
    syntax_colors: &CodeSyntaxColors,
    token_colors: &BTreeMap<String, Color>,
) -> Result<HighlightedCode, HighlightError> {
    u32::try_from(source.len()).map_err(|_| HighlightError::SourceTooLong)?;
    if !syntax_colors.is_valid()
        || token_colors
            .values()
            .copied()
            .any(|color| !is_valid_code_color(color))
    {
        return Err(HighlightError::InvalidColors);
    }
    if let Some(token) = token_colors
        .keys()
        .find(|token| !is_valid_code_token_selector(token))
    {
        return Err(HighlightError::InvalidToken(token.clone()));
    }
    let mut theme = theme(style)?;
    let foreground = syntax_colors.foreground.unwrap_or_else(|| {
        color(
            theme
                .settings
                .foreground
                .expect("native Code theme has foreground"),
        )
    });
    if syntax_colors.foreground.is_some() {
        // The bundled theme rules are opaque. A private transparent marker
        // identifies only the fallback, so arbitrary authored f32 colors survive
        // Syntect's byte-color machinery without recoloring equal-colored rules.
        theme.settings.foreground = Some(CUSTOM_FOREGROUND);
    }
    let token_tree = TokenColorTree::new(token_colors);
    let syntax = match language {
        Some(language) => syntax_for_language(language)?,
        None => syntaxes()
            .find_syntax_by_first_line(source.lines().next().unwrap_or(""))
            .unwrap_or_else(|| syntaxes().find_syntax_plain_text()),
    };
    let cuda = language.is_some_and(cuda_alias);
    let highlighter = Highlighter::new(&theme);
    let mut parse = ParseState::new(syntax);
    let mut state = HighlightState::new(&highlighter, ScopeStack::new());
    let mut lines = Vec::new();
    let mut source_offset = 0;
    // `split`, unlike `lines`, retains the final empty row. Pass each real
    // newline into the grammar, while source fills exclude line separators.
    for (index, line) in source.split('\n').enumerate() {
        let end = source_offset + line.len();
        let parsed_end = end + usize::from(end < source.len());
        let parsed_line = &source[source_offset..parsed_end];
        let changes =
            parse
                .parse_line(parsed_line, syntaxes())
                .map_err(|error| HighlightError::Parse {
                    line: index + 1,
                    message: error.to_string(),
                })?;
        let overrides = if cuda {
            cuda_fills(
                parsed_line,
                &changes,
                state.path.clone(),
                &highlighter,
                syntax_colors,
            )
            .map_err(|message| HighlightError::Parse {
                line: index + 1,
                message,
            })?
        } else if syntax.name == "Python" {
            python_word_operator_fills(
                parsed_line,
                &changes,
                state.path.clone(),
                &highlighter,
                syntax_colors,
            )
            .map_err(|message| HighlightError::Parse {
                line: index + 1,
                message,
            })?
        } else {
            Vec::new()
        };
        let (category_fills, token_fills) = custom_fills(
            parsed_line,
            &changes,
            state.path.clone(),
            syntax_colors,
            &token_tree,
        )
        .map_err(|message| HighlightError::Parse {
            line: index + 1,
            message,
        })?;
        let mut fills = Vec::new();
        for (style, _, range) in
            RangedHighlightIterator::new(&mut state, &changes, parsed_line, &highlighter)
        {
            push_fill(
                &mut fills,
                range.start.min(line.len()),
                range.end.min(line.len()),
                if style.foreground == CUSTOM_FOREGROUND && syntax_colors.foreground.is_some() {
                    foreground
                } else {
                    color(style.foreground)
                },
            );
        }
        if !category_fills.is_empty() {
            fills = overlay_fills(fills, &category_fills);
        }
        if !overrides.is_empty() {
            fills = overlay_fills(fills, &overrides);
        }
        if !token_fills.is_empty() {
            fills = overlay_fills(fills, &token_fills);
        }
        lines.push(HighlightedLine {
            source: line.into(),
            fills,
        });
        source_offset = parsed_end;
    }
    Ok(HighlightedCode {
        source: source.into(),
        resolved_language: if cuda {
            "CUDA C++".into()
        } else {
            syntax.name.clone()
        },
        lines,
        background: color(
            theme
                .settings
                .background
                .expect("native Code theme has background"),
        ),
        foreground,
        line_number: theme
            .settings
            .gutter_foreground
            .map(color)
            .unwrap_or(foreground),
    })
}

const CUSTOM_FOREGROUND: SyntaxColor = SyntaxColor {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

fn push_fill(fills: &mut Vec<TextSourceFill>, start: usize, end: usize, color: Color) {
    if start >= end {
        return;
    }
    if let Some(previous) = fills.last_mut() {
        if previous.color == color && previous.source_span.end as usize == start {
            previous.source_span.end = end as u32;
            return;
        }
    }
    // The complete source was checked against u32 before parsing.
    fills.push(TextSourceFill::new(
        TextSourceSpan::new(start as u32, end as u32),
        color,
    ));
}

#[derive(Default)]
struct TokenColorNode<'a> {
    color: Option<Color>,
    next: BTreeMap<&'a str, usize>,
}

/// A flat, authoring-local trie avoids scanning every configured selector at
/// every source token. No matcher or syntax state survives source admission.
struct TokenColorTree<'a> {
    nodes: Vec<TokenColorNode<'a>>,
}

impl<'a> TokenColorTree<'a> {
    fn new(colors: &'a BTreeMap<String, Color>) -> Self {
        let mut nodes = vec![TokenColorNode::default()];
        for (selector, &color) in colors {
            let mut index = 0;
            for component in selector.split('.') {
                index = match nodes[index].next.get(component) {
                    Some(&next) => next,
                    None => {
                        let next = nodes.len();
                        nodes.push(TokenColorNode::default());
                        nodes[index].next.insert(component, next);
                        next
                    }
                };
            }
            nodes[index].color = Some(color);
        }
        Self { nodes }
    }

    fn longest_match(&self, source: &str) -> Option<(usize, Color)> {
        let mut index = 0;
        let mut cursor = 0;
        let mut best = None;
        loop {
            let remainder = &source[cursor..];
            let first = remainder.chars().next()?;
            if first != '_' && !unicode_ident::is_xid_start(first) {
                return best;
            }
            let length = identifier_length(remainder);
            let Some(&next) = self.nodes[index].next.get(&remainder[..length]) else {
                return best;
            };
            index = next;
            cursor += length;
            if let Some(color) = self.nodes[index].color {
                best = Some((cursor, color));
            }
            if source.as_bytes().get(cursor) != Some(&b'.') || self.nodes[index].next.is_empty() {
                return best;
            }
            cursor += 1;
            if cursor == source.len() {
                return best;
            }
        }
    }

    fn fills(&self, source: &str, regions: &[Range<usize>]) -> Vec<TextSourceFill> {
        if self.nodes[0].next.is_empty() {
            return Vec::new();
        }
        let mut fills = Vec::new();
        for region in regions {
            let mut cursor = region.start;
            while cursor < region.end {
                let remainder = &source[cursor..region.end];
                let character = remainder.chars().next().expect("nonempty token region");
                if unicode_ident::is_xid_continue(character) {
                    if let Some((length, color)) = self.longest_match(remainder) {
                        push_fill(&mut fills, cursor, cursor + length, color);
                        cursor += length;
                    } else {
                        cursor += identifier_length(remainder);
                    }
                } else {
                    cursor += character.len_utf8();
                }
            }
        }
        fills
    }
}

fn identifier_length(source: &str) -> usize {
    source
        .char_indices()
        .find(|(_, character)| !unicode_ident::is_xid_continue(*character))
        .map_or(source.len(), |(offset, _)| offset)
}

fn custom_fills(
    line: &str,
    changes: &[(usize, syntect::parsing::ScopeStackOp)],
    mut stack: ScopeStack,
    colors: &CodeSyntaxColors,
    tokens: &TokenColorTree<'_>,
) -> Result<(Vec<TextSourceFill>, Vec<TextSourceFill>), String> {
    if [
        colors.comment,
        colors.keyword,
        colors.number,
        colors.string,
        colors.operator,
    ]
    .into_iter()
    .all(|color| color.is_none())
        && tokens.nodes[0].next.is_empty()
    {
        return Ok((Vec::new(), Vec::new()));
    }
    let comment = scope("comment");
    let string = scope("string");
    let number = scope("constant.numeric");
    let operator = scope("keyword.operator");
    let keywords = [
        scope("keyword"),
        scope("storage"),
        scope("support.type"),
        scope("constant.language"),
    ];
    let mut fills = Vec::new();
    let mut code_regions: Vec<Range<usize>> = Vec::new();
    for (range, operation) in ScopeRangeIterator::new(changes, line) {
        stack.apply(operation).map_err(|error| error.to_string())?;
        if range.is_empty() {
            continue;
        }
        let contains = |prefix: Scope| stack.as_slice().iter().any(|s| prefix.is_prefix_of(*s));
        let lexical = contains(comment) || contains(string);
        let color = if contains(comment) {
            colors.comment
        } else if contains(string) {
            colors.string
        } else if contains(number) {
            colors.number
        } else if contains(operator) {
            colors.operator
        } else if keywords.into_iter().any(contains) {
            colors.keyword
        } else {
            None
        };
        if let Some(color) = color {
            push_fill(&mut fills, range.start, range.end, color);
        }
        if !lexical {
            if let Some(previous) = code_regions
                .last_mut()
                .filter(|previous| previous.end == range.start)
            {
                previous.end = range.end;
            } else {
                code_regions.push(range);
            }
        }
    }
    Ok((fills, tokens.fills(line, &code_regions)))
}

/// CUDA uses the bundled C++ grammar for strings, comments, preprocessor and
/// nested expressions, then extends code tokens with CUDA qualifiers/builtins
/// and launch delimiters. This explicit profile is not a plain C++ alias and
/// does not recolor keywords embedded inside comments or quoted/raw strings.
/// Syntect's bundled Python grammar marks some word operators as ordinary
/// keywords (notably \`in\` in comprehensions). Pygments 2.19.2 calls these
/// Operator.Word. Overlay only whole Python operator words outside lexically
/// scoped comments and strings; identifiers and all other syntax retain the
/// bundled grammar's original highlighting.
fn python_word_operator_fills(
    line: &str,
    changes: &[(usize, syntect::parsing::ScopeStackOp)],
    mut stack: ScopeStack,
    highlighter: &Highlighter<'_>,
    syntax_colors: &CodeSyntaxColors,
) -> Result<Vec<TextSourceFill>, String> {
    let comment = scope("comment");
    let string = scope("string");
    let word_operator = syntax_colors.operator.unwrap_or_else(|| {
        color(
            highlighter
                .style_for_stack(&[scope("keyword.operator.word")])
                .foreground,
        )
    });
    let mut fills = Vec::new();
    for (range, operation) in ScopeRangeIterator::new(changes, line) {
        stack.apply(operation).map_err(|error| error.to_string())?;
        if range.is_empty()
            || stack
                .as_slice()
                .iter()
                .any(|s| comment.is_prefix_of(*s) || string.is_prefix_of(*s))
        {
            continue;
        }
        let mut cursor = range.start;
        while cursor < range.end {
            let remaining = &line[cursor..range.end];
            let character = remaining.chars().next().expect("nonempty region");
            if character == '_' || character.is_alphanumeric() {
                let length = remaining
                    .char_indices()
                    .find(|(_, c)| *c != '_' && !c.is_alphanumeric())
                    .map_or(remaining.len(), |(offset, _)| offset);
                let token = &remaining[..length];
                let start_boundary = line[..cursor]
                    .chars()
                    .next_back()
                    .is_none_or(|c| c != '_' && !c.is_alphanumeric());
                let end_boundary = line[cursor + length..]
                    .chars()
                    .next()
                    .is_none_or(|c| c != '_' && !c.is_alphanumeric());
                if matches!(token, "and" | "or" | "not" | "in" | "is")
                    && start_boundary
                    && end_boundary
                {
                    push_fill(&mut fills, cursor, cursor + length, word_operator);
                }
                cursor += length;
            } else {
                cursor += character.len_utf8();
            }
        }
    }
    Ok(fills)
}

fn cuda_fills(
    line: &str,
    changes: &[(usize, syntect::parsing::ScopeStackOp)],
    mut stack: ScopeStack,
    highlighter: &Highlighter<'_>,
    syntax_colors: &CodeSyntaxColors,
) -> Result<Vec<TextSourceFill>, String> {
    let comment = scope("comment");
    let string = scope("string");
    let mut regions: Vec<Range<usize>> = Vec::new();
    for (range, operation) in ScopeRangeIterator::new(changes, line) {
        stack.apply(operation).map_err(|error| error.to_string())?;
        if range.is_empty()
            || stack
                .as_slice()
                .iter()
                .any(|s| comment.is_prefix_of(*s) || string.is_prefix_of(*s))
        {
            continue;
        }
        if let Some(previous) = regions
            .last_mut()
            .filter(|previous| previous.end == range.start)
        {
            previous.end = range.end;
        } else {
            regions.push(range);
        }
    }
    let keyword = syntax_colors.keyword.unwrap_or_else(|| {
        color(
            highlighter
                .style_for_stack(&[scope("storage.modifier.cuda")])
                .foreground,
        )
    });
    // Pygments CudaLexer variable qualifiers (including __device__) use
    // Keyword.Type. Keep this separate from function qualifiers (Keyword.Reserved).
    let type_keyword = syntax_colors.keyword.unwrap_or_else(|| {
        color(
            highlighter
                .style_for_stack(&[scope("storage.type.cuda")])
                .foreground,
        )
    });
    let function = color(
        highlighter
            .style_for_stack(&[scope("entity.name.function")])
            .foreground,
    );
    let builtin = color(
        highlighter
            .style_for_stack(&[scope("support.function.cuda")])
            .foreground,
    );
    let value = color(
        highlighter
            .style_for_stack(&[scope("support.variable.cuda")])
            .foreground,
    );
    let operator = syntax_colors.operator.unwrap_or_else(|| {
        color(
            highlighter
                .style_for_stack(&[scope("keyword.operator.cuda")])
                .foreground,
        )
    });
    let mut fills = Vec::new();
    for region in regions {
        let mut cursor = region.start;
        while cursor < region.end {
            let remaining = &line[cursor..region.end];
            if remaining.starts_with("<<<") || remaining.starts_with(">>>") {
                push_fill(&mut fills, cursor, cursor + 3, operator);
                cursor += 3;
                continue;
            }
            let character = remaining.chars().next().expect("nonempty region");
            if character == '_' || character.is_alphanumeric() {
                let length = remaining
                    .char_indices()
                    .find(|(_, c)| *c != '_' && !c.is_alphanumeric())
                    .map_or(remaining.len(), |(offset, _)| offset);
                let token = &remaining[..length];
                let fill = match token {
                    "__device__" | "__shared__" | "__constant__" | "__restrict__" => {
                        Some(type_keyword)
                    }
                    "__global__" | "__host__" | "__managed__" | "__noinline__"
                    | "__forceinline__" | "__launch_bounds__" | "__align__"
                    | "__grid_constant__" => Some(keyword),
                    "threadIdx" | "blockIdx" | "blockDim" | "gridDim" | "warpSize" => Some(value),
                    "__syncthreads"
                    | "__syncthreads_count"
                    | "__syncthreads_and"
                    | "__syncthreads_or"
                    | "__threadfence"
                    | "__threadfence_block"
                    | "__threadfence_system" => Some(function),
                    "__syncwarp" | "__shfl_sync" | "__shfl_up_sync" | "__shfl_down_sync"
                    | "__shfl_xor_sync" | "__ballot_sync" | "__any_sync" | "__all_sync"
                    | "atomicAdd" | "atomicSub" | "atomicExch" | "atomicMin" | "atomicMax"
                    | "atomicInc" | "atomicDec" | "atomicCAS" | "atomicAnd" | "atomicOr"
                    | "atomicXor" => Some(builtin),
                    _ => None,
                };
                if let Some(fill) = fill {
                    push_fill(&mut fills, cursor, cursor + length, fill);
                }
                cursor += length;
            } else {
                cursor += character.len_utf8();
            }
        }
    }
    Ok(fills)
}

fn overlay_fills(base: Vec<TextSourceFill>, overrides: &[TextSourceFill]) -> Vec<TextSourceFill> {
    let mut result = Vec::new();
    let mut first_override = 0;
    for fill in base {
        let start = fill.source_span.start as usize;
        let end = fill.source_span.end as usize;
        while first_override < overrides.len()
            && overrides[first_override].source_span.end as usize <= start
        {
            first_override += 1;
        }
        let mut cursor = start;
        for replacement in overrides[first_override..]
            .iter()
            .take_while(|replacement| (replacement.source_span.start as usize) < end)
        {
            let from = (replacement.source_span.start as usize).max(cursor);
            let to = (replacement.source_span.end as usize).min(end);
            push_fill(&mut result, cursor, from, fill.color);
            push_fill(&mut result, from, to, replacement.color);
            cursor = to;
        }
        push_fill(&mut result, cursor, end, fill.color);
    }
    result
}

fn scope(name: &str) -> Scope {
    Scope::new(name).expect("built-in Code scope is valid")
}

fn color(value: SyntaxColor) -> Color {
    Color::rgba(
        f32::from(value.r) / 255.0,
        f32::from(value.g) / 255.0,
        f32::from(value.b) / 255.0,
        f32::from(value.a) / 255.0,
    )
}

fn rgb(value: u32) -> SyntaxColor {
    SyntaxColor {
        r: (value >> 16) as u8,
        g: (value >> 8) as u8,
        b: value as u8,
        a: 255,
    }
}

fn theme(name: &str) -> Result<Theme, HighlightError> {
    // Order: background, foreground, gutter, comment, preprocessor, keyword,
    // declaration, namespace keyword, type, operator, function, class, builtin,
    // variable, attribute, decorator, tag, string, escape, number.
    let p = match name {
        "vim" => [
            0x000000, 0xcccccc, 0xcccccc, 0x000080, 0x000080, 0xcdcd00, 0x00cd00, 0xcd00cd,
            0x00cd00, 0x3399cc, 0xcccccc, 0x00cdcd, 0xcd00cd, 0x00cdcd, 0xcccccc, 0xcccccc,
            0xcccccc, 0xcd0000, 0xcd0000, 0xcd00cd,
        ],
        "monokai" => [
            0x272822, 0xf8f8f2, 0xf8f8f2, 0x959077, 0x959077, 0x66d9ef, 0x66d9ef, 0xff4689,
            0x66d9ef, 0xff4689, 0xa6e22e, 0xa6e22e, 0xf8f8f2, 0xf8f8f2, 0xa6e22e, 0xa6e22e,
            0xff4689, 0xe6db74, 0xae81ff, 0xae81ff,
        ],
        "friendly" => [
            0xf0f0f0, 0x000000, 0x666666, 0x60a0b0, 0x007020, 0x007020, 0x007020, 0x007020,
            0x902000, 0x666666, 0x06287e, 0x0e84b5, 0x007020, 0xbb60d5, 0x4070a0, 0x555555,
            0x062873, 0x4070a0, 0x4070a0, 0x40a070,
        ],
        "emacs" => [
            0xf8f8f8, 0x000000, 0x000000, 0x008800, 0x008800, 0xaa22ff, 0xaa22ff, 0xaa22ff,
            0x00bb00, 0x666666, 0x00a000, 0x0000ff, 0xaa22ff, 0xb8860b, 0xbb4444, 0xaa22ff,
            0x008000, 0xbb4444, 0xbb6622, 0x666666,
        ],
        _ => return Err(HighlightError::UnknownStyle(name.into())),
    };
    let rules = [
        ("comment", 3),
        ("meta.preprocessor, comment.block.preprocessor, keyword.control.import, punctuation.definition.preprocessor", 4),
        ("keyword, storage.modifier", 5),
        ("storage.type.function, storage.type.class", 6),
        ("keyword.control.import, keyword.control.from, keyword.control.include, keyword.control.namespace", 7),
        ("storage.type, support.type", 8),
        ("keyword.operator", 9),
        ("keyword.operator.word", if name == "monokai" { 9 } else { 5 }),
        ("entity.name.function", 10),
        ("entity.name.class, entity.name.struct, entity.name.type, entity.name.namespace", 11),
        ("support.function, support.variable, support.constant, variable.language", 12),
        ("variable.other.member, variable.other.global, variable.other.constant", 13),
        ("entity.other.attribute-name", 14),
        ("entity.name.function.decorator, entity.name.tag.decorator", 15),
        ("entity.name.tag", 16),
        ("string", 17),
        ("constant.character.escape", 18),
        ("constant.numeric", 19),
        ("constant.language", 5),
    ];
    Ok(Theme {
        name: Some(name.into()),
        settings: ThemeSettings {
            foreground: Some(rgb(p[1])),
            background: Some(rgb(p[0])),
            gutter_foreground: Some(rgb(p[2])),
            ..ThemeSettings::default()
        },
        scopes: rules
            .into_iter()
            .map(|(selector, index)| ThemeItem {
                scope: selector
                    .parse()
                    .expect("built-in Code theme selector is valid"),
                style: StyleModifier {
                    foreground: Some(rgb(p[index])),
                    ..StyleModifier::default()
                },
            })
            .collect(),
        ..Theme::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn highlight(
        source: &str,
        language: Option<&str>,
        style: &str,
    ) -> Result<HighlightedCode, HighlightError> {
        super::highlight(
            source,
            language,
            style,
            &CodeSyntaxColors::default(),
            &BTreeMap::new(),
        )
    }

    fn at(line: &HighlightedLine, needle: &str) -> Color {
        let offset = line.source.find(needle).unwrap() as u32;
        line.fills
            .iter()
            .find(|fill| fill.source_span.start <= offset && offset < fill.source_span.end)
            .unwrap()
            .color
    }

    fn customized(
        source: &str,
        language: &str,
        colors: CodeSyntaxColors,
        tokens: &[(&str, Color)],
    ) -> HighlightedCode {
        super::highlight(
            source,
            Some(language),
            "vim",
            &colors,
            &tokens
                .iter()
                .map(|&(name, color)| (name.into(), color))
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn custom_cuda_colors_preserve_whole_identifiers_and_source_associations() {
        let source = "__shared__ float tile[TILE_DIM + 1];\nint inside = blockIdx.x + threadIdx.x;\nout[0] = in[0]; __syncthreads(); // in tile out";
        let result = customized(
            source,
            "cuda",
            CodeSyntaxColors {
                keyword: Some(Color::PURPLE),
                number: Some(Color::ORANGE),
                comment: Some(Color::GRAY),
                ..CodeSyntaxColors::default()
            },
            &[
                ("tile", Color::BLUE),
                ("in", Color::BLUE),
                ("out", Color::GREEN),
                ("TILE_DIM", Color::ORANGE),
                ("blockIdx.x", Color::ORANGE),
                ("threadIdx.x", Color::ORANGE),
                ("__syncthreads", Color::PINK),
            ],
        );
        assert_eq!(result.source, source);
        assert_eq!(at(&result.lines[0], "__shared__"), Color::PURPLE);
        assert_eq!(at(&result.lines[0], "float"), Color::PURPLE);
        assert_eq!(at(&result.lines[0], "tile"), Color::BLUE);
        assert_eq!(at(&result.lines[0], "TILE_DIM"), Color::ORANGE);
        assert_eq!(at(&result.lines[0], "1"), Color::ORANGE);
        assert_eq!(at(&result.lines[1], "int"), Color::PURPLE);
        assert_ne!(at(&result.lines[1], "inside"), Color::BLUE);
        for token in ["blockIdx.x", "threadIdx.x"] {
            let start = result.lines[1].source.find(token).unwrap();
            for offset in start..start + token.len() {
                let fill = result.lines[1]
                    .fills
                    .iter()
                    .find(|fill| {
                        fill.source_span.start as usize <= offset
                            && offset < fill.source_span.end as usize
                    })
                    .unwrap();
                assert_eq!(fill.color, Color::ORANGE);
            }
        }
        assert_eq!(at(&result.lines[2], "out"), Color::GREEN);
        assert_eq!(at(&result.lines[2], "in["), Color::BLUE);
        assert_eq!(at(&result.lines[2], "__syncthreads"), Color::PINK);
        let comment = result.lines[2].source.find("//").unwrap() as u32;
        assert!(result.lines[2]
            .fills
            .iter()
            .filter(|fill| fill.source_span.end > comment)
            .all(|fill| fill.color == Color::GRAY));
    }

    #[test]
    fn custom_tokens_obey_multiline_comment_and_raw_string_precedence() {
        let result = customized(
            "/* in\ntile */ in;\nconst char *s = R\"tag(out\nin)tag\"; out;",
            "cuda",
            CodeSyntaxColors {
                comment: Some(Color::GRAY),
                string: Some(Color::ORANGE),
                ..CodeSyntaxColors::default()
            },
            &[
                ("in", Color::BLUE),
                ("tile", Color::BLUE),
                ("out", Color::GREEN),
            ],
        );
        assert_eq!(at(&result.lines[0], "in"), Color::GRAY);
        assert_eq!(at(&result.lines[1], "tile"), Color::GRAY);
        assert_eq!(at(&result.lines[1], "in;"), Color::BLUE);
        assert_eq!(at(&result.lines[2], "out"), Color::ORANGE);
        assert_eq!(at(&result.lines[3], "in"), Color::ORANGE);
        assert_eq!(at(&result.lines[3], "out;"), Color::GREEN);
    }

    #[test]
    fn exact_unicode_tokens_and_longest_dotted_selectors_have_valid_local_spans() {
        let result = customized(
            "café cafe\u{301} in\u{301} inside in_ _in 1in 🙂 in\na.b a.bc a.b.c a.c",
            "text",
            CodeSyntaxColors::default(),
            &[
                ("café", Color::BLUE),
                ("cafe\u{301}", Color::GREEN),
                ("in", Color::PINK),
                ("a", Color::BLUE),
                ("a.b", Color::GREEN),
                ("a.b.c", Color::ORANGE),
            ],
        );
        assert_eq!(at(&result.lines[0], "café"), Color::BLUE);
        assert_eq!(at(&result.lines[0], "cafe\u{301}"), Color::GREEN);
        for token in ["in\u{301}", "inside", "in_", "_in", "1in"] {
            assert_ne!(at(&result.lines[0], token), Color::PINK, "{token}");
        }
        assert_eq!(result.lines[0].fills.last().unwrap().color, Color::PINK);
        assert_eq!(at(&result.lines[1], "a.b "), Color::GREEN);
        assert_eq!(at(&result.lines[1], "a.bc"), Color::BLUE);
        assert_eq!(at(&result.lines[1], "a.b.c"), Color::ORANGE);
        assert_eq!(at(&result.lines[1], "a.c"), Color::BLUE);
        for line in result.lines {
            let mut end = 0;
            for fill in line.fills {
                assert_eq!(fill.source_span.start as usize, end);
                end = fill.source_span.end as usize;
                assert!(line.source.is_char_boundary(end));
            }
            assert_eq!(end, line.source.len());
        }
    }

    #[test]
    fn keyword_and_operator_categories_keep_language_extensions_and_exact_overrides() {
        let colors = CodeSyntaxColors {
            keyword: Some(Color::GREEN),
            operator: Some(Color::RED),
            ..CodeSyntaxColors::default()
        };
        let python = customized("for item in values: return item + 1", "python", colors, &[]);
        assert_eq!(at(&python.lines[0], "for"), Color::GREEN);
        assert_eq!(at(&python.lines[0], "in"), Color::RED);
        assert_eq!(at(&python.lines[0], "+"), Color::RED);
        let cuda = customized(
            "__global__ void f() {}\nf<<<1, 1>>>();",
            "cuda",
            colors,
            &[("void", Color::BLUE)],
        );
        assert_eq!(at(&cuda.lines[0], "__global__"), Color::GREEN);
        assert_eq!(at(&cuda.lines[0], "void"), Color::BLUE);
        assert_eq!(at(&cuda.lines[1], "<<<"), Color::RED);
        assert_eq!(at(&cuda.lines[1], ">>>"), Color::RED);
    }

    #[test]
    fn foreground_preserves_full_color_precision_and_explicit_equal_palette_rules() {
        let foreground = Color::rgba(0.1234567, 0.2345678, 0.3456789, 0.456789);
        let result = customized(
            "def function(value):\n    return value",
            "python",
            CodeSyntaxColors {
                foreground: Some(foreground),
                ..CodeSyntaxColors::default()
            },
            &[],
        );
        assert_eq!(result.foreground, foreground);
        assert_eq!(at(&result.lines[1], "value"), foreground);
        // Vim deliberately has the same byte color for its function rule and
        // original fallback. Only the fallback follows `foreground`.
        assert_eq!(at(&result.lines[0], "function"), color(rgb(0xcccccc)));
    }

    #[test]
    fn invalid_color_or_token_choices_fail_before_highlighting() {
        assert_eq!(
            super::highlight(
                "in",
                Some("text"),
                "vim",
                &CodeSyntaxColors {
                    number: Some(Color::rgba(f32::NAN, 0.0, 0.0, 1.0)),
                    ..CodeSyntaxColors::default()
                },
                &BTreeMap::new()
            )
            .unwrap_err(),
            HighlightError::InvalidColors,
        );
        for selector in ["", "in[0]", "x.", "x .y"] {
            assert_eq!(
                super::highlight(
                    "in",
                    Some("text"),
                    "vim",
                    &CodeSyntaxColors::default(),
                    &BTreeMap::from([(selector.into(), Color::BLUE)])
                )
                .unwrap_err(),
                HighlightError::InvalidToken(selector.into()),
            );
        }
    }

    #[test]
    fn python_vim_has_native_keyword_string_number_and_comment_paints() {
        let result = highlight(
            "def twice(x):\n    return x + 42 # value\nprint(\"ok\")",
            Some("python"),
            "vim",
        )
        .unwrap();
        // Pygments 2.19.2 PythonLexer classifies `def` as Token.Keyword,
        // not Keyword.Declaration; VimStyle therefore paints it #cdcd00.
        assert_eq!(at(&result.lines[0], "def"), color(rgb(0xcdcd00)));
        assert_eq!(at(&result.lines[1], "return"), color(rgb(0xcdcd00)));
        assert_eq!(at(&result.lines[1], "42"), color(rgb(0xcd00cd)));
        assert_eq!(at(&result.lines[1], "#"), color(rgb(0x000080)));
        assert_eq!(at(&result.lines[2], "ok"), color(rgb(0xcd0000)));
    }

    #[test]
    fn multiline_lexical_state_resumes_after_comments_and_strings() {
        let comment = color(rgb(0x000080));
        let c = highlight("/* return\n  const */ int value = 2;", Some("c"), "vim").unwrap();
        assert_eq!(at(&c.lines[0], "return"), comment);
        assert_eq!(at(&c.lines[1], "const"), comment);
        assert_ne!(at(&c.lines[1], "int"), comment);
        let python = highlight(
            "value = \"\"\"def\nreturn\"\"\"\nreturn 1",
            Some("python"),
            "vim",
        )
        .unwrap();
        assert_eq!(at(&python.lines[1], "return"), color(rgb(0xcd0000)));
        assert_eq!(at(&python.lines[2], "return"), color(rgb(0xcdcd00)));
        let rust = highlight(
            "/* outer /* nested */\nfn */ fn live() {}",
            Some("rust"),
            "vim",
        )
        .unwrap();
        assert_eq!(at(&rust.lines[1], "fn"), comment);
        assert_ne!(at(&rust.lines[1], "live"), comment);
    }

    #[test]
    fn monokai_python_word_operator_and_constant_colors_match_pinned_pygments() {
        // Pygments 2.19.2: Python `def`/`return`/`for` are Keyword,
        // `in` is Operator.Word, and `True` is Keyword.Constant.
        let result = highlight(
            "return True if x in values else False",
            Some("python"),
            "monokai",
        )
        .unwrap();
        assert_eq!(at(&result.lines[0], "return"), color(rgb(0x66d9ef)));
        assert_eq!(at(&result.lines[0], "True"), color(rgb(0x66d9ef)));
        assert_eq!(at(&result.lines[0], "in"), color(rgb(0xff4689)));
        assert_eq!(at(&result.lines[0], "False"), color(rgb(0x66d9ef)));
    }

    #[test]
    fn python_operator_words_match_pygments_without_recoloring_strings_comments_or_names() {
        let result = highlight(
            "for x in values:\n    if x is not None and y or z:\n        # in is not and or\n        inside = \"in and or is not\"",
            Some("python"),
            "monokai",
        )
        .unwrap();
        let word_operator = color(rgb(0xff4689));
        assert_eq!(at(&result.lines[0], "for"), color(rgb(0x66d9ef)));
        assert_eq!(at(&result.lines[0], "in"), word_operator);
        for token in ["is", "not", "and", "or"] {
            assert_eq!(at(&result.lines[1], token), word_operator, "{token}");
        }
        assert_eq!(at(&result.lines[2], "in"), color(rgb(0x959077)));
        assert_ne!(at(&result.lines[3], "inside"), word_operator);
        let string_offset = result.lines[3].source.find('"').unwrap() as u32 + 1;
        let string_color = result.lines[3]
            .fills
            .iter()
            .find(|fill| {
                fill.source_span.start <= string_offset && string_offset < fill.source_span.end
            })
            .unwrap()
            .color;
        assert_eq!(string_color, color(rgb(0xe6db74)));
    }

    #[test]
    fn common_languages_use_bundled_grammars_and_cuda_has_explicit_extensions() {
        for (language, source) in [
            ("python", "def f(): return 42"),
            ("rust", "pub fn f() -> u32 { 42 }"),
            ("javascript", "const f = (x) => x + 42;"),
            ("typescript", "const f = (x: number): number => x + 42;"),
            ("C", "int main(void) { return 42; }"),
            ("C++", "template<typename T> T f(T x) { return x; }"),
            (
                "cuda",
                "__global__ void f() { __shared__ float tile[8][9]; }",
            ),
        ] {
            let result = highlight(source, Some(language), "emacs").unwrap();
            assert!(
                result.lines[0]
                    .fills
                    .iter()
                    .any(|fill| fill.color != result.foreground),
                "{language}"
            );
        }
        let cuda = highlight("__global__ void f() {\n __shared__ int x = threadIdx.x;\n __syncthreads();\n}\nf<<<2, 32>>>();", Some("cuda"), "vim").unwrap();
        assert_eq!(at(&cuda.lines[0], "__global__"), color(rgb(0xcdcd00)));
        assert_eq!(at(&cuda.lines[1], "__shared__"), color(rgb(0x00cd00)));
        assert_eq!(at(&cuda.lines[1], "threadIdx"), color(rgb(0xcd00cd)));
        assert_eq!(at(&cuda.lines[2], "__syncthreads"), color(rgb(0xcccccc)));
        assert_eq!(at(&cuda.lines[4], "<<<"), color(rgb(0x3399cc)));
    }

    #[test]
    fn cuda_keywords_inside_multiline_comments_and_raw_strings_are_not_code() {
        let source = "/* __global__\nthreadIdx */\nconst char *s = R\"tag(__shared__\n__syncthreads() >>>)tag\";\n__global__ void live() {}";
        let result = highlight(source, Some("cuda"), "vim").unwrap();
        assert_eq!(at(&result.lines[0], "__global__"), color(rgb(0x000080)));
        assert_eq!(at(&result.lines[1], "threadIdx"), color(rgb(0x000080)));
        assert_eq!(at(&result.lines[2], "__shared__"), color(rgb(0xcd0000)));
        assert_eq!(at(&result.lines[3], "__syncthreads"), color(rgb(0xcd0000)));
        assert_eq!(at(&result.lines[4], "__global__"), color(rgb(0xcdcd00)));
    }

    #[test]
    fn utf8_and_outer_empty_rows_are_preserved_with_valid_local_spans() {
        let source = "\n  café = \"🙂\"\n\n";
        let result = highlight(source, Some("python"), "monokai").unwrap();
        assert_eq!(result.source, source);
        assert_eq!(
            result
                .lines
                .iter()
                .map(|line| line.source.as_str())
                .collect::<Vec<_>>(),
            ["", "  café = \"🙂\"", "", ""]
        );
        for line in result.lines {
            let mut cursor = 0;
            for fill in line.fills {
                assert_eq!(fill.source_span.start as usize, cursor);
                cursor = fill.source_span.end as usize;
                assert!(line.source.is_char_boundary(cursor));
            }
            assert_eq!(cursor, line.source.len());
        }
    }

    #[test]
    fn filename_and_first_line_inference_are_explicit_and_portable() {
        assert_eq!(language_for_file_name("/tmp/a.py"), Some("Python".into()));
        assert_eq!(
            language_for_file_name("C:\\source\\a.cu"),
            Some("CUDA C++".into())
        );
        assert_eq!(language_for_file_name("file.ts"), Some("TypeScript".into()));
        let guessed = highlight("#!/usr/bin/env python\nreturn 2", None, "vim").unwrap();
        assert_eq!(guessed.resolved_language, "Python");
        assert_eq!(at(&guessed.lines[1], "return"), color(rgb(0xcdcd00)));
        let plain = highlight("some words with no language signal", None, "vim").unwrap();
        assert_eq!(plain.resolved_language, "Plain Text");
        assert!(plain.lines[0]
            .fills
            .iter()
            .all(|fill| fill.color == plain.foreground));
        let cuda = highlight("__shared__ float tile[2];", Some("cu"), "vim").unwrap();
        assert_eq!(cuda.resolved_language, "CUDA C++");
    }

    #[test]
    fn palettes_are_distinct_and_unsupported_inputs_fail_explicitly() {
        let mut backgrounds = Vec::new();
        for name in style_names() {
            let result = highlight("return \"text\" # comment", Some("python"), &name).unwrap();
            assert_ne!(at(&result.lines[0], "text"), result.foreground);
            backgrounds.push(result.background);
        }
        for (index, background) in backgrounds.iter().enumerate() {
            assert!(!backgrounds[..index].contains(background));
        }
        assert!(matches!(
            highlight("x", Some("absent-language"), "vim"),
            Err(HighlightError::UnknownLanguage(_))
        ));
        assert!(matches!(
            highlight("x", Some("python"), "absent-style"),
            Err(HighlightError::UnknownStyle(_))
        ));
        let languages = language_names();
        assert!(languages.contains(&"TypeScript".into()));
        assert!(languages.contains(&"CUDA C++".into()));
        assert!(languages.contains(&"Python".into()));
    }

    // Pinned independent ManimCE 0.21.0 Code color observations, extracted
    // from Pygments 2.19.2. The Python oracle recomputes lexer/theme output
    // without importing Noon; this test holds the native subset to that data.
    // It does not claim pixel-perfect equivalence with Manim's renderer.
    #[cfg(feature = "diagnostics")]
    #[test]
    fn pinned_manim_pygments_code_colors_match_supported_native_profile() {
        let oracle: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../fixtures/code/pygments-2.19.2-code-colors.json"
        ))
        .expect("pinned Code oracle is valid JSON");
        assert_eq!(oracle["reference"]["manim"], "0.21.0");
        assert_eq!(oracle["reference"]["pygments"], "2.19.2");
        for case in oracle["cases"].as_array().expect("Code reference cases") {
            let id = case["id"].as_str().unwrap();
            let source = case["source"].as_str().unwrap();
            let language = case["language"].as_str().unwrap();
            let style = case["style"].as_str().unwrap();
            let observed = highlight(source, Some(language), style).unwrap();
            for probe in case["probes"].as_array().unwrap() {
                let line_index = probe["line"].as_u64().unwrap() as usize;
                let start = probe["start"].as_u64().unwrap() as usize;
                let needle = probe["text"].as_str().unwrap();
                let end = start + needle.len();
                let row = &observed.lines[line_index];
                assert_eq!(
                    row.source.get(start..end),
                    Some(needle),
                    "{id}: stale source span"
                );
                let fill = row
                    .fills
                    .iter()
                    .find(|fill| {
                        fill.source_span.start as usize <= start
                            && start < fill.source_span.end as usize
                    })
                    .unwrap_or_else(|| panic!("{id}: missing paint for {needle}"));
                let expected = u32::from_str_radix(probe["rgb"].as_str().unwrap(), 16).unwrap();
                assert_eq!(
                    fill.color,
                    color(rgb(expected)),
                    "{id}: {needle} at line {line_index} byte {start}"
                );
            }
        }
    }
}

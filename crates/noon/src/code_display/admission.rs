use super::*;
use noon_core::{
    SemanticMutationTransaction, SemanticNodeCreation, SemanticObjectRole, SemanticObjectState,
    SemanticPaint, SemanticStyle, SemanticTransform2_5D, StoredGeometry, StrokeCap, StrokeJoin,
    StrokeWidthMode, TextResource, TextSourceFill, VectorPath,
};

type TextDependency = (
    noon_core::TextCompilationIdentity,
    TextResource,
    noon_core::FontResourceArena,
    noon_core::GeometryResourceArena,
);

pub(super) enum Publisher<'a, 'session> {
    Store(Rc<RefCell<SemanticStore>>),
    Scene(&'a mut Scene),
    Live(&'a mut LiveSession<'session>),
}
impl Publisher<'_, '_> {
    fn store(&self) -> Rc<RefCell<SemanticStore>> {
        match self {
            Self::Store(store) => Rc::clone(store),
            Self::Scene(scene) => Rc::clone(scene.integration_store()),
            Self::Live(live) => Rc::clone(live.integration_store()),
        }
    }

    fn publish(
        self,
        prepared: PreparedCode,
    ) -> Result<noon_core::SemanticNodeId, CodeAuthoringError> {
        match self {
            Self::Store(store) => prepared
                .publish(&mut store.borrow_mut(), &mut |store, tx| {
                    tx.apply(store).map_err(AuthoringError::from)
                })
                .map_err(Into::into),
            Self::Scene(scene) => scene
                .with_semantic_publication(|store, publish| Ok(prepared.publish(store, publish)))?
                .map_err(Into::into),
            Self::Live(live) => live
                .with_semantic_publication(|store, publish| Ok(prepared.publish(store, publish)))
                .map_err(CodeAuthoringError::LiveSession)?
                .map_err(Into::into),
        }
    }
}

pub(super) fn publish(
    publisher: Publisher<'_, '_>,
    source: &str,
    options: CodeOptions,
) -> Result<Code, CodeAuthoringError> {
    // All source processing, font resolution, shaping and geometry construction
    // finish before any retained resource or semantic node is published.
    let prepared = PreparedCode::new(source, options)?;
    let store = publisher.store();
    let root = publisher.publish(prepared)?;
    Code::from_family(MobjectFamily::from_node(store, root)?)
}

struct PreparedLine {
    dependency: TextDependency,
    transform: SemanticTransform2_5D,
    style: SemanticStyle,
}
impl PreparedLine {
    fn new(
        source: String,
        fills: Vec<TextSourceFill>,
        color: Color,
        config: &CodeParagraphConfig,
    ) -> Result<Self, CodeAuthoringError> {
        let font = if config.font.eq_ignore_ascii_case("monospace") {
            DEFAULT_NATIVE_TEXT_FONT_FAMILY
        } else {
            &config.font
        };
        let mut text = crate::Text::new(source)
            .with_font(font)
            .with_font_size(config.font_size)
            .with_disable_ligatures(config.disable_ligatures)
            .with_source_fills(fills)
            .color(color);
        if let Some(face) = &config.font_face {
            text = text.with_font_face(face.clone());
        }
        let (identity, resource, fonts, geometry, transform, style) =
            crate::text_authoring::prepare_native_text(text)?
                .into_compiled_resource_parts_with_presentation();
        Ok(Self {
            dependency: (identity, resource, fonts, geometry),
            transform,
            style,
        })
    }

    fn resource(&self) -> &TextResource {
        &self.dependency.1
    }

    fn width(&self) -> f64 {
        self.resource()
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter())
            .map(|glyph| f64::from(glyph.advance.x))
            .sum::<f64>()
            * self.transform.scale.x
    }

    fn recenter_x(&self) -> f64 {
        self.resource()
            .runs
            .first()
            .map_or(0.0, |run| f64::from(run.transform.tx))
    }

    fn ink_horizontal_bounds(&self) -> (f64, f64) {
        let resource = self.resource();
        let scale = self.transform.scale.x;
        let recenter = self.recenter_x();
        (
            0.0f64.min((f64::from(resource.bounds.min.x) - recenter) * scale),
            self.width()
                .max((f64::from(resource.bounds.max.x) - recenter) * scale),
        )
    }

    fn vertical_bounds(&self) -> (f64, f64) {
        let resource = self.resource();
        let scale = self.transform.scale.y;
        (
            f64::from(resource.bounds.min.y - resource.baseline) * scale,
            f64::from(resource.bounds.max.y - resource.baseline) * scale,
        )
    }

    fn position(&mut self, logical_x: f64, baseline_y: f64) {
        // Native Text is centered on ink at compilation. Undo that presentation
        // centering using the retained run origin; never left-align each line's
        // visible ink, which would erase indentation and leading spaces.
        self.transform.translation.x = logical_x - self.recenter_x() * self.transform.scale.x;
        self.transform.translation.y =
            baseline_y - f64::from(self.resource().baseline) * self.transform.scale.y;
    }
}

struct PreparedCode {
    lines: Vec<PreparedLine>,
    numbers: Option<Vec<PreparedLine>>,
    panel: VectorPath,
    panel_style: SemanticStyle,
    buttons: Vec<SemanticObjectState>,
}
impl PreparedCode {
    fn new(source: &str, options: CodeOptions) -> Result<Self, CodeAuthoringError> {
        options.validate()?;
        let source = normalize_source(source, options.tab_width)?;
        let file_language = options
            .source_path
            .as_deref()
            .and_then(highlight::language_for_file_name);
        let language = options.language.as_deref().or(file_language.as_deref());
        let highlighted = highlight::highlight(&source, language, &options.formatter_style)
            .map_err(|error| CodeAuthoringError::Highlight(error.to_string()))?;
        debug_assert_eq!(highlighted.source, source);
        let config = &options.paragraph_config;
        let mut lines = highlighted
            .lines
            .into_iter()
            .map(|line| PreparedLine::new(line.source, line.fills, highlighted.foreground, config))
            .collect::<Result<Vec<_>, _>>()?;
        let mut numbers = options
            .add_line_numbers
            .then(|| {
                (0..lines.len())
                    .map(|index| {
                        let number = i64::from(options.line_numbers_from)
                            + i64::try_from(index)
                                .map_err(|_| invalid("source", "too many lines"))?;
                        PreparedLine::new(
                            number.to_string(),
                            Vec::new(),
                            highlighted.line_number,
                            config,
                        )
                    })
                    .collect::<Result<Vec<_>, CodeAuthoringError>>()
            })
            .transpose()?;

        // A separately shaped reference fixes first/last empty-line bounds and
        // cap/descender extents. It is never admitted as a fake visible glyph.
        let reference = PreparedLine::new(
            format!("pA{}", options.line_numbers_from),
            Vec::new(),
            highlighted.foreground,
            config,
        )?;
        let (mut row_bottom, mut row_top) = reference.vertical_bounds();
        for line in lines.iter().chain(numbers.iter().flatten()) {
            let (bottom, top) = line.vertical_bounds();
            row_bottom = row_bottom.min(bottom);
            row_top = row_top.max(top);
        }
        // Manim Paragraph's line-spacing convention at the native points/scene
        // conversion (72 points per unit). -1 selects its default .3 spacing.
        let spacing = if config.line_spacing == -1.0 {
            0.3
        } else {
            f64::from(config.line_spacing)
        };
        let step = 0.75 * f64::from(config.font_size) * (1.0 + spacing) / 72.0;
        let number_width = numbers
            .iter()
            .flatten()
            .map(PreparedLine::width)
            .fold(0.0f64, f64::max);
        let gutter = if numbers.is_some() {
            number_width + 0.25
        } else {
            0.0
        };
        let body_width = gutter + lines.iter().map(PreparedLine::width).fold(0.0f64, f64::max);
        let window = options.background == CodeBackground::Window;
        let mut min_x: f64 = if window { -0.1 } else { 0.0 };
        let mut max_x = body_width.max(if window { 0.7 } else { 0.0 });
        for line in &lines {
            let (left, right) = line.ink_horizontal_bounds();
            min_x = min_x.min(gutter + left);
            max_x = max_x.max(gutter + right);
        }
        for number in numbers.iter().flatten() {
            let x = number_width - number.width();
            let (left, right) = number.ink_horizontal_bounds();
            min_x = min_x.min(x + left);
            max_x = max_x.max(x + right);
        }
        let min_y = row_bottom - (lines.len().saturating_sub(1) as f64) * step;
        let max_y = row_top + if window { 0.3 } else { 0.0 };
        let center_x = (min_x + max_x) * 0.5;
        let center_y = (min_y + max_y) * 0.5;
        for (index, line) in lines.iter_mut().enumerate() {
            line.position(gutter - center_x, -(index as f64) * step - center_y);
        }
        for (index, number) in numbers.iter_mut().flatten().enumerate() {
            number.position(
                number_width - number.width() - center_x,
                -(index as f64) * step - center_y,
            );
        }
        let background = &options.background_config;
        let width = panel_dimension("width", max_x - min_x + 2.0 * background.buff)?;
        let height = panel_dimension("height", max_y - min_y + 2.0 * background.buff)?;
        let panel = crate::rounded_rectangle_authoring::manim_rounded_rectangle_path(
            width,
            height,
            [background.corner_radius as f32; 4],
        )
        .map_err(|error| invalid("background", error))?;
        let panel_style = SemanticStyle {
            stroke_pattern: None,
            fill: Some(SemanticPaint::Solid(
                background.fill_color.unwrap_or(highlighted.background),
            )),
            fill_opacity: background.fill_opacity,
            stroke: Some(SemanticPaint::Solid(background.stroke_color)),
            stroke_opacity: background.stroke_opacity,
            stroke_width: background.stroke_width,
            stroke_width_mode: StrokeWidthMode::ScreenSpace,
            stroke_join: StrokeJoin::Miter,
            stroke_cap: StrokeCap::Butt,
            object_opacity: 1.0,
            ..SemanticStyle::default()
        };
        let buttons = if window {
            [0xff5f56, 0xffbd2e, 0x27c93f]
                .into_iter()
                .enumerate()
                .map(|(index, color)| {
                    let mut state =
                        SemanticObjectState::new(StoredGeometry::Circle { radius: 0.1 });
                    state.transform.translation.x = index as f64 * 0.3 - center_x;
                    state.transform.translation.y = row_top + 0.2 - center_y;
                    state.style.fill = Some(SemanticPaint::Solid(Color::from_hex(color)));
                    state
                })
                .collect()
        } else {
            Vec::new()
        };
        Ok(Self {
            lines,
            numbers,
            panel,
            panel_style,
            buttons,
        })
    }

    fn publish(
        self,
        semantic: &mut SemanticStore,
        publish: &mut dyn FnMut(
            &mut SemanticStore,
            SemanticMutationTransaction,
        ) -> Result<
            noon_core::SemanticMutationTransactionResult,
            AuthoringError,
        >,
    ) -> Result<noon_core::SemanticNodeId, TextAuthoringError> {
        let dependencies = self
            .lines
            .iter()
            .chain(self.numbers.iter().flatten())
            .map(|line| line.dependency.clone())
            .collect();
        semantic.with_compiled_text_dependency_batch::<TextAuthoringError, _>(
            dependencies,
            |store, handles| {
                handles
                    .iter()
                    .map(|handle| {
                        store
                            .text_resources()
                            .get(*handle)
                            .cloned()
                            .ok_or(TextAuthoringError::MissingGeometryResource)
                    })
                    .collect()
            },
            |store, handles| {
                let mut tx = SemanticMutationTransaction::new();
                let background = tx.create_node(SemanticNodeCreation::family());
                let panel = tx
                    .stage_geometry_path(self.panel)
                    .map_err(|error| TextAuthoringError::Semantic(error.into()))?;
                let panel = tx.create_node(SemanticNodeCreation::pending_path_object(
                    panel,
                    SemanticTransform2_5D::default(),
                    self.panel_style,
                    0.0,
                    SemanticObjectRole::default(),
                ));
                tx.add_member(background, panel);
                for button in self.buttons {
                    let button = tx.create_node(SemanticNodeCreation::object(button));
                    tx.add_member(background, button);
                }
                let code_lines = stage_lines(&mut tx, &self.lines, &handles[..self.lines.len()]);
                let line_numbers = self
                    .numbers
                    .as_ref()
                    .map(|numbers| stage_lines(&mut tx, numbers, &handles[self.lines.len()..]));
                let root = tx.create_node(SemanticNodeCreation::family());
                tx.add_member(root, background);
                if let Some(numbers) = line_numbers {
                    tx.add_member(root, numbers);
                }
                tx.add_member(root, code_lines);
                let result = publish(store, tx).map_err(TextAuthoringError::Semantic)?;
                result.resolve(root).ok_or(TextAuthoringError::Semantic(
                    AuthoringError::UnresolvedCreatedNode(root),
                ))
            },
        )
    }
}

fn stage_lines(
    tx: &mut SemanticMutationTransaction,
    lines: &[PreparedLine],
    handles: &[noon_core::TextResourceHandle],
) -> noon_core::SemanticLocalNodeToken {
    let family = tx.create_node(SemanticNodeCreation::family());
    for (line, handle) in lines.iter().zip(handles) {
        let mut state = SemanticObjectState::new(*handle);
        state.transform = line.transform.into();
        state.style = line.style.clone();
        let object = tx.create_node(SemanticNodeCreation::object(state));
        tx.add_member(family, object);
    }
    family
}

fn panel_dimension(name: &'static str, value: f64) -> Result<f32, CodeAuthoringError> {
    if !value.is_finite() || value < 0.0 || value > f64::from(f32::MAX) {
        return Err(invalid(name, value));
    }
    // Empty, unpadded listings still have a real background path and empty Text
    // line. Avoid injecting a visible placeholder character for this edge case.
    Ok((value as f32).max(f32::EPSILON))
}

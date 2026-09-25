//! Real LaTeX authoring values and the explicit compiler-host boundary.
//!
//! A backend owns DVI production and exact font resolution. This module owns
//! generated-document identity and the normalized semantic text contract; it
//! never substitutes another markup or math engine for TeX source.

use std::{cell::RefCell, rc::Rc, sync::Arc};

pub use noon_text::{
    latex::DviFontResource,
    latex_document::{LatexDocument, LatexFormat},
};

use crate::TextAuthoringError;
use noon_core::{
    Color, SemanticMutationTransaction, SemanticNodeCreation, TextPart, TextSourceKind,
    TextSourceSpan, Transform2D, Vec2, WHITE,
};

#[derive(Clone)]
pub struct LatexParts {
    family: crate::MobjectFamily,
    members: Vec<crate::Mobject>,
    source: Arc<str>,
    parts: Vec<TextPart>,
}

impl LatexParts {
    pub fn rebind_family(
        &self,
        family: crate::MobjectFamily,
    ) -> Result<Self, crate::AuthoringError> {
        if !Rc::ptr_eq(self.family.integration_store(), family.integration_store()) {
            return Err(crate::AuthoringError::ForeignStore);
        }
        family.validate()?;
        Ok(Self {
            family,
            members: Vec::new(),
            source: Arc::clone(&self.source),
            parts: self.parts.clone(),
        })
    }

    pub fn family(&self) -> &crate::MobjectFamily {
        &self.family
    }
    pub fn members(&self) -> &[crate::Mobject] {
        &self.members
    }
    pub fn source(&self) -> &Arc<str> {
        &self.source
    }
    pub fn parts(&self) -> &[TextPart] {
        &self.parts
    }

    /// Current authoritative leaf members in family order.
    pub fn current_members(&self) -> Result<Vec<crate::Mobject>, crate::AuthoringError> {
        self.family.validate()?;
        let store = self.family.integration_store();
        let leaves = store
            .borrow()
            .ordered_leaf_nodes(self.family.node_id())
            .map_err(crate::AuthoringError::from)?;
        leaves
            .into_iter()
            .map(|node| crate::Mobject::from_node(Rc::clone(store), node))
            .collect()
    }

    /// Manim-compatible current font size: authored point size scaled by the
    /// current vertical transform of the first authoritative leaf.
    pub fn current_font_size(&self) -> Result<f64, crate::TextPartAuthoringError> {
        let Some(member) = self.current_members()?.into_iter().next() else {
            return Ok(0.0);
        };
        let state = member.state()?;
        let handle = state
            .content
            .text()
            .ok_or(crate::TextPartAuthoringError::NotText(member.node_id()))?;
        let store = member.integration_store().borrow();
        let resource = store
            .text_resources()
            .get(handle)
            .ok_or(crate::AuthoringError::MissingTextResource(handle))?;
        let authored = resource
            .runs
            .first()
            .map_or(0.0, |run| run.font_size as f64);
        Ok(authored * state.transform.scale.y.abs())
    }

    /// Select current family leaves whose compiler-authored part overlaps a
    /// canonical substring match. Source matching stays in retained Rust text.
    pub fn current_member_indices_for(
        &self,
        needle: &str,
    ) -> Result<Vec<usize>, crate::TextPartAuthoringError> {
        let members = self
            .current_members()
            .map_err(crate::TextPartAuthoringError::from)?;
        let mut selected = Vec::new();
        for (index, member) in members.iter().enumerate() {
            let state = member.state()?;
            let handle = state
                .content
                .text()
                .ok_or(crate::TextPartAuthoringError::NotText(member.node_id()))?;
            let store = member.integration_store().borrow();
            let resource = store
                .text_resources()
                .get(handle)
                .ok_or(crate::AuthoringError::MissingTextResource(handle))?;
            let [member_part] = resource.parts.as_ref() else {
                return Err(crate::TextPartQueryError::InvalidSourceSpan.into());
            };
            if resource.source_parts_for(needle)?.iter().any(|matched| {
                matched.source_span.start < member_part.source_span.end
                    && member_part.source_span.start < matched.source_span.end
            }) {
                selected.push(index);
            }
        }
        Ok(selected)
    }

    pub fn set_current_member_colors(
        &self,
        colors: &[Option<Color>],
    ) -> Result<(), crate::AuthoringError> {
        self.family.set_member_colors(colors)
    }
}

/// Explicit host boundary for the pinned LaTeX engine.
///
/// `identity` must describe immutable engine, package, and font-resolution
/// inputs. It participates in the compiled-artifact cache key, so equal values
/// promise identical output for an equal generated document.
pub trait LatexBackend {
    fn identity(&self) -> &str;
    fn format(&self) -> LatexFormat;
    fn compile(&mut self, document: &str) -> Result<Vec<u8>, String>;
    fn font(&mut self, name: &str) -> Result<DviFontResource, String>;
}

/// Presentation shared by [`Tex`] and [`MathTex`]. It remains outside the
/// generated document so colour and ordinary transforms reuse compiled output.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LatexPresentation {
    pub(crate) color: Color,
    pub(crate) opacity: f32,
    pub(crate) transform: Transform2D,
}

impl Default for LatexPresentation {
    fn default() -> Self {
        Self {
            color: WHITE,
            opacity: 1.0,
            transform: Transform2D::default(),
        }
    }
}

impl LatexPresentation {
    pub(crate) fn validate(&self) -> Result<(), TextAuthoringError> {
        if !self.opacity.is_finite() || !(0.0..=1.0).contains(&self.opacity) {
            return Err(TextAuthoringError::InvalidOpacity(self.opacity));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LatexSpec {
    pub(crate) document: LatexDocument,
    pub(crate) font_size: f32,
    pub(crate) presentation: LatexPresentation,
}

impl LatexSpec {
    fn new(source: impl Into<Arc<str>>, kind: TextSourceKind) -> Result<Self, TextAuthoringError> {
        Ok(Self {
            document: LatexDocument::new(source, kind)?,
            font_size: DEFAULT_LATEX_FONT_SIZE,
            presentation: LatexPresentation::default(),
        })
    }

    fn from_arguments<I, S>(arguments: I, kind: TextSourceKind) -> Result<Self, TextAuthoringError>
    where
        I: IntoIterator<Item = S>,
        S: Into<Arc<str>>,
    {
        Ok(Self {
            document: LatexDocument::from_arguments(arguments, kind)?,
            font_size: DEFAULT_LATEX_FONT_SIZE,
            presentation: LatexPresentation::default(),
        })
    }

    pub(crate) fn validate(&self) -> Result<(), TextAuthoringError> {
        if !self.font_size.is_finite() || self.font_size <= 0.0 {
            return Err(TextAuthoringError::InvalidFontSize(self.font_size));
        }
        self.presentation.validate()
    }

    pub(crate) fn authored_transform(&self) -> Transform2D {
        let mut transform = self.presentation.transform;
        let scale = self.font_size * LATEX_POINT_TO_SCENE_SCALE;
        transform.scale = transform.scale.component_mul(Vec2::new(scale, scale));
        transform
    }
}

/// ManimCE 0.21 scales SVG big points by `font_size / 960`. DVI uses
/// TeX points (72.27 per inch), so convert to SVG big points (72 per inch) first.
pub const LATEX_POINT_TO_SCENE_SCALE: f32 = (72.0 / 72.27) / 960.0;
pub const DEFAULT_LATEX_FONT_SIZE: f32 = 48.0;

pub(crate) struct LatexAdmission {
    identity: noon_core::TextCompilationIdentity,
    resource: noon_core::TextResource,
    fonts: noon_core::FontResourceArena,
    geometry: noon_core::GeometryResourceArena,
    transform: noon_core::SemanticTransform2_5D,
    style: noon_core::SemanticStyle,
}

impl LatexAdmission {
    pub(crate) fn publish<T>(
        self,
        store: &mut noon_core::SemanticStore,
        publish: impl FnOnce(
            &mut noon_core::SemanticStore,
            noon_core::SemanticMutationTransaction,
        ) -> Result<T, TextAuthoringError>,
    ) -> Result<T, TextAuthoringError> {
        let Self {
            identity,
            resource,
            fonts,
            geometry,
            transform,
            style,
        } = self;
        store.publish_compiled_detached_text(
            identity,
            resource,
            fonts,
            &geometry,
            move |handle| semantic_text_state(handle, transform, style),
            publish,
        )
    }

    pub(crate) fn publish_parts<T>(
        self,
        store: &mut noon_core::SemanticStore,
        publish: impl FnOnce(
            &mut noon_core::SemanticStore,
            SemanticMutationTransaction,
        ) -> Result<T, TextAuthoringError>,
    ) -> Result<
        (
            T,
            noon_core::SemanticLocalNodeToken,
            Vec<noon_core::SemanticLocalNodeToken>,
            Arc<str>,
            Vec<TextPart>,
        ),
        TextAuthoringError,
    > {
        let Self {
            identity,
            resource,
            fonts,
            geometry,
            transform,
            style,
        } = self;
        let part_fonts = fonts.clone();
        store.publish_compiled_text_resource(
            identity,
            resource,
            fonts,
            &geometry,
            move |store, base| {
                let resource = store
                    .text_resources()
                    .get(base)
                    .expect("compiled text resource is live");
                let source = Arc::clone(&resource.source);
                let parts = if resource.parts.is_empty() {
                    vec![resource.source_part(TextSourceSpan::new(
                        0,
                        u32::try_from(resource.source.len())
                            .map_err(|_| noon_core::TextPartQueryError::InvalidSourceSpan)?,
                    ))?]
                } else {
                    resource.parts.to_vec()
                };
                let projections = parts
                    .iter()
                    .map(|part| resource.projected_part(part, store.geometry_resources()))
                    .collect::<Result<Vec<_>, _>>()?;
                store.with_derived_text_resources(projections, &part_fonts, |store, handles| {
                    let mut transaction = SemanticMutationTransaction::new();
                    let family = transaction.create_node(SemanticNodeCreation::family());
                    let members = handles
                        .iter()
                        .map(|handle| {
                            transaction.create_node(SemanticNodeCreation::object(
                                semantic_text_state(*handle, transform, style.clone()),
                            ))
                        })
                        .collect::<Vec<_>>();
                    for member in &members {
                        transaction.add_member(family, *member);
                    }
                    let result = publish(store, transaction)?;
                    Ok((result, family, members, source, parts))
                })
            },
        )
    }
}

pub(crate) fn finish_latex_parts(
    store: Rc<RefCell<noon_core::SemanticStore>>,
    result: &noon_core::SemanticMutationTransactionResult,
    family: noon_core::SemanticLocalNodeToken,
    members: Vec<noon_core::SemanticLocalNodeToken>,
    source: Arc<str>,
    parts: Vec<TextPart>,
) -> Result<LatexParts, TextAuthoringError> {
    let resolve = |token| {
        result
            .resolve(token)
            .ok_or(crate::AuthoringError::UnresolvedCreatedNode(token))
    };
    let family = crate::MobjectFamily::from_node(
        Rc::clone(&store),
        resolve(family).map_err(TextAuthoringError::Semantic)?,
    )
    .map_err(TextAuthoringError::Semantic)?;
    let members = members
        .into_iter()
        .map(|token| {
            crate::Mobject::from_node(
                Rc::clone(&store),
                resolve(token).map_err(TextAuthoringError::Semantic)?,
            )
            .map_err(TextAuthoringError::Semantic)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LatexParts {
        family,
        members,
        source,
        parts,
    })
}

pub(crate) fn prepare_tex(
    text: Tex,
    backend: &mut impl LatexBackend,
) -> Result<LatexAdmission, TextAuthoringError> {
    prepare_latex(text.0, backend)
}

pub(crate) fn prepare_math_tex(
    text: MathTex,
    backend: &mut impl LatexBackend,
) -> Result<LatexAdmission, TextAuthoringError> {
    prepare_latex(text.0, backend)
}

fn prepare_latex(
    text: LatexSpec,
    backend: &mut impl LatexBackend,
) -> Result<LatexAdmission, TextAuthoringError> {
    text.validate()?;
    let transform = text.authored_transform();
    let (transform, style) = latex_presentation(
        transform,
        text.presentation.color,
        text.presentation.opacity,
    )?;
    let document = text.document.render(backend.format())?;
    let backend_identity: Arc<str> = Arc::from(backend.identity());
    let identity = crate::text_authoring::compiler::latex_identity(
        &document,
        &backend_identity,
        text.document.kind(),
    );
    let artifact = crate::text_authoring::compiler::compile_latex(
        &text,
        &document,
        backend_identity,
        backend,
    )?;
    Ok(LatexAdmission {
        identity,
        resource: artifact.resource.as_ref().clone(),
        fonts: artifact.fonts.as_ref().clone(),
        geometry: artifact.geometry.as_ref().clone(),
        transform,
        style,
    })
}

fn latex_presentation(
    transform: Transform2D,
    color: Color,
    opacity: f32,
) -> Result<(noon_core::SemanticTransform2_5D, noon_core::SemanticStyle), TextAuthoringError> {
    let transform = noon_core::SemanticTransform2_5D {
        translation: noon_core::SemanticVec3::new(
            transform.translation.x as f64,
            transform.translation.y as f64,
            0.0,
        ),
        scale: noon_core::SemanticVec3::new(
            transform.scale.x as f64,
            transform.scale.y as f64,
            1.0,
        ),
        rotation_z: transform.rotation as f64,
    };
    if !transform.translation.is_finite()
        || !transform.scale.is_finite()
        || !transform.rotation_z.is_finite()
    {
        return Err(TextAuthoringError::Semantic(
            crate::AuthoringError::NonFiniteTransform,
        ));
    }
    let style = noon_core::SemanticStyle {
        fill: Some(noon_core::SemanticPaint::Solid(color)),
        fill_opacity: 1.0,
        stroke: None,
        stroke_width: 0.0,
        object_opacity: opacity as f64,
        ..Default::default()
    };
    if !style.is_finite() {
        return Err(TextAuthoringError::Semantic(
            crate::AuthoringError::NonFiniteStyle,
        ));
    }
    Ok((transform, style))
}

fn semantic_text_state(
    handle: noon_core::TextResourceHandle,
    transform: noon_core::SemanticTransform2_5D,
    style: noon_core::SemanticStyle,
) -> noon_core::SemanticObjectState {
    let mut state = noon_core::SemanticObjectState::new(handle);
    state.transform = transform;
    state.style = style;
    state
}

macro_rules! latex_object {
    ($name:ident, $kind:expr) => {
        #[derive(Clone, Debug, PartialEq)]
        pub struct $name(pub(crate) LatexSpec);

        impl $name {
            pub fn new(source: impl Into<Arc<str>>) -> Result<Self, TextAuthoringError> {
                Ok(Self(LatexSpec::new(source, $kind)?))
            }

            pub fn from_strings<I, S>(strings: I) -> Result<Self, TextAuthoringError>
            where
                I: IntoIterator<Item = S>,
                S: Into<Arc<str>>,
            {
                Ok(Self(LatexSpec::from_arguments(strings, $kind)?))
            }

            pub fn source(&self) -> &str {
                self.0.document.source()
            }

            /// UTF-8 source ranges for explicit string arguments and isolated
            /// MathTex double-brace parts.
            pub fn part_spans(&self) -> &[noon_core::TextSourceSpan] {
                self.0.document.part_spans()
            }

            pub const fn font_size(&self) -> f32 {
                self.0.font_size
            }

            pub fn with_font_size(mut self, font_size: f32) -> Self {
                self.0.font_size = font_size;
                self
            }

            pub fn with_preamble(
                mut self,
                preamble: impl Into<Arc<str>>,
            ) -> Result<Self, TextAuthoringError> {
                self.0.document = self.0.document.with_preamble(preamble)?;
                Ok(self)
            }

            pub fn with_environment(
                mut self,
                environment: Option<Arc<str>>,
            ) -> Result<Self, TextAuthoringError> {
                self.0.document = self.0.document.with_environment(environment)?;
                Ok(self)
            }

            pub fn color(mut self, color: Color) -> Self {
                self.0.presentation.color = color;
                self
            }

            pub fn set_opacity(mut self, opacity: f32) -> Self {
                self.0.presentation.opacity = opacity;
                self
            }

            pub fn shift(mut self, offset: Vec2) -> Self {
                self.0.presentation.transform.translation += offset;
                self
            }

            pub fn move_to(mut self, point: Vec2) -> Self {
                self.0.presentation.transform.translation = point;
                self
            }

            pub fn scale(mut self, factor: f32) -> Self {
                self.0.presentation.transform.scale = Vec2::new(
                    self.0.presentation.transform.scale.x * factor,
                    self.0.presentation.transform.scale.y * factor,
                );
                self
            }

            pub fn scale_xy(mut self, factor: Vec2) -> Self {
                self.0.presentation.transform.scale =
                    self.0.presentation.transform.scale.component_mul(factor);
                self
            }

            pub fn rotate(mut self, angle: f32) -> Self {
                self.0.presentation.transform.rotation += angle;
                self
            }
        }
    };
}

latex_object!(Tex, TextSourceKind::Tex);
latex_object!(MathTex, TextSourceKind::MathTex);

impl crate::Mobject {
    /// Compile real TeX into a detached Mobject without bootstrapping execution.
    pub fn from_tex(
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        text: Tex,
        backend: &mut impl LatexBackend,
    ) -> Result<Self, TextAuthoringError> {
        publish_detached(store, prepare_tex(text, backend)?)
    }

    /// Compile real display math into a detached Mobject without bootstrapping execution.
    pub fn from_math_tex(
        store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
        text: MathTex,
        backend: &mut impl LatexBackend,
    ) -> Result<Self, TextAuthoringError> {
        publish_detached(store, prepare_math_tex(text, backend)?)
    }
}

impl LatexParts {
    pub fn from_tex(
        store: Rc<RefCell<noon_core::SemanticStore>>,
        text: Tex,
        backend: &mut impl LatexBackend,
    ) -> Result<Self, TextAuthoringError> {
        publish_detached_parts(store, prepare_tex(text, backend)?)
    }
    pub fn from_math_tex(
        store: Rc<RefCell<noon_core::SemanticStore>>,
        text: MathTex,
        backend: &mut impl LatexBackend,
    ) -> Result<Self, TextAuthoringError> {
        publish_detached_parts(store, prepare_math_tex(text, backend)?)
    }
}

fn publish_detached(
    store: std::rc::Rc<std::cell::RefCell<noon_core::SemanticStore>>,
    admission: LatexAdmission,
) -> Result<crate::Mobject, TextAuthoringError> {
    let result = {
        let mut store_ref = store.borrow_mut();
        admission.publish(&mut store_ref, |store, transaction| {
            transaction
                .apply(store)
                .map_err(crate::AuthoringError::from)
                .map_err(TextAuthoringError::Semantic)
        })
    }?;
    let [noon_core::SemanticMutationImpact::NodeAdded { node }] = result.impacts() else {
        unreachable!("one LaTeX admission creates one detached semantic node")
    };
    crate::Mobject::from_node(store, *node).map_err(TextAuthoringError::Semantic)
}

fn publish_detached_parts(
    store: Rc<RefCell<noon_core::SemanticStore>>,
    admission: LatexAdmission,
) -> Result<LatexParts, TextAuthoringError> {
    let (result, family, members, source, parts) = {
        let mut store_ref = store.borrow_mut();
        admission.publish_parts(&mut store_ref, |store, transaction| {
            transaction
                .apply(store)
                .map_err(crate::AuthoringError::from)
                .map_err(TextAuthoringError::Semantic)
        })?
    };
    finish_latex_parts(store, &result, family, members, source, parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct CountingBackend {
        compile_calls: usize,
        font_calls: usize,
    }

    impl LatexBackend for CountingBackend {
        fn identity(&self) -> &str {
            "test-engine-v1"
        }

        fn format(&self) -> LatexFormat {
            LatexFormat::Preloaded
        }

        fn compile(&mut self, _: &str) -> Result<Vec<u8>, String> {
            self.compile_calls += 1;
            Err("test backend must not compile invalid input".into())
        }

        fn font(&mut self, _: &str) -> Result<DviFontResource, String> {
            self.font_calls += 1;
            Err("test backend must not resolve invalid input".into())
        }
    }

    #[test]
    fn invalid_presentation_is_rejected_before_backend_work() {
        let mut backend = CountingBackend::default();
        let result = prepare_tex(
            Tex::new("x").unwrap().with_font_size(f32::NAN),
            &mut backend,
        );
        assert!(
            matches!(result, Err(TextAuthoringError::InvalidFontSize(value)) if value.is_nan())
        );
        assert_eq!(backend.compile_calls, 0);
        assert_eq!(backend.font_calls, 0);
    }

    #[test]
    #[ignore = "requires host-supplied real DVI and BaKoMa resources"]
    fn real_fixture_keeps_fraction_rules_and_skips_backend_on_cache_hit() {
        use std::{collections::BTreeMap, fs, path::PathBuf};

        struct FixtureBackend {
            dvi: Vec<u8>,
            fonts: BTreeMap<Arc<str>, DviFontResource>,
            compile_calls: usize,
            font_calls: usize,
        }

        impl LatexBackend for FixtureBackend {
            fn identity(&self) -> &str {
                "real-fixture-backend-v1"
            }

            fn format(&self) -> LatexFormat {
                LatexFormat::Preloaded
            }

            fn compile(&mut self, _: &str) -> Result<Vec<u8>, String> {
                self.compile_calls += 1;
                Ok(self.dvi.clone())
            }

            fn font(&mut self, name: &str) -> Result<DviFontResource, String> {
                self.font_calls += 1;
                self.fonts
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("missing fixture font {name}"))
            }
        }

        let env_path = |name: &str| {
            PathBuf::from(std::env::var_os(name).unwrap_or_else(|| panic!("{name} must be set")))
        };
        let dvi = fs::read(env_path("NOON_LATEX_DVI")).unwrap();
        let names =
            noon_text::latex::required_dvi_fonts(&dvi, noon_text::latex::DviLimits::default())
                .unwrap();
        let ttf_dir = env_path("NOON_LATEX_TTF_DIR");
        let tfm_dir = env_path("NOON_LATEX_TFM_DIR");
        let fonts = names
            .iter()
            .map(|name| {
                (
                    name.clone(),
                    DviFontResource::bakoma(
                        name.clone(),
                        "real-fixture-backend-v1",
                        fs::read(tfm_dir.join(format!("{name}.tfm"))).unwrap(),
                        fs::read(ttf_dir.join(format!("{name}.ttf"))).unwrap(),
                    )
                    .unwrap(),
                )
            })
            .collect();
        let mut backend = FixtureBackend {
            dvi,
            fonts,
            compile_calls: 0,
            font_calls: 0,
        };
        crate::text_authoring::compiler::clear_text_compiler_cache();
        let source = "x^2+\\frac{1}{2}";
        let mut scene = crate::Scene::new();
        let first = scene
            .math_tex(MathTex::new(source).unwrap(), &mut backend)
            .unwrap();
        let first_handle = first.state().unwrap().content.text().unwrap();
        let resource = scene
            .integration_store()
            .borrow()
            .text_resources()
            .get(first_handle)
            .unwrap()
            .clone();
        assert_eq!(resource.vector_items.len(), 1, "fraction rule is retained");
        assert_eq!(backend.compile_calls, 1);
        assert_eq!(backend.font_calls, names.len());

        let second = scene
            .math_tex(MathTex::new(source).unwrap(), &mut backend)
            .unwrap();
        assert_eq!(second.state().unwrap().content.text(), Some(first_handle));
        assert_eq!(backend.compile_calls, 1);
        assert_eq!(backend.font_calls, names.len());

        let direct = crate::Mobject::from_math_tex(
            std::rc::Rc::clone(scene.integration_store()),
            MathTex::new(source).unwrap(),
            &mut backend,
        )
        .unwrap();
        assert_eq!(direct.state().unwrap().content.text(), Some(first_handle));

        let mut execution = scene.execution_session().unwrap();
        let live = scene
            .live(&mut execution)
            .create_math_tex(MathTex::new(source).unwrap(), &mut backend)
            .unwrap();
        assert_eq!(live.state().unwrap().content.text(), Some(first_handle));
        assert_eq!(
            backend.compile_calls, 1,
            "compiler cache skips host recompilation"
        );
        assert_eq!(
            backend.font_calls,
            names.len(),
            "cached fonts skip host lookup"
        );

        assert_eq!(
            scene.integration_store().borrow().font_resources().len(),
            names.len(),
            "compiled font resources are retained once"
        );
    }

    #[test]
    fn mathtex_from_strings_keeps_argument_and_double_brace_parts() {
        let text = MathTex::from_strings(["x{{+}}y", "{{ z }}"]).unwrap();
        assert_eq!(text.source(), "x{{+}}y  z ");
        assert_eq!(
            text.part_spans(),
            [
                noon_core::TextSourceSpan::new(0, 7),
                noon_core::TextSourceSpan::new(8, 11),
            ]
        );
    }

    #[test]
    fn generated_document_and_backend_identity_define_latex_identity() {
        let source = Tex::new("x^2").unwrap();
        let styled = source.clone().color(Color::RED).shift(Vec2::new(2.0, -1.0));
        let source_document = source.0.document.render(LatexFormat::Preloaded).unwrap();
        let styled_document = styled.0.document.render(LatexFormat::Preloaded).unwrap();
        assert_eq!(source_document, styled_document);
        let source_identity = crate::text_authoring::compiler::latex_identity(
            &source_document,
            "engine-a",
            TextSourceKind::Tex,
        );
        assert_eq!(
            source_identity,
            crate::text_authoring::compiler::latex_identity(
                &styled_document,
                "engine-a",
                TextSourceKind::Tex,
            )
        );
        assert_ne!(
            source_identity,
            crate::text_authoring::compiler::latex_identity(
                &styled_document,
                "engine-b",
                TextSourceKind::Tex,
            )
        );
        let math_document = MathTex::new("x^2")
            .unwrap()
            .0
            .document
            .render(LatexFormat::Preloaded)
            .unwrap();
        assert_ne!(source_document, math_document);

        let tex_as_math = Tex::new("x^2")
            .unwrap()
            .with_environment(Some(Arc::from("align*")))
            .unwrap();
        let tex_as_math_document = tex_as_math
            .0
            .document
            .render(LatexFormat::Preloaded)
            .unwrap();
        assert_eq!(tex_as_math_document, math_document);
        assert_ne!(
            crate::text_authoring::compiler::latex_identity(
                &tex_as_math_document,
                "engine-a",
                TextSourceKind::Tex,
            ),
            crate::text_authoring::compiler::latex_identity(
                &math_document,
                "engine-a",
                TextSourceKind::MathTex,
            ),
            "resource identity carries source kind even when host input matches"
        );
    }
}

//! Complete M0 source review, NOT a compiled shipping example.
//! Proposed operations are labelled below; no placeholder implementation exists.
//! The executable supported subset is crates/noon/examples/effect_authoring_contract.rs.
//! Same scenes/checkpoints as authoring_review.py. Normal LiveContinuation handles
//! segment completion; no per-frame callback, private timeline or renderer loop.
use effect_fixture::{ScanBand, ScanBandUpdate};
use noon::effects::{Bloom, BloomUpdate, EffectOptions, EffectScope, Glow, GlowUpdate, Pixels}; // scope options: M3/M4
use noon::{
    AnimationOptions, ContinuationStep, DeclaredAnimation, LiveContinuation, LiveProgram,
    LiveSession, ManimGeometryOptions, Mobject, MobjectFamily, MobjectTarget, RateFunction, Scene,
    SemanticAnimationCompositionKind as Kind, SemanticStyle, Text, Vec2, VectorPath,
};
use noon_core::SemanticAnimationIntent as Intent;

type Error = Box<dyn std::error::Error>;
fn linear(seconds: f64) -> AnimationOptions {
    AnimationOptions::new()
        .run_time(seconds)
        .rate_func(RateFunction::Linear)
}
fn white_dot(scene: &mut Scene) -> Result<Mobject, Error> {
    let mut shape = ManimGeometryOptions::circle(0.08)?;
    shape.set_fill(1.0, 1.0, 1.0, 1.0)?;
    shape.disable_stroke();
    Ok(scene.geometry(shape)?)
}

pub struct LuminousExplanation {
    dot: Mobject,
    movement: DeclaredAnimation,
    step: u8,
}
impl LiveContinuation for LuminousExplanation {
    type Error = Error;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Error> {
        let step = self.step;
        self.step += 1;
        let segment = match step {
            0 => live.play_animation(&self.movement)?,
            // M2: a typed ordinary animation intent, not another playback owner.
            1 => live.declare_and_activate_glow_pulse(
                &self.dot,
                2.0,
                AnimationOptions::new().run_time(0.6),
            )?,
            2 => live.wait_segment(0.5)?,
            // M1: exact attachment-parameter channels; no stale geometry target.
            3 => live.declare_and_activate_effect_to(
                &self.dot,
                "glow",
                GlowUpdate::default().intensity(0.0),
                linear(0.4),
            )?,
            _ => {
                live.remove_glow(&self.dot)?;
                return Ok(ContinuationStep::Finished);
            }
        };
        Ok(ContinuationStep::Await(segment))
    }
}
pub fn luminous_program() -> Result<LiveProgram<LuminousExplanation>, Error> {
    let mut scene = Scene::new();
    let dot = white_dot(&mut scene)?;
    scene.set_glow(&dot, GlowUpdate::default().radius(0.15).intensity(0.35))?;
    scene.add(&dot)?;
    let mut target = dot.target_editor()?;
    target.shift(2.0, 0.0)?;
    target.set_glow(GlowUpdate::default().intensity(1.2))?;
    let movement = scene.declare_transform_to(&dot, &target, linear(1.5))?;
    Ok(scene.into_live_program(LuminousExplanation {
        dot,
        movement,
        step: 0,
    })?)
}

pub struct MixedEffectsReview {
    dot: Mobject,
    title: Mobject,
    group: MobjectFamily,
    frame: Mobject,
    _aliases: MobjectFamily,
    parallel: DeclaredAnimation,
    sequence: DeclaredAnimation,
    dark_view: DeclaredAnimation,
    step: u8,
}
impl LiveContinuation for MixedEffectsReview {
    type Error = Error;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, Error> {
        let step = self.step;
        self.step += 1;
        let segment = match step {
            0 => live.play_animation(&self.parallel)?,
            1 => live.declare_and_activate_glow_pulse(
                &self.dot,
                2.0,
                AnimationOptions::new().run_time(0.6),
            )?,
            2 => live.play_animation(&self.sequence)?,
            3 => {
                live.set_effect(
                    &self.group,
                    "group-halo",
                    GlowUpdate::default().intensity(0.25),
                )?;
                live.wait_segment(0.5)?
            }
            4 => live.play_animation(&self.dark_view)?,
            _ => {
                live.remove_effect(&self.frame, "bloom")?;
                live.remove_effect(&self.group, "group-halo")?;
                live.remove_effect(&self.title, "scan")?;
                live.on_click(&self.dot, None)?;
                return Ok(ContinuationStep::Finished);
            }
        };
        Ok(ContinuationStep::Await(segment))
    }
}

pub fn mixed_program() -> Result<LiveProgram<MixedEffectsReview>, Error> {
    let mut scene = Scene::new();
    let frame = scene.camera_frame()?; // existing semantic camera, before adding source objects
    let mut title = scene.text(
        Text::new("Signal")
            .with_font("DejaVu Sans Mono")
            .with_font_size(48.0),
    )?;
    title.shift(0.0, 2.0)?;
    let path = scene.path(
        VectorPath::new()
            .move_to(Vec2::new(-2.0, -1.0))
            .line_to(Vec2::new(0.0, 1.0))
            .line_to(Vec2::new(2.0, -1.0)),
        SemanticStyle {
            fill_opacity: 0.0,
            stroke_width: 2.0,
            ..SemanticStyle::default()
        },
    )?;
    let mut image = scene.image_rgba8(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 128, 255, 255, 255, 255,
        ],
    )?;
    image.set_height(1.0)?;
    let dot = white_dot(&mut scene)?;
    let inner = scene.family(&[MobjectTarget::Object(&path), (&image).into()])?;
    let group = scene.family(&[(&inner).into(), (&dot).into()])?;
    let aliases = scene.family(&[(&inner).into(), (&dot).into(), (&path).into()])?;
    scene.add_many(&[(&group).into(), (&title).into()])?;
    scene.set_glow(&dot, GlowUpdate::default().intensity(0.35))?;
    scene.add_effect(
        &title,
        Glow::new(GlowUpdate::default().radius(Pixels(12.0)))?,
        "accent",
    )?;
    scene.add_effect(&title, ScanBand::new(0.2, 0.0), "scan")?;
    // M3/M4: explicit authored scopes; never infer isolation or recurse on lookup.
    scene.add_effect(
        &group,
        Glow::new(GlowUpdate::default().radius(Pixels(12.0)))?,
        EffectOptions::named("group-halo").scope(EffectScope::Composed),
    )?;
    scene.add_effect(
        &frame,
        Bloom::new(0.5),
        EffectOptions::named("bloom").scope(EffectScope::View),
    )?;
    let click = scene.declare_glow_pulse(&dot, 2.0, AnimationOptions::new().run_time(0.6))?;
    scene.on_click(&dot, Some(&click))?;
    let mut target = dot.target_editor()?;
    target.shift(2.0, 0.0)?;
    target.set_glow(GlowUpdate::default().intensity(1.2))?;
    let movement = scene.declare_transform_to(&dot, &target, linear(1.5))?;
    // M1/M4: typed partial requests, with values captured at activation, not here.
    let accent = scene.declare_effect_to(
        &title,
        "accent",
        GlowUpdate::default().intensity(1.4),
        linear(1.5),
    )?;
    let parallel = scene.declare_animation(
        Intent::Composition {
            kind: Kind::Parallel,
            children: vec![movement.node_id(), accent.node_id()],
        },
        AnimationOptions::new().rate_func(RateFunction::Linear),
    )?;
    let scan = scene.declare_effect_to(&title, "scan", ScanBandUpdate::phase(1.0), linear(0.5))?;
    let dim = scene.declare_effect_to(
        &dot,
        "glow",
        GlowUpdate::default().intensity(0.5),
        linear(0.5),
    )?;
    let sequence = scene.declare_animation(
        Intent::Composition {
            kind: Kind::Sequence,
            children: vec![scan.node_id(), dim.node_id()],
        },
        AnimationOptions::new().rate_func(RateFunction::Linear),
    )?;
    let dark_view =
        scene.declare_effect_to(&frame, "bloom", BloomUpdate::intensity(0.0), linear(0.3))?;
    Ok(scene.into_live_program(MixedEffectsReview {
        dot,
        title,
        group,
        frame,
        _aliases: aliases,
        parallel,
        sequence,
        dark_view,
        step: 0,
    })?)
}

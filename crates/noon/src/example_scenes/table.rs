//! One retained plain, math, and composite-entry table scene for every host.

use crate::{
    AnimationOptions, Color, ContinuationScene, ContinuationStep, DecimalFormat, DecimalTable,
    LatexBackend, LiveContinuation, LiveProgram, MathTable, MathTex, MobjectFamily, MobjectTable,
    MobjectTarget, RateFunction, Scene, ScriptError, StrokeCap, StrokeJoin, Table, TableOptions,
    Vec2,
};

pub struct LabeledTableShift {
    family: MobjectFamily,
    step: u8,
}

impl LiveContinuation for LabeledTableShift {
    const EFFECTS: crate::ContinuationEffects = crate::ContinuationEffects::SceneOnly;
    type Error = ScriptError;

    fn resume(&mut self, scene: &mut ContinuationScene) -> Result<ContinuationStep, ScriptError> {
        let delta = match self.step {
            0 => Vec2::new(1.5, 0.0),
            1 => Vec2::new(0.0, 1.0),
            _ => return Ok(ContinuationStep::Finished),
        };
        let segment = crate::animation_request::activate(
            &mut scene.live(),
            self.family.animate().shift(delta).into(),
            AnimationOptions::new()
                .run_time(1.0)
                .rate_func(RateFunction::Linear),
        )?;
        self.step += 1;
        Ok(ContinuationStep::Await(segment))
    }
}

/// The labeled table enters with its original row label and cell handles.
pub fn family_labels_program() -> Result<LiveProgram<LabeledTableShift>, String> {
    let mut scene = Scene::new();
    let first = scene.square(0.36).map_err(|error| error.to_string())?;
    let second = scene.circle(0.22).map_err(|error| error.to_string())?;
    let label_dot = scene.circle(0.12).map_err(|error| error.to_string())?;
    let label_square = scene.square(0.16).map_err(|error| error.to_string())?;
    scene
        .shift(&label_square, Vec2::new(0.3, 0.0))
        .map_err(|error| error.to_string())?;
    let label = scene
        .family(&[(&label_dot).into(), (&label_square).into()])
        .map_err(|error| error.to_string())?;
    let table = MobjectTable::from_target_rows_with_optional_composite_labels(
        &mut scene,
        [[(&first).into(), (&second).into()]],
        Some(vec![(&label).into()]),
        None,
        None,
        TableOptions {
            h_buff: 0.7,
            v_buff: 0.45,
            include_outer_lines: true,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    let family = table.table().family().clone();
    family.shift(-1.5, 0.0).map_err(|error| error.to_string())?;
    scene
        .into_live_program(LabeledTableShift { family, step: 0 })
        .map_err(|error| error.to_string())
}

#[cfg(all(test, feature = "prepared-prefix"))]
#[test]
fn labeled_table_rewinds_across_detached_and_mounted_shifts() {
    let mut player = family_labels_program().unwrap().into_playback().unwrap();
    let mut callbacks = crate::RustHostCallbackTable::new();
    let snapshot = |player: &crate::PlaybackProgram<LabeledTableShift>| {
        let session = player.session().unwrap();
        session
            .painter_order()
            .iter()
            .map(|&index| {
                let row = &session.frame().objects[index as usize];
                (row.id, row.transform.translation)
            })
            .collect::<Vec<_>>()
    };
    let mut samples = Vec::new();
    for time in [0.5, 1.0, 1.5, 2.0] {
        for _ in 0..8 {
            player.prepare_ready(&mut callbacks).unwrap();
            player.advance_to(&mut callbacks, time).unwrap();
            if !player.buffering() && player.session().unwrap().frame().time == time {
                break;
            }
        }
        assert!(!player.buffering());
        assert_eq!(player.session().unwrap().frame().time, time);
        samples.push(snapshot(&player));
    }
    let first = &samples[0];
    let second = &samples[2];
    assert!(!first.is_empty());
    assert_eq!(first.len(), second.len());
    for ((first_id, first_position), (second_id, second_position)) in first.iter().zip(second) {
        assert_eq!(first_id, second_id);
        let delta = *second_position - *first_position - Vec2::new(0.75, 0.5);
        assert!(delta.x.abs() < 1e-6 && delta.y.abs() < 1e-6);
    }
    player.seek(0.5).unwrap();
    assert_eq!(snapshot(&player), *first);
}

/// Plain-text Table counterpart of the ManimCE fixture with retained labels.
pub fn labels_session() -> Result<crate::SceneExecution, String> {
    let mut scene = Scene::new();
    let row_labels = [
        scene.circle(0.15).map_err(|error| error.to_string())?,
        scene.square(0.4).map_err(|error| error.to_string())?,
    ];
    let column_labels = [scene.circle(0.25).map_err(|error| error.to_string())?];
    let top_left_entry = scene.square(0.3).map_err(|error| error.to_string())?;
    let table = Table::from_rows_with_composite_labels(
        &mut scene,
        [[" "], [" "]],
        Some(row_labels.iter().map(MobjectTarget::from).collect()),
        Some(column_labels.iter().map(MobjectTarget::from).collect()),
        Some(MobjectTarget::from(&top_left_entry)),
        TableOptions {
            h_buff: 0.7,
            v_buff: 0.45,
            include_outer_lines: true,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    scene
        .add_many(&[table.family().into()])
        .map_err(|error| error.to_string())?;
    scene.into_execution().map_err(|error| error.to_string())
}

/// Retained counterpart of the ManimCE line-config fixture, including cap and
/// join style values that are carried through Table's sparse layout.
pub fn line_config_session() -> Result<crate::SceneExecution, String> {
    let mut scene = Scene::new();
    let first_circle = scene.circle(0.3).map_err(|error| error.to_string())?;
    let first_square = scene.square(0.5).map_err(|error| error.to_string())?;
    let second_square = scene.square(0.4).map_err(|error| error.to_string())?;
    let second_circle = scene.circle(0.2).map_err(|error| error.to_string())?;
    let table = MobjectTable::from_rows_with_options(
        &mut scene,
        vec![
            vec![first_circle, first_square],
            vec![second_square, second_circle],
        ],
        TableOptions {
            h_buff: 0.7,
            v_buff: 0.45,
            include_outer_lines: true,
            line_stroke_color: Color::RED,
            line_stroke_width: 0.06,
            line_stroke_opacity: 0.6,
            line_stroke_join: StrokeJoin::Bevel,
            line_stroke_cap: StrokeCap::Square,
            cell_alignment_x: -1.0,
            cell_alignment_y: 1.0,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?
    .into_table();
    scene
        .add_many(&[table.family().into()])
        .map_err(|error| error.to_string())?;
    scene.into_execution().map_err(|error| error.to_string())
}

pub fn session(backend: &mut impl LatexBackend) -> Result<crate::SceneExecution, String> {
    let mut scene = Scene::new();
    let options = TableOptions {
        include_outer_lines: true,
        ..Default::default()
    };
    let plain = Table::from_rows_with_options(
        &mut scene,
        [["plain", "native"], ["text", "table"]],
        options,
    )
    .map_err(|error| error.to_string())?;
    plain
        .family()
        .shift(-3.0, 1.4)
        .map_err(|error| error.to_string())?;
    // Generic Table entry handles retain text content without a frontend class.
    // Every direct native/WASM host must select the existing content query.
    for entry in plain.get_entries().map_err(|error| error.to_string())? {
        assert!(entry
            .as_target()
            .requires_content_boundary()
            .map_err(|error| error.to_string())?);
    }
    let math = MathTable::from_rows_with_options(
        &mut scene,
        backend,
        [["x^2", "y"], [r"\alpha", r"\frac{1}{2}"]],
        options,
    )
    .map_err(|error| error.to_string())?
    .into_table();
    math.family()
        .shift(3.0, 1.4)
        .map_err(|error| error.to_string())?;

    let x = scene
        .math_tex(
            MathTex::new("x").map_err(|error| error.to_string())?,
            backend,
        )
        .map_err(|error| error.to_string())?;
    let mut plus = scene
        .math_tex(
            MathTex::new("+").map_err(|error| error.to_string())?,
            backend,
        )
        .map_err(|error| error.to_string())?;
    plus.shift(0.5, 0.0).map_err(|error| error.to_string())?;
    let group = MobjectFamily::create(
        std::rc::Rc::clone(scene.integration_store()),
        &[(&x).into(), (&plus).into()],
    )
    .map_err(|error| error.to_string())?;
    let y = scene
        .math_tex(
            MathTex::new("y").map_err(|error| error.to_string())?,
            backend,
        )
        .map_err(|error| error.to_string())?;
    let row_label = scene.text("r").map_err(|error| error.to_string())?;
    let column_labels = ["a", "b"]
        .into_iter()
        .map(|label| scene.text(label).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let top_left_entry = scene.text("i").map_err(|error| error.to_string())?;
    let composite_options = TableOptions {
        include_inner_lines: false,
        line_stroke_color: Color::YELLOW,
        line_stroke_width: 0.06,
        line_stroke_opacity: 0.75,
        ..options
    };
    let composite = MobjectTable::from_target_rows_with_labels_and_top_left_entry(
        &mut scene,
        vec![vec![(&group).into(), (&y).into()]],
        vec![row_label],
        column_labels,
        top_left_entry,
        composite_options,
    )
    .map_err(|error| error.to_string())?
    .into_table();
    composite
        .family()
        .shift(0.0, -2.0)
        .map_err(|error| error.to_string())?;
    scene
        .add_many(&[
            plain.family().into(),
            math.family().into(),
            composite.family().into(),
        ])
        .map_err(|error| error.to_string())?;
    let execution = scene.into_execution().map_err(|error| error.to_string())?;
    for entry in plain.get_entries().map_err(|error| error.to_string())? {
        assert!(execution
            .scene()
            .requires_content_boundary(entry.as_target())
            .map_err(|error| error.to_string())?);
    }
    Ok(execution)
}

/// DecimalTable counterpart for ManimCE's configurable DecimalNumber entries.
pub fn decimal_format_session(
    backend: &mut impl LatexBackend,
) -> Result<crate::SceneExecution, String> {
    let mut scene = Scene::new();
    let table = DecimalTable::from_rows_with_options(
        &mut scene,
        backend,
        [[1234.5, -2.25]],
        DecimalFormat {
            decimal_places: 2,
            include_sign: true,
            group_with_commas: true,
            ..Default::default()
        },
        TableOptions {
            include_outer_lines: true,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?
    .into_table();
    scene
        .add_many(&[table.family().into()])
        .map_err(|error| error.to_string())?;
    scene.into_execution().map_err(|error| error.to_string())
}

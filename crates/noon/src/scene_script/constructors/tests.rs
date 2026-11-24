use std::error::Error;

use super::*;
use crate::{AuthoringError, CoordinateAuthoringError, LiveProgramStatus, Scene};

fn assert_rejected_without_change<T>(
    script: &ScriptScene,
    operation: impl FnOnce() -> Result<T, ScriptError>,
) -> ScriptError {
    let snapshot = || {
        script
            .with_scene(|scene| {
                let store = scene.integration_store().borrow();
                Ok((
                    scene.revision(),
                    store.len(),
                    store.text_resources().len(),
                    store.font_resources().len(),
                    store.geometry_resources().len(),
                    store.raster_image_resources().len(),
                    scene.running_execution().map(|execution| {
                        (execution.publication_context(), execution.frame().clone())
                    }),
                ))
            })
            .unwrap()
    };
    let before = snapshot();
    let error = operation().err().expect("invalid admission must fail");
    assert_eq!(snapshot(), before);
    error
}

#[test]
fn image_and_coordinate_admission_is_atomic_before_and_after_a_boundary() {
    let mut program = Scene::script(|script| async move {
        let anchor = script.square(0.5)?;
        script.add(&anchor)?;
        for _ in 0..2 {
            let before = script.with_scene(|scene| {
                Ok((
                    anchor.state()?,
                    scene.running_execution().map(|execution| {
                        (
                            execution.runtime_identity(),
                            execution.execution_object_id(anchor.node_id()),
                        )
                    }),
                ))
            })?;
            let error =
                assert_rejected_without_change(&script, || script.image_rgba8(2, 1, vec![255; 4]));
            assert!(matches!(
                error,
                ScriptError::Authoring(AuthoringError::ImageResource(_))
            ));

            let mut options = ManimAxesOptions::new([-1.0, 1.0, 1.0], [-1.0, 1.0, 1.0], 2.0, 2.0);
            options.x_length = 0.0;
            let error = assert_rejected_without_change(&script, || script.axes(&options));
            assert!(matches!(error, ScriptError::Coordinate(_)));
            assert!(error.source().unwrap().is::<CoordinateAuthoringError>());
            options.x_length = 2.0;

            let image = script.image_rgba8(1, 1, vec![255, 0, 0, 255])?;
            assert_eq!(image.image_dimensions()?, (1, 1));
            script.add(&image)?;
            let duplicate =
                script.image(ImageMobjectOptions::rgba8(1, 1, vec![255, 0, 0, 255])?)?;
            assert_ne!(image.node_id(), duplicate.node_id());
            script.add(&duplicate)?;
            let axes = script.axes(&options)?;
            let line = script.number_line(&ManimNumberLineOptions::new([-1.0, 1.0, 1.0]))?;
            let plane = script.number_plane(&ManimNumberPlaneOptions {
                x_range: [-1.0, 1.0, 1.0],
                y_range: [-1.0, 1.0, 1.0],
                faded_line_ratio: 0,
                ..Default::default()
            })?;
            script.add(axes.family())?;
            script.add(line.family())?;
            script.add(plane.family())?;
            script.with_scene(|scene| {
                assert_eq!(anchor.state()?, before.0);
                if let Some((runtime, object)) = before.1 {
                    let execution = scene.running_execution().unwrap();
                    assert_eq!(execution.runtime_identity(), runtime);
                    assert_eq!(execution.execution_object_id(anchor.node_id()), object);
                }
                // Pixel preparation deduplicates resources; constructors do not
                // introduce a second resource owner for each capability call.
                assert_eq!(
                    scene
                        .integration_store()
                        .borrow()
                        .raster_image_resources()
                        .len(),
                    1
                );
                Ok(())
            })?;
            script.wait(0.25).await?;
        }
        Ok(())
    })
    .unwrap();
    program.resume().unwrap();
    super::super::tests::finish_wait(&mut program, 0.25);
    program.resume().unwrap();
    super::super::tests::finish_wait(&mut program, 0.5);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
}

#[cfg(all(
    feature = "native-text",
    feature = "typst",
    feature = "code",
    feature = "bundled-fonts"
))]
#[test]
fn shared_text_and_code_constructors_preserve_causes_and_recover_without_partial_resources() {
    use crate::{CodeAuthoringError, CodeOptions, MathTypst, Text, TextAuthoringError, Typst};

    let mut program = Scene::script(|script| async move {
        for _ in 0..2 {
            let error = assert_rejected_without_change(&script, || {
                script.text(Text::new("invalid").with_font_size(0.0))
            });
            assert!(matches!(
                error,
                ScriptError::Text(TextAuthoringError::InvalidFontSize(0.0))
            ));
            assert!(error.source().unwrap().is::<TextAuthoringError>());
            let error = assert_rejected_without_change(&script, || {
                script.typst(Typst::new("#noon_missing_release_test_name"))
            });
            assert!(matches!(
                error,
                ScriptError::Text(TextAuthoringError::Typst(_))
            ));
            let error = assert_rejected_without_change(&script, || {
                script.code(
                    "let x = 1;",
                    CodeOptions {
                        tab_width: 257,
                        ..Default::default()
                    },
                )
            });
            assert!(matches!(
                error,
                ScriptError::Code(CodeAuthoringError::InvalidOption { .. })
            ));
            assert!(error.source().unwrap().is::<CodeAuthoringError>());

            let title = script.text("A retained title")?;
            let markup = script.typst(Typst::new("A *retained* label"))?;
            let equation = script.math_typst(MathTypst::new("1 + 1 = 2"))?;
            let code = script.code(
                "let x = 1;",
                CodeOptions {
                    language: Some("rust".into()),
                    ..Default::default()
                },
            )?;
            for object in [&title, &markup, &equation] {
                assert!(object.state()?.content.text().is_some());
                script.add(object)?;
            }
            assert_eq!(code.line_source(0)?.as_ref(), "let x = 1;");
            script.add(code.family())?;
            script.wait(0.25).await?;
        }
        Ok(())
    })
    .unwrap();
    program.resume().unwrap();
    super::super::tests::finish_wait(&mut program, 0.25);
    program.resume().unwrap();
    super::super::tests::finish_wait(&mut program, 0.5);
    assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
}

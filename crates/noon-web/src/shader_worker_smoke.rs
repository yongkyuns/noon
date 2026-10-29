//! The native/direct-WASM material lesson through the genuine worker codecs.
//! Qualification exports are excluded from production builds.
use crate::{
    RetainedFamilyExecutionDeltaEncoder, RetainedFamilyExecutionDeltaEnvelope,
    RetainedResourceBundle,
};
use noon::{LiveProgramStatus, RustHostCallbackTable};
use serde::Serialize;

#[derive(Serialize)]
struct Sample {
    time: f64,
    deltas: Vec<RetainedFamilyExecutionDeltaEnvelope>,
}
#[derive(Serialize)]
struct Fixture {
    resources: Vec<u8>,
    samples: Vec<Sample>,
}

fn fixture() -> Result<Fixture, String> {
    let mut program = noon::example_scenes::shader_sweep::late_program()?;
    let mut callbacks = RustHostCallbackTable::default();
    let mut encoder = RetainedFamilyExecutionDeltaEncoder::new(91);
    let mut snapshot = true;
    let mut samples = Vec::new();
    for time in [0.0, 0.25, 0.5, 1.5, 2.5, 3.0, 4.0, 5.0, 5.5, 6.0] {
        let mut deltas = Vec::new();
        loop {
            match program.status() {
                LiveProgramStatus::ReadyToResume => {
                    program.resume().map_err(|e| e.to_string())?;
                }
                LiveProgramStatus::Awaiting(_) if program.session().frame().time < time => {
                    program
                        .drive_to(&mut callbacks, time)
                        .map_err(|e| e.to_string())?;
                }
                LiveProgramStatus::PublicationPending(context) => {
                    let camera = program
                        .session()
                        .inspection_camera()
                        .map_err(|e| e.to_string())?;
                    if let Some(delta) = encoder.encode_renderer_publication(
                        &program.take_renderer_publication(),
                        camera,
                        snapshot,
                        false,
                    )? {
                        deltas.push(delta);
                        snapshot = false;
                    }
                    program
                        .admit_publication(context)
                        .map_err(|e| e.to_string())?;
                }
                LiveProgramStatus::Terminal => {
                    return Err("material continuation terminated".into())
                }
                _ => break,
            }
        }
        let camera = program
            .session()
            .inspection_camera()
            .map_err(|e| e.to_string())?;
        if let Some(delta) = encoder.encode_renderer_publication(
            &program.take_renderer_publication(),
            camera,
            snapshot,
            false,
        )? {
            deltas.push(delta);
            snapshot = false;
        }
        samples.push(Sample { time, deltas });
    }
    Ok(Fixture {
        resources: RetainedResourceBundle::empty()
            .encode_binary()
            .map_err(|e| e.to_string())?,
        samples,
    })
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(js_name = shaderWorkerSmokeFixture)]
pub fn shader_worker_smoke_fixture() -> Result<String, wasm_bindgen::JsValue> {
    serde_json::to_string(&fixture().map_err(|e| wasm_bindgen::JsValue::from_str(&e))?)
        .map_err(|e| wasm_bindgen::JsValue::from_str(&e.to_string()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn lesson_uses_sparse_program_resources_and_retires_them() {
        let fixture = super::fixture().unwrap();
        let mut mirror =
            crate::InstalledRetainedExecutionMirror::from_bundle_bytes(&fixture.resources).unwrap();
        let mut transferred = 0;
        let mut retired = 0;
        for sample in fixture.samples {
            for delta in sample.deltas {
                transferred += delta
                    .resource_additions
                    .as_ref()
                    .map_or(0, |b| b.shader_program_count());
                retired += delta.resource_retirements.shader_programs.len();
                mirror
                    .apply_json(&serde_json::to_string(&delta).unwrap())
                    .unwrap();
            }
        }
        assert_eq!((transferred, retired), (2, 2));
        assert_eq!(mirror.resources().shader_program_count(), 0);
    }
}

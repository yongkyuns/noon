//! Explicit diagnostic/export boundary; the Rust engine itself uses typed state.
use noon::{diagnostics::execution_frame_value, example_scenes, LiveProgramStatus,
    RustHostCallbackTable};
use serde_json::json;

fn main() -> Result<(), String> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("usage: svg_morph_probe SOURCE.svg TARGET.svg".into());
    }
    let source = std::fs::read_to_string(&args[0]).map_err(|error| error.to_string())?;
    let target = std::fs::read_to_string(&args[1]).map_err(|error| error.to_string())?;
    let mut program = example_scenes::svg_morph::program(&source, &target)?;
    let mut callbacks = RustHostCallbackTable::new();
    let mut samples = vec![("original".to_owned(), 0.0)];
    for (direction, start) in [("forward", 0.5), ("return", 3.05)] {
        for alpha in [0.0, 0.01, 0.25, 0.5, 0.75, 0.99, 1.0] {
            samples.push((format!("{direction}-{alpha:.2}"), start + 1.8 * alpha));
        }
    }
    samples.push(("restored-hold".to_owned(), 5.6));
    samples.push(("finished".to_owned(), 5.7));
    let mut captures = Vec::new();
    for (label, time) in samples {
        loop {
            match program.status() {
                LiveProgramStatus::ReadyToResume => { program.resume().map_err(|e| e.to_string())?; }
                LiveProgramStatus::PublicationPending(expected) => {
                    let context = program.take_renderer_publication().context();
                    assert_eq!(context, expected);
                    program.admit_publication(context).map_err(|e| e.to_string())?;
                }
                LiveProgramStatus::Awaiting(_) => {
                    let status = program.drive_to(&mut callbacks, time).map_err(|e| e.to_string())?;
                    if matches!(status, LiveProgramStatus::Awaiting(_)) {
                        program.take_renderer_publication();
                        break;
                    }
                }
                LiveProgramStatus::Finished => break,
                LiveProgramStatus::Terminal => return Err("SVG morph entered terminal state".into()),
            }
        }
        let frame = execution_frame_value(program.session());
        assert!((frame["time"].as_f64().unwrap() - time).abs() < 1e-8);
        captures.push(json!({"label": label, "time": time, "debug": frame}));
    }
    assert_eq!(program.status(), LiveProgramStatus::Finished);
    println!("{}", json!({"outcome": "pass", "engine": "native-rust", "captures": captures}));
    Ok(())
}

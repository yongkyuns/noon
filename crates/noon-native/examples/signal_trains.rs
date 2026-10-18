//! Run with `cargo run -p noon-native --example signal_trains`.
//! Add `-- --replay` to record once with shared retention before playback.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => {
            noon_native::run_live_program(
                noon::example_scenes::line_passing_flash::train_program()?,
            )?;
        }
        [argument] if argument == "--replay" => {
            noon_native::run(noon::example_scenes::line_passing_flash::train_playback()?)?;
        }
        _ => return Err("usage: signal_trains [--replay]".into()),
    }
    Ok(())
}

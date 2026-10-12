//! `cargo run -p noon-native --example svg_morph -- tiger.svg rocket.svg`
//! Supply the same path-only SVG files as the paired Python example.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("usage: svg_morph SOURCE.svg TARGET.svg".into());
    }
    let source = std::fs::read_to_string(&args[0])?;
    let target = std::fs::read_to_string(&args[1])?;
    let program = noon::example_scenes::svg_morph::program(&source, &target)?;
    noon_native::run_live_program(program)?;
    Ok(())
}

//! Run with `-- 1 intensity` or `-- 100 radius`; the authored track lasts 60s.
use noon::example_scenes::glow_profile::{session, Property};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let active = args.next().map(|s| s.parse()).transpose()?.unwrap_or(1);
    let property = args
        .next()
        .map(|s| s.parse::<Property>())
        .transpose()?
        .unwrap_or(Property::Intensity);
    if args.next().is_some() {
        return Err("usage: glow_profile [1|100] [intensity|radius]".into());
    }
    noon_native::run(session(active, property, 60.0)?)?;
    Ok(())
}

//! Run with `-- 1000 fit` (or fixed/overdraw) to view the shared analytic workload.
use noon::example_scenes::analytic_profile::{session, Layout};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let count = args
        .next()
        .map(|value| value.parse())
        .transpose()?
        .unwrap_or(1_000);
    let layout = args
        .next()
        .map(|value| value.parse::<Layout>())
        .transpose()?
        .unwrap_or(Layout::Fit);
    if args.next().is_some() {
        return Err("usage: analytic_profile [count] [fit|fixed|overdraw]".into());
    }
    noon_native::run(session(count, layout, 16.0 / 9.0, 60.0)?)?;
    Ok(())
}

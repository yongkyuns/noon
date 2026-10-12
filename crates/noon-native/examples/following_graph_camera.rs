//! Native execution of the paired FollowingGraphCamera example.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (program, callbacks) = noon::example_scenes::following_graph_camera::program()?;
    noon_native::run_live_program_with_callbacks(program, callbacks)?;
    Ok(())
}

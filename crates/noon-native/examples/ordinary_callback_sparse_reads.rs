//! Native presentation of scoped scalar and sparse callback reads.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (program, callbacks) = noon::example_scenes::ordinary_callback_sparse_reads_program()?;
    noon_native::run_live_program_with_callbacks(program, callbacks)?;
    Ok(())
}

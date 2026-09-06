use noon_render_wgpu::RendererConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let program = noon::example_scenes::ordinary_uncreate_continuation_program()?;
    noon_native::run_live_program("Noon ordinary Uncreate", RendererConfig::default(), program)
}

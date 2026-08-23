from pathlib import Path

path = Path("crates/noon-web/src/lib.rs")
text = path.read_text()

old = """    const GPU_QUERY_BYTES: u64 = 16;
    const GPU_PROFILE_SLOT_COUNT: usize = 4;
    const GPU_PROFILE_SAMPLE_CAPACITY: usize = 512;

    #[derive(Clone, Copy)]
"""
new = """    const GPU_QUERY_BYTES: u64 = 16;
    const GPU_PROFILE_SLOT_COUNT: usize = 4;
    const GPU_PROFILE_SAMPLE_CAPACITY: usize = 512;

    #[derive(Debug)]
    struct WebDisplaySource;

    impl wgpu::rwh::HasDisplayHandle for WebDisplaySource {
        fn display_handle(
            &self,
        ) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
            Ok(wgpu::rwh::DisplayHandle::web())
        }
    }

    #[derive(Clone, Copy)]
"""
if text.count(old) != 1:
    raise SystemExit("expected exactly one GPU constants insertion point")
text = text.replace(old, new, 1)

old = """            let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            instance_descriptor.backends = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
"""
new = """            let mut instance_descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
            instance_descriptor.backends = wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL;
            instance_descriptor.display = Some(Box::new(WebDisplaySource));
"""
if text.count(old) != 1:
    raise SystemExit("expected exactly one instance descriptor insertion point")
text = text.replace(old, new, 1)

path.write_text(text)
Path(__file__).unlink()

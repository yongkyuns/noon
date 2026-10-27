use super::super::analytic_glow::Placement;
use super::*;

impl PreparedSource {
    pub(super) fn push_filtered(
        &mut self,
        capture: &FilteredCapture,
        alignment: usize,
    ) -> Option<()> {
        let start = self.bytes.len().checked_next_multiple_of(alignment)?;
        let placement = Placement {
            origin: capture.origin,
            size: capture.size,
            viewport: [0; 2],
            padding: [0; 2],
        };
        self.bytes.resize(start, 0);
        self.bytes.extend_from_slice(bytemuck::bytes_of(&placement));
        self.include(capture.bounds);
        self.has_filtered = true;
        self.draws.push(SourceDraw::Filtered {
            capture: capture.clone(),
            bytes: start as u64..self.bytes.len() as u64,
        });
        Some(())
    }

    /// Parent-local bindings own their uniform range. Child main-pass placement
    /// uniforms are never rewritten while an earlier capture is pending.
    pub(in crate::gpu) fn prepare_filtered_bindings(
        &self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        instances: &wgpu::Buffer,
        bindings: &mut Vec<FilteredBinding>,
    ) {
        let mut count = 0;
        for draw in &self.draws {
            let SourceDraw::Filtered { capture, bytes } = draw else {
                continue;
            };
            if bindings
                .get(count)
                .is_none_or(|old| old.texture != capture.texture || old.offset != bytes.start)
            {
                let view = capture.texture.create_view(&Default::default());
                let binding =
                    device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Noon nested source tile binding"),
                        layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                                    buffer: instances,
                                    offset: bytes.start,
                                    size: wgpu::BufferSize::new(
                                        std::mem::size_of::<Placement>() as u64
                                    ),
                                }),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&view),
                            },
                        ],
                    });
                let value = FilteredBinding {
                    texture: capture.texture.clone(),
                    offset: bytes.start,
                    binding,
                };
                if count == bindings.len() {
                    bindings.push(value);
                } else {
                    bindings[count] = value;
                }
            }
            count += 1;
        }
        bindings.truncate(count);
    }
}

//! Disposable image texture/instance residency in the existing retained renderer.
use super::{
    raster_image_prepare::{ImageUniform, RasterImageFramePreparer},
    CameraUniform, PATH_SAMPLE_COUNT,
};
use noon_core::RasterImageResourceHandle;
use std::{collections::HashMap, mem::size_of, sync::Arc};
use wgpu::util::DeviceExt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RasterImageUploadStats {
    pub textures_uploaded: usize,
    pub pixel_bytes_uploaded: usize,
    pub instances_uploaded: usize,
    pub instance_bytes_uploaded: usize,
    pub textures_retired: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RasterImageResidencyStats {
    pub textures: usize,
    pub objects: usize,
    pub pixel_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RasterImageDrawError {
    NotUploaded,
    MissingObject(usize),
}
impl std::fmt::Display for RasterImageDrawError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotUploaded => f.write_str("retained images must be uploaded before drawing"),
            Self::MissingObject(index) => write!(f, "retained image slot {index} is not resident"),
        }
    }
}
impl std::error::Error for RasterImageDrawError {}

#[derive(Debug)]
struct TextureResidency {
    _texture: wgpu::Texture,
    binding: wgpu::BindGroup,
    references: usize,
    pixel_bytes: usize,
}
#[derive(Debug)]
struct ObjectResidency {
    resource: RasterImageResourceHandle,
    uniform: ImageUniform,
    buffer: wgpu::Buffer,
    binding: wgpu::BindGroup,
}

#[derive(Debug)]
pub(super) struct RasterImageGpuRenderer {
    pipeline: wgpu::RenderPipeline,
    pipeline_msaa: wgpu::RenderPipeline,
    texture_layout: wgpu::BindGroupLayout,
    object_layout: wgpu::BindGroupLayout,
    textures: HashMap<RasterImageResourceHandle, TextureResidency>,
    objects: HashMap<usize, ObjectResidency>,
    owner: Option<Arc<()>>,
    generation: Option<u64>,
    pixel_bytes: usize,
}

impl RasterImageGpuRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let camera_layout = uniform_layout(
            device,
            "image camera",
            wgpu::ShaderStages::VERTEX,
            size_of::<CameraUniform>(),
        );
        let object_layout = uniform_layout(
            device,
            "image instance",
            wgpu::ShaderStages::VERTEX_FRAGMENT,
            size_of::<ImageUniform>(),
        );
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Noon image texture layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Noon retained image pipeline layout"),
            bind_group_layouts: &[
                Some(&camera_layout),
                Some(&texture_layout),
                Some(&object_layout),
            ],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("raster_image.wgsl"));
        Self {
            pipeline: pipeline(device, &layout, &shader, format, 1),
            pipeline_msaa: pipeline(device, &layout, &shader, format, PATH_SAMPLE_COUNT),
            texture_layout,
            object_layout,
            textures: HashMap::new(),
            objects: HashMap::new(),
            owner: None,
            generation: None,
            pixel_bytes: 0,
        }
    }

    pub fn stats(&self) -> RasterImageResidencyStats {
        RasterImageResidencyStats {
            textures: self.textures.len(),
            objects: self.objects.len(),
            pixel_bytes: self.pixel_bytes,
        }
    }

    pub fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        prepared: &RasterImageFramePreparer,
    ) -> RasterImageUploadStats {
        let mut stats = RasterImageUploadStats::default();
        if !self
            .owner
            .as_ref()
            .is_some_and(|owner| Arc::ptr_eq(owner, &prepared.owner))
        {
            stats.textures_retired = self.textures.len();
            self.objects.clear();
            self.textures.clear();
            self.pixel_bytes = 0;
            self.owner = Some(prepared.owner.clone());
            self.generation = None;
        }
        if self.generation == Some(prepared.generation) {
            return stats;
        }
        let partial = self.generation == Some(prepared.base_generation);
        let mut retired = Vec::new();
        if partial {
            for &index in &prepared.removed {
                self.remove_object(index, &mut retired);
            }
        } else {
            // A new consumer or skipped prepared generations needs reconciliation;
            // ordinary deltas touch only dirty or retired slots.
            let removed: Vec<_> = self
                .objects
                .keys()
                .copied()
                .filter(|index| !prepared.objects.contains_key(index))
                .collect();
            for index in removed {
                self.remove_object(index, &mut retired);
            }
        }
        let mut update = |index: usize| {
            let Some(row) = prepared.objects.get(&index) else {
                return;
            };
            let handle = row.content.resource();
            if self
                .objects
                .get(&index)
                .is_some_and(|old| old.resource != handle)
            {
                self.remove_object(index, &mut retired);
            }
            if let Some(old) = self.objects.get_mut(&index) {
                if old.uniform != row.uniform {
                    queue.write_buffer(&old.buffer, 0, bytemuck::bytes_of(&row.uniform));
                    old.uniform = row.uniform;
                    stats.instances_uploaded += 1;
                    stats.instance_bytes_uploaded += size_of::<ImageUniform>();
                }
                return;
            }
            let texture = self.textures.entry(handle).or_insert_with(|| {
                let size = wgpu::Extent3d {
                    width: row.resource.width(),
                    height: row.resource.height(),
                    depth_or_array_layers: 1,
                };
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("Noon immutable raster image"),
                    size,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    row.resource.rgba8(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(size.width * 4),
                        rows_per_image: Some(size.height),
                    },
                    size,
                );
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Noon image texture binding"),
                    layout: &self.texture_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    }],
                });
                let pixel_bytes = row.resource.rgba8().len();
                stats.textures_uploaded += 1;
                stats.pixel_bytes_uploaded += pixel_bytes;
                self.pixel_bytes += pixel_bytes;
                TextureResidency {
                    _texture: texture,
                    binding,
                    references: 0,
                    pixel_bytes,
                }
            });
            texture.references += 1;
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Noon retained image instance"),
                contents: bytemuck::bytes_of(&row.uniform),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Noon image instance binding"),
                layout: &self.object_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                }],
            });
            self.objects.insert(
                index,
                ObjectResidency {
                    resource: handle,
                    uniform: row.uniform,
                    buffer,
                    binding,
                },
            );
            stats.instances_uploaded += 1;
            stats.instance_bytes_uploaded += size_of::<ImageUniform>();
        };
        if partial {
            for &index in &prepared.dirty {
                update(index);
            }
        } else {
            for &index in prepared.objects.keys() {
                update(index);
            }
        }
        for handle in retired {
            if self
                .textures
                .get(&handle)
                .is_some_and(|texture| texture.references == 0)
            {
                let texture = self.textures.remove(&handle).expect("retirement checked");
                self.pixel_bytes -= texture.pixel_bytes;
                stats.textures_retired += 1;
            }
        }
        self.generation = Some(prepared.generation);
        stats
    }

    fn remove_object(&mut self, index: usize, retired: &mut Vec<RasterImageResourceHandle>) {
        if let Some(row) = self.objects.remove(&index) {
            let texture = self
                .textures
                .get_mut(&row.resource)
                .expect("resident image texture");
            texture.references -= 1;
            if texture.references == 0 {
                retired.push(row.resource);
            }
        }
    }

    /// Returns false when the painter stream carries an absent image whose slot was
    /// retired. Keeping that item resident avoids rebuilding the mixed scratch stream
    /// for visibility churn; no draw is encoded for the absent slot.
    pub fn draw_if_resident<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        camera: &'a wgpu::BindGroup,
        index: usize,
        sample_count: u32,
    ) -> Result<bool, RasterImageDrawError> {
        let Some(row) = self.objects.get(&index) else {
            return Ok(false);
        };
        let texture = self
            .textures
            .get(&row.resource)
            .ok_or(RasterImageDrawError::MissingObject(index))?;
        pass.set_pipeline(if sample_count == 1 {
            &self.pipeline
        } else {
            &self.pipeline_msaa
        });
        pass.set_bind_group(0, camera, &[]);
        pass.set_bind_group(1, &texture.binding, &[]);
        pass.set_bind_group(2, &row.binding, &[]);
        pass.draw(0..6, 0..1);
        Ok(true)
    }
}

fn uniform_layout(
    device: &wgpu::Device,
    label: &str,
    visibility: wgpu::ShaderStages,
    bytes: usize,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(bytes as u64),
            },
            count: None,
        }],
    })
}
fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
    count: u32,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Noon retained image pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_image"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_image"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState {
            count,
            ..Default::default()
        },
        multiview_mask: None,
        cache: None,
    })
}

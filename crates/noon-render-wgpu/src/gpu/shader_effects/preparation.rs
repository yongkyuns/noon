//! Owned device work, so a platform may await compilation without borrowing a
//! scene or renderer. Publication installation remains a separate checked step.
use noon_core::WgslEffectProgram;

use super::*;

#[derive(Debug)]
pub(super) struct RowRequest {
    pub object: ObjectId,
    pub programs: Vec<(WgslEffectProgram, Option<GpuLease<PreparedShaderProgram>>)>,
}

#[derive(Debug)]
pub struct EffectProgramPreparation {
    pub(super) authority: Arc<()>,
    pub(super) device: wgpu::Device,
    pub(super) context: PublicationContext,
    pub(super) rows: Vec<(usize, Option<RowRequest>)>,
    pub(super) stats: EffectProgramStats,
    pub(super) missing: bool,
}

#[derive(Debug)]
pub struct PreparedEffectPrograms {
    pub(super) authority: Arc<()>,
    pub(super) device: wgpu::Device,
    pub(super) context: PublicationContext,
    pub(super) rows: Vec<(usize, Option<ResidentRow>)>,
    pub(super) stats: EffectProgramStats,
}

impl PreparedEffectPrograms {
    pub fn stats(&self) -> EffectProgramStats {
        self.stats
    }
}

impl EffectProgramPreparation {
    /// Ready/retirement-only work never creates an asynchronous host task.
    pub fn try_ready(self) -> Result<PreparedEffectPrograms, Self> {
        if self.missing {
            return Err(self);
        }
        Ok(PreparedEffectPrograms {
            authority: self.authority,
            device: self.device,
            context: self.context,
            stats: self.stats,
            rows: self
                .rows
                .into_iter()
                .map(|(index, row)| {
                    (
                        index,
                        row.map(|row| ResidentRow {
                            object: row.object,
                            programs: row
                                .programs
                                .into_iter()
                                .map(|(_, program)| program.expect("ready program"))
                                .collect(),
                            path_bounds: None,
                        }),
                    )
                })
                .collect(),
        })
    }

    /// Compile only missing immutable modules. Earlier successful leases remain
    /// local until the complete batch succeeds; no live renderer state changes.
    pub async fn prepare(self) -> Result<PreparedEffectPrograms, EffectProgramPrepareError> {
        self.prepare_with(super::super::sample_filter::SampleFilter::default())
            .await
    }
    async fn prepare_with(
        mut self,
        mut filter: super::super::sample_filter::SampleFilter,
    ) -> Result<PreparedEffectPrograms, EffectProgramPrepareError> {
        for (_, row) in &mut self.rows {
            if let Some(row) = row {
                for (source, program) in &mut row.programs {
                    if program.is_none() {
                        let (prepared, work) = filter
                            .prepare_shader_program(&self.device, source)
                            .await
                            .map_err(EffectProgramPrepareError::Gpu)?;
                        self.stats.pipeline_compiles += work.pipeline_compiles;
                        *program = Some(prepared);
                    }
                }
            }
        }
        self.missing = false;
        Ok(self
            .try_ready()
            .expect("every requested program was prepared"))
    }
}

#[cfg(all(test, feature = "wgsl-effects"))]
mod tests {
    use super::*;
    use noon_compile::{CompiledEffect, CompiledEffectStack, CompiledObject, CompiledScene};
    use noon_core::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    #[ignore = "requires a real adapter; second shader pipeline fails after the first succeeds"]
    fn failed_second_pipeline_installs_no_partial_batch_and_keeps_source_retryable() {
        static COMPILES: AtomicUsize = AtomicUsize::new(0);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
        let (device, queue) =
            pollster::block_on(adapter.request_device(&Default::default())).unwrap();
        let mut renderer = GpuRenderer::new(&device, &queue, wgpu::TextureFormat::Rgba8Unorm);
        let objects = (0..2).map(|i| {
            let program = WgslEffectProgram::prepare(ShaderSchema {
                profile: ShaderProfile::PaintedPointwiseV1,
                dependencies: ShaderDependencies { coordinates: ShaderCoordinates::Scene, authored_time: false, seed: false },
                parameters: Box::new([]),
            }, format!("fn shade(input: NoonShaderInput, p: NoonShaderParameters) -> vec4<f32> {{ return input.color; }}\n// program {i}")).unwrap();
            let mut row = CompiledObject::new(ObjectId::new(i+1), GeometryRef::circle(0.5), Transform2D::IDENTITY, Style::default());
            row.effects = Some(Arc::new(CompiledEffectStack::new([CompiledEffect { attachment: SemanticNodeId::new(i as u32+10, 1), definition: ShaderEffect::new(program).into() }]).unwrap()));
            row
        }).collect::<Vec<_>>();
        let runtime =
            noon_runtime::SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
        let publication = runtime.resource_publication();
        let context = publication.context();
        let changes = publication.changes().clone();
        let request = renderer
            .effect_program_preparation(&device, &publication)
            .unwrap();
        let mut filter = crate::gpu::sample_filter::SampleFilter::default();
        filter.test_shader_source = Some(|source| {
            COMPILES.fetch_add(1, Ordering::Relaxed);
            if source.contains("// program 1") {
                format!("{source}\ninvalid generated WGSL")
            } else {
                source
            }
        });
        assert!(matches!(
            pollster::block_on(request.prepare_with(filter)),
            Err(EffectProgramPrepareError::Gpu(_))
        ));
        assert_eq!(
            COMPILES.load(Ordering::Relaxed),
            2,
            "first pipeline succeeded before the actual second compilation failed"
        );
        assert_eq!(renderer.resident_effect_program_counts(), [0, 0]);
        assert_eq!(runtime.publication_context(), context);
        assert_eq!(runtime.resource_publication().changes(), &changes);
        let work = pollster::block_on(
            renderer.prepare_effect_programs(&device, &runtime.resource_publication()),
        )
        .unwrap();
        assert_eq!(work.pipeline_compiles, 2);
        assert_eq!(renderer.resident_effect_program_counts(), [2, 2]);
    }
}

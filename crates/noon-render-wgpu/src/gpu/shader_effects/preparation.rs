//! Owned device work, so a platform may await compilation without borrowing a
//! scene or renderer. Publication installation remains a separate checked step.
use noon_core::WgslEffectProgram;

use super::*;

#[derive(Debug)]
pub(super) struct RowRequest {
    pub object: ObjectId,
    pub programs: Vec<(WgslEffectProgram, Option<Arc<PreparedShaderProgram>>)>,
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
    pub async fn prepare(mut self) -> Result<PreparedEffectPrograms, EffectProgramPrepareError> {
        let mut filter = super::super::sample_filter::SampleFilter::default();
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

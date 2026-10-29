//! Immutable source sharing at the worker boundary. Transport keys come from the
//! existing session/sequence and first attachment; no semantic IDs are allocated.
use super::*;
use crate::{TransportEffectDefinition, TransportShaderProgramHandle};
use noon_core::WgslEffectProgram;

#[derive(Clone, Debug)]
struct PublishedProgram {
    handle: TransportShaderProgramHandle,
    program: WgslEffectProgram,
}
#[derive(Clone, Debug, Default)]
pub(super) struct ShaderProgramInventory {
    programs: HashMap<usize, PublishedProgram>,
    keys: HashMap<TransportShaderProgramHandle, usize>,
}
#[derive(Default)]
pub(super) struct StagedPrograms {
    programs: Vec<PublishedProgram>,
    indices: HashMap<usize, usize>,
}
fn identity(program: &WgslEffectProgram) -> usize {
    // The entry pins this immutable Arc, so its allocation cannot be reused
    // while the derived key remains live. Addresses never cross the wire.
    std::ptr::from_ref(program.schema()) as usize
}

impl RetainedFamilyExecutionDeltaEncoder {
    #[cfg(target_arch = "wasm32")]
    pub(crate) fn shader_program_handle(
        &self,
        program: &WgslEffectProgram,
    ) -> Option<TransportShaderProgramHandle> {
        self.shader_programs
            .as_ref()?
            .programs
            .get(&identity(program))
            .map(|p| p.handle)
    }

    pub(super) fn stage_shader_programs(
        &self,
        envelope: &mut RetainedFamilyExecutionDeltaEnvelope,
    ) -> Result<StagedPrograms, RetainedResourceTransportError> {
        let mut staged = StagedPrograms::default();
        for row in &mut envelope.retained.objects {
            for effect in &mut row.effects {
                let TransportEffectDefinition::Shader {
                    program: handle, ..
                } = &mut effect.definition
                else {
                    continue;
                };
                let program = self
                    .retained
                    .published_shader_program(row.slot, effect.attachment)
                    .ok_or_else(|| RetainedResourceTransportError::InvalidShaderProgram {
                        handle: *handle,
                        reason: "published source lease is unavailable".into(),
                    })?;
                let key = identity(program);
                if let Some(previous) = self
                    .shader_programs
                    .as_ref()
                    .and_then(|i| i.programs.get(&key))
                {
                    *handle = previous.handle;
                } else if let Some(&index) = staged.indices.get(&key) {
                    *handle = staged.programs[index].handle;
                } else {
                    staged.indices.insert(key, staged.programs.len());
                    staged.programs.push(PublishedProgram {
                        handle: *handle,
                        program: program.clone(),
                    });
                }
            }
        }
        Ok(staged)
    }
    pub(super) fn commit_shader_programs(&mut self, staged: StagedPrograms) {
        if staged.programs.is_empty() {
            return;
        }
        let inventory = self.shader_programs.get_or_insert_with(Default::default);
        for entry in staged.programs {
            let key = identity(&entry.program);
            inventory.keys.insert(entry.handle, key);
            inventory.programs.insert(key, entry);
        }
    }
    pub(super) fn retire_shader_programs(&mut self, retired: &[TransportShaderProgramHandle]) {
        if let Some(inventory) = &mut self.shader_programs {
            for handle in retired {
                if let Some(key) = inventory.keys.remove(handle) {
                    inventory.programs.remove(&key);
                }
            }
        }
    }
}
impl StagedPrograms {
    pub(super) fn has_additions(&self, inventory: &RetainedResourceInventory) -> bool {
        self.programs
            .iter()
            .any(|p| !inventory.contains_shader_program(p.handle))
    }
    pub(super) fn capture(
        &self,
        bundle: RetainedResourceBundle,
        inventory: &RetainedResourceInventory,
    ) -> Result<RetainedResourceBundle, RetainedResourceTransportError> {
        bundle.with_shader_programs(
            self.programs
                .iter()
                .filter(|p| !inventory.contains_shader_program(p.handle))
                .map(|p| (p.handle, &p.program)),
        )
    }
}

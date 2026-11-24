//! Derived source leases and checked program lookups for the worker codec.
use super::*;
use crate::retained_resource_transport::shader_programs::ShaderPrograms;
use crate::TransportShaderProgramHandle;
use noon_core::{ShaderValue, WgslEffectProgram};

pub(super) type PublishedShaderSources =
    HashMap<TransportSlotId, Vec<(SemanticNodeId, WgslEffectProgram)>>;

pub(super) fn valid_wire_effect(effect: &TransportEffectState) -> bool {
    match &effect.definition {
        TransportEffectDefinition::Shader { values, .. } => {
            values.len() <= 16
                && values.iter().all(|v| match v {
                    ShaderValue::Scalar(v) => v.is_finite() && (*v as f32).is_finite(),
                    ShaderValue::Vector2(v) => {
                        v.iter().all(|v| v.is_finite() && (*v as f32).is_finite())
                    }
                    ShaderValue::Color(c) => [c.red, c.green, c.blue, c.alpha]
                        .iter()
                        .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                    ShaderValue::Boolean(_) | ShaderValue::Unsigned(_) => true,
                })
        }
        _ => effect.to_compiled().is_some(),
    }
}

impl RetainedExecutionDeltaEncoder {
    pub(super) fn commit_shader_sources(
        &mut self,
        frame: &FrameState,
        objects: &[RetainedTransportObjectState],
        removed: &HashSet<usize>,
        snapshot: bool,
    ) {
        if snapshot {
            self.published_shader_sources = None;
        }
        if let Some(sources) = &mut self.published_shader_sources {
            for index in removed {
                if let Ok(slot) = u32::try_from(*index) {
                    sources.remove(&TransportSlotId {
                        slot,
                        generation: 0,
                    });
                }
            }
        }
        for object in objects {
            let row = &frame.objects[object.slot.slot as usize];
            let shaders = || {
                row.effects
                    .as_ref()
                    .into_iter()
                    .flat_map(|stack| stack.attachments().iter().zip(stack.definitions()))
                    .filter_map(|(&attachment, d)| d.as_shader().map(|s| (attachment, s.program())))
            };
            let count = shaders().count();
            if count == 0 {
                if let Some(sources) = &mut self.published_shader_sources {
                    sources.remove(&object.slot);
                }
                continue;
            }
            if self
                .published_shader_sources
                .as_ref()
                .and_then(|sources| sources.get(&object.slot))
                .is_some_and(|old| {
                    old.len() == count
                        && old.iter().zip(shaders()).all(
                            |((attachment, program), (current, source))| {
                                *attachment == current
                                    && std::ptr::eq(program.schema(), source.schema())
                            },
                        )
                })
            {
                continue;
            }
            self.published_shader_sources
                .get_or_insert_with(HashMap::new)
                .insert(
                    object.slot,
                    shaders().map(|(id, p)| (id, p.clone())).collect(),
                );
        }
    }
    pub(crate) fn published_shader_program(
        &self,
        slot: TransportSlotId,
        attachment: SemanticNodeId,
    ) -> Option<&WgslEffectProgram> {
        self.published_shader_sources
            .as_ref()?
            .get(&slot)?
            .iter()
            .find(|(id, _)| *id == attachment)
            .map(|(_, p)| p)
    }
}

impl RetainedExecutionFrameMirror {
    pub(crate) fn extend_installed_shader_programs(&mut self, additions: &ShaderPrograms) {
        self.shader_programs
            .extend(additions.iter().map(|(&h, p)| (h, p.clone())));
    }
    pub(crate) fn remove_installed_shader_programs<'a>(
        &mut self,
        handles: impl IntoIterator<Item = &'a TransportShaderProgramHandle>,
    ) {
        for handle in handles {
            self.shader_programs.remove(handle);
        }
    }
}

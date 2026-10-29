//! Checked immutable programs at the genuine worker resource boundary. An origin
//! is the first publishing attachment, not a live semantic lookup or allocator.
use super::*;
use noon_core::{SemanticNodeId, ShaderSchema, WgslEffectProgram};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransportShaderProgramHandle {
    pub session: u32,
    pub origin: SemanticNodeId,
    pub version: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TransportShaderProgramEntry {
    pub(super) handle: TransportShaderProgramHandle,
    schema: ShaderSchema,
    source: Arc<str>,
}

pub(crate) type ShaderPrograms = HashMap<TransportShaderProgramHandle, WgslEffectProgram>;

pub(super) fn install_shader_programs(
    entries: Vec<TransportShaderProgramEntry>,
) -> Result<ShaderPrograms, RetainedResourceTransportError> {
    #[cfg(not(feature = "wgsl-effects"))]
    if let Some(entry) = entries.first() {
        return Err(RetainedResourceTransportError::InvalidShaderProgram {
            handle: entry.handle,
            reason: "WGSL effects are disabled in this build".into(),
        });
    }
    #[cfg(not(feature = "wgsl-effects"))]
    let installed = HashMap::new();
    #[cfg(feature = "wgsl-effects")]
    let mut installed = HashMap::with_capacity(entries.len());
    #[cfg(feature = "wgsl-effects")]
    for entry in entries {
        let invalid = |reason: String| RetainedResourceTransportError::InvalidShaderProgram {
            handle: entry.handle,
            reason,
        };
        if installed.contains_key(&entry.handle) {
            return Err(invalid("duplicate program handle".into()));
        }
        let program = WgslEffectProgram::prepare(entry.schema, entry.source)
            .map_err(|e| invalid(e.to_string()))?;
        installed.insert(entry.handle, program);
    }
    Ok(installed)
}

impl RetainedResourceBundle {
    /// Capture source/schema once per immutable program. Frame rows carry only
    /// this handle, typed parameter samples and seed.
    pub fn with_shader_programs<'a>(
        mut self,
        programs: impl IntoIterator<Item = (TransportShaderProgramHandle, &'a WgslEffectProgram)>,
    ) -> Result<Self, RetainedResourceTransportError> {
        let mut seen = self
            .shader_programs
            .iter()
            .map(|p| p.handle)
            .collect::<HashSet<_>>();
        for (handle, program) in programs {
            if !seen.insert(handle) {
                return Err(RetainedResourceTransportError::InvalidShaderProgram {
                    handle,
                    reason: "duplicate program handle".into(),
                });
            }
            self.shader_programs.push(TransportShaderProgramEntry {
                handle,
                schema: program.schema().clone(),
                source: Arc::from(program.source()),
            });
        }
        Ok(self)
    }
    pub fn shader_program_count(&self) -> usize {
        self.shader_programs.len()
    }
}

impl InstalledRetainedResources {
    pub(crate) fn shader_program_remap(&self) -> ShaderPrograms {
        self.shader_programs.clone()
    }
    pub fn shader_program_count(&self) -> usize {
        self.shader_programs.len()
    }
    pub fn shader_program(
        &self,
        handle: TransportShaderProgramHandle,
    ) -> Option<&WgslEffectProgram> {
        self.shader_programs.get(&handle)
    }
}

impl PreparedRetainedResourceAdditions {
    pub(crate) fn shader_program_remap(&self) -> ShaderPrograms {
        self.installed.shader_program_remap()
    }
}

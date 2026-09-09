"""One-shot source edit; not shipped in product PRs."""
from pathlib import Path
import re
R=Path('crates/noon/src')
p=R/'execution_session.rs';s=p.read_text()
assert 'InvalidComposition(String)' in s
s=s.replace('use crate::execution_segment::{','use crate::CompositionAdmissionError;\nuse crate::execution_segment::{',1)
s=s.replace('InvalidComposition(String)','InvalidComposition(CompositionAdmissionError)').replace('Self::InvalidComposition(error) => formatter.write_str(error),','Self::InvalidComposition(error) => error.fmt(formatter),')
for a,b,n in [
 ('ExecutionSessionAnimationError::InvalidComposition(error.to_string())','ExecutionSessionAnimationError::InvalidComposition(error.into())',5),
 ('ExecutionSessionAnimationError::InvalidComposition(\n                                    error.to_string(),\n                                )','ExecutionSessionAnimationError::InvalidComposition(CompositionAdmissionError::TextMembers(error))',1),
]:
 assert s.count(a)==n,(a,s.count(a));s=s.replace(a,b)
reasons=[
 ('subset display supports monotone rate functions and retained membership without lag, path arcs, or reversal','SubsetDisplayOptions',None),
 ('subset display requires at least one direct family member','EmptySubsetDisplay','family: *target'),
 ('family fade lifecycle is fixed by its direction and does not support path arcs','FamilyFadeOptions',None),
 ('TextReveal introducer must match Create/Uncreate direction','TextRevealIntroducer',None),
 ('this direct convenience accepts only transform, rotate, wait, and nested composition leaves','UnsupportedDirectLeaf',None),
 ('family glyph animation target does not exist','MissingFamilyGlyphTarget','target'),
 ('family Reveal introducer must match Create/Uncreate direction','FamilyRevealIntroducer',None),
 ('family Create target must be detached','FamilyCreateAlreadyPresent','target'),
 ('family glyph animation lost a Text resource','MissingTextResource','target: *leaf, handle'),
 ('family glyph animation supports plain Text and Reveal geometry','UnsupportedFamilyGlyphSource','target: *leaf, kind: resource.kind'),
 ('family glyph member count exceeds u32','GlyphCountExhausted','family: target'),
 ('family Write supports only plain Text leaves','UnsupportedFamilyWriteContent','target: *leaf'),
 ('family TextWrite requires at least one visible glyph','EmptyFamilyTextWrite','family: target'),
 ('family fade target must be a semantic family','FamilyFadeTargetKind','target'),
 ('family fade requires at least one ordinary leaf','EmptyFamilyFade','family: target'),
 ('Indicate scale and color must be finite and representable','IndicateValues','scale_factor: indication.scale_factor, color'),
 ('restoring Indicate supports there-and-back timing without path or lifecycle overrides','IndicateOptions',None),
 ('Indicate requires an object already present in the execution domain','IndicateTargetNotPresent','target'),
]
for message,name,fields in reasons:
 pattern=re.escape('"'+message+'"')+r'\s*\.into\(\)'
 assert len(re.findall(pattern,s))==(2 if name=='GlyphCountExhausted' else 1),name
 s=re.sub(pattern,'CompositionAdmissionError::'+name+(' { '+fields+' }' if fields else ''),s)
for a,b in [
 ('''store.semantic_object_state_checked(leaf).map_err(|_| {
                            ExecutionSessionAnimationError::InvalidComposition(
                                "subset display supports direct object members, not nested families".into(),
                            )
                        })?''','''store.semantic_object_state_checked(leaf).map_err(|error| {
                            ExecutionSessionAnimationError::InvalidComposition(
                                CompositionAdmissionError::SubsetDisplayMember { member: leaf, error },
                            )
                        })?'''),
 ('''store.semantic_object_state_checked(*leaf).map_err(|_| {
                ExecutionSessionAnimationError::InvalidComposition(
                    "family glyph animation requires ordinary leaves".into(),
                )
            })?''','''store.semantic_object_state_checked(*leaf).map_err(|error| {
                ExecutionSessionAnimationError::InvalidComposition(
                    CompositionAdmissionError::FamilyGlyphMember { member: *leaf, error },
                )
            })?'''),
 ('''ExecutionSessionAnimationError::InvalidComposition(format!(
                "family fade target {target:?} does not exist"
            ))''','''ExecutionSessionAnimationError::InvalidComposition(
                CompositionAdmissionError::MissingFamilyFadeTarget { target },
            )'''),
]:
 assert s.count(a)==1,a;s=s.replace(a,b)
p.write_text(s)
p=R/'focus_on_authoring.rs';s=p.read_text()
s=s.replace('use noon_core::{','use crate::CompositionAdmissionError;\nuse noon_core::{',1)
s=s.replace('Result<(SemanticLocalNodeToken, SemanticLocalNodeToken), String>','Result<(SemanticLocalNodeToken, SemanticLocalNodeToken), CompositionAdmissionError>')
s=s.replace('"FocusOn requires a finite 2D point/color and opacity in [0, 1]".into()','CompositionAdmissionError::FocusOnValues { point: self.point, opacity: self.opacity, color: self.color }')
s=re.sub(r'"FocusOn has fixed transient membership without lag, path arcs or rate reversal"\s*\.into\(\)','CompositionAdmissionError::FocusOnOptions',s);p.write_text(s)
p=R/'animation_authoring.rs';s=p.read_text()
s=s.replace('use crate::Mobject;','use crate::{CompositionAdmissionError, Mobject};',1)
s=s.replace('let options = normalized_passing_flash_options(options)?;','let options = normalized_passing_flash_options(options).map_err(|error| error.to_string())?;',1)
s=s.replace('''pub(crate) fn normalized_passing_flash_options(
    options: AnimationOptions,
) -> Result<AnimationOptions, String>''','''pub(crate) fn normalized_passing_flash_options(
    options: AnimationOptions,
) -> Result<AnimationOptions, CompositionAdmissionError>''')
s=re.sub(r'"PassingFlash has fixed transient membership and does not support lag, path arcs, or rate reversal"\s*\.into\(\)','CompositionAdmissionError::PassingFlashOptions',s);p.write_text(s)
forward={
 'execution_session.rs': {
  'ExecutionSessionInputError':['NativeInput(error)','Reactive(error)','Evaluation(error)'],
  'ExecutionSessionAnimationError':['Schedule(error)','Segment(error)','Payload(error)','TextGlyph(error)','PreparedAnimation(error)','PreparedSchedule(error)','PreparedScalarAnimation(error)','PreparedScalarTimeline(error)','ScalarTimeline(error)','ScalarQuery(error)','ReactiveEnrollment(error)','TargetState { error, .. }','FadeTarget { error, .. }','CreateTarget { error, .. }','InvalidComposition(error)','PreparedTrack(error)','Publication(error)','AuthoredPublication(error)'],
 },
 'execution_segment.rs': {'ExecutionSegmentAdvanceError':['Evaluation(error)','Callback(error)']},
 'execution_session/completion.rs': {'ExecutionSegmentCompletionError':['PreparedScalarTimeline(error)','ScalarTimeline(error)','Publication(error)']},
 'execution_session/callback.rs': {'ExecutionSessionCallbackError':['Read(error)','Evaluation(error)','InvalidEffectiveWrite(error)','Commit(error)']},
}
for relative,types in forward.items():
 p=R/relative;s=p.read_text()
 for name,arms in types.items():
  old=f'impl std::error::Error for {name} {{}}';assert s.count(old)==1,(relative,name)
  body=''.join(f'            Self::{arm} => Some(error),\n' for arm in arms)
  s=s.replace(old,f"impl std::error::Error for {name} {{\n    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {{\n        match self {{\n"+body+'            _ => None,\n        }\n    }\n}')
 p.write_text(s)
p=R/'lib.rs';s=p.read_text()
s=s.replace('mod compact_value_authoring;','mod compact_value_authoring;\nmod composition_admission_error;',1)
s=s.replace('pub use animation_authoring::DeclaredAnimation;','pub use animation_authoring::DeclaredAnimation;\npub use composition_admission_error::CompositionAdmissionError;',1);p.write_text(s)
text='''//! Reasons rejected by the existing composition-admission preflight.
//!
//! These variants describe the same checks that precede coherent publication.
//! They introduce no validation policy and hold actual domain causes, not messages.
use noon_core::{Color, SemanticNodeId, TextResourceHandle, TextSourceKind};

/// A typed composition-admission failure carried by
/// [`crate::ExecutionSessionAnimationError::InvalidComposition`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum CompositionAdmissionError {
    /// Shared family pairing failed before staging any animation identity.
    FamilyPairing(noon_core::SemanticFamilyPairingError),
    /// Shared membership/object validation failed.
    Semantic(noon_core::SemanticSceneOperationError),
    /// Family traversal rejected the supplied semantic identity.
    Store(noon_core::SemanticStoreError),
    /// Plain-text member derivation rejected an immutable text resource.
    TextMembers(noon_core::TextAnimationMemberError),
    /// Focus point/color is not representable or opacity is outside [0, 1].
    FocusOnValues { point: (f64, f64), opacity: f64, color: Color },
    /// FocusOn has fixed transient lifecycle and timing constraints.
    FocusOnOptions,
    /// PassingFlash has fixed transient lifecycle and timing constraints.
    PassingFlashOptions,
    /// Subset display requires monotone timing and retained membership.
    SubsetDisplayOptions,
    /// Subset display requires a nonempty list of direct members.
    EmptySubsetDisplay { family: SemanticNodeId },
    /// A direct subset member is not a valid semantic object.
    SubsetDisplayMember { member: SemanticNodeId, error: noon_core::SemanticSceneOperationError },
    /// A family fade's lifecycle is fixed by its direction.
    FamilyFadeOptions,
    /// TextReveal introduction conflicts with its direction.
    TextRevealIntroducer,
    /// A direct transform convenience does not support this composition leaf.
    UnsupportedDirectLeaf,
    /// The requested family glyph target has been removed or never existed.
    MissingFamilyGlyphTarget { target: SemanticNodeId },
    /// A family Reveal's introduction conflicts with its direction.
    FamilyRevealIntroducer,
    /// Family Create requires a detached target, not an already-live one.
    FamilyCreateAlreadyPresent { target: SemanticNodeId },
    /// An ordinary family glyph leaf failed shared object validation.
    FamilyGlyphMember { member: SemanticNodeId, error: noon_core::SemanticSceneOperationError },
    /// A glyph leaf references an unavailable immutable text resource.
    MissingTextResource { target: SemanticNodeId, handle: TextResourceHandle },
    /// The immutable text source is not plain Text.
    UnsupportedFamilyGlyphSource { target: SemanticNodeId, kind: TextSourceKind },
    /// A family's total glyph-member count exceeds the supported index range.
    GlyphCountExhausted { family: SemanticNodeId },
    /// Family Write does not support a non-text leaf.
    UnsupportedFamilyWriteContent { target: SemanticNodeId },
    /// Family TextWrite needs at least one visible glyph.
    EmptyFamilyTextWrite { family: SemanticNodeId },
    /// The requested family fade target has been removed or never existed.
    MissingFamilyFadeTarget { target: SemanticNodeId },
    /// The target identity exists but is not a semantic family.
    FamilyFadeTargetKind { target: SemanticNodeId },
    /// Family fade needs at least one ordinary leaf.
    EmptyFamilyFade { family: SemanticNodeId },
    /// Indicate received non-finite or unrepresentable scale/color values.
    IndicateValues { scale_factor: f64, color: Color },
    /// Restoring Indicate has fixed there-and-back timing and lifecycle.
    IndicateOptions,
    /// Indicate requires a target already in the execution domain.
    IndicateTargetNotPresent { target: SemanticNodeId },
}

impl std::fmt::Display for CompositionAdmissionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FamilyPairing(error) => error.fmt(formatter),
            Self::Semantic(error) => error.fmt(formatter),
            Self::Store(error) => error.fmt(formatter),
            Self::TextMembers(error) => error.fmt(formatter),
            Self::FocusOnValues { .. } => formatter.write_str("FocusOn requires a finite 2D point/color and opacity in [0, 1]"),
            Self::FocusOnOptions => formatter.write_str("FocusOn has fixed transient membership without lag, path arcs or rate reversal"),
            Self::PassingFlashOptions => formatter.write_str("PassingFlash has fixed transient membership and does not support lag, path arcs, or rate reversal"),
            Self::SubsetDisplayMember { .. } => formatter.write_str("subset display supports direct object members, not nested families"),
            Self::FamilyGlyphMember { .. } => formatter.write_str("family glyph animation requires ordinary leaves"),
            Self::MissingFamilyFadeTarget { target } => write!(formatter, "family fade target {target:?} does not exist"),
'''
for message,name,fields in reasons:
 text+=f'            Self::{name}'+(' { .. }' if fields else '')+f' => formatter.write_str("{message}"),\n'
text+='''        }
    }
}
impl std::error::Error for CompositionAdmissionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::FamilyPairing(error) => Some(error),
            Self::Semantic(error) => Some(error),
            Self::Store(error) => Some(error),
            Self::TextMembers(error) => Some(error),
            Self::SubsetDisplayMember { error, .. } | Self::FamilyGlyphMember { error, .. } => Some(error),
            _ => None,
        }
    }
}
impl From<noon_core::SemanticFamilyPairingError> for CompositionAdmissionError {
    fn from(error: noon_core::SemanticFamilyPairingError) -> Self { Self::FamilyPairing(error) }
}
impl From<noon_core::SemanticSceneOperationError> for CompositionAdmissionError {
    fn from(error: noon_core::SemanticSceneOperationError) -> Self { Self::Semantic(error) }
}
impl From<noon_core::SemanticStoreError> for CompositionAdmissionError {
    fn from(error: noon_core::SemanticStoreError) -> Self { Self::Store(error) }
}
'''
p=R/'composition_admission_error.rs';assert not p.exists();p.write_text(text)

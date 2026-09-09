from pathlib import Path
import re
import subprocess

root = Path.cwd()
lib = root / 'crates/noon/src/lib.rs'
assert 'pub use noon_core::*;' in lib.read_text(), 'expected pre-R2 facade'
ordinary_core = set('''AnimationOptions Bounds2D64 Color GeometryRef PathCommand PublicationContext
RateFunction Rect SceneRevision ExecutionRevision FrameEpoch SemanticAnimationCompositionKind
SemanticFadeDirection SemanticNodeId SemanticObjectProperty SemanticObjectState SemanticPaint
SemanticSignalValue SemanticStyle SemanticTransform2_5D SemanticTransformInterpolation SemanticVec3
StoredGeometry StrokeCap StrokeJoin StrokeWidthMode Style Transform2D Vec2 VectorPath
ORIGIN UP DOWN LEFT RIGHT UL UR DL DR PI TAU DEGREES SMALL_BUFF MED_SMALL_BUFF
MED_LARGE_BUFF LARGE_BUFF DEFAULT_MOBJECT_TO_EDGE_BUFFER DEFAULT_MOBJECT_TO_MOBJECT_BUFFER
DEFAULT_FRAME_HEIGHT DEFAULT_FRAME_WIDTH WHITE BLACK BLUE BLUE_A BLUE_B BLUE_C BLUE_D BLUE_E
TEAL TEAL_A TEAL_B TEAL_C TEAL_D TEAL_E GREEN GREEN_A GREEN_B GREEN_C GREEN_D GREEN_E YELLOW
YELLOW_A YELLOW_B YELLOW_C YELLOW_D YELLOW_E GOLD RED RED_A RED_B RED_C RED_D RED_E MAROON
PURPLE PURPLE_A PURPLE_B PURPLE_C PURPLE_D PURPLE_E ORANGE PINK LIGHT_PINK GRAY GREY'''.split())
core_names = set()
for p in (root / 'crates/noon-core/src').rglob('*.rs'):
    core_names.update(re.findall(r'(?m)^pub (?:const (?:fn )?|(?:async )?fn |struct |enum |trait |type |mod )([A-Za-z_]\w*)', p.read_text()))
core_names.update('ObjectId GeometryId TrackId SignalId'.split())
segment_integration = 'ExecutionSegmentSequence ExecutionSegmentToken'.split()
callback_integration = '''CallbackRendererDirtyClassification CommittedCallbackRendererObservation
CallbackRendererObservationOutcome RequiredCallbackInvocation CallbackAdvance CallbackReadRequest
CallbackReadValue CallbackTerminationKind CallbackTermination CallbackSequence CallbackPhaseToken
EffectiveSemanticPropertyWrite EffectivePropertyBatch CallbackPhaseOverlay'''.split()
session_integration = ['ExecutionViewportQuery','StructuralPublicationStats','EffectiveSemanticObject'] + callback_integration
runtime_integration = '''EffectiveObjectProperties FrameChanges FrameObjectState FrameState
RendererPublication RuntimeIdentity RuntimeWakeState TimelineWakeState'''.split()
host_integration = '''rotate_effective_transform_about_point effective_style_with_color effective_style_with_fill_color
 effective_style_with_fill_opacity effective_style_with_fill effective_style_with_stroke_color'''.split()
text_integration = ['RetainedScene','RetainedMobject','NATIVE_POINT_TO_SCENE_SCALE','SCALE_FACTOR_PER_FONT_POINT']
mobject_integration = ['line_match_transform','authoring_render_f64','authoring_xy_f64']
moved_module = {}
for module, names in [('execution_segment',segment_integration),('execution_session',session_integration),
                      ('host_callbacks',host_integration),('text_authoring',text_integration),
                      ('semantic_mobject',mobject_integration),('compact_value_authoring',['semantic_object_state_from_compact'])]:
    moved_module.update({n:module for n in names})

def segments(s):
    level = 0
    begin = 0
    for i,c in enumerate(s):
        if c == '{': level += 1
        if c == '}': level -= 1
        if c == ',' and level == 0:
            if s[begin:i].strip(): yield s[begin:i].strip()
            begin = i+1
    if s[begin:].strip(): yield s[begin:].strip()

fixture_core = set()
def rewrite_imports(text, p):
    internal = p.as_posix().startswith('crates/noon/src/')
    fixture = p.as_posix().startswith('fixtures/')
    prefix = 'crate' if internal else 'noon'
    def destination(name):
        if name in moved_module:
            return f'crate::{moved_module[name]}' if internal else 'noon::integration'
        if name in runtime_integration:
            return 'noon_runtime' if internal else 'noon::integration'
        if name in core_names - ordinary_core:
            if fixture:
                fixture_core.add(name)
                return 'noon::integration'
            return 'noon_core'
        return prefix
    pattern = re.compile(r'(?P<visibility>pub(?:\([^)]*\))?\s+)?use\s+'+prefix+r'::(?P<body>[^;]+);')
    def change(match):
        body = match['body'].strip()
        items = list(segments(body[1:-1])) if body.startswith('{') and body.endswith('}') else [body]
        groups = {}
        for item in items:
            if not internal and item.startswith('semantic_mobject::'):
                tail=item[len('semantic_mobject::'):]
                subitems=list(segments(tail[1:-1])) if tail.startswith('{') else [tail]
                for sub in subitems:
                    name=re.match(r'\w+',sub).group()
                    groups.setdefault(destination(name),[]).append(sub)
                continue
            name = re.match(r'\w+',item)
            dest = destination(name.group()) if name else prefix
            groups.setdefault(dest,[]).append(item)
        if len(groups)==1 and prefix in groups and groups[prefix]==items:
            return match[0]
        vis=match['visibility'] or ''
        return '\n'.join(vis+'use '+dest+'::{'+', '.join(items)+'};' for dest,items in groups.items())
    text=pattern.sub(change,text)
    def qualified(match):
        name = match[1]
        return destination(name)+'::'+name
    text = re.sub(r'\b'+prefix+r'::([A-Za-z_]\w*)', qualified, text)
    if not internal:
        for name in ['ManimNextToArgs','Mobject','ManimBecomeOptions','ManimGeometryOptions','ManimLineEndpoints']:
            text=text.replace('noon::semantic_mobject::'+name,'noon::'+name)
        for name in mobject_integration:
            text=text.replace('noon::semantic_mobject::'+name,'noon::integration::'+name)
    return text

for p in [Path(x) for x in subprocess.check_output(['git','ls-files','*.rs'],text=True).splitlines()]:
    if p == Path('crates/noon/src/lib.rs'): continue
    if not p.as_posix().startswith(('crates/noon/','crates/noon-web/','crates/noon-native/','fixtures/')): continue
    text = p.read_text()
    before = text
    text = rewrite_imports(text,p)
    # Compiler prepared transaction store() is a different read-only API.
    text = text.replace('.store()', '.integration_store()')
    text = text.replace('prepared.integration_store()', 'prepared.store()')
    text = re.sub(r'\bfn store\(', 'fn integration_store(', text)
    text = re.sub(r'\bScene::with_store\(', 'Scene::with_integration_store(',text)
    if p == Path('crates/noon/src/scene.rs'):
        text = text.replace('Self::with_store(', 'Self::with_integration_store(')
        text = text.replace('pub fn with_store(', 'pub fn with_integration_store(')
    if text!=before: p.write_text(text)
print('Independent consumer integration types:',sorted(fixture_core))

lib.write_text('''//! Direct Rust authoring and coherent live execution for Noon.
//!
//! Start with [`Scene`] and [`Mobject`]. Before execution, their edits author the
//! scene. After lowering, use [`Scene::live`] for edits and effective observations:
//! it publishes through the existing [`ExecutionSession`], not a second scene.
//!
//! # Public surface
//!
//! - The crate root and [`prelude`] contain explicit authoring values, handles,
//!   live operations, logical completion, and their errors.
//! - [`integration`] contains raw arena access types, host/callback plumbing and
//!   renderer observations. These are advanced facilities, not ordinary authoring.
//! - `diagnostics` (feature gated) provides explicit debug/export observations.
//!
//! Use [`LiveSession::authored`] for base state and [`LiveSession::effective`] for
//! the current published value. Complete a segment with
//! [`LiveSession::complete_segment`] before resuming dependent authoring. Ordinary
//! completion is not a GPU-retirement fence. See the `shared_authoring` example.
//!
//! A glob import cannot accidentally expose raw semantic storage or frame plumbing:
//!
//! ```compile_fail,E0433
//! use noon::*;
//! let _ = SemanticStore::new();
//! ```
//!
//! ```compile_fail,E0432
//! use noon::FrameState;
//! ```
//!
//! Implementation modules are not an alternative public facade:
//!
//! ```compile_fail,E0603
//! use noon::semantic_mobject::Mobject;
//! ```
//!
//! Raw mutable storage requires the explicitly named integration accessors:
//!
//! ```compile_fail,E0599
//! let _ = noon::Scene::new().store();
//! ```
//!
//! ```compile_fail,E0599
//! fn raw(object: &noon::Mobject) { let _ = object.store(); }
//! ```
//!
//! ```compile_fail,E0599
//! fn raw(family: &noon::MobjectFamily) { let _ = family.store(); }
//! ```

#![forbid(unsafe_code)]

mod animation_authoring;
mod arc_authoring;
mod camera_authoring;
mod compact_value_authoring;
mod dashed_line_authoring;
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
mod elbow_authoring;
pub mod example_scenes;
mod execution_segment;
mod execution_session;
mod family_arrangement;
mod family_authoring;
mod family_copy;
mod family_layout;
mod focus_on_authoring;
mod geometry_authoring;
mod host_callbacks;
pub mod integration;
mod live_program;
mod live_session;
mod native_signal_authoring;
mod rotation_authoring;
mod rounded_rectangle_authoring;
mod scalar_authoring;
mod scene;
mod scene_membership;
mod sector_authoring;
mod semantic_mobject;
#[cfg(any(feature = "native-text", feature = "typst"))]
mod text_authoring;

pub use animation_authoring::DeclaredAnimation;
pub use execution_segment::{
    ExecutionSegment, ExecutionSegmentAdvanceError, ExecutionSegmentError, ExecutionSegmentState,
};
pub use execution_session::{
    ExecutionSession, ExecutionSegmentCompletionError, ExecutionSessionAnimationError,
    ExecutionSessionCallbackError, ExecutionSessionCallbackReadError, ExecutionSessionCameraError,
    ExecutionSessionCreateError, ExecutionSessionFadeError, ExecutionSessionInputError,
    ExecutionSessionPublicationError, SignalTimelineAppendError,
};
pub use family_arrangement::FamilyArrangeOptions;
pub use family_authoring::{MobjectFamily, MobjectFamilyMember};
pub use family_copy::FamilyCopy;
pub use family_layout::{FamilyLayout, FamilyLayoutTarget, LayoutAnchor};
pub use focus_on_authoring::FocusOnOptions;
pub use host_callbacks::{RustHostCallbackContext, RustHostCallbackError, RustHostCallbackTable};
pub use live_program::{ContinuationStep, LiveContinuation, LiveProgram, LiveProgramError, LiveProgramStatus};
pub use live_session::{
    AffineLifecycleDirection, AffineLifecycleEndpoint, AnimationCompositionRequest,
    DrawBorderThenFillOptions, EffectiveMobjectLayout, EffectiveMobjectState, FadeEndpoint,
    FadeTranslation, IndicateOptions, LiveLayoutTarget, LiveSession, LiveSessionError,
    SubsetDisplayMode, TransformToRequest,
};
pub use native_signal_authoring::{NativeBoolSignal, NativeVectorSignal};
pub use noon_core::{'''+', '.join(sorted(ordinary_core))+'''};
pub use noon_runtime::EvaluationError;
pub use rotation_authoring::ManimRotationPivot;
pub use scalar_authoring::{TrackerPosition, ValueTracker, ValueTrackerPlay};
pub use scene::Scene;
pub use scene_membership::SceneMembershipRequest;
pub use semantic_mobject::{ManimBecomeOptions, ManimGeometryOptions, ManimLineEndpoints, ManimNextToArgs, Mobject};
#[cfg(any(feature = "native-text", feature = "typst"))]
pub use text_authoring::TextAuthoringError;
#[cfg(feature = "native-text")]
pub use text_authoring::{Text, NativeFontFace, DEFAULT_NATIVE_TEXT_FONT_SIZE, DEFAULT_NATIVE_TEXT_FONT_FAMILY};
#[cfg(feature = "typst")]
pub use text_authoring::{Typst, MathTypst, TypstBackendError, DEFAULT_TYPST_FONT_SIZE};

/// Common imports for direct typed semantic authoring and live publication.
/// Host integration and mutable arena access must be imported explicitly.
pub mod prelude {
    pub use crate::{
        AnimationOptions, Color, ContinuationStep, DeclaredAnimation,
        DrawBorderThenFillOptions, EffectiveMobjectState, ExecutionSession,
        FadeEndpoint, FadeTranslation, LiveContinuation, LiveProgram, LiveSession,
        LiveSessionError, Mobject, MobjectFamily, MobjectFamilyMember, NativeBoolSignal,
        NativeVectorSignal, RateFunction, Scene, SemanticObjectState, SemanticStyle,
        StoredGeometry, TrackerPosition, ValueTracker, Vec2, VectorPath,
    };
}
''')

integration_core = sorted(set('''SemanticStore SemanticStoreIdentity SemanticStoreError SemanticNodeCreation
SemanticMutationTransaction SemanticMutationTransactionError SemanticMutationTransactionResult
SemanticMutationImpact SemanticTransactionNodeRef SemanticSceneOperationError SemanticNodeKind
GeometryResource GeometryResourceLookup GeometryResourceHandle GeometryResourceArena
TextResource TextResourceArena TextResourceHandle HostCallbackId'''.split())|fixture_core)
Path('crates/noon/src/integration.rs').write_text('''//! Explicit low-level integration with Noon's existing authorities.
//!
//! Ordinary authoring uses [`Scene`](crate::Scene), [`Mobject`](crate::Mobject),
//! and [`LiveSession`](crate::LiveSession). This module exposes only the specific
//! types needed by language wrappers, platform hosts, resource adapters and
//! diagnostics; it owns no scene, runtime, registry or scheduler.
//!
//! # Raw semantic access
//!
//! [`Scene::with_integration_store`](crate::Scene::with_integration_store) accepts
//! an existing arena. The `integration_store()` accessors on Scene, Mobject and
//! MobjectFamily return the same arena, not a snapshot. Keep RefCell borrows short
//! and release them before calling authoring/session operations.
//!
//! Mutating the arena (including through an authored handle) after lowering does
//! **not** publish that change into a live session. Existing revision and identity
//! checks reject the resulting stale or foreign context. They are never disabled
//! by using this module. Prefer `scene.live(&mut session)` for live mutation; raw
//! callers must use the session's coherent semantic-transaction publication API.
//! If raw edits have already invalidated a session, explicitly discard/rebuild it
//! rather than overwriting its publication revision or treating old frames as new.
//!
//! The retained text adapter below is still needed by explicit transport callers
//! and is deletion-owned by #959. It is not the ordinary Scene authoring API.

pub use crate::compact_value_authoring::semantic_object_state_from_compact;
pub use crate::execution_segment::{'''+', '.join(segment_integration)+'''};
pub use crate::execution_session::{'''+', '.join(session_integration)+'''};
pub use crate::host_callbacks::{'''+', '.join(host_integration)+'''};
pub use crate::semantic_mobject::{'''+', '.join(mobject_integration)+'''};
pub use noon_core::{'''+', '.join(integration_core)+'''};
pub use noon_runtime::{'''+', '.join(runtime_integration)+'''};
#[cfg(any(feature = "native-text", feature = "typst"))]
pub use crate::text_authoring::{RetainedMobject, RetainedScene};
#[cfg(feature = "native-text")]
pub use crate::text_authoring::NATIVE_POINT_TO_SCENE_SCALE;
#[cfg(feature = "typst")]
pub use crate::text_authoring::SCALE_FACTOR_PER_FONT_POINT;
''')
p=Path('crates/noon/src/execution_session.rs')
s=p.read_text()
s=s.replace('pub use callback::*;', 'pub use callback::{'+', '.join(callback_integration+['ExecutionSessionCallbackError','ExecutionSessionCallbackReadError'])+'};')
s=s.replace('pub use completion::*;', 'pub use completion::ExecutionSegmentCompletionError;')
s=s.replace('pub use publication::*;', 'pub use publication::{ExecutionSessionPublicationError, StructuralPublicationStats, EffectiveSemanticObject};')
p.write_text(s)
for relative in ('scene.rs','semantic_mobject.rs','family_authoring.rs'):
    p=Path('crates/noon/src')/relative
    s=p.read_text().replace('    /// Integration access; handles reject stale identities after external edits.\n','')
    s=s.replace('    pub fn integration_store(&self) -> &Rc<RefCell<SemanticStore>> {', '''    /// Raw shared arena access for explicit integration, not live mutation.
    ///
    /// External edits can invalidate generational handles and leave an existing
    /// execution session on a stale scene revision. Use `Scene::live` and its
    /// coherent publication operations for edits after lowering. No revision
    /// validation is bypassed by this accessor; see [`crate::integration`].
    pub fn integration_store(&self) -> &Rc<RefCell<SemanticStore>> {''')
    p.write_text(s)
p=Path('crates/noon/src/scene.rs')
s=p.read_text().replace('    /// Integration entry point for language wrappers sharing one semantic arena.', '''    /// Integration entry point for language wrappers sharing one semantic arena.
    ///
    /// Creates a new root in the existing arena. It does not attach an existing
    /// session or publish changes into one; see [`crate::integration`].''')
s=s.replace('    pub fn root(&self)', '''    /// Current authored revision without exposing mutable arena access.
    pub fn revision(&self) -> noon_core::SceneRevision {
        self.store.borrow().scene_revision()
    }

    /// Construct detached geometry through the shared authoring implementation.
    /// Use `LiveSession::create_manim_geometry` after initial lowering instead.
    pub fn geometry(&self, options: crate::ManimGeometryOptions) -> Result<Mobject, String> {
        Mobject::from_manim_geometry(Rc::clone(&self.store), options)
    }

    pub fn root(&self)''')
p.write_text(s)
Path('crates/noon/examples/shared_authoring.rs').write_text('''//! Direct public-API counterpart of web/python/examples/live_affine_completion.py.
//! Construction, authored/effective queries, live edits and logical completion
//! need no raw store, compiler transaction, or host ownership knowledge.
use noon::{AnimationOptions, RateFunction, Scene, Vec2};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let circle = scene.circle(1.0)?;
    scene.add(&circle)?;
    let mut first_target = circle.target_editor()?;
    first_target.shift(2.0, -2.0)?;
    let mut second_target = circle.target_editor()?;
    second_target.shift(5.0, -2.0)?;
    let options = AnimationOptions::new().run_time(2.0).rate_func(RateFunction::Linear);
    let first = scene.declare_transform_to(&circle, &first_target, options)?;
    let second = scene.declare_transform_to(&circle, &second_target, options)?;
    let mut session = scene.execution_session()?;
    let mut live = scene.live(&mut session);
    let segment = live.play_animation(&first)?;
    live.advance_segment_to(segment, 1.0)?;
    assert_eq!(live.effective(&circle)?.transform.translation, Vec2::new(1.0, -1.0));
    assert_eq!(live.authored(&circle)?.transform.translation.x, 0.0);
    live.advance_segment_to(segment, segment.end_time())?;
    assert!(!live.segment_state(segment).is_complete());
    live.complete_segment(segment)?;
    assert!(live.segment_state(segment).is_complete());
    assert_eq!(live.effective(&circle)?.transform.translation, Vec2::new(2.0, -2.0));
    assert_eq!(live.authored(&circle)?.transform.translation.x, 2.0);
    live.set_translation(&circle, 3.0, -2.0)?;
    let wait = live.wait_segment(0.25)?;
    live.advance_segment_to(wait, wait.end_time())?;
    live.complete_segment(wait)?;
    assert_eq!(live.effective(&circle)?.transform.translation, Vec2::new(3.0, -2.0));
    // Activation reads the edited effective value, not the declaration-time base.
    let segment = live.play_animation(&second)?;
    live.advance_segment_to(segment, segment.end_time() - 1.0)?;
    assert_eq!(live.effective(&circle)?.transform.translation, Vec2::new(4.0, -2.0));
    assert_eq!(live.authored(&circle)?.transform.translation.x, 3.0);
    live.advance_segment_to(segment, segment.end_time())?;
    live.complete_segment(segment)?;
    assert_eq!(live.effective(&circle)?.transform.translation, Vec2::new(5.0, -2.0));
    assert_eq!(live.authored(&circle)?.transform.translation.x, 5.0);
    assert!(live.segment_state(segment).is_complete());
    Ok(())
}
''')
Path('fixtures/provider-consumer/tests/public_facade.rs').write_text('''//! Compile and exercise the public facade as an independent consumer of only noon.
//! Provider-feature CI runs these tests natively and compiles them for WASM.
use noon::{
    AnimationOptions, ExecutionSessionPublicationError, LiveSessionError,
    ManimGeometryOptions, MobjectFamilyMember, RateFunction, Scene, Vec2,
};

#[test]
fn public_authoring_live_queries_and_completion() -> Result<(), Box<dyn std::error::Error>> {
    let mut scene = Scene::new();
    let before = scene.revision();
    let circle = scene.geometry(ManimGeometryOptions::circle(1.0)?)?;
    assert!(scene.revision().get() > before.get());
    scene.add(&circle)?;
    let mut session = scene.execution_session()?;
    let mut live = scene.live(&mut session);
    let target = live.target_editor(&circle)?;
    live.shift(&target, 4.0, -2.0)?;
    let segment = live.declare_and_activate_transform_to(
        &circle, &target,
        AnimationOptions::new().run_time(2.0).rate_func(RateFunction::Linear),
    )?;
    live.advance_segment_to(segment, 1.0)?;
    let halfway = live.effective(&circle)?;
    assert_eq!(halfway.transform.translation, Vec2::new(2.0, -1.0));
    assert_eq!(live.authored(&circle)?.transform.translation.x, 0.0);
    live.advance_segment_to(segment, segment.end_time())?;
    assert!(!live.segment_state(segment).is_complete());
    live.complete_segment(segment)?;
    assert!(live.segment_state(segment).is_complete());
    assert_eq!(live.authored(&circle)?.transform.translation.x, 4.0);
    live.shift(&circle, 1.0, 0.0)?;
    let after = live.effective(&circle)?;
    assert_eq!(after.transform.translation, Vec2::new(5.0, -2.0));
    assert_eq!(after.publication.scene_revision(), scene.revision());
    assert_eq!(halfway.transform.translation, Vec2::new(2.0, -1.0));
    let added = live.create_manim_geometry(ManimGeometryOptions::square(0.5)?)?;
    live.add(&added)?;
    assert!(live.contains(&added)?);
    let wait = live.wait_segment(0.25)?;
    live.advance_segment_to(wait, wait.end_time())?;
    live.complete_segment(wait)?;
    assert!(live.segment_state(wait).is_complete());
    Ok(())
}

#[test]
fn integration_access_keeps_one_arena_and_stale_publication_protection()
    -> Result<(), Box<dyn std::error::Error>>
{
    use noon::integration::{SemanticMutationTransaction, SemanticStore};
    use std::{cell::RefCell, rc::Rc};
    let arena = Rc::new(RefCell::new(SemanticStore::new()));
    let mut scene = Scene::with_integration_store(Rc::clone(&arena));
    let circle = scene.circle(1.0)?;
    let family = scene.family(&[MobjectFamilyMember::Mobject(&circle)])?;
    assert!(Rc::ptr_eq(scene.integration_store(), &arena));
    assert!(Rc::ptr_eq(circle.integration_store(), &arena));
    assert!(Rc::ptr_eq(family.integration_store(), &arena));
    scene.add(&circle)?;
    let mut session = scene.execution_session()?;
    session.take_frame_changes();
    let published = session.publication_context();
    let original_transform = session.frame().objects[0].transform;
    let mut transaction = SemanticMutationTransaction::new();
    transaction.set_property(circle.node_id(), noon::SemanticObjectProperty::Translation,
                             noon::SemanticVec3::new(9.0, 0.0, 0.0));
    transaction.apply(&mut scene.integration_store().borrow_mut())?;
    let raw_revision = scene.revision();
    assert_ne!(raw_revision, published.scene_revision());
    {
        let mut live = scene.live(&mut session);
        let error = live.shift(&circle, 1.0, 0.0).unwrap_err();
        assert!(matches!(error, LiveSessionError::Publication(
            ExecutionSessionPublicationError::StaleSceneRevision { expected, actual }
        ) if expected == published.scene_revision() && actual == raw_revision));
        assert!(matches!(live.effective(&circle), Err(LiveSessionError::Publication(
            ExecutionSessionPublicationError::StaleSceneRevision { .. }
        ))));
    }
    assert_eq!(scene.revision(), raw_revision);
    assert_eq!(session.publication_context(), published);
    assert_eq!(session.frame().objects[0].transform, original_transform);
    assert!(session.take_frame_changes().is_empty());
    Ok(())
}
''')
p=Path('README.md')
s=p.read_text()
a=s.index('The older fluent snapshot authoring API')
b=s.index('After creating a session',a)
s=s[:a]+'''### Ordinary API versus integration

The crate root and `noon::prelude` deliberately export authoring handles, values,
live operations, completion and errors. Implementation modules and blanket
lower-layer exports are not public authoring APIs. `noon::integration` explicitly
exposes the raw semantic/resource types and host/callback/renderer plumbing needed
by adapters. `noon::diagnostics` is feature-gated debug/export access. None of these
namespaces introduces another scene, runtime, scheduler or integration crate.

`Scene::revision()` reads the authored revision without mutable arena access.
`Scene::geometry(ManimGeometryOptions)` constructs a detached specialized shape;
after lowering, use `LiveSession::create_manim_geometry` instead. The
[`shared_authoring` example](crates/noon/examples/shared_authoring.rs) shows
construction, authored/effective queries, live edits and two logical completions
using only ordinary public APIs. It is the direct Rust counterpart of
[`live_affine_completion.py`](web/python/examples/live_affine_completion.py), which
remains in the browser authoring qualification suite.

Raw integration is deliberately named: `Scene::with_integration_store` accepts a
shared arena; `Scene`, `Mobject` and `MobjectFamily` expose it through
`integration_store()`. This is not a snapshot or a live-mutation shortcut. Release
RefCell borrows before calling authoring/session APIs. Edits made outside coherent
publication can stale the existing session; its identity/revision checks still
reject them. A consumer that already made such an edit must explicitly discard
and rebuild that session, not alter its revision bookkeeping. No old-name aliases
are retained. The `RetainedScene` text adapter in `noon::integration` remains a
transport-consumer facility owned for deletion by #959, not the ordinary Scene API.

'''+s[b:]
p.write_text(s)
p=Path('fixtures/provider-consumer/README.md')
s=p.read_text()+'''
## Public authoring boundary

The `public_facade` target depends on `noon` alone. It qualifies the ordinary
constructor/live-query/edit/completion path, immutable effective observations, and
an explicitly opted-in raw integration edit that must retain typed stale-publication
rejection and leave the old runtime/frame unchanged. It runs in the existing
native provider cells and is compiled (not executed) in WASM cells.

```sh
cargo test --manifest-path fixtures/provider-consumer/Cargo.toml --test public_facade
cargo test -p noon --no-default-features --doc
cargo run -p noon --no-default-features --example shared_authoring
```

The doc tests reject accidental root exports, the private implementation module,
and unqualified raw-store access. These boundary checks complement typed membership
and provider qualification; they do not claim all geometry/animation errors or
Python exception producers have been converted to structured errors.
'''
p.write_text(s)

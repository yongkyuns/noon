from pathlib import Path
import re

R=Path(__file__).resolve().parents[1]
def p(x): return R/x
def read(x): return p(x).read_text()
def write(x,s): p(x).write_text(s)
def one(x,a,b):
 s=read(x); n=s.count(a)
 if n!=1: raise RuntimeError(f'{x}: anchor count {n}: {a[:80]!r}')
 write(x,s.replace(a,b,1))
def sub1(x,pat,repl):
 s=read(x); s2,n=re.subn(pat,repl,s,count=1,flags=re.S)
 if n!=1: raise RuntimeError(f'{x}: regex count {n}: {pat[:80]!r}')
 write(x,s2)

# core layout center, matching existing Manim layout-bounds rules
one('crates/noon-core/src/semantic_store.rs','mod semantic_model;\npub use semantic_model::*;','mod semantic_model;\npub use semantic_model::*;\nmod effective_layout;\npub use effective_layout::*;')
write('crates/noon-core/src/semantic_store/effective_layout.rs',r'''use crate::{Bounds2D64, GeometryResource, PathCommand, SemanticObjectContent, SemanticObjectState, SemanticStore, StoredGeometry, Transform2D, Vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectiveLayoutError { MissingGeometry, MissingText, Empty }
impl std::fmt::Display for EffectiveLayoutError { fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result { write!(f,"effective layout unavailable: {self:?}") } }
impl std::error::Error for EffectiveLayoutError {}

fn xy(t:Transform2D,x:f64,y:f64)->(f64,f64){
 let x=x*f64::from(t.scale.x); let y=y*f64::from(t.scale.y);
 let r=f64::from(t.rotation); let (s,c)=r.sin_cos();
 (x*c-y*s+f64::from(t.translation.x),x*s+y*c+f64::from(t.translation.y))
}
fn include(b:&mut Option<Bounds2D64>,q:(f64,f64)){ if let Some(b)=b { b.include(q.0,q.1) } else { *b=Some(Bounds2D64::point(q.0,q.1)) } }
fn path_bounds(path:&crate::VectorPath,t:Transform2D)->Option<Bounds2D64>{
 let mut b=None; let mut cur=None; let mut start=None;
 for cmd in path.commands(){ match *cmd {
  PathCommand::MoveTo{to}=>{let q=xy(t,to.x.into(),to.y.into());include(&mut b,q);cur=Some(q);start=Some(q)},
  PathCommand::LineTo{to}=>{let q=xy(t,to.x.into(),to.y.into());if let Some(c)=cur{include(&mut b,c)}include(&mut b,q);cur=Some(q)},
  PathCommand::QuadraticTo{control,to}=>{let q=xy(t,to.x.into(),to.y.into());let c=xy(t,control.x.into(),control.y.into());if let Some(a)=cur{include(&mut b,a);include(&mut b,(a.0+(c.0-a.0)*2.0/3.0,a.1+(c.1-a.1)*2.0/3.0));include(&mut b,(q.0+(c.0-q.0)*2.0/3.0,q.1+(c.1-q.1)*2.0/3.0));}include(&mut b,q);cur=Some(q)},
  PathCommand::CubicTo{control1,control2,to}=>{let q=xy(t,to.x.into(),to.y.into());if let Some(a)=cur{include(&mut b,a)}include(&mut b,xy(t,control1.x.into(),control1.y.into()));include(&mut b,xy(t,control2.x.into(),control2.y.into()));include(&mut b,q);cur=Some(q)},
  PathCommand::Close=>{if let Some(c)=cur{include(&mut b,c)}if let Some(a)=start{include(&mut b,a);cur=Some(a)}}
 }} b
}
pub fn effective_layout_bounds(store:&SemanticStore,state:&SemanticObjectState,t:Transform2D)->Result<Bounds2D64,EffectiveLayoutError>{
 let mut b=None;
 match state.content {
  SemanticObjectContent::Text(h)=>{let r=store.text_resources().get(h).ok_or(EffectiveLayoutError::MissingText)?; for q in [r.bounds.min,Vec2::new(r.bounds.min.x,r.bounds.max.y),r.bounds.max,Vec2::new(r.bounds.max.x,r.bounds.min.y)]{include(&mut b,xy(t,q.x.into(),q.y.into()))}},
  SemanticObjectContent::Geometry(g)=>match g {
   StoredGeometry::Circle{radius}=>{let r=f64::from(radius); let k=(4.0/3.0)*(std::f64::consts::PI/16.0).tan(); for i in 0..8 {let a=i as f64*std::f64::consts::PI/4.0;let e=(i+1) as f64*std::f64::consts::PI/4.0;let(sa,ca)=a.sin_cos();let(se,ce)=e.sin_cos();for(x,y) in [(ca,sa),(ca-k*sa,sa+k*ca),(ce+k*se,se-k*ce),(ce,se)]{include(&mut b,xy(t,r*x,r*y))}}},
   StoredGeometry::Rectangle{size}=>{let x=f64::from(size.x)/2.0;let y=f64::from(size.y)/2.0;for q in [(-x,-y),(-x,y),(x,-y),(x,y)]{include(&mut b,xy(t,q.0,q.1))}},
   StoredGeometry::Line{start,end}=>{include(&mut b,xy(t,start.x.into(),start.y.into()));include(&mut b,xy(t,end.x.into(),end.y.into()))},
   StoredGeometry::Resource(h)=>match store.geometry_resources().get(h).ok_or(EffectiveLayoutError::MissingGeometry)? { GeometryResource::VectorPath(path)=> b=path_bounds(path,t) },
  }
 }
 b.ok_or(EffectiveLayoutError::Empty)
}
pub fn effective_layout_center(store:&SemanticStore,state:&SemanticObjectState,t:Transform2D)->Result<Vec2,EffectiveLayoutError>{ let b=effective_layout_bounds(store,state,t)?; Ok(Vec2::new(((b.min_x+b.max_x)*0.5) as f32,((b.min_y+b.max_y)*0.5) as f32)) }
''')

# new bounded transform interpolation
one('crates/noon-core/src/semantic_store/semantic_animations.rs','    /// Interpolate corresponding analytic path points while retaining semantic affine channels.\n    PointCorrespondence,','    /// Interpolate corresponding analytic path points while retaining semantic affine channels.\n    PointCorrespondence,\n    /// Preserve the source presentation while translating its activation-time layout center to the target-state center.\n    CenterTranslation,')

# live request variant
one('crates/noon/src/live_session.rs','    FamilyTransformTo {\n        source: &\'a MobjectFamily,\n        target_state: &\'a MobjectFamily,\n        options: AnimationOptions,\n    },','    FamilyTransformTo {\n        source: &\'a MobjectFamily,\n        target_state: &\'a MobjectFamily,\n        options: AnimationOptions,\n    },\n    CyclicReplace {\n        family: &\'a MobjectFamily,\n        options: AnimationOptions,\n    },')

# semantic request variant
one('crates/noon/src/execution_session.rs','    FamilyTransformTo {\n        source: SemanticNodeId,\n        target_state: SemanticNodeId,\n        options: AnimationOptions,\n    },','    FamilyTransformTo {\n        source: SemanticNodeId,\n        target_state: SemanticNodeId,\n        options: AnimationOptions,\n    },\n    CyclicReplace {\n        family: SemanticNodeId,\n        options: AnimationOptions,\n    },')
# Don't snapshot peer state for center-translation leaves.
one('crates/noon/src/execution_session.rs','                let target_state =\n                    self.stage_animation_target_state(store, declaration, *target_state)?;\n                let animation = declaration.create_transform_animation_with_interpolation(','                let target_state = if *interpolation == noon_core::SemanticTransformInterpolation::CenterTranslation {\n                    *target_state\n                } else {\n                    self.stage_animation_target_state(store, declaration, *target_state)?\n                };\n                let animation = declaration.create_transform_animation_with_interpolation(')
# add CyclicReplace staging before family transform
anchor='            SemanticCompositionRequest::FamilyTransformTo {\n                source,\n                target_state,\n                options,\n            } => {'
insert=r'''            SemanticCompositionRequest::CyclicReplace { family, options } => {
                let members = store
                    .semantic_family_members_checked(*family)
                    .map_err(|error| ExecutionSessionAnimationError::InvalidComposition(error.to_string()))?;
                if members.len() < 2 || members.iter().any(|member| {
                    !self.reachability.is_object_reachable(*member)
                        || store.node(*member).is_none_or(|node| !matches!(node.kind(), noon_core::SemanticNodeKind::AuthoringObject))
                }) {
                    return Err(ExecutionSessionAnimationError::InvalidComposition(
                        "CyclicReplace requires at least two scene-bound flat object members".into(),
                    ));
                }
                let path_arc = options.path_arc.unwrap_or(std::f64::consts::FRAC_PI_2);
                let mut child_options = AnimationOptions::new().rate_func(RateFunction::Linear).path_arc(path_arc);
                child_options.run_time = None;
                let children = members.iter().enumerate().map(|(index, source)| {
                    SemanticCompositionRequest::TransformTo {
                        source: *source,
                        target_state: members[(index + 1) % members.len()],
                        interpolation: noon_core::SemanticTransformInterpolation::CenterTranslation,
                        complete_priority: false,
                        options: child_options,
                    }
                }).collect();
                let mut composition_options = *options;
                composition_options.path_arc = None;
                let expanded = SemanticCompositionRequest::Composition {
                    kind: SemanticAnimationCompositionKind::Parallel,
                    children,
                    options: composition_options,
                };
                self.stage_composition_request(store, root, &expanded, declaration, admitted, removals)
            }
'''
one('crates/noon/src/execution_session.rs',anchor,insert+anchor)

# prepared lowerer: special center-translation before ordinary affine validation/lowering
old=r'''                let target = prepared.object_state(target_state).map_err(|error| {
                    PreparedSemanticAnimationLoweringError::Target {
                        animation: leaf.animation,
                        node: target_state,
                        error,
                    }
                })?;
                validate_affine_payload(source, target, leaf.options)
                    .map_err(|issue| prepared_payload_error(leaf, target_state, issue))?;
                let from = capture_effective(
                    leaf,
                    source,
                    admitted.contains(&leaf.target),
                    &mut captures,
                    &mut effective_properties,
                )?;
                let channels = lower_transform_channels(
                    prepared.store(),
                    source,
                    target,
                    from,
                    interpolation,
                    leaf.options.path_arc,
                )
                .map_err(|issue| prepared_payload_error(leaf, target_state, issue))?;'''
new=r'''                let target = prepared.object_state(target_state).map_err(|error| {
                    PreparedSemanticAnimationLoweringError::Target {
                        animation: leaf.animation,
                        node: target_state,
                        error,
                    }
                })?;
                let from = capture_effective(
                    leaf,
                    source,
                    admitted.contains(&leaf.target),
                    &mut captures,
                    &mut effective_properties,
                )?;
                let channels = if interpolation == noon_core::SemanticTransformInterpolation::CenterTranslation {
                    let peer = target_state.existing().and_then(|node| index.execution_object_id(node)).ok_or(
                        PreparedSemanticAnimationLoweringError::MissingEffectiveProperties {
                            animation: leaf.animation,
                            target: target_state,
                            execution_object_id: ObjectId::new(u64::MAX),
                        },
                    )?;
                    let mut peer_leaf = leaf.clone();
                    peer_leaf.target = target_state;
                    peer_leaf.execution_object_id = peer;
                    captures.begin_leaf(&peer_leaf, &mut driven, &tracks, &intervals, true);
                    let peer_from = capture_effective(
                        &peer_leaf,
                        target,
                        false,
                        &mut captures,
                        &mut effective_properties,
                    )?;
                    let source_center = noon_core::effective_layout_center(prepared.store(), source, from.transform)
                        .map_err(|_| PreparedSemanticAnimationLoweringError::UnsupportedContentChange { animation: leaf.animation, target: leaf.target, target_state })?;
                    let target_center = noon_core::effective_layout_center(prepared.store(), target, peer_from.transform)
                        .map_err(|_| PreparedSemanticAnimationLoweringError::UnsupportedContentChange { animation: leaf.animation, target: leaf.target, target_state })?;
                    let delta = target_center - source_center;
                    let to = from.transform.translation + delta;
                    if source.signal_bindings().iter().any(|binding| binding.property() == SemanticObjectProperty::Translation) {
                        return Err(PreparedSemanticAnimationLoweringError::ReactiveDriverConflict { animation: leaf.animation, target: leaf.target, property: SemanticObjectProperty::Translation });
                    }
                    let values = if leaf.options.path_arc.abs() >= noon_core::MANIM_STRAIGHT_PATH_ARC_THRESHOLD {
                        TrackValues::ArcVec2 { from: from.transform.translation, to, arc_angle: leaf.options.path_arc }
                    } else {
                        TrackValues::Vec2 { from: from.transform.translation, to }
                    };
                    vec![super::affine::LoweredAffineChannel {
                        property: Property::Position,
                        conflict_property: SemanticObjectProperty::Translation,
                        completion: SemanticAnimationCompletion::Property {
                            property: SemanticObjectProperty::Translation,
                            value: noon_core::SemanticVec3::new(f64::from(to.x), f64::from(to.y), source.transform.translation.z).into(),
                        },
                        values,
                    }]
                } else {
                    validate_affine_payload(source, target, leaf.options)
                        .map_err(|issue| prepared_payload_error(leaf, target_state, issue))?;
                    lower_transform_channels(
                        prepared.store(), source, target, from, interpolation, leaf.options.path_arc,
                    ).map_err(|issue| prepared_payload_error(leaf, target_state, issue))?
                };'''
one('crates/noon-compile/src/semantic_lowering/animation_payload/prepared_composition.rs',old,new)

print('patched center translation')

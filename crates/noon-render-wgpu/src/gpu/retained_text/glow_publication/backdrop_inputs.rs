//! Derived local painter-prefix dependencies over existing execution identities.
//! Geometry comes from the publication's index; this cache owns only effect
//! footprints, capture selections and reverse links, never authored values.
use super::*;
use crate::{EffectCaptureTile, EffectPixelBounds, EffectStackParameters};
use noon_core::{FrameEpoch, Rect};
use noon_runtime::{ExecutionSlotId, ExecutionSpatialIndex};

#[derive(Clone, Debug)]
pub(super) struct BackdropInput {
    pub owner: ObjectId,
    pub anchor: ObjectId,
    pub window: EffectCaptureTile,
    pub candidates: Arc<[ObjectId]>,
    pub source_epoch: FrameEpoch,
    members: Vec<usize>,
    dependencies: BTreeSet<usize>,
}
impl BackdropInput {
    pub fn dependencies(&self) -> impl Iterator<Item = usize> + '_ {
        self.dependencies.iter().copied()
    }
}
struct Selected {
    candidates: Arc<[ObjectId]>,
    members: Vec<usize>,
    dependencies: BTreeSet<usize>,
}
#[derive(Clone, Copy, Debug)]
struct Footprint {
    owner: ObjectId,
    input: Option<Rect>,
    original: Option<Rect>,
    output: Option<Rect>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct QueryStats {
    pub queries: usize,
    pub candidates: usize,
    pub footprints: usize,
    pub links: usize,
}
#[derive(Debug, Default)]
pub(super) struct BackdropInputs {
    pub entries: BTreeMap<usize, BackdropInput>,
    contributions: ExecutionSpatialIndex,
    footprints: BTreeMap<usize, Footprint>,
    windows: ExecutionSpatialIndex,
    reverse: BTreeMap<usize, BTreeSet<usize>>,
    context: Option<PublicationContext>,
    view: Option<(Camera2D, [u32; 2])>,
    pub stats: QueryStats,
}
fn union(a: Option<Rect>, b: Option<Rect>) -> Option<Rect> {
    match (a, b) {
        (Some(a), Some(b)) => Some(Rect::new(
            Vec2::new(a.min.x.min(b.min.x), a.min.y.min(b.min.y)),
            Vec2::new(a.max.x.max(b.max.x), a.max.y.max(b.max.y)),
        )),
        _ => a.or(b),
    }
}
fn expanded(bounds: Rect, amount: Vec2) -> Rect {
    Rect::new(bounds.min - amount, bounds.max + amount)
}
fn tile_bounds(tile: EffectCaptureTile, camera: Camera2D, viewport: [u32; 2]) -> Rect {
    EffectPixelBounds {
        min: tile.origin.map(f64::from),
        max: std::array::from_fn(|axis| f64::from(tile.origin[axis]) + f64::from(tile.size[axis])),
    }
    .world_rect(camera, viewport)
}
fn slot(index: usize) -> ExecutionSlotId {
    ExecutionSlotId::new(index as u32, 0)
}
impl BackdropInputs {
    pub fn visible_members(&self) -> impl Iterator<Item = usize> + '_ {
        self.entries
            .values()
            .flat_map(|entry| entry.members.iter().copied())
    }
    pub fn required(&self, index: usize) -> bool {
        self.reverse
            .get(&index)
            .is_some_and(|owners| !owners.is_empty())
    }
    fn raw_bounds(publication: &RendererPublication<'_>, index: usize) -> Option<Rect> {
        let row = publication.frame().objects.get(index)?;
        publication.spatial_index()?.bounds_for_object(row.id)
    }
    fn footprint(
        &self,
        publication: &RendererPublication<'_>,
        state: &RetainedEffectPublication,
        index: usize,
        full: bool,
        camera: Camera2D,
        viewport: [u32; 2],
    ) -> Result<Footprint, EffectPrepareError> {
        let frame = publication.frame();
        let row = &frame.objects[index];
        let old = self
            .footprints
            .get(&index)
            .filter(|old| old.owner == row.id);
        let (input, original) = if let Some(scope) = state.composed.scopes.get(&index) {
            if let Some(old) = old.filter(|_| !full && !state.capture_dirty.contains(&index)) {
                (old.input, old.original)
            } else {
                let mut input = None;
                let mut original = None;
                let members = scope.source.members();
                let mut at = 0;
                while at < members.len() {
                    let member = publication
                        .object_index(members[at])
                        .ok_or(EffectPrepareError::PublicationMismatch)?;
                    original = union(original, Self::raw_bounds(publication, member));
                    // The outermost active child owns this complete interval.
                    let child =
                        state
                            .composed_candidates(index, member)
                            .filter(|(child, _, _, _, _)| state.sources.contains_key(child))
                            .filter_map(|candidate| match RetainedEffectPublication::active_source(
                                frame,
                                candidate.0,
                                (camera, viewport),
                            ) {
                                Ok(true) => Some(Ok(candidate)),
                                Ok(false) => None,
                                Err(error) => Some(Err(error)),
                            })
                            .collect::<Result<Vec<_>, _>>()?
                            .into_iter()
                            .min_by_key(|c| c.4);
                    if let Some((child, _, count, first, _)) = child {
                        if first != members[at] || count > members.len() - at {
                            return Err(EffectPrepareError::PublicationMismatch);
                        }
                        input = union(input, self.footprints.get(&child).and_then(|f| f.output));
                        for &object in &members[at + 1..at + count] {
                            original = union(
                                original,
                                Self::raw_bounds(
                                    publication,
                                    publication
                                        .object_index(object)
                                        .ok_or(EffectPrepareError::PublicationMismatch)?,
                                ),
                            );
                        }
                        at += count;
                    } else {
                        input = union(
                            input,
                            self.footprints
                                .get(&member)
                                .and_then(|f| f.output)
                                .or_else(|| Self::raw_bounds(publication, member)),
                        );
                        at += 1;
                    }
                }
                (input, original)
            }
        } else {
            let bounds = Self::raw_bounds(publication, index);
            (bounds, bounds)
        };
        let opacity = crate::pack_style(row).opacity;
        let output = if opacity <= 0.0 {
            None
        } else if row
            .effects
            .as_ref()
            .is_some_and(|s| s.backdrop_blur().is_some())
        {
            // Backdrop processing is clipped to original target coverage; it
            // never contributes a blur halo outside the target itself.
            original.map(|b| {
                expanded(
                    b,
                    Vec2::new(
                        camera.world_size.x / viewport[0] as f32,
                        camera.world_size.y / viewport[1] as f32,
                    ),
                )
            })
        } else if let Some(bounds) = input {
            let definitions = row.effects.as_ref().map_or(&[][..], |s| s.definitions());
            EffectCaptureTile::prepare_stack_input(
                EffectPixelBounds::from_world_rect(bounds, camera, viewport),
                viewport,
                definitions,
                EffectStackParameters {
                    shader: None,
                    working_format: wgpu::TextureFormat::Rgba8Unorm,
                    output_height: viewport[1],
                    world_view_height: f64::from(camera.world_size.y),
                    scope_opacity: opacity,
                    scratch_budget_bytes: u64::MAX,
                    source_bounds: None,
                },
                u32::MAX,
                u64::MAX,
                crate::gpu::glow_filter::CaptureDemand {
                    required_by_parent: true,
                    isolate: row.content.composed_source().is_some() && opacity != 1.0,
                    ..Default::default()
                },
            )?
            .map(|tile| tile_bounds(tile, camera, viewport))
            .or(input)
        } else {
            None
        };
        Ok(Footprint {
            owner: row.id,
            input,
            original,
            output,
        })
    }
    fn unlink(&mut self, owner: usize, entry: &BackdropInput) {
        for &input in &entry.dependencies {
            if let Some(owners) = self.reverse.get_mut(&input) {
                owners.remove(&owner);
                if owners.is_empty() {
                    self.reverse.remove(&input);
                }
            }
        }
    }
    fn remove(&mut self, owner: usize) {
        if let Some(old) = self.entries.remove(&owner) {
            self.unlink(owner, &old);
            self.windows.remove_object(old.owner);
        }
    }
    fn selected(
        &mut self,
        publication: &RendererPublication<'_>,
        state: &RetainedEffectPublication,
        anchor: ObjectId,
        window: Rect,
    ) -> Result<Selected, EffectPrepareError> {
        let anchor_index = publication
            .object_index(anchor)
            .ok_or(EffectPrepareError::PublicationMismatch)?;
        let anchor_rank = state.painter_ranks[anchor_index];
        let geometry = publication
            .query_painter_prefix_geometry(window, anchor)
            .map_err(|_| EffectPrepareError::PublicationMismatch)?;
        self.stats.queries += 1;
        self.stats.candidates += geometry.stats.candidates_tested;
        let mut members = geometry
            .objects
            .into_iter()
            .filter_map(|object| publication.object_index(object))
            .filter(|&index| publication.frame().is_drawable(index))
            .collect::<BTreeSet<_>>();
        let effects = self.contributions.query_rect(window);
        self.stats.candidates += effects.stats().candidates_tested;
        let mut dependencies = BTreeSet::new();
        for &source in effects.slots() {
            let object = self
                .contributions
                .object_for_slot(source)
                .ok_or(EffectPrepareError::PublicationMismatch)?;
            let index = publication
                .object_index(object)
                .ok_or(EffectPrepareError::PublicationMismatch)?;
            let interval = state
                .composed
                .scopes
                .get(&index)
                .map(|scope| scope.source.members());
            let first = interval.map_or(object, |members| members[0]);
            let last = interval.map_or(object, |members| *members.last().unwrap());
            let first = publication
                .object_index(first)
                .ok_or(EffectPrepareError::PublicationMismatch)?;
            let last = publication
                .object_index(last)
                .ok_or(EffectPrepareError::PublicationMismatch)?;
            if state.painter_ranks[first] >= anchor_rank {
                continue;
            }
            if state.painter_ranks[last] >= anchor_rank {
                return Err(EffectPrepareError::UnsupportedCapture);
            }
            dependencies.insert(index);
            if let Some(interval) = interval {
                for &member in interval {
                    members.insert(
                        publication
                            .object_index(member)
                            .ok_or(EffectPrepareError::PublicationMismatch)?,
                    );
                }
            } else {
                members.insert(index);
            }
        }
        // Spatial gaps are valid, but every substituted isolated interval must
        // still include all its physical members, in canonical order.
        let initial = members.iter().copied().collect::<Vec<_>>();
        let mut expanded_scopes = BTreeSet::new();
        for member in initial {
            let mut outer = None;
            for candidate in state.composed_candidates(usize::MAX, member) {
                if RetainedEffectPublication::active_source(
                    publication.frame(),
                    candidate.0,
                    self.view.unwrap(),
                )? && outer.is_none_or(|old: (usize, ObjectId, usize, ObjectId, usize)| {
                    candidate.4 < old.4
                }) {
                    outer = Some(candidate);
                }
            }
            if let Some((owner, _, _, _, _)) = outer {
                if expanded_scopes.insert(owner) {
                    let source = &state.composed.scopes[&owner].source;
                    let last = publication
                        .object_index(*source.members().last().unwrap())
                        .ok_or(EffectPrepareError::PublicationMismatch)?;
                    if state.painter_ranks[last] >= anchor_rank {
                        return Err(EffectPrepareError::UnsupportedCapture);
                    }
                    dependencies.insert(owner);
                    for &object in source.members() {
                        let member = publication
                            .object_index(object)
                            .ok_or(EffectPrepareError::PublicationMismatch)?;
                        members.insert(member);
                        for (child, _, _, _, _) in state.composed_candidates(owner, member) {
                            if RetainedEffectPublication::active_source(
                                publication.frame(),
                                child,
                                self.view.unwrap(),
                            )? {
                                dependencies.insert(child);
                            }
                        }
                    }
                }
            }
        }
        dependencies.extend(members.iter().copied());
        let mut members = members.into_iter().collect::<Vec<_>>();
        members.sort_unstable_by_key(|&i| state.painter_ranks[i]);
        if members.iter().any(|&i| {
            state.painter_ranks[i] >= anchor_rank
                || publication.frame().objects[i].spatial.is_some()
        }) {
            return Err(EffectPrepareError::UnsupportedCapture);
        }
        let objects = members
            .iter()
            .map(|&i| publication.frame().objects[i].id)
            .collect::<Vec<_>>()
            .into();
        Ok(Selected {
            candidates: objects,
            members,
            dependencies,
        })
    }
    pub fn refresh(
        &mut self,
        publication: &RendererPublication<'_>,
        state: &mut RetainedEffectPublication,
        camera: Camera2D,
        viewport: [u32; 2],
    ) -> Result<(), EffectPrepareError> {
        if state.prefix_owners.is_empty() {
            *self = Self::default();
            return Ok(());
        }
        let view = (camera, viewport);
        if self.context == Some(publication.context())
            && self.view == Some(view)
            && !state.invalidated
            && !state.sync_failed
        {
            return Ok(());
        }
        let full = self.context.is_none()
            || self.view != Some(view)
            || publication.changes().is_all()
            || state.invalidated
            || state.sync_failed;
        self.view = Some(view);
        let mut changed = if full {
            state.sources.keys().copied().collect::<Vec<_>>()
        } else {
            state.dirty.iter().copied().collect()
        };
        // Physical sources first; child scope footprints precede their parents.
        changed.sort_unstable_by_key(|&index| {
            state
                .composed
                .depth(index)
                .map_or((false, std::cmp::Reverse(0)), |depth| {
                    (true, std::cmp::Reverse(depth))
                })
        });
        if full {
            self.contributions = ExecutionSpatialIndex::default();
            self.footprints.clear();
            self.entries.clear();
            self.windows = ExecutionSpatialIndex::default();
            self.reverse.clear();
        }
        for &index in &changed {
            if state.sources.contains_key(&index) {
                let footprint =
                    self.footprint(publication, state, index, full, camera, viewport)?;
                self.stats.footprints += 1;
                if let Some(bounds) = footprint.output {
                    self.contributions.upsert_bounds(
                        slot(index),
                        footprint.owner,
                        bounds,
                        state.painter_ranks[index] as u64,
                    );
                } else {
                    self.contributions.remove_object(footprint.owner);
                }
                self.footprints.insert(index, footprint);
            } else if let Some(old) = self.footprints.remove(&index) {
                self.contributions.remove_object(old.owner);
            }
        }
        let retired = changed
            .iter()
            .filter(|index| !state.prefix_owners.contains(index))
            .copied()
            .collect::<Vec<_>>();
        for index in retired {
            self.remove(index);
        }
        let mut input_changed = BTreeSet::new();
        let mut reselect = BTreeSet::new();
        if full {
            input_changed.extend(state.prefix_owners.iter().copied());
            reselect.extend(state.prefix_owners.iter().copied());
        }
        let mut touched = publication
            .changes()
            .object_indices()
            .iter()
            .copied()
            .chain(changed.iter().copied())
            .collect::<BTreeSet<_>>();
        if let Some(range) = publication.changes().painter_order_range() {
            touched.extend(
                publication
                    .painter_order()
                    .iter()
                    .take(range.end)
                    .skip(range.start)
                    .map(|&row| row as usize),
            );
        }
        for input in touched {
            if let Some(owners) = self.reverse.get(&input) {
                input_changed.extend(owners.iter().copied());
            }
            if let Some(row) = publication.frame().objects.get(input) {
                let bounds = self
                    .footprints
                    .get(&input)
                    .and_then(|f| f.output)
                    .or_else(|| Self::raw_bounds(publication, input));
                if let Some(bounds) = bounds {
                    for &owner in self.windows.query_rect(bounds).slots() {
                        let object = self
                            .windows
                            .object_for_slot(owner)
                            .ok_or(EffectPrepareError::PublicationMismatch)?;
                        let owner = publication
                            .object_index(object)
                            .ok_or(EffectPrepareError::PublicationMismatch)?;
                        let anchor = self.entries[&owner].anchor;
                        let anchor = publication
                            .object_index(anchor)
                            .ok_or(EffectPrepareError::PublicationMismatch)?;
                        if state.painter_ranks[input] < state.painter_ranks[anchor] {
                            input_changed.insert(owner);
                        }
                    }
                }
                if row
                    .effects
                    .as_ref()
                    .is_some_and(|stack| stack.backdrop_blur().is_some())
                {
                    reselect.insert(input);
                }
            }
        }
        // An earlier backdrop's changed output is an ordinary input to later
        // prefixes, even when no authored value on that later target changed.
        let mut pending = input_changed.iter().copied().collect::<Vec<_>>();
        while let Some(input) = pending.pop() {
            for &owner in self.reverse.get(&input).into_iter().flatten() {
                if input_changed.insert(owner) {
                    pending.push(owner);
                }
            }
        }
        reselect.extend(input_changed.iter().copied());
        for &owner in &changed {
            if state.prefix_owners.contains(&owner) && !self.entries.contains_key(&owner) {
                reselect.insert(owner);
            }
        }
        for owner in reselect {
            if !state.prefix_owners.contains(&owner) {
                continue;
            }
            let row = &publication.frame().objects[owner];
            let stack = row.effects.as_ref().unwrap();
            let effect = stack.backdrop_blur().unwrap();
            let sigma = crate::EffectCaptureParameters {
                definition: effect.definition.clone(),
                output_height: viewport[1],
                world_view_height: f64::from(camera.world_size.y),
                scope_opacity: crate::pack_style(row).opacity,
            }
            .sigma_pixels()?;
            if sigma == 0.0 || crate::pack_style(row).opacity == 0.0 {
                self.remove(owner);
                continue;
            }
            let Some(bounds) = self.footprints.get(&owner).and_then(|f| f.original) else {
                self.remove(owner);
                continue;
            };
            let Some(window) = EffectCaptureTile::prepare_stack_input(
                EffectPixelBounds::from_world_rect(bounds, camera, viewport),
                viewport,
                std::slice::from_ref(&effect.definition),
                EffectStackParameters {
                    shader: None,
                    working_format: wgpu::TextureFormat::Rgba8Unorm,
                    output_height: viewport[1],
                    world_view_height: f64::from(camera.world_size.y),
                    scope_opacity: crate::pack_style(row).opacity,
                    scratch_budget_bytes: DEFAULT_EFFECT_TEXTURE_BUDGET,
                    source_bounds: None,
                },
                u32::MAX,
                DEFAULT_EFFECT_TEXTURE_BUDGET,
                crate::gpu::glow_filter::CaptureDemand {
                    required_by_parent: true,
                    ..Default::default()
                },
            )?
            else {
                self.remove(owner);
                continue;
            };
            let anchor = row
                .content
                .composed_source()
                .map_or(row.id, |source| source.members()[0]);
            let old = self.entries.get(&owner);
            if !full
                && !input_changed.contains(&owner)
                && old.is_some_and(|old| old.anchor == anchor && old.window == window)
            {
                continue;
            }
            let epoch = old
                .filter(|_| !input_changed.contains(&owner))
                .map_or(publication.context().frame_epoch(), |old| old.source_epoch);
            // Include the contributor's raster AA fringe, not just its exact
            // geometric box, when gathering raw physical source candidates.
            let query_window = expanded(
                tile_bounds(window, camera, viewport),
                Vec2::new(
                    2.0 * camera.world_size.x / viewport[0] as f32,
                    2.0 * camera.world_size.y / viewport[1] as f32,
                ),
            );
            let Selected {
                candidates,
                members,
                dependencies,
            } = self.selected(publication, state, anchor, query_window)?;
            if let Some(old) = self.entries.remove(&owner) {
                self.unlink(owner, &old);
            }
            for &input in &dependencies {
                self.reverse.entry(input).or_default().insert(owner);
                self.stats.links += 1;
            }
            self.windows.upsert_bounds(
                slot(owner),
                row.id,
                query_window,
                state.painter_ranks[owner] as u64,
            );
            self.entries.insert(
                owner,
                BackdropInput {
                    owner: row.id,
                    anchor,
                    window,
                    candidates,
                    source_epoch: epoch,
                    members,
                    dependencies,
                },
            );
            state.dirty.insert(owner);
        }
        self.context = Some(publication.context());
        Ok(())
    }
}

#[cfg(test)]
mod tests;

//! Split slices capture their query profile before host index protocols run.
use crate::{AuthoringError, CompositeEntryHandle, FamilyMemberView, MobjectFamily};
use std::num::NonZeroI64;

/// A prepared slice over one family. Point-free roots retain the member list;
/// own-point roots retain the split length and read current members at selection.
#[derive(Debug)]
pub struct FamilySlicePlan {
    root: MobjectFamily,
    captured_members: Option<FamilyMemberView>,
    split_length: usize,
}

impl MobjectFamily {
    pub fn slice_plan(&self) -> Result<FamilySlicePlan, AuthoringError> {
        self.slice_plan_with_self(self.get_num_points()? != 0)
    }

    /// Supply root inclusion selected by a host predicate before index coercion.
    pub fn slice_plan_with_self(
        &self,
        include_self: bool,
    ) -> Result<FamilySlicePlan, AuthoringError> {
        self.validate()?;
        Ok(FamilySlicePlan {
            root: self.clone(),
            captured_members: if include_self {
                None
            } else {
                Some(self.member_view()?)
            },
            split_length: self.split_count_with_self(include_self)?,
        })
    }
}

impl FamilySlicePlan {
    pub fn includes_self(&self) -> bool {
        self.captured_members.is_none()
    }

    /// Call after operand protocols for a point-free list. For an own-point
    /// root, this is the length captured before those protocols.
    pub fn normalization_count(&self) -> Result<usize, AuthoringError> {
        self.root.validate()?;
        self.captured_members
            .as_ref()
            .map_or(Ok(self.split_length), FamilyMemberView::len)
    }

    /// Native callers can normalize signed slice operands without a frontend.
    pub fn select_slice(
        &self,
        start: Option<i64>,
        stop: Option<i64>,
        step: i64,
    ) -> Result<Option<Vec<CompositeEntryHandle>>, AuthoringError> {
        let step = NonZeroI64::new(step).ok_or(AuthoringError::ZeroSliceStep)?;
        self.select_span(SliceSpan::normalize(
            start,
            stop,
            step,
            self.normalization_count()?,
        ))
    }

    /// A language may normalize against its own earlier length, then invoke this
    /// operation after arbitrary index callbacks. Missing current ranks return
    /// `None`, with no partial result or semantic mutation.
    pub fn select_normalized(
        &self,
        start: i64,
        stop: i64,
        step: i64,
    ) -> Result<Option<Vec<CompositeEntryHandle>>, AuthoringError> {
        let step = NonZeroI64::new(step).ok_or(AuthoringError::ZeroSliceStep)?;
        self.select_span(SliceSpan { start, stop, step })
    }

    fn select_span(
        &self,
        mut span: SliceSpan,
    ) -> Result<Option<Vec<CompositeEntryHandle>>, AuthoringError> {
        self.root.validate()?;
        if span.is_empty() {
            return Ok(Some(Vec::new()));
        }
        let include_root = self.includes_self() && span.contains_zero();
        if self.includes_self() && !include_root {
            // CE's own-root-free fast path slices the current child list again.
            // A negative range stop at zero/-1 becomes an omitted list stop.
            let stop = if span.step.get() > 0 || span.stop > 0 {
                Some(span.stop.saturating_sub(1))
            } else {
                None
            };
            span = SliceSpan::normalize(
                Some(span.start.saturating_sub(1)),
                stop,
                span.step,
                self.root.member_count()?,
            );
        }
        let mut selected = Vec::new();
        for index in span.indices() {
            let member = if include_root && index == 0 {
                Some(CompositeEntryHandle::Family(self.root.clone()))
            } else {
                let rank = if include_root {
                    index.checked_sub(1)
                } else {
                    Some(index)
                };
                let Some(rank) = rank.and_then(|value| usize::try_from(value).ok()) else {
                    return Ok(None);
                };
                match &self.captured_members {
                    Some(view) => view.member_at(rank)?,
                    None => self.root.member_at(rank)?,
                }
            };
            let Some(member) = member else {
                return Ok(None);
            };
            selected.push(member);
        }
        Ok(Some(selected))
    }
}

#[derive(Clone, Copy)]
struct SliceSpan {
    start: i64,
    stop: i64,
    step: NonZeroI64,
}
impl SliceSpan {
    fn normalize(start: Option<i64>, stop: Option<i64>, step: NonZeroI64, count: usize) -> Self {
        let count = i64::try_from(count).unwrap_or(i64::MAX);
        let backwards = step.get() < 0;
        let bound = |value: Option<i64>, is_stop: bool| match value {
            None => {
                if backwards {
                    if is_stop {
                        -1
                    } else {
                        count - 1
                    }
                } else if is_stop {
                    count
                } else {
                    0
                }
            }
            Some(value) => {
                let value = if value < 0 {
                    value.saturating_add(count)
                } else {
                    value
                };
                if backwards {
                    value.clamp(-1, count - 1)
                } else {
                    value.clamp(0, count)
                }
            }
        };
        Self {
            start: bound(start, false),
            stop: bound(stop, true),
            step,
        }
    }
    fn is_empty(self) -> bool {
        if self.step.get() > 0 {
            self.start >= self.stop
        } else {
            self.start <= self.stop
        }
    }
    fn contains_zero(self) -> bool {
        let spans_zero = if self.step.get() > 0 {
            self.start <= 0 && self.stop > 0
        } else {
            self.start >= 0 && self.stop < 0
        };
        spans_zero && -(self.start as i128) % (self.step.get() as i128) == 0
    }
    fn indices(self) -> impl Iterator<Item = i64> {
        let step = self.step.get() as i128;
        std::iter::successors(Some(self.start as i128), move |value| Some(value + step))
            .take_while(move |value| {
                if step > 0 {
                    *value < self.stop as i128
                } else {
                    *value > self.stop as i128
                }
            })
            .map(|value| value as i64)
    }
}

#[cfg(all(test, feature = "diagnostics"))]
mod reference_tests;

/// Disposable index of zero-paint spans in one packed buffer. Source paint is
/// authoritative; only uploaded dirty rows update this cache. Coalesced spans
/// make an all-transparent batch O(log spans), not a per-frame instance scan.
#[derive(Debug, Default)]
pub(crate) struct ZeroContributionRanges {
    ranges: std::collections::BTreeMap<u32, u32>,
}

impl ZeroContributionRanges {
    pub(crate) fn set(&mut self, mut span: Range<u32>, zero: bool) {
        if span.is_empty() {
            return;
        }
        let previous = self.ranges.range(..=span.start).next_back().map(|(&a, &b)| (a, b));
        if zero {
            if let Some((start, end)) = previous {
                if end >= span.end {
                    return;
                }
                if end >= span.start {
                    self.ranges.remove(&start);
                    span.start = start;
                }
            }
            while let Some((&start, &end)) = self.ranges.range(span.start..).next() {
                if start > span.end {
                    break;
                }
                self.ranges.remove(&start);
                span.end = span.end.max(end);
            }
            self.ranges.insert(span.start, span.end);
        } else {
            if let Some((start, end)) = previous.filter(|&(start, end)| start < span.start && end > span.start) {
                *self.ranges.get_mut(&start).expect("retained preceding span") = span.start;
                if end > span.end {
                    self.ranges.insert(span.end, end);
                    return;
                }
            }
            while let Some((&start, &end)) = self.ranges.range(span.start..span.end).next() {
                self.ranges.remove(&start);
                if end > span.end {
                    self.ranges.insert(span.end, end);
                    break;
                }
            }
        }
    }

    fn visible_ranges(zero: Option<&Self>, span: Range<u32>) -> impl Iterator<Item = Range<u32>> + '_ {
        let mut cursor = span.start;
        let end = span.end;
        let start = zero.and_then(|zero| zero.ranges.range(..=cursor).next_back().map(|(&start, _)| start)).unwrap_or(cursor);
        let mut ranges = zero.into_iter().flat_map(move |zero| zero.ranges.range(start..end));
        std::iter::from_fn(move || {
            for (&start, &stop) in ranges.by_ref() {
                let visible_start = cursor;
                let visible_end = start.min(end);
                cursor = cursor.max(stop.min(end));
                if visible_start < visible_end {
                    return Some(visible_start..visible_end);
                }
            }
            let start = cursor;
            cursor = end;
            (start < end).then_some(start..end)
        })
    }
}

impl FramePreparer {
    /// Only already-dirty packed rows are visited. A clean frame does no work;
    /// opacity changes never erase source painter anchors or rebuild their order.
    pub(crate) fn sync_zero_contribution(&mut self) {
        for range in &self.circle_dirty_ranges {
            for index in range.clone() {
                self.zero_contribution[0].set(packed_row_span(index), !self.circles[index].style.may_contribute_color());
            }
        }
        for range in &self.rectangle_dirty_ranges {
            for index in range.clone() {
                self.zero_contribution[1].set(packed_row_span(index), !self.rectangles[index].style.may_contribute_color());
            }
        }
        for range in &self.line_dirty_ranges {
            for index in range.clone() {
                self.zero_contribution[2].set(packed_row_span(index), !self.lines[index].style.may_contribute_color());
            }
        }
        for range in &self.path_dirty_ranges {
            for index in range.clone() {
                self.zero_contribution[3].set(packed_row_span(index), !self.paths[index].style.may_contribute_color());
            }
        }
    }

    pub(crate) fn set_mega_zero_contribution(&mut self, range: Range<u32>, style: crate::PackedStyle) {
        self.zero_contribution[4].set(range, !style.may_contribute_color());
    }
}

fn packed_row_span(index: usize) -> Range<u32> {
    u32::try_from(index).expect("packed instance index exceeds renderer limits")
        ..u32::try_from(index + 1).expect("packed instance count exceeds renderer limits")
}

impl PreparedFrame<'_> {
    /// Preserve source descriptors (including transient anchors), but submit
    /// only contributing instance spans at the shared GPU draw boundary.
    pub(crate) fn contributing_instance_ranges(&self, batch: &OrderedRenderBatch) -> impl Iterator<Item = Range<u32>> + '_ {
        let zero = match batch.primitive {
            RenderPrimitive::Circle => Some(&self.zero_contribution[0]),
            RenderPrimitive::Rectangle => Some(&self.zero_contribution[1]),
            RenderPrimitive::Line => Some(&self.zero_contribution[2]),
            RenderPrimitive::Path { .. } => Some(&self.zero_contribution[3]),
            // Packed mega draws are filtered in index units, below.
            RenderPrimitive::MegaPath { .. } => None,
        };
        ZeroContributionRanges::visible_ranges(zero, batch.instance_range.clone())
    }

    pub(crate) fn contributing_mega_index_ranges(&self, span: Range<u32>) -> impl Iterator<Item = (Range<u32>, usize)> + '_ {
        ZeroContributionRanges::visible_ranges(Some(&self.zero_contribution[4]), span).map(|span| {
            let start = self.mega_path_offsets.partition_point(|&offset| offset < span.start);
            let end = self.mega_path_offsets.partition_point(|&offset| offset < span.end);
            (span, end - start)
        })
    }
}

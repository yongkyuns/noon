from pathlib import Path
import os

root = Path('crates/noon-render-wgpu/src')
def replace(text, old, new):
    assert text.count(old) == 1, (old, text.count(old))
    return text.replace(old, new, 1)

p = root / 'lib.rs'
s = p.read_text()
s = replace(s, 'impl From<Style> for PackedStyle {', '''impl PackedStyle {
    /// Exact, conservative source-over eligibility using effective packed paint.
    /// No epsilon: every nonzero alpha remains eligible, including tiny fades.
    pub(crate) fn may_contribute_color(self) -> bool {
        if !self.opacity.is_finite()
            || !self.stroke_width.is_finite()
            || !self.fill.iter().chain(self.stroke.iter()).all(|v| v.is_finite())
        {
            return true;
        }
        self.opacity != 0.0
            && ((self.fill_enabled != 0 && self.fill[3] != 0.0)
                || (self.stroke_enabled & 1 != 0 && self.stroke[3] != 0.0))
    }
}

impl From<Style> for PackedStyle {''')
s = replace(s, '            submission_membership: self.complete_submission.then_some(true),', '''            submission_membership: self.complete_submission.then(|| {
                slot.may_contribute_color(self.circles, self.rectangles, self.lines, self.paths)
            }),''')
s = replace(s, 'const fn prepared_slot_instance_count(slot: PreparedSlot) -> usize {', '''impl PreparedSlot {
    fn may_contribute_color(
        self,
        circles: &[CircleInstance],
        rectangles: &[RectangleInstance],
        lines: &[LineInstance],
        paths: &[PathInstance],
    ) -> bool {
        match self {
            Self::Absent | Self::Unsupported(_) => false,
            Self::Circle(index) => circles[index].style.may_contribute_color(),
            Self::Rectangle(index) => rectangles[index].style.may_contribute_color(),
            Self::Line(index) => lines[index].style.may_contribute_color(),
            Self::Path { index, reveal_head, .. } => {
                paths[index].style.may_contribute_color()
                    || reveal_head.is_some_and(|head| lines[head].style.may_contribute_color())
            }
        }
    }
}

const fn prepared_slot_instance_count(slot: PreparedSlot) -> usize {''')
s = replace(s, '''        for chunk in replacement_chunks {
            let start = chunk * Self::RENDER_ORDER_CHUNK_SIZE;
            self.rebuild_render_order_chunks(Some(start..start + Self::RENDER_ORDER_CHUNK_SIZE));
        }

''', '')
s = replace(s, '''            let object = &frame.objects[object_index];
            match self.slots[object_index] {''', '''            let prior_draw_slot = self.color_contributing_slot(self.slots[object_index]);
            let object = &frame.objects[object_index];
            match self.slots[object_index] {''')
s = replace(s, '''                PreparedSlot::Unsupported(_) => {}
            }
        }

        if let Some(range) = changes.painter_order_range() {''', '''                PreparedSlot::Unsupported(_) => {}
            }
            if prior_draw_slot != self.color_contributing_slot(self.slots[object_index]) {
                self.record_render_order_chunk(object_index, &mut replacement_chunks);
            }
        }

        // Eligibility must see the newly packed effective style. Reuse bounded
        // painter partitions, never a whole-scene rebuild for an opacity change.
        for chunk in replacement_chunks {
            let start = chunk * Self::RENDER_ORDER_CHUNK_SIZE;
            self.rebuild_render_order_chunks(Some(start..start + Self::RENDER_ORDER_CHUNK_SIZE));
        }

        if let Some(range) = changes.painter_order_range() {''')
s = replace(s, '        let appended_to_mega = mega_eligible && self.append_mega_path_draw(batch, packed);', '''        let appended_to_mega = mega_eligible
            && self.color_contributing_slot(slot) != PreparedSlot::Absent
            && self.append_mega_path_draw(batch, packed);''')
p.write_text(s)

p = root / 'render_order.rs'
s = p.read_text()
s = replace(s, '''impl FramePreparer {
    pub(crate) fn append_ordered_render_slot''', '''impl FramePreparer {
    /// Filter only derived submission metadata, not identity or runtime presence.
    pub(crate) fn color_contributing_slot(&self, slot: PreparedSlot) -> PreparedSlot {
        if slot.may_contribute_color(&self.circles, &self.rectangles, &self.lines, &self.paths) {
            slot
        } else {
            PreparedSlot::Absent
        }
    }

    pub(crate) fn append_ordered_render_slot''')
s = replace(s, '''        push_slot_batches(&mut self.render_batches, slot);''', '''        let slot = self.color_contributing_slot(slot);
        push_slot_batches(&mut self.render_batches, slot);''')
s = replace(s, '        push_batch(&mut self.render_batches, RenderPrimitive::Line, line_index);', '''        if self.lines[line_index].style.may_contribute_color() {
            push_batch(&mut self.render_batches, RenderPrimitive::Line, line_index);
        }''')
s = replace(s, '                let slot = self.slots[object_index];', '                let slot = self.color_contributing_slot(self.slots[object_index]);')
s = replace(s, 'cached.slot != self.slots[object_index]', 'cached.slot != self.color_contributing_slot(self.slots[object_index])')
s = replace(s, '                    push_slot_batches(&mut raw_render_batches, self.slots[object_index]);', '''                    push_slot_batches(&mut raw_render_batches, self.color_contributing_slot(self.slots[object_index]));''')
s = replace(s, '''                        push_slot_batches(&mut self.render_batches, slot);''', '''                        let slot = self.color_contributing_slot(slot);
                        push_slot_batches(&mut self.render_batches, slot);''')
s = replace(s, '''            if self.slot_presences[object_index] {
                push_slot_batches(&mut self.render_batches, slot);''', '''            if self.slot_presences[object_index] {
                let slot = self.color_contributing_slot(slot);
                push_slot_batches(&mut self.render_batches, slot);''')
s = replace(s, '                        push_slot_batches(&mut raw, slot);', '                        push_slot_batches(&mut raw, self.color_contributing_slot(slot));')
s = replace(s, 'fn push_slot_batches(', '''impl PreparedFrame<'_> {
    /// Mixed-content source membership survives fades. Filter current packed
    /// ranges during its existing submission walk instead of rebuilding text or
    /// geometry order. Geometry-only frames use retained painter partitions.
    pub(crate) fn contributing_instance_ranges(
        &self,
        batch: &OrderedRenderBatch,
    ) -> impl Iterator<Item = Range<u32>> + '_ {
        let primitive = batch.primitive;
        let mut remaining = batch.instance_range.clone();
        std::iter::from_fn(move || {
            let contributes = |index: u32| match primitive {
                RenderPrimitive::Circle => self.circles[index as usize].style.may_contribute_color(),
                RenderPrimitive::Rectangle => self.rectangles[index as usize].style.may_contribute_color(),
                RenderPrimitive::Line => self.lines[index as usize].style.may_contribute_color(),
                RenderPrimitive::Path { .. } => self.paths[index as usize].style.may_contribute_color(),
                // Mega streams are already filtered by retained painter projection.
                RenderPrimitive::MegaPath { .. } => true,
            };
            while remaining.start < remaining.end && !contributes(remaining.start) {
                remaining.start += 1;
            }
            let start = remaining.start;
            while remaining.start < remaining.end && contributes(remaining.start) {
                remaining.start += 1;
            }
            (start < remaining.start).then_some(start..remaining.start)
        })
    }
}

fn push_slot_batches(''')
p.write_text(s)

p = root / 'gpu/retained_text.rs'
s = p.read_text()
s = replace(s, '''                        stats.geometry += self.draw_resolved_ordered_batch(
                            pass,
                            &prepared.geometry,
                            &super::ResolvedOrderedBatch {
                                batch: batch.clone(),
                                mega,
                            },
                            sample_count == 1,
                            &mut binding,
                        );''', '''                        for instance_range in prepared.geometry.contributing_instance_ranges(batch) {
                            stats.geometry += self.draw_resolved_ordered_batch(
                                pass,
                                &prepared.geometry,
                                &super::ResolvedOrderedBatch {
                                    batch: OrderedRenderBatch { primitive: batch.primitive, instance_range },
                                    mega: mega.clone(),
                                },
                                sample_count == 1,
                                &mut binding,
                            );
                        }''')
s = replace(s, '            submission_membership: !items.is_empty(),', '''            submission_membership: items.iter().any(|item| match item {
                RetainedRenderItem::Geometry { batch, .. } => {
                    self.geometry.contributing_instance_ranges(batch).next().is_some()
                }
                _ => true,
            }),''')
p.write_text(s)

p = root / 'gpu/mod.rs'
s = p.read_text()
start = s.index('        let mut stats = DrawStats::default();', s.index("    pub fn draw<'a>("))
end = s.index('\n    }', start) + len('\n    }')
s = s[:start] + '        self.draw_ordered(pass, prepared, false)\n    }' + s[end:]
p.write_text(s)

p = Path('docs/architecture.md')
s = p.read_text()
s = replace(s, '`noon-render-wgpu` owns reusable retained GPU rendering.', '''Zero-contribution 2D geometry may be omitted from draw submission using effective
packed paint, without changing runtime presence or retiring its resident slot.
Identity, camera transforms, bounds, callbacks and independently visible family
members remain live. Zero/nonzero transitions restore draw eligibility from the
current publication, not a later full rebuild. Unknown paint stays conservative;
no visual epsilon defines invisibility.

`noon-render-wgpu` owns reusable retained GPU rendering.''')
p.write_text(s)

assets = Path(os.environ['TASK_ASSETS'])
for target, asset in [('render_order.rs', 'order-tests.rs'), ('gpu/retained_text.rs', 'mixed-tests.rs')]:
    p = root / target
    s = p.read_text().rstrip()
    assert s.endswith('}')
    p.write_text(s[:-1] + '\n' + (assets / asset).read_text() + '\n}\n')
p = Path('crates/noon-render-wgpu/tests/secondary_viewport.rs')
p.write_text(p.read_text() + '\n' + (assets / 'raster-test.rs').read_text())

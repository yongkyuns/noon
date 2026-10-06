from pathlib import Path
import subprocess, os
root = Path.cwd()
assets = Path(os.environ['TASK_ASSETS'])
# Reuse the compiled proof and fixtures from the exact prior candidate.
old_lib = (root/'crates/noon-render-wgpu/src/lib.rs').read_text()
packed = old_lib[old_lib.index('impl PackedStyle {'):old_lib.index('impl From<Style> for PackedStyle {')]
slot = old_lib[old_lib.index('impl PreparedSlot {'):old_lib.index('const fn prepared_slot_instance_count')]
old_mixed = (root/'crates/noon-render-wgpu/src/gpu/retained_text.rs').read_text()
start = old_mixed.index('    #[test]\n    fn zero_contribution_mixed_camera')
end = old_mixed.index('    use noon_compile', start)
mixed_test = old_mixed[start:end]

def base(path):
    return subprocess.check_output(['git','show',os.environ['BASE_SHA']+':'+path],cwd=root,text=True)
def replace(s, old, new):
    if old.startswith(' '):
        old, new = '\n' + old, '\n' + new
    assert s.count(old)==1, (old, s.count(old))
    return s.replace(old,new,1)
def save(path,s):
    (root/path).write_text(s)

p='crates/noon-render-wgpu/src/lib.rs';s=base(p)
s=replace(s,'impl From<Style> for PackedStyle {',packed+'impl From<Style> for PackedStyle {')
s=replace(s,'const fn prepared_slot_instance_count(slot: PreparedSlot) -> usize {',slot+'const fn prepared_slot_instance_count(slot: PreparedSlot) -> usize {')
s=replace(s,'            submission_membership: self.complete_submission.then_some(true),','            submission_membership: self.complete_submission.then(|| slot.may_contribute_color(self.circles, self.rectangles, self.lines, self.paths)),')
s=replace(s,'    complete_submission: bool,',"    complete_submission: bool,\n    zero_contribution: &'a [render_order::ZeroContributionRanges; 5],\n    mega_path_offsets: &'a [u32],")
s=replace(s,'    mega_path_segments: Vec<Option<Range<u32>>>,','    mega_path_segments: Vec<Option<Range<u32>>>,\n    mega_path_offsets: Vec<u32>,\n    zero_contribution: [render_order::ZeroContributionRanges; 5],')
s=replace(s,'        normalize_dirty_ranges(&mut self.mega_path_index_dirty_ranges);','        normalize_dirty_ranges(&mut self.mega_path_index_dirty_ranges);\n        self.sync_zero_contribution();')
s=replace(s,'        self.active_instance_count = 0;\n        self.clear_dirty_ranges();','        self.active_instance_count = 0;\n        self.zero_contribution = Default::default();\n        self.clear_dirty_ranges();')
s=replace(s,'        self.initialized = true;','        self.sync_zero_contribution();\n        self.initialized = true;')
s=replace(s,'            complete_submission: true,','            complete_submission: true,\n            zero_contribution: &self.zero_contribution,\n            mega_path_offsets: &self.mega_path_offsets,')
s=replace(s,'    fn capacities(&self) -> [usize; 33] {','    fn capacities(&self) -> [usize; 34] {')
s=replace(s,'            self.mega_path_segments.capacity(),','            self.mega_path_segments.capacity(),\n            self.mega_path_offsets.capacity(),')
s=s.replace('Live primitive instances referenced by the current submission projection.', 'Present resident primitive instances, before zero-contribution suppression.')
save(p,s)
p='crates/noon-render-wgpu/src/render_order.rs';s=base(p)
s=replace(s,'        complete_submission: false,','        complete_submission: false,\n        zero_contribution: &preparer.zero_contribution,\n        mega_path_offsets: &preparer.mega_path_offsets,')
s=replace(s,'fn push_slot_batches(', (assets/'eligibility.rs').read_text()+'\nfn push_slot_batches(')
save(p,s)
p='crates/noon-render-wgpu/src/path_residency.rs';s=base(p)
s=replace(s,'            complete_submission: false,','            complete_submission: false,\n            zero_contribution: &self.zero_contribution,\n            mega_path_offsets: &self.mega_path_offsets,')
save(p,s)
p='crates/noon-render-wgpu/src/mega_mesh.rs';s=base(p)
s=replace(s,'        self.mega_path_segments.clear();','        self.mega_path_segments.clear();\n        self.mega_path_offsets.clear();')
s=replace(s,'            self.mega_path_segments[path_batch_index] = Some(packed_start..packed_end);','''            let segment = packed_start..packed_end;
            self.mega_path_segments[path_batch_index] = Some(segment.clone());
            self.mega_path_offsets.push(packed_start);
            // The painter traversal borrows render_batches; update disjoint cache fields.
            self.zero_contribution[4].set(segment, !self.paths[path_batch.instance_range.start as usize].style.may_contribute_color());''')
s=replace(s,'        self.mega_path_segments[path_batch_index] = Some(segment.clone());','        self.mega_path_segments[path_batch_index] = Some(segment.clone());\n        self.mega_path_offsets.push(segment_start);\n        self.set_mega_zero_contribution(segment.clone(), packed.style);')
s=replace(s,'        let Some(vertex_range) = self.path_batch_vertex_ranges.get(path_batch_index) else {','''        let segment = self.mega_path_segments[path_batch_index].clone().expect("checked mega segment");
        self.set_mega_zero_contribution(segment, packed.style);
        let Some(vertex_range) = self.path_batch_vertex_ranges.get(path_batch_index) else {''')
save(p,s)
p='crates/noon-render-wgpu/src/gpu/mod.rs';s=base(p)
marker="    fn draw_resolved_ordered_batch<'a>("
wrapper='''    fn draw_resolved_ordered_batch<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        prepared: &PreparedFrame<'_>,
        resolved: &ResolvedOrderedBatch,
        single_sample_analytics: bool,
        binding: &mut Option<GeometryBinding>,
    ) -> DrawStats {
        let mut stats = DrawStats::default();
        for instance_range in prepared.contributing_instance_ranges(&resolved.batch) {
            let segment = ResolvedOrderedBatch {
                batch: OrderedRenderBatch { primitive: resolved.batch.primitive, instance_range },
                mega: resolved.mega.clone(),
            };
            stats += self.draw_contributing_ordered_batch(pass, prepared, &segment, single_sample_analytics, binding);
        }
        stats
    }

'''
s=replace(s,marker,wrapper+"    fn draw_contributing_ordered_batch<'a>(")
s=replace(s,'''                pass.draw_indexed(mega.index_range.clone(), 0, 0..1);
                return DrawStats {
                    draw_calls: 1,
                    instances_drawn: mega.path_count,
                };''','''                let mut stats = DrawStats::default();
                for (index_range, path_count) in prepared.contributing_mega_index_ranges(mega.index_range.clone()) {
                    pass.draw_indexed(index_range, 0, 0..1);
                    stats.draw_calls += 1;
                    stats.instances_drawn += path_count;
                }
                return stats;''')
start=s.index('        let mut stats = DrawStats::default();',s.index("    pub fn draw<'a>("));end=s.index('\n    }',start)+len('\n    }')
s=s[:start]+'        self.draw_ordered(pass, prepared, false)\n    }'+s[end:]
save(p,s)
p='crates/noon-render-wgpu/src/gpu/retained_text.rs';s=base(p)
s=replace(s,'            submission_membership: !items.is_empty(),','''            submission_membership: items.iter().any(|item| match item {
                RetainedRenderItem::Geometry { batch, .. } => self.geometry.contributing_instance_ranges(batch).next().is_some(),
                _ => true,
            }),''')
s=replace(s,'#[cfg(test)]\nmod tests {\n','#[cfg(test)]\nmod tests {\n'+mixed_test+'\n')
save(p,s)
p='docs/architecture.md';s=base(p)
s=replace(s,'`noon-render-wgpu` owns reusable retained GPU rendering.','''Zero-contribution geometry is suppressed only at shared GPU draw submission,
using an exact derived index of effective packed paint. Semantic presence, resident
slots and source painter anchors remain intact, including anchors for independently
visible transient effects. Only dirty packed rows update the coalesced exclusion
index; clean frames do not scan instances or rebuild painter order. Packed unique
paths use index spans at existing mesh boundaries. Opacity restoration uses the
current publication; no visibility epsilon or host-side camera workaround exists.

`noon-render-wgpu` owns reusable retained GPU rendering.''')
save(p,s)
for target, asset in [('crates/noon-render-wgpu/src/render_order.rs','order-tests-v2.rs'), ('crates/noon-render-wgpu/src/gpu/derived_display.rs','transient-test.rs')]:
    s = (root/target).read_text() if target.endswith('render_order.rs') else base(target)
    s = replace(s, '#[cfg(test)]\nmod tests {\n', '#[cfg(test)]\nmod tests {\n' + (assets/asset).read_text() + '\n')
    save(target, s)

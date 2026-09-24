"""Validation only: reconstruct the same two-file candidate for both controls."""
from pathlib import Path
import subprocess
import sys

BASE = '3546b230d3e0b012b21444946b896d98d21e95c6'
p = Path('crates/noon-render-wgpu/src/gpu/retained_text/family_plan_set_prepare.rs')
source = subprocess.check_output(['git', 'show', f'{BASE}:{p}']).decode()
assert subprocess.check_output(['git', 'hash-object', '--stdin'], input=source.encode()).decode().strip() == 'ded5bd18291e751397ad899ff25f4551b89bd5d7'
assert sys.argv[1] in ('negative', 'fixed')
old = '''        let scratch_changes = FrameChanges::objects(scratch_changes);
        self.project_mixed_visibility(frame.retained, visible_object_indices);
        let geometry = self
            .geometry
            .prepare_incremental(&self.scratch, &scratch_changes);
'''
new = '''        let scratch_changes = FrameChanges::objects(scratch_changes);
        if let Some(indices) = visible_object_indices {
            self.project_image_visibility(indices);
        }
        let geometry = self
            .geometry
            .prepare_incremental(&self.scratch, &scratch_changes);
        // A stable family plan does not imply stable child packing. Analytic
        // reveal endpoints and shared glyph-mesh changes may rebuild geometry,
        // invalidating the path/instance indices retained by the mixed stream.
        if geometry.stats.full_rebuilds > 0 {
            self.render_items.clear();
            rebuild_mixed_order(
                &mut self.render_items,
                &self.sources,
                &self.snapshot_text_items,
                &geometry,
            );
            reorder_mixed_items(&mut self.render_items, frame.retained, &self.painter_order_indices);
            rebuild_render_item_ranges(&mut self.render_item_ranges, &self.render_items);
            self.incremental_stats.mixed_order_rebuilds = self
                .incremental_stats
                .mixed_order_rebuilds
                .saturating_add(1);
        } else if let Some(range) = changes.painter_order_range() {
            reorder_mixed_items_range(
                &mut self.render_items,
                &mut self.render_item_ranges,
                frame.retained,
                &self.painter_order_indices,
                range,
            );
            self.incremental_stats.mixed_order_rebuilds = self
                .incremental_stats
                .mixed_order_rebuilds
                .saturating_add(1);
        }
        // Visibility candidates can be unchanged while their path indices moved.
        // Project only after repairing the canonical mixed painter stream.
        if let Some(indices) = visible_object_indices {
            if let Some(projected) = project_mixed_visibility_cached(
                frame.retained,
                indices,
                &self.render_items,
                &self.render_item_ranges,
                &mut self.visible_projection_ready,
                &mut self.visible_projection_candidates,
                &mut self.visible_render_items,
            ) {
                self.visibility_stats.record(indices.len(), projected);
            }
        }
'''
assert source.count(old) == 1
if sys.argv[1] == 'fixed':
    source = source.replace(old, new)
p.write_text(source + '\n#[cfg(test)]\nmod tests;\n')
print('Reconstructed family painter candidate:', sys.argv[1])

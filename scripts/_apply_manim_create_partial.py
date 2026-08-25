from pathlib import Path

path = Path("crates/noon-render-wgpu/src/lib.rs")
text = path.read_text()

old = '''    Path {
        index: usize,
        batch: usize,
        analytic_reveal: Option<AnalyticRevealKey>,
        reveal_head: Option<usize>,
    },
'''
new = '''    Path {
        index: usize,
        batch: usize,
        analytic_reveal: Option<AnalyticRevealKey>,
        partial_reveal_bits: Option<u32>,
        reveal_head: Option<usize>,
    },
'''
assert text.count(old) == 1, "PreparedSlot::Path anchor mismatch"
text = text.replace(old, new)

old = '''                PreparedSlot::Path {
                    index,
                    batch,
                    reveal_head,
                    ..
                } => {
                    let reveal = frame.reveal(object_index);
                    let packed = pack_path(object, reveal, frame.morph(object_index));
                    instances_repacked += 1;
                    if self.paths[index] != packed {
                        self.paths[index] = packed;
                        push_dirty_range(&mut self.path_dirty_ranges, index);
                        self.update_mega_path_instance(batch, packed);
                    }
                    if let Some(head_index) = reveal_head {
                        let cache_index = self.path_batch_cache_indices[batch];
                        let packed_head = pack_path_reveal_head(
                            object,
                            &self.path_mesh_cache[cache_index].mesh,
                            reveal,
                        );
                        instances_repacked += 1;
                        if self.lines[head_index] != packed_head {
                            self.lines[head_index] = packed_head;
                            push_dirty_range(&mut self.line_dirty_ranges, head_index);
                        }
                    }
                }
'''
new = '''                PreparedSlot::Path {
                    index,
                    batch,
                    partial_reveal_bits,
                    reveal_head,
                    ..
                } => {
                    let reveal = frame.reveal(object_index);
                    let packed = if partial_reveal_bits.is_some() {
                        // The temporary geometry already *is* Manim's partial VMobject.
                        // Never apply the legacy path shader reveal a second time.
                        pack_path(object, 1.0, 0.0)
                    } else {
                        pack_path(object, reveal, frame.morph(object_index))
                    };
                    instances_repacked += 1;
                    if self.paths[index] != packed {
                        self.paths[index] = packed;
                        push_dirty_range(&mut self.path_dirty_ranges, index);
                        self.update_mega_path_instance(batch, packed);
                    }
                    if let Some(head_index) = reveal_head {
                        let cache_index = self.path_batch_cache_indices[batch];
                        let packed_head = pack_path_reveal_head(
                            object,
                            &self.path_mesh_cache[cache_index].mesh,
                            reveal,
                        );
                        instances_repacked += 1;
                        if self.lines[head_index] != packed_head {
                            self.lines[head_index] = packed_head;
                            push_dirty_range(&mut self.line_dirty_ranges, head_index);
                        }
                    }
                }
'''
assert text.count(old) == 1, "incremental Path branch anchor mismatch"
text = text.replace(old, new)

old_start = text.index("    fn can_replace_unique_path_geometry(")
old_end = text.index("\n    fn rebuild<'a>", old_start)
old_block = text[old_start:old_end]
new_block = '''    fn can_replace_unique_path_geometry(&self, frame: &FrameState, object_index: usize) -> bool {
        let Some(object) = frame.objects.get(object_index) else {
            return false;
        };
        if !frame.is_present(object_index) {
            return false;
        }
        let Some(PreparedSlot::Path { index, batch, .. }) = self.slots.get(object_index) else {
            return false;
        };
        let Some(path_batch) = self.path_batches.get(*batch) else {
            return false;
        };
        let render_geometry = frame.render_geometry(object_index);
        let reveal = frame.reveal(object_index);
        let has_replacement_path = temporary_reveal_path(render_geometry, reveal).is_some()
            || matches!(render_geometry, GeometryRef::VectorPath(_));
        path_batch.instance_range.end == path_batch.instance_range.start + 1
            && self.path_ids.get(*index) == Some(&object.id)
            && has_replacement_path
    }

    fn replace_unique_path_geometry(
        &mut self,
        frame: &FrameState,
        object_index: usize,
    ) -> Result<PathReplacementStats, noon_geometry::GeometryError> {
        let object = &frame.objects[object_index];
        let render_geometry = frame.render_geometry(object_index);
        let reveal = frame.reveal(object_index);
        let temporary_reveal = temporary_reveal_path(render_geometry, reveal);
        let path = temporary_reveal
            .as_ref()
            .map(|(_, path)| path)
            .or(match render_geometry {
                GeometryRef::VectorPath(path) => Some(path),
                _ => None,
            })
            .expect("unique path replacement preflight requires renderable path geometry");
        let PreparedSlot::Path { batch, .. } = self.slots[object_index] else {
            unreachable!("unique path replacement preflight requires a path slot");
        };
        let (cache_index, cache_miss) = self.cache_path_mesh(path, object.style)?;
        let mesh = &self.path_mesh_cache[cache_index].mesh;
        let packed_vertices = mesh
            .vertices
            .iter()
            .map(|vertex| PathVertex {
                position: [vertex.position.x, vertex.position.y],
                target_position: [vertex.target_position.x, vertex.target_position.y],
                surface: pack_path_surface(vertex.surface, vertex.path_progress),
            })
            .collect::<Vec<_>>();
        let local_indices = mesh.indices.clone();

        let old_vertex_range = self.path_batch_vertex_ranges[batch].clone();
        let old_index_range = self.path_batches[batch].index_range.clone();
        let vertex_range = allocate_replacement_range(
            old_vertex_range,
            packed_vertices.len(),
            &mut self.path_vertex_free_ranges,
            self.path_vertices.len(),
        );
        let index_range = allocate_replacement_range(
            old_index_range,
            local_indices.len(),
            &mut self.path_index_free_ranges,
            self.path_indices.len(),
        );

        let vertex_range_usize = range_usize_u32(&vertex_range);
        if self.path_vertices.len() < vertex_range_usize.end {
            self.path_vertices
                .resize(vertex_range_usize.end, PathVertex::zeroed());
        }
        self.path_vertices[vertex_range_usize.clone()].copy_from_slice(&packed_vertices);
        self.path_vertex_dirty_ranges.push(vertex_range_usize);

        let index_range_usize = range_usize_u32(&index_range);
        if self.path_indices.len() < index_range_usize.end {
            self.path_indices.resize(index_range_usize.end, 0);
        }
        let vertex_start = vertex_range.start;
        for (target, local) in self.path_indices[index_range_usize.clone()]
            .iter_mut()
            .zip(local_indices.iter().copied())
        {
            *target = local
                .checked_add(vertex_start)
                .expect("path index exceeds renderer limits");
        }
        self.path_index_dirty_ranges.push(index_range_usize);

        self.path_batch_vertex_ranges[batch] = vertex_range;
        self.path_batches[batch].index_range = index_range;
        self.path_batch_cache_indices[batch] = cache_index;
        if let PreparedSlot::Path {
            analytic_reveal,
            partial_reveal_bits,
            reveal_head,
            ..
        } = &mut self.slots[object_index]
        {
            *analytic_reveal = temporary_reveal.as_ref().map(|(key, _)| *key);
            *partial_reveal_bits = temporary_reveal
                .as_ref()
                .map(|_| reveal.clamp(0.0, 1.0).to_bits());
            *reveal_head = None;
        }
        self.detach_mega_path(batch);
        self.path_geometry_dirty = true;
        Ok(PathReplacementStats {
            cache_miss,
            vertices_repacked: packed_vertices.len(),
            indices_repacked: local_indices.len(),
        })
    }
'''
text = text[:old_start] + new_block + text[old_end:]

old = '''                let reveal = frame.reveal(object_index);
                let index = path_groups[batch].instances.len();
                path_groups[batch].ids.push(object.id);
                path_groups[batch].instances.push(pack_path(
                    object,
                    reveal,
                    frame.morph(object_index),
                ));
                let reveal_head = if should_create_path_reveal_head(object, reveal) {
                    let head_index = self.lines.len();
                    self.line_ids.push(object.id);
                    self.lines.push(pack_path_reveal_head(
                        object,
                        &self.path_mesh_cache[cache_index].mesh,
                        reveal,
                    ));
                    Some(head_index)
                } else {
                    None
                };
                self.slots.push(PreparedSlot::Path {
                    index,
                    batch,
                    analytic_reveal: temporary_reveal.as_ref().map(|(key, _)| *key),
                    reveal_head,
                });
'''
new = '''                let reveal = frame.reveal(object_index);
                let index = path_groups[batch].instances.len();
                let partial_reveal_bits = temporary_reveal
                    .as_ref()
                    .map(|_| reveal.clamp(0.0, 1.0).to_bits());
                path_groups[batch].ids.push(object.id);
                path_groups[batch].instances.push(if partial_reveal_bits.is_some() {
                    pack_path(object, 1.0, 0.0)
                } else {
                    pack_path(object, reveal, frame.morph(object_index))
                });
                let reveal_head = if partial_reveal_bits.is_none()
                    && should_create_path_reveal_head(object, reveal)
                {
                    let head_index = self.lines.len();
                    self.line_ids.push(object.id);
                    self.lines.push(pack_path_reveal_head(
                        object,
                        &self.path_mesh_cache[cache_index].mesh,
                        reveal,
                    ));
                    Some(head_index)
                } else {
                    None
                };
                self.slots.push(PreparedSlot::Path {
                    index,
                    batch,
                    analytic_reveal: temporary_reveal.as_ref().map(|(key, _)| *key),
                    partial_reveal_bits,
                    reveal_head,
                });
'''
assert text.count(old) == 1, "rebuild Path packing anchor mismatch"
text = text.replace(old, new)

old = '''            PreparedSlot::Path {
                index,
                batch,
                analytic_reveal,
                reveal_head,
            } => {
'''
new = '''            PreparedSlot::Path {
                index,
                batch,
                analytic_reveal,
                partial_reveal_bits,
                reveal_head,
            } => {
'''
assert text.count(old) == 1, "slot_matches pattern anchor mismatch"
text = text.replace(old, new)

old = '''                    Some(expected) => {
                        frame.reveal(object_index) < 1.0
                            && analytic_reveal_key(render_geometry) == Some(*expected)
                    }
'''
new = '''                    Some(expected) => {
                        let reveal = frame.reveal(object_index).clamp(0.0, 1.0);
                        reveal < 1.0
                            && analytic_reveal_key(render_geometry) == Some(*expected)
                            && *partial_reveal_bits == Some(reveal.to_bits())
                            && temporary_reveal_path(render_geometry, reveal)
                                .is_some_and(|(_, path)| cache.path == path)
                    }
'''
assert text.count(old) == 1, "slot_matches analytic reveal anchor mismatch"
text = text.replace(old, new)

# Replace the old Circle-only fast-path regression with the actual partial-geometry contract.
old_start = text.index("    #[test]\n    fn circle_create_stays_on_the_analytic_fast_path()")
old_end = text.index("\n    #[test]\n    fn circle_and_line_create_stay_analytic_while_rectangle_uses_a_path()", old_start)
new_test = '''    #[test]
    fn circle_create_uses_partial_geometry_then_returns_to_analytic_circle() {
        let mut state = object(7, GeometryRef::circle(1.25));
        state.style.fill = Some(Color::rgba(1.0, 0.0, 0.5, 0.5));
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.08;
        let mut frame = frame(vec![state]);
        frame.reveals[0] = 0.25;
        let mut preparer = FramePreparer::new();

        let (cold_vertices, cold_indices) = {
            let cold = preparer.prepare(&frame);
            assert!(cold.circles.is_empty());
            assert_eq!(cold.paths.len(), 1);
            assert!(cold.lines.is_empty());
            assert_eq!(cold.paths[0].path_params, [1.0, 0.0]);
            assert_eq!(cold.stats.geometry_cache_misses, 1);
            assert!(cold.path_geometry_dirty);
            (cold.path_vertices.to_vec(), cold.path_indices.to_vec())
        };

        frame.reveals[0] = 0.6;
        let steady = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
        assert!(steady.circles.is_empty());
        assert_eq!(steady.paths.len(), 1);
        assert_eq!(steady.paths[0].path_params, [1.0, 0.0]);
        assert!(steady.lines.is_empty());
        assert_eq!(steady.stats.geometry_cache_misses, 1);
        assert!(steady.path_geometry_dirty);
        assert!(steady.stats.path_vertices_repacked > 0);
        assert!(steady.stats.path_indices_repacked > 0);
        assert!(steady.path_vertices != cold_vertices || steady.path_indices != cold_indices);

        frame.reveals[0] = 1.0;
        let complete = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
        assert_eq!(complete.circles.len(), 1);
        assert!(complete.paths.is_empty());
        assert_eq!(complete.circles[0].padding[0], 1.0);
        assert_eq!(complete.stats.instance_count, 1);
    }
'''
text = text[:old_start] + new_test + text[old_end:]

old_start = text.index("    #[test]\n    fn circle_and_line_create_stay_analytic_while_rectangle_uses_a_path()")
old_end = text.index("\n    #[test]\n    fn path_morph_changes_only_dirty_the_instance_record()", old_start)
new_test = '''    #[test]
    fn closed_analytic_create_uses_partial_paths_while_line_stays_analytic() {
        let mut circle = object(1, GeometryRef::circle(1.0));
        let mut rectangle = object(2, GeometryRef::rectangle(2.0, 1.0));
        let mut line = object(
            3,
            GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
        );
        for state in [&mut circle, &mut rectangle, &mut line] {
            state.style.fill = None;
            state.style.stroke = Some(Color::WHITE);
            state.style.stroke_width = 0.05;
        }
        let mut frame = frame(vec![circle, rectangle, line]);
        frame.reveals.fill(0.5);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);
        assert!(prepared.circles.is_empty());
        assert!(prepared.rectangles.is_empty());
        assert_eq!(prepared.paths.len(), 2);
        assert!(prepared.paths.iter().all(|path| path.path_params == [1.0, 0.0]));
        assert_eq!(prepared.lines.len(), 1);
        assert_eq!(prepared.lines[0].transform.padding, 0.5);
        assert_eq!(prepared.stats.instance_count, 3);
        assert_eq!(prepared.stats.unsupported_count, 0);
        assert_eq!(prepared.stats.geometry_cache_misses, 2);

        frame.reveals[2] = 0.8;
        let advanced = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![2]));
        assert_eq!(advanced.lines.len(), 1);
        assert_eq!(advanced.lines[0].transform.padding, 0.8);
        assert_eq!(advanced.stats.geometry_cache_misses, 0);
        assert_eq!(advanced.stats.instances_repacked, 1);
        assert_eq!(advanced.line_dirty_ranges.len(), 1);
        assert_eq!(advanced.line_dirty_ranges[0], 0..1);
        assert!(!advanced.path_geometry_dirty);
    }
'''
text = text[:old_start] + new_test + text[old_end:]

path.write_text(text)

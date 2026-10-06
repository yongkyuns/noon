    fn eligible_geometry_count(prepared: &PreparedFrame<'_>) -> usize {
        prepared.ordered_render_batches().map(|ordered| {
            if let Some(mega) = ordered.mega_path_batch {
                prepared.contributing_mega_index_ranges(mega.index_range.clone()).map(|(_, count)| count).sum()
            } else {
                prepared.contributing_instance_ranges(ordered.batch).map(|range| range.len()).sum()
            }
        }).sum()
    }

    #[test]
    fn zero_contribution_keeps_camera_and_painter_anchors_without_order_rebuilds() {
        let mut frame = frame((0..8192).map(|id| object(id, GeometryRef::rectangle(14.222222, 8.0))).collect());
        let camera = 4096;
        frame.objects[camera].style.opacity = 0.0;
        let mut preparer = FramePreparer::new();
        let cold = preparer.prepare(&frame);
        assert_eq!(cold.rectangles.len(), 8192);
        assert_eq!(cold.observe_object(camera).unwrap().submission_membership, Some(false));
        assert_eq!(cold.ordered_render_batches().map(|b| b.batch.instance_range.len()).sum::<usize>(), 8192);
        assert_eq!(eligible_geometry_count(&cold), 8191);
        for opacity in [0.25, 1.0, 0.0, f32::MIN_POSITIVE, 0.0] {
            frame.objects[camera].style.opacity = opacity;
            frame.objects[camera].transform.translation.x += 0.125;
            let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![camera]));
            assert_eq!(prepared.stats.full_rebuilds, 0);
            assert_eq!(prepared.stats.instances_repacked, 1);
            assert_eq!(prepared.stats.geometry_cache_misses, 0);
            assert_eq!(prepared.stats.render_order_positions_visited, 0);
            assert_eq!(prepared.stats.render_order_chunks_rebuilt, 0);
            assert_eq!(prepared.observe_object(camera).unwrap().submission_membership, Some(opacity != 0.0));
            assert_eq!(prepared.rectangles.len(), 8192);
            assert_eq!(prepared.rectangle_ids[camera], frame.objects[camera].id);
            assert_eq!(prepared.rectangles[camera].transform.translation[0], frame.objects[camera].transform.translation.x);
            assert_eq!(eligible_geometry_count(&prepared), 8191 + usize::from(opacity != 0.0));
            assert_eq!(prepared.zero_contribution[1].ranges.len(), usize::from(opacity == 0.0));
        }
        let clean = preparer.prepare_incremental(&frame, &FrameChanges::default());
        assert_eq!(clean.stats.instances_repacked, 0);
        assert_eq!(clean.stats.render_order_positions_visited, 0);
        assert_eq!(clean.stats.render_order_chunks_rebuilt, 0);
        assert!(clean.rectangle_dirty_ranges.is_empty());
        assert!(frame.presences[camera]);
    }

    #[test]
    fn zero_contribution_is_exact_and_preserves_stroke_and_flag_semantics() {
        use noon_core::{Color, StrokeWidthMode};
        let mut state = object(0, GeometryRef::rectangle(2.0, 2.0));
        let mut preparer = FramePreparer::new();
        for (fill, stroke, opacity, expected) in [
            (None, None, 1.0, false),
            (Some(Color::rgba(1.0, 0.5, 0.3, 0.0)), None, 1.0, false),
            (None, Some(Color::WHITE), 1.0, true),
            (Some(Color::rgba(1.0, 1.0, 1.0, 0.0)), Some(Color::WHITE), 1.0, true),
            (Some(Color::WHITE), Some(Color::WHITE), 0.0, false),
            (Some(Color::WHITE), None, -0.0, false),
            (Some(Color::WHITE), None, f32::MIN_POSITIVE, true),
            (Some(Color::rgba(1.0, 1.0, 1.0, f32::MIN_POSITIVE)), None, 1.0, true),
            (None, None, f32::NAN, true),
            (Some(Color::rgba(f32::NAN, 0.0, 0.0, 0.0)), None, 0.0, true),
        ] {
            state.style.fill = fill;
            state.style.stroke = stroke;
            state.style.opacity = opacity;
            state.style.stroke_width = 0.1;
            state.style.stroke_width_mode = StrokeWidthMode::ScreenSpace;
            let prepared = preparer.prepare(&frame(vec![state.clone()]));
            assert_eq!(eligible_geometry_count(&prepared) != 0, expected, "style: {:?}", state.style);
        }
    }

    #[test]
    fn zero_contribution_appearance_restores_without_rebuilding_visibility_projection() {
        let mut frame = frame(vec![object(0, GeometryRef::rectangle(2.0, 2.0))]);
        let mut preparer = FramePreparer::new();
        for appearance in [0.0, 0.5, 1.0, 0.0, 1.0] {
            frame.objects[0].appearance = appearance;
            let prepared = preparer.prepare_incremental_visible(&frame, &FrameChanges::objects(vec![0]), &[0]).unwrap();
            assert_eq!(eligible_geometry_count(&prepared), usize::from(appearance != 0.0));
            assert_eq!(prepared.rectangles.len(), 1);
            assert_eq!(prepared.observe_object(0).unwrap().submission_membership, None);
            assert_eq!(preparer.visible_projection_stats().projections, 1);
        }
    }

    #[test]
    fn zero_contribution_batched_holes_keep_painter_anchors_and_independent_rows() {
        let mut frame = frame((0..5).map(|id| object(id, GeometryRef::rectangle(2.0, 2.0))).collect());
        frame.objects[1].style.opacity = 0.0;
        frame.objects[3].style.opacity = 0.0;
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);
        assert_eq!(prepared.render_batches[0].instance_range, 0..5);
        assert_eq!(prepared.contributing_instance_ranges(&prepared.render_batches[0]).collect::<Vec<_>>(), vec![0..1, 2..3, 4..5]);
        preparer.set_painter_order(&frame, &[4, 3, 2, 1, 0]);
        let prepared = preparer.prepare(&frame);
        assert_eq!(prepared.ordered_render_batches().count(), 5);
        let ranges = prepared.ordered_render_batches().flat_map(|b| prepared.contributing_instance_ranges(b.batch)).collect::<Vec<_>>();
        assert_eq!(ranges, vec![4..5, 2..3, 0..1]);
    }

    #[test]
    fn zero_contribution_reveal_primitive_transitions_agree_with_fresh_preparation() {
        use noon_core::Color;
        let mut state = object(0, GeometryRef::rectangle(2.0, 1.0));
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.1;
        let mut frame = frame(vec![state]);
        let mut preparer = FramePreparer::new();
        for (reveal, opacity) in [(1.0, 0.0), (0.25, 0.0), (0.5, 0.5), (1.0, 1.0), (1.0, 0.0), (0.75, 1.0)] {
            frame.reveals[0] = reveal;
            frame.objects[0].style.opacity = opacity;
            let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
            let mut fresh = FramePreparer::new();
            let reference = fresh.prepare(&frame);
            assert_eq!(prepared.observe_object(0).unwrap().primitive, reference.observe_object(0).unwrap().primitive);
            assert_eq!(prepared.observe_object(0).unwrap().submission_membership, Some(opacity != 0.0));
            assert_eq!(eligible_geometry_count(&prepared), eligible_geometry_count(&reference));
            assert!(frame.presences[0]);
        }
    }

    #[test]
    fn zero_contribution_interval_edits_match_dense_reference_and_coalesce() {
        let mut zero = ZeroContributionRanges::default();
        let mut expected = [false; 64];
        let mut seed = 0x1653_u32;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..1000 {
            let a = next() as usize % 65;
            let b = next() as usize % 65;
            let hidden = next() & 1 != 0;
            let span = a.min(b)..a.max(b);
            expected[span.clone()].fill(hidden);
            zero.set(span.start as u32..span.end as u32, hidden);
            let mut previous_end = None;
            for (&start, &end) in &zero.ranges {
                assert!(start < end);
                assert!(previous_end.is_none_or(|previous| previous < start));
                previous_end = Some(end);
            }
            for query_start in [0, a.min(b), a.max(b), 64] {
                let query = query_start..64;
                let actual = ZeroContributionRanges::visible_ranges(Some(&zero), query.start as u32..query.end as u32).flatten().map(|index| index as usize).collect::<Vec<_>>();
                let reference = query.filter(|&index| !expected[index]).collect::<Vec<_>>();
                assert_eq!(actual, reference);
            }
        }
        let mut huge = ZeroContributionRanges::default();
        huge.set(100..2_000_000_000, true);
        huge.set(1000..2000, false);
        assert_eq!(huge.ranges.len(), 2);
        assert_eq!(ZeroContributionRanges::visible_ranges(Some(&huge), 0..u32::MAX).collect::<Vec<_>>(), vec![0..100, 1000..2000, 2_000_000_000..u32::MAX]);
    }

    #[test]
    fn zero_contribution_mega_paths_restore_append_and_reorder_without_mesh_rebuild() {
        use noon_core::{Color, Vec2, VectorPath};
        let path_object = |id| {
            let y = id as f32 * 0.02;
            let mut state = object(id, GeometryRef::path(VectorPath::new().move_to(Vec2::new(-0.5, y)).line_to(Vec2::new(0.5, y))));
            state.style.fill = None;
            state.style.stroke = Some(Color::WHITE);
            state.style.stroke_width = 0.01;
            state
        };
        let mut frame = frame((0..5).map(path_object).collect());
        frame.objects[1].style.opacity = 0.0;
        frame.objects[3].style.opacity = 0.0;
        let mut preparer = FramePreparer::new();
        let cold = preparer.prepare(&frame);
        assert_eq!(cold.stats.mega_path_count, 5);
        assert_eq!(eligible_geometry_count(&cold), 3);
        let vertices = cold.path_vertices.to_vec();
        let indices = cold.mega_path_indices.to_vec();
        for opacity in [1.0, 0.0, 0.5] {
            frame.objects[1].style.opacity = opacity;
            let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![1]));
            assert_eq!(prepared.stats.full_rebuilds, 0);
            assert_eq!(prepared.stats.render_order_positions_visited, 0);
            assert_eq!(prepared.stats.geometry_cache_misses, 0);
            assert_eq!(eligible_geometry_count(&prepared), 3 + usize::from(opacity != 0.0));
            assert_eq!(prepared.path_vertices, vertices);
            assert_eq!(prepared.mega_path_indices, indices);
            assert!(prepared.mega_path_index_dirty_ranges.is_empty());
        }
        let mut appended = path_object(5);
        appended.style.opacity = 0.0;
        frame.objects.push(appended);
        frame.presences.push(true);
        frame.reveals.push(1.0);
        frame.morphs.push(0.0);
        frame.render_geometries.push(None);
        frame.render_transforms.push(None);
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::structural(vec![5], Vec::new()));
        assert_eq!(prepared.stats.full_rebuilds, 0);
        assert_eq!(eligible_geometry_count(&prepared), 4);
        assert_eq!(prepared.mega_path_offsets.len(), 6);
        frame.objects[5].style.opacity = 1.0;
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![5]));
        assert_eq!(eligible_geometry_count(&prepared), 5);
        assert!(prepared.mega_path_index_dirty_ranges.is_empty());
        preparer.set_painter_order(&frame, &[5, 4, 3, 2, 1, 0]);
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::painter_order(0..6));
        assert_eq!(eligible_geometry_count(&prepared), 5);
        assert!(prepared.mega_path_index_dirty_ranges.is_empty());
        frame.presences[2] = false;
        let prepared = preparer.prepare_incremental(&frame, &FrameChanges::structural(Vec::new(), vec![2]));
        assert_eq!(eligible_geometry_count(&prepared), 4);
    }

    #[test]
    fn zero_contribution_keeps_camera_resident_and_updates_only_one_partition() {
        let mut frame = frame((0..8192).map(|id| object(id, GeometryRef::rectangle(14.222222, 8.0))).collect());
        let camera = 4096;
        frame.objects[camera].style.opacity = 0.0;
        let mut preparer = FramePreparer::new();
        let cold = preparer.prepare(&frame);
        assert_eq!(cold.rectangles.len(), 8192);
        assert_eq!(cold.observe_object(camera).unwrap().submission_membership, Some(false));
        assert_eq!(cold.ordered_render_batches().map(|b| b.batch.instance_range.len()).sum::<usize>(), 8191);
        for opacity in [0.25, 1.0, 0.0, f32::MIN_POSITIVE, 0.0] {
            frame.objects[camera].style.opacity = opacity;
            frame.objects[camera].transform.translation.x += 0.125;
            let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![camera]));
            assert_eq!(prepared.stats.full_rebuilds, 0);
            assert_eq!(prepared.stats.instances_repacked, 1);
            assert_eq!(prepared.stats.geometry_cache_misses, 0);
            assert!(prepared.stats.render_order_positions_visited <= FramePreparer::RENDER_ORDER_CHUNK_SIZE);
            assert!(prepared.stats.render_order_chunks_rebuilt <= 1);
            assert_eq!(prepared.observe_object(camera).unwrap().submission_membership, Some(opacity != 0.0));
            assert_eq!(prepared.rectangles.len(), 8192);
            assert_eq!(prepared.rectangle_ids[camera], frame.objects[camera].id);
            assert_eq!(prepared.rectangles[camera].transform.translation[0], frame.objects[camera].transform.translation.x);
            assert_eq!(prepared.ordered_render_batches().map(|b| b.batch.instance_range.len()).sum::<usize>(), 8191 + usize::from(opacity != 0.0));
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
            assert_eq!(prepared.ordered_render_batches().next().is_some(), expected, "style: {:?}", state.style);
        }
    }

    #[test]
    fn zero_contribution_visibility_cache_tracks_appearance_without_stale_membership() {
        let mut frame = frame(vec![object(0, GeometryRef::rectangle(2.0, 2.0))]);
        frame.objects[0].appearance = 0.0;
        let mut preparer = FramePreparer::new();
        for (appearance, expected_projections) in [(0.0, 1), (0.5, 2), (1.0, 2), (0.0, 3), (1.0, 4)] {
            frame.objects[0].appearance = appearance;
            let prepared = preparer.prepare_incremental_visible(&frame, &FrameChanges::objects(vec![0]), &[0]).unwrap();
            assert_eq!(prepared.ordered_render_batches().next().is_some(), appearance != 0.0);
            assert_eq!(prepared.rectangles.len(), 1);
            assert_eq!(prepared.observe_object(0).unwrap().submission_membership, None);
            assert_eq!(preparer.visible_projection_stats().projections, expected_projections);
        }
    }

    #[test]
    fn zero_contribution_batched_holes_preserve_painter_order_and_independent_rows() {
        let mut frame = frame((0..5).map(|id| object(id, GeometryRef::rectangle(2.0, 2.0))).collect());
        frame.objects[1].style.opacity = 0.0;
        frame.objects[3].style.opacity = 0.0;
        let mut preparer = FramePreparer::new();
        let prepared = preparer.prepare(&frame);
        let ranges: Vec<_> = prepared.ordered_render_batches().map(|b| b.batch.instance_range.clone()).collect();
        assert_eq!(ranges, vec![0..1, 2..3, 4..5]);
        let batch = OrderedRenderBatch { primitive: RenderPrimitive::Rectangle, instance_range: 0..5 };
        assert_eq!(prepared.contributing_instance_ranges(&batch).collect::<Vec<_>>(), ranges);
        preparer.set_painter_order(&frame, &[4, 3, 2, 1, 0]);
        let prepared = preparer.prepare(&frame);
        assert_eq!(prepared.ordered_render_batches().map(|b| b.batch.instance_range.clone()).collect::<Vec<_>>(), vec![4..5, 2..3, 0..1]);
    }

    #[test]
    fn zero_contribution_paths_reappear_without_losing_resident_meshes() {
        use noon_core::{Color, Vec2, VectorPath};
        let mut state = object(0, GeometryRef::path(VectorPath::new().move_to(Vec2::new(-1.0, 0.0)).cubic_to(Vec2::new(-0.5, 1.0), Vec2::new(0.5, -1.0), Vec2::new(1.0, 0.0))));
        state.style.fill = None;
        state.style.stroke = Some(Color::WHITE);
        state.style.stroke_width = 0.1;
        state.style.opacity = 0.0;
        let mut frame = frame(vec![state]);
        let mut preparer = FramePreparer::new();
        let cold = preparer.prepare(&frame);
        let vertices = cold.path_vertices.to_vec();
        assert_eq!(cold.paths.len(), 1);
        assert_eq!(cold.ordered_render_batches().count(), 0);
        for opacity in [1.0, 0.0, 0.5] {
            frame.objects[0].style.opacity = opacity;
            let prepared = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![0]));
            assert_eq!(prepared.stats.full_rebuilds, 0);
            assert_eq!(prepared.stats.geometry_cache_misses, 0);
            assert_eq!(prepared.path_vertices, vertices);
            assert_eq!(prepared.ordered_render_batches().next().is_some(), opacity != 0.0);
            assert!(!prepared.path_geometry_dirty);
        }
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
            assert_eq!(prepared.ordered_render_batches().count(), reference.ordered_render_batches().count());
            assert!(frame.presences[0]);
        }
    }

from pathlib import Path

path = Path("crates/noon-geometry/src/morph.rs")
text = path.read_text()

text = text.replace(
    "struct FlattenedContour {\n    points: Vec<Vec2>,\n    closed: bool,\n}\n",
    "struct FlattenedContour {\n    points: Vec<Vec2>,\n    feature_indices: Vec<usize>,\n    closed: bool,\n}\n",
    1,
)

old_flatten = '''fn flatten_path(path: &VectorPath, tolerance: f32) -> Result<Vec<FlattenedContour>, GeometryError> {
    let mut contours = Vec::new();
    let mut points = Vec::new();
    let mut current = Vec2::ZERO;
    let mut start = Vec2::ZERO;
    let mut active = false;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                finite(to)?;
                if active {
                    contours.push(FlattenedContour {
                        points: std::mem::take(&mut points),
                        closed: false,
                    });
                }
                points.push(to);
                current = to;
                start = to;
                active = true;
            }
            PathCommand::LineTo { to } => {
                require_active(active)?;
                finite(to)?;
                push_distinct(&mut points, to);
                current = to;
            }
            PathCommand::QuadraticTo { control, to } => {
                require_active(active)?;
                finite(control)?;
                finite(to)?;
                flatten_quadratic(current, control, to, tolerance, 0, &mut points);
                current = to;
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                require_active(active)?;
                finite(control1)?;
                finite(control2)?;
                finite(to)?;
                flatten_cubic(current, control1, control2, to, tolerance, 0, &mut points);
                current = to;
            }
            PathCommand::Close => {
                if !active {
                    return Err(GeometryError::CloseBeforeMove);
                }
                if points.last().copied() == Some(start) {
                    points.pop();
                }
                contours.push(FlattenedContour {
                    points: std::mem::take(&mut points),
                    closed: true,
                });
                current = start;
                active = false;
            }
        }
    }
    if active {
        contours.push(FlattenedContour {
            points,
            closed: false,
        });
    }
    Ok(contours)
}
'''

new_flatten = '''fn flatten_path(path: &VectorPath, tolerance: f32) -> Result<Vec<FlattenedContour>, GeometryError> {
    let mut contours = Vec::new();
    let mut points = Vec::new();
    let mut feature_indices = Vec::new();
    let mut current = Vec2::ZERO;
    let mut start = Vec2::ZERO;
    let mut active = false;

    for command in path.commands() {
        match *command {
            PathCommand::MoveTo { to } => {
                finite(to)?;
                if active {
                    contours.push(FlattenedContour {
                        points: std::mem::take(&mut points),
                        feature_indices: std::mem::take(&mut feature_indices),
                        closed: false,
                    });
                }
                points.push(to);
                feature_indices.push(0);
                current = to;
                start = to;
                active = true;
            }
            PathCommand::LineTo { to } => {
                require_active(active)?;
                finite(to)?;
                push_distinct(&mut points, to);
                mark_feature(&mut feature_indices, points.len() - 1);
                current = to;
            }
            PathCommand::QuadraticTo { control, to } => {
                require_active(active)?;
                finite(control)?;
                finite(to)?;
                flatten_quadratic(current, control, to, tolerance, 0, &mut points);
                mark_feature(&mut feature_indices, points.len() - 1);
                current = to;
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                require_active(active)?;
                finite(control1)?;
                finite(control2)?;
                finite(to)?;
                flatten_cubic(current, control1, control2, to, tolerance, 0, &mut points);
                mark_feature(&mut feature_indices, points.len() - 1);
                current = to;
            }
            PathCommand::Close => {
                if !active {
                    return Err(GeometryError::CloseBeforeMove);
                }
                if points.last().copied() == Some(start) {
                    let removed = points.len() - 1;
                    points.pop();
                    feature_indices.retain(|index| *index != removed);
                }
                contours.push(FlattenedContour {
                    points: std::mem::take(&mut points),
                    feature_indices: std::mem::take(&mut feature_indices),
                    closed: true,
                });
                current = start;
                active = false;
            }
        }
    }
    if active {
        contours.push(FlattenedContour {
            points,
            feature_indices,
            closed: false,
        });
    }
    Ok(contours)
}
'''

if old_flatten not in text:
    raise SystemExit("flatten_path block not found")
text = text.replace(old_flatten, new_flatten, 1)

old_resample = '''fn resample_contour(
    contour: &FlattenedContour,
    sample_count: usize,
    contour_index: usize,
    side: MorphSide,
) -> Result<Vec<Vec2>, MorphError> {
    if contour.points.len() < 2 {
        return Err(MorphError::DegenerateContour {
            contour: contour_index,
            side,
        });
    }
    let segment_count = if contour.closed {
        contour.points.len()
    } else {
        contour.points.len() - 1
    };
    let mut cumulative = Vec::with_capacity(segment_count + 1);
    cumulative.push(0.0_f32);
    let mut total = 0.0_f32;
    for segment in 0..segment_count {
        let next = if segment + 1 == contour.points.len() {
            0
        } else {
            segment + 1
        };
        total += distance(contour.points[segment], contour.points[next]);
        cumulative.push(total);
    }
    if total <= DEGENERATE_LENGTH_EPSILON {
        return Err(MorphError::DegenerateContour {
            contour: contour_index,
            side,
        });
    }
    let denominator = if contour.closed {
        sample_count as f32
    } else {
        (sample_count - 1) as f32
    };
    Ok((0..sample_count)
        .map(|sample| {
            let target_distance = total * sample as f32 / denominator;
            sample_polyline(contour, &cumulative, target_distance)
        })
        .collect())
}
'''

new_resample = '''fn resample_contour(
    contour: &FlattenedContour,
    sample_count: usize,
    contour_index: usize,
    side: MorphSide,
) -> Result<Vec<Vec2>, MorphError> {
    if contour.points.len() < 2 {
        return Err(MorphError::DegenerateContour {
            contour: contour_index,
            side,
        });
    }
    let segment_count = if contour.closed {
        contour.points.len()
    } else {
        contour.points.len() - 1
    };
    let mut cumulative = Vec::with_capacity(segment_count + 1);
    cumulative.push(0.0_f32);
    let mut total = 0.0_f32;
    for segment in 0..segment_count {
        let next = if segment + 1 == contour.points.len() {
            0
        } else {
            segment + 1
        };
        total += distance(contour.points[segment], contour.points[next]);
        cumulative.push(total);
    }
    if total <= DEGENERATE_LENGTH_EPSILON {
        return Err(MorphError::DegenerateContour {
            contour: contour_index,
            side,
        });
    }

    let feature_distances: Vec<f32> = contour
        .feature_indices
        .iter()
        .copied()
        .filter(|index| *index < contour.points.len())
        .map(|index| cumulative[index])
        .collect();
    let interval_count = if contour.closed {
        feature_distances.len()
    } else {
        feature_distances.len().saturating_sub(1)
    };
    let segment_budget = if contour.closed {
        sample_count
    } else {
        sample_count.saturating_sub(1)
    };

    if interval_count == 0 || segment_budget < interval_count {
        let denominator = if contour.closed {
            sample_count as f32
        } else {
            (sample_count - 1) as f32
        };
        return Ok((0..sample_count)
            .map(|sample| {
                let target_distance = total * sample as f32 / denominator;
                sample_polyline(contour, &cumulative, target_distance)
            })
            .collect());
    }

    let mut interval_lengths = Vec::with_capacity(interval_count);
    for interval in 0..interval_count {
        let start = feature_distances[interval];
        let end = if interval + 1 < feature_distances.len() {
            feature_distances[interval + 1]
        } else {
            total
        };
        interval_lengths.push((end - start).max(0.0));
    }

    let mut allocations = vec![1_usize; interval_count];
    for _ in interval_count..segment_budget {
        let next = (0..interval_count)
            .max_by(|left, right| {
                let left_span = interval_lengths[*left] / allocations[*left] as f32;
                let right_span = interval_lengths[*right] / allocations[*right] as f32;
                left_span
                    .partial_cmp(&right_span)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| right.cmp(left))
            })
            .expect("non-empty interval set");
        allocations[next] += 1;
    }

    let mut samples = Vec::with_capacity(sample_count);
    for interval in 0..interval_count {
        let start = feature_distances[interval];
        let end = if interval + 1 < feature_distances.len() {
            feature_distances[interval + 1]
        } else {
            total
        };
        let count = allocations[interval];
        for step in 0..count {
            let progress = step as f32 / count as f32;
            let target_distance = start + (end - start) * progress;
            samples.push(sample_polyline(contour, &cumulative, target_distance));
        }
    }
    if !contour.closed {
        samples.push(*contour.points.last().expect("validated contour"));
    }
    debug_assert_eq!(samples.len(), sample_count);
    Ok(samples)
}
'''

if old_resample not in text:
    raise SystemExit("resample_contour block not found")
text = text.replace(old_resample, new_resample, 1)

text = text.replace(
    '''fn push_distinct(points: &mut Vec<Vec2>, point: Vec2) {
''',
    '''fn mark_feature(features: &mut Vec<usize>, index: usize) {
    if features.last().copied() != Some(index) {
        features.push(index);
    }
}

fn push_distinct(points: &mut Vec<Vec2>, point: Vec2) {
''',
    1,
)

marker = '''    #[test]\n    fn invalid_options_are_rejected() {\n'''
test = '''    #[test]
    fn authored_star_vertices_survive_morph_resampling_exactly() {
        let source = VectorPath::new()
            .move_to(Vec2::new(0.0, 1.65))
            .cubic_to(
                Vec2::new(0.95, 1.65),
                Vec2::new(1.65, 0.95),
                Vec2::new(1.65, 0.0),
            )
            .cubic_to(
                Vec2::new(1.65, -0.95),
                Vec2::new(0.95, -1.65),
                Vec2::new(0.0, -1.65),
            )
            .cubic_to(
                Vec2::new(-0.95, -1.65),
                Vec2::new(-1.65, -0.95),
                Vec2::new(-1.65, 0.0),
            )
            .cubic_to(
                Vec2::new(-1.65, 0.95),
                Vec2::new(-0.95, 1.65),
                Vec2::new(0.0, 1.65),
            )
            .close();
        let target_vertices = [
            Vec2::new(0.0, 2.0),
            Vec2::new(0.47, 0.65),
            Vec2::new(1.9, 0.62),
            Vec2::new(0.76, -0.25),
            Vec2::new(1.18, -1.62),
            Vec2::new(0.0, -0.82),
            Vec2::new(-1.18, -1.62),
            Vec2::new(-0.76, -0.25),
            Vec2::new(-1.9, 0.62),
            Vec2::new(-0.47, 0.65),
        ];
        let mut target = VectorPath::new().move_to(target_vertices[0]);
        for vertex in &target_vertices[1..] {
            target = target.line_to(*vertex);
        }
        target = target.close();

        let plan = plan_morph(&source, &target, MorphOptions::DEFAULT).expect("valid morph");
        let samples = &plan.contours[0].target_points;
        assert_eq!(samples.len(), MorphOptions::DEFAULT.samples_per_contour);
        for vertex in target_vertices {
            assert!(samples
                .iter()
                .any(|sample| squared_distance(*sample, vertex) < 1.0e-10));
        }
    }

'''
if marker not in text:
    raise SystemExit("test insertion marker missing")
text = text.replace(marker, test + marker, 1)

path.write_text(text)

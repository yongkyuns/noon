//! Shared f64 layout minimization and source-defined circular initialization.
//! Layouts prepare values once before the existing semantic publication.

type LayoutEvaluation = (f64, Vec<f64>);
type SearchTrial = (Vec<f64>, LayoutEvaluation, f64);

pub(super) fn minimize_kamada_kawai(
    initial: &[(f64, f64)],
    distances: &[f64],
    count: usize,
) -> Vec<f64> {
    let mut position = initial
        .iter()
        .flat_map(|&(x, y)| [x, y])
        .collect::<Vec<_>>();
    let mut evaluation = kamada_kawai_cost_gradient(&position, distances, count);
    let mut history: Vec<(Vec<f64>, Vec<f64>, f64)> = Vec::new();
    let gradient_tolerance = 1e-5;

    for iteration in 0..15_000 {
        if evaluation
            .1
            .iter()
            .fold(0.0_f64, |norm, value| norm.max(value.abs()))
            <= gradient_tolerance
        {
            break;
        }
        let mut direction = lbfgs_direction(&evaluation.1, &history);
        if dot(&evaluation.1, &direction) >= 0.0 {
            direction = evaluation.1.iter().map(|value| -value).collect();
        }
        let slope = dot(&evaluation.1, &direction);
        let first_step = if iteration == 0 {
            1.0 / dot(&direction, &direction).sqrt()
        } else {
            1.0
        };
        let Some((candidate, candidate_evaluation, step)) = more_thuente_search(
            &position,
            &direction,
            evaluation.0,
            slope,
            first_step,
            |points| kamada_kawai_cost_gradient(points, distances, count),
        ) else {
            break;
        };
        let s = candidate
            .iter()
            .zip(&position)
            .map(|(next, previous)| next - previous)
            .collect::<Vec<_>>();
        let y = candidate_evaluation
            .1
            .iter()
            .zip(&evaluation.1)
            .map(|(next, previous)| next - previous)
            .collect::<Vec<_>>();
        let curvature = dot(&s, &y);
        if curvature > f64::EPSILON * (-slope * step) {
            if history.len() == 10 {
                history.remove(0);
            }
            history.push((s, y, 1.0 / curvature));
        }
        let converged_function = evaluation.0 - candidate_evaluation.0
            <= f64::EPSILON
                * 1e7
                * evaluation
                    .0
                    .abs()
                    .max(candidate_evaluation.0.abs())
                    .max(1.0);
        position = candidate;
        evaluation = candidate_evaluation;
        if converged_function {
            break;
        }
    }
    position
}

fn kamada_kawai_cost_gradient(
    position: &[f64],
    distances: &[f64],
    count: usize,
) -> (f64, Vec<f64>) {
    let mut cost = 0.0;
    let mut gradient = vec![0.0; count * 2];
    let mut inverse_distances = vec![0.0; count * count];
    let mut mean = [0.0, 0.0];
    for index in 0..count {
        mean[0] += position[index * 2];
        mean[1] += position[index * 2 + 1];
        for other in 0..count {
            let diagonal_epsilon = if index == other { 1e-3 } else { 0.0 };
            inverse_distances[index * count + other] =
                1.0 / (distances[index * count + other] + diagonal_epsilon);
        }
    }

    for index in 0..count {
        for other in 0..count {
            if index == other {
                continue;
            }
            let dx = position[index * 2] - position[other * 2];
            let dy = position[index * 2 + 1] - position[other * 2 + 1];
            // Match NumPy norm operation order and the same IEEE sqrt on native/WASM.
            let separation = (dx * dx + dy * dy).sqrt();
            let inverse_distance = inverse_distances[index * count + other];
            let offset = separation * inverse_distance - 1.0;
            cost += 0.5 * offset * offset;
            let factor = inverse_distance * offset / separation;
            gradient[index * 2] += factor * dx;
            gradient[index * 2 + 1] += factor * dy;
            gradient[other * 2] -= factor * dx;
            gradient[other * 2 + 1] -= factor * dy;
        }
    }
    cost += 0.5e-3 * (mean[0] * mean[0] + mean[1] * mean[1]);
    for index in 0..count {
        gradient[index * 2] += 1e-3 * mean[0];
        gradient[index * 2 + 1] += 1e-3 * mean[1];
    }
    (cost, gradient)
}

fn lbfgs_direction(gradient: &[f64], history: &[(Vec<f64>, Vec<f64>, f64)]) -> Vec<f64> {
    let mut direction = gradient.to_vec();
    let mut alpha = vec![0.0; history.len()];
    for (index, (s, y, inverse_curvature)) in history.iter().enumerate().rev() {
        alpha[index] = inverse_curvature * dot(s, &direction);
        axpy(&mut direction, y, -alpha[index]);
    }
    let scale = history.last().map_or(1.0, |(s, y, _)| {
        let yy = dot(y, y);
        if yy > 0.0 {
            dot(s, y) / yy
        } else {
            1.0
        }
    });
    direction.iter_mut().for_each(|value| *value *= scale);
    for (index, (s, y, inverse_curvature)) in history.iter().enumerate() {
        let beta = inverse_curvature * dot(y, &direction);
        axpy(&mut direction, s, alpha[index] - beta);
    }
    direction.iter_mut().for_each(|value| *value = -*value);
    direction
}

fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn axpy(target: &mut [f64], source: &[f64], factor: f64) {
    for (target, source) in target.iter_mut().zip(source) {
        *target += factor * source;
    }
}

// More–Thuente interval search, following SciPy 1.18.1 optimize/_dcsrch.py.
// MINPACK authors: Jorge J. More and David J. Thuente, Argonne National Laboratory.
// Copyright (c) 2001-2002 Enthought, Inc. 2003, SciPy Developers.
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
// 1. Redistributions of source code must retain the above copyright notice,
//    this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright notice,
//    this list of conditions and the following disclaimer in the documentation
//    and/or other materials provided with the distribution.
// 3. Neither the name of the copyright holder nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT OWNER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE
// GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT
// OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

#[derive(Clone, Copy)]
struct SearchPoint {
    step: f64,
    value: f64,
    slope: f64,
}

/// Search one unbounded descent direction with SciPy's L-BFGS-B parameters.
/// Every trial is scratch data; only an accepted finite trial leaves the search.
fn more_thuente_search(
    position: &[f64],
    direction: &[f64],
    initial_value: f64,
    initial_slope: f64,
    mut step: f64,
    mut evaluate: impl FnMut(&[f64]) -> LayoutEvaluation,
) -> Option<SearchTrial> {
    const MIN: f64 = 0.0;
    const MAX: f64 = 1e10;
    const FTOL: f64 = 1e-3;
    const GTOL: f64 = 0.9;
    const XTOL: f64 = 0.1;
    if !initial_value.is_finite()
        || !initial_slope.is_finite()
        || initial_slope >= 0.0
        || !step.is_finite()
        || !(MIN..=MAX).contains(&step)
    {
        return None;
    }
    let mut best = SearchPoint {
        step: 0.0,
        value: initial_value,
        slope: initial_slope,
    };
    let mut endpoint = best;
    let mut bracketed = false;
    let mut first_stage = true;
    let sufficient_slope = FTOL * initial_slope;
    let mut width = MAX - MIN;
    let mut previous_width = width * 2.0;
    let mut minimum = 0.0;
    let mut maximum = step * 5.0;
    for _ in 0..20 {
        let candidate: Vec<_> = position
            .iter()
            .zip(direction)
            .map(|(point, delta)| point + step * delta)
            .collect();
        let evaluation = evaluate(&candidate);
        let slope = dot(&evaluation.1, direction);
        if !evaluation.0.is_finite()
            || !slope.is_finite()
            || evaluation.1.iter().any(|value| !value.is_finite())
        {
            return None;
        }
        let sufficient_value = initial_value + step * sufficient_slope;
        if first_stage && evaluation.0 <= sufficient_value && slope >= 0.0 {
            first_stage = false;
        }
        let warning = (bracketed && (step <= minimum || step >= maximum))
            || (bracketed && maximum - minimum <= XTOL * maximum)
            || (step == MAX && evaluation.0 <= sufficient_value && slope <= sufficient_slope)
            || (step == MIN && (evaluation.0 > sufficient_value || slope >= sufficient_slope));
        let converged = evaluation.0 <= sufficient_value && slope.abs() <= GTOL * -initial_slope;
        // L-BFGS-B accepts the finite warning point, then its outer convergence
        // checks decide whether to stop or continue from the accepted trial.
        if converged || warning {
            return Some((candidate, evaluation, step));
        }
        let current = SearchPoint {
            step,
            value: evaluation.0,
            slope,
        };
        if first_stage && current.value <= best.value && current.value > sufficient_value {
            // The first interval minimizes psi(step), not the original objective.
            best.value -= best.step * sufficient_slope;
            endpoint.value -= endpoint.step * sufficient_slope;
            best.slope -= sufficient_slope;
            endpoint.slope -= sufficient_slope;
            let modified = SearchPoint {
                value: current.value - step * sufficient_slope,
                slope: current.slope - sufficient_slope,
                ..current
            };
            step = update_search_interval(
                &mut best,
                &mut endpoint,
                modified,
                &mut bracketed,
                minimum,
                maximum,
            );
            best.value += best.step * sufficient_slope;
            endpoint.value += endpoint.step * sufficient_slope;
            best.slope += sufficient_slope;
            endpoint.slope += sufficient_slope;
        } else {
            step = update_search_interval(
                &mut best,
                &mut endpoint,
                current,
                &mut bracketed,
                minimum,
                maximum,
            );
        }
        if bracketed {
            if (endpoint.step - best.step).abs() >= 0.66 * previous_width {
                step = best.step + 0.5 * (endpoint.step - best.step);
            }
            previous_width = width;
            width = (endpoint.step - best.step).abs();
            minimum = best.step.min(endpoint.step);
            maximum = best.step.max(endpoint.step);
        } else {
            minimum = step + 1.1 * (step - best.step);
            maximum = step + 4.0 * (step - best.step);
        }
        if !step.is_finite() {
            return None;
        }
        step = step.clamp(MIN, MAX);
        if bracketed && (step <= minimum || step >= maximum || maximum - minimum <= XTOL * maximum)
        {
            step = best.step;
        }
    }
    None
}

/// Safeguarded cubic/quadratic interpolation of the interval endpoints.
fn update_search_interval(
    best: &mut SearchPoint,
    endpoint: &mut SearchPoint,
    current: SearchPoint,
    bracketed: &mut bool,
    minimum: f64,
    maximum: f64,
) -> f64 {
    let SearchPoint { step, value, slope } = current;
    let opposite = slope != 0.0 && slope.is_sign_positive() != best.slope.is_sign_positive();
    let interpolate = |left: SearchPoint, right: SearchPoint, clamp: bool| {
        let theta =
            3.0 * (left.value - right.value) / (right.step - left.step) + left.slope + right.slope;
        let scale = theta.abs().max(left.slope.abs()).max(right.slope.abs());
        let discriminant = (theta / scale).powi(2) - (left.slope / scale) * (right.slope / scale);
        let gamma = scale
            * (if clamp {
                discriminant.max(0.0)
            } else {
                discriminant
            })
            .sqrt();
        (theta, gamma)
    };
    let next = if value > best.value {
        let (theta, mut gamma) = interpolate(*best, current, false);
        if step < best.step {
            gamma = -gamma;
        }
        let ratio = ((gamma - best.slope) + theta) / (((gamma - best.slope) + gamma) + slope);
        let cubic = best.step + ratio * (step - best.step);
        let quadratic = best.step
            + ((best.slope / ((best.value - value) / (step - best.step) + best.slope)) / 2.0)
                * (step - best.step);
        *bracketed = true;
        if (cubic - best.step).abs() <= (quadratic - best.step).abs() {
            cubic
        } else {
            cubic + (quadratic - cubic) / 2.0
        }
    } else if opposite {
        let (theta, mut gamma) = interpolate(*best, current, false);
        if step > best.step {
            gamma = -gamma;
        }
        let ratio = ((gamma - slope) + theta) / (((gamma - slope) + gamma) + best.slope);
        let cubic = step + ratio * (best.step - step);
        let secant = step + (slope / (slope - best.slope)) * (best.step - step);
        *bracketed = true;
        if (cubic - step).abs() > (secant - step).abs() {
            cubic
        } else {
            secant
        }
    } else if slope.abs() < best.slope.abs() {
        let (theta, mut gamma) = interpolate(*best, current, true);
        if step > best.step {
            gamma = -gamma;
        }
        let ratio = ((gamma - slope) + theta) / ((gamma + (best.slope - slope)) + gamma);
        let cubic = if ratio < 0.0 && gamma != 0.0 {
            step + ratio * (best.step - step)
        } else if step > best.step {
            maximum
        } else {
            minimum
        };
        let secant = step + (slope / (slope - best.slope)) * (best.step - step);
        if *bracketed {
            let next = if (cubic - step).abs() < (secant - step).abs() {
                cubic
            } else {
                secant
            };
            let safeguard = step + 0.66 * (endpoint.step - step);
            if step > best.step {
                next.min(safeguard)
            } else {
                next.max(safeguard)
            }
        } else {
            let next = if (cubic - step).abs() > (secant - step).abs() {
                cubic
            } else {
                secant
            };
            next.clamp(minimum, maximum)
        }
    } else if *bracketed {
        let (theta, mut gamma) = interpolate(*endpoint, current, false);
        if step > endpoint.step {
            gamma = -gamma;
        }
        let ratio = ((gamma - slope) + theta) / (((gamma - slope) + gamma) + endpoint.slope);
        step + ratio * (endpoint.step - step)
    } else if step > best.step {
        maximum
    } else {
        minimum
    };
    if value > best.value {
        *endpoint = current;
    } else {
        if opposite {
            *endpoint = *best;
        }
        *best = current;
    }
    next
}

#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn safeguarded_search_brackets_a_large_quadratic_trial() {
        let (point, (value, gradient), step) =
            more_thuente_search(&[0.0], &[1.0], 4.0, -4.0, 10.0, |point| {
                ((point[0] - 2.0).powi(2), vec![2.0 * (point[0] - 2.0)])
            })
            .expect("quadratic has a finite minimizing step");
        assert!((point[0] - 2.0).abs() < 1e-12);
        assert!(value < 1e-24);
        assert!(gradient[0].abs() < 1e-12);
        assert!(step < 10.0);
    }

    #[test]
    fn safeguarded_search_expands_a_trial_until_curvature_is_satisfied() {
        let (_, (value, gradient), step) =
            more_thuente_search(&[0.0], &[1.0], 4.0, -4.0, 0.01, |point| {
                ((point[0] - 2.0).powi(2), vec![2.0 * (point[0] - 2.0)])
            })
            .unwrap();
        assert!(step > 0.01);
        assert!(value <= 4.0 - 1e-3 * step * 4.0);
        assert!(gradient[0].abs() <= 0.9 * 4.0);
    }

    #[test]
    fn safeguarded_search_preserves_strong_wolfe_on_rosenbrock_direction() {
        let direction = [215.6, 88.0];
        let slope = -dot(&direction, &direction);
        let (_, (value, gradient), step) = more_thuente_search(
            &[-1.2, 1.0],
            &direction,
            24.2,
            slope,
            1.0 / dot(&direction, &direction).sqrt(),
            |point| {
                let (x, y) = (point[0], point[1]);
                (
                    (1.0 - x).powi(2) + 100.0 * (y - x * x).powi(2),
                    vec![
                        2.0 * (x - 1.0) - 400.0 * x * (y - x * x),
                        200.0 * (y - x * x),
                    ],
                )
            },
        )
        .unwrap();
        assert!(value <= 24.2 + 1e-3 * step * slope);
        assert!(dot(&gradient, &direction).abs() <= 0.9 * -slope);
    }

    #[test]
    fn safeguarded_search_rejects_non_descent_and_nonfinite_trials() {
        assert!(
            more_thuente_search(&[0.0], &[1.0], 1.0, 1.0, 1.0, |_| unreachable!(
                "non-descent input must not be evaluated"
            ))
            .is_none()
        );
        assert!(
            more_thuente_search(&[0.0], &[1.0], 1.0, -1.0, 1.0, |_| (f64::NAN, vec![1.0]))
                .is_none()
        );
    }
}

// Bounded circular initialization follows NumPy 2.5.4 SIMD trigonometry.
// Source: numpy/_core/src/umath/loops_trigonometric.dispatch.cpp
// Portable fused operations preserve the pinned float32 profile on every host.
// Copyright (c) 2005-2025, NumPy Developers.
// All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met:
//
//     * Redistributions of source code must retain the above copyright
//        notice, this list of conditions and the following disclaimer.
//
//     * Redistributions in binary form must reproduce the above
//        copyright notice, this list of conditions and the following
//        disclaimer in the documentation and/or other materials provided
//        with the distribution.
//
//     * Neither the name of the NumPy Developers nor the names of any
//        contributors may be used to endorse or promote products derived
//        from this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

/// Cody-Waite reduction and the pinned polynomials for angles in [0, TAU].
/// This bound is established by the circular initializer, so no large-angle
/// platform-libm fallback is needed.
pub(super) fn circular_reference_point(angle: f32) -> (f64, f64) {
    const TWO_OVER_PI: f32 = f32::from_bits(0x3f22f983);
    const PIO2_HIGH: f32 = f32::from_bits(0xbfc90fd8);
    const PIO2_MEDIUM: f32 = f32::from_bits(0xb4a8885a);
    const PIO2_LOW: f32 = f32::from_bits(0xa7c234c4);
    const ROUND_MAGIC: f32 = f32::from_bits(0x4b400000);
    const COS_EIGHT: f32 = f32::from_bits(0x37cc730b);
    const COS_SIX: f32 = f32::from_bits(0xbab6036e);
    const COS_FOUR: f32 = f32::from_bits(0x3d2aaa9e);
    const COS_TWO: f32 = f32::from_bits(0xbf000000);
    const COS_ZERO: f32 = f32::from_bits(0x3f800000);
    const SIN_NINE: f32 = f32::from_bits(0x363e9dde);
    const SIN_SEVEN: f32 = f32::from_bits(0xb95035dd);
    const SIN_FIVE: f32 = f32::from_bits(0x3c0888cd);
    const SIN_THREE: f32 = f32::from_bits(0xbe2aaaab);
    let quadrant = ((angle * TWO_OVER_PI) + ROUND_MAGIC) - ROUND_MAGIC;
    let reduced = quadrant.mul_add(PIO2_HIGH, angle);
    let reduced = quadrant.mul_add(PIO2_MEDIUM, reduced);
    let reduced = quadrant.mul_add(PIO2_LOW, reduced);
    let square = reduced * reduced;
    let cosine = COS_EIGHT.mul_add(square, COS_SIX);
    let cosine = cosine.mul_add(square, COS_FOUR);
    let cosine = cosine.mul_add(square, COS_TWO);
    let cosine = cosine.mul_add(square, COS_ZERO);
    let sine = SIN_NINE.mul_add(square, SIN_SEVEN);
    let sine = sine.mul_add(square, SIN_FIVE);
    let sine = sine.mul_add(square, SIN_THREE);
    let sine = sine.mul_add(square, 0.0);
    let sine = sine.mul_add(reduced, reduced);
    let select = |quadrant: i32| {
        let value = if quadrant & 1 == 0 { sine } else { cosine };
        if quadrant & 2 != 0 {
            -value
        } else {
            value
        }
    };
    (
        f64::from(select(quadrant as i32 + 1)),
        f64::from(select(quadrant as i32)),
    )
}

#[cfg(test)]
mod circular_tests {
    use super::circular_reference_point;

    #[test]
    fn bounded_circular_initializer_matches_pinned_numpy_float32_bits() {
        let mut samples = 0;
        for line in include_str!("layout_optimizer/circular_reference.csv").lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let values: Vec<u32> = line.split(',').map(|x| x.parse().unwrap()).collect();
            assert_eq!(values.len(), 5);
            let (count, index, angle_bits) = (values[0], values[1], values[2]);
            let angle = (std::f64::consts::TAU * index as f64 / count as f64) as f32;
            assert_eq!(angle.to_bits(), angle_bits);
            let (x, y) = circular_reference_point(angle);
            assert_eq!(
                (x as f32).to_bits(),
                values[3],
                "cosine count={count} index={index}"
            );
            assert_eq!(
                (y as f32).to_bits(),
                values[4],
                "sine count={count} index={index}"
            );
            samples += 1;
        }
        assert_eq!(samples, 2080);
    }
}

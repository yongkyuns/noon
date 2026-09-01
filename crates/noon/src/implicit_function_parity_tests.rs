use crate::{ImplicitFunctionAuthoringError, ImplicitFunctionPlan};

fn evaluate<F>(
    function: F,
) -> impl FnMut(f64, f64) -> Result<f64, ImplicitFunctionAuthoringError>
where
    F: Fn(f64, f64) -> f64,
{
    move |x, y| Ok(function(x, y))
}

#[test]
fn open_contour_remains_open_and_tracks_the_zero_set() {
    let plan = ImplicitFunctionPlan::new([-1.0, 1.0], [-1.0, 1.0], 3, 256, false).unwrap();
    let curves = plan
        .curves_with_evaluator(evaluate(|x, _| x))
        .unwrap();

    assert_eq!(curves.len(), 1);
    let curve = &curves[0];
    assert!(curve.len() > 2);
    assert_ne!(curve.first(), curve.last());
    assert!(curve.iter().all(|point| point[0].abs() <= 1.0e-6));
}

#[test]
fn pole_sign_change_is_not_misclassified_as_an_implicit_zero() {
    let plan = ImplicitFunctionPlan::new([-1.0, 1.0], [-1.0, 1.0], 3, 512, false).unwrap();
    let curves = plan
        .curves_with_evaluator(evaluate(|x, _| {
            if x == 0.0 {
                f64::INFINITY
            } else {
                1.0 / x
            }
        }))
        .unwrap();

    assert!(curves.is_empty(), "a pole must not become a false zero contour");
}

//! Inert language-binding arguments. Timing/validation/lifecycle are resolved by Noon.
use crate::{engine_error, mobject::MobjectHandle};
use noon::{
    AnimationCompositionRequest as Request, AnimationOptions, FadeEndpoint, FadeTranslation,
    Mobject, SemanticAnimationCompositionKind as Kind, SemanticFadeDirection, TransformToRequest,
};
use pyo3::prelude::*;

pub(crate) fn rate(value: &str) -> PyResult<noon::RateFunction> {
    noon::RateFunction::from_semantic_id(value).ok_or_else(|| {
        engine_error(noon::integration::AuthoringFailure::new(
            "invalid_input",
            "animation.invalid_rate_function",
            format!("unsupported rate function semantic id: {value}"),
        ))
    })
}
fn options(duration: f64, easing: &str) -> PyResult<AnimationOptions> {
    Ok(AnimationOptions::new()
        .run_time(duration)
        .rate_func(rate(easing)?))
}
#[derive(Clone)]
pub(crate) enum Child {
    Transform {
        source: Mobject,
        target: Mobject,
        point: bool,
        method: bool,
        options: AnimationOptions,
    },
    Create(Mobject, AnimationOptions),
    Uncreate(Mobject, AnimationOptions),
    Fade(
        Mobject,
        SemanticFadeDirection,
        FadeEndpoint,
        AnimationOptions,
    ),
    Wait(f64),
    Add(Mobject, AnimationOptions),
    Nested(Composition),
    Rotate(Mobject, f64, noon::ManimRotationPivot, AnimationOptions),
    Indicate(Mobject, noon::IndicateOptions, AnimationOptions),
}
impl Child {
    fn request(&self) -> Request<'_> {
        match self {
            Self::Transform {
                source,
                target,
                point,
                method,
                options,
            } => {
                let request = if *point {
                    TransformToRequest::point_correspondence(source, target, *options)
                } else {
                    TransformToRequest::new(source, target, *options)
                };
                Request::TransformTo(if *method {
                    request.method_target()
                } else {
                    request
                })
            }
            Self::Create(target, options) => Request::Create {
                target,
                options: *options,
            },
            Self::Uncreate(target, options) => Request::Uncreate {
                target,
                options: *options,
            },
            Self::Fade(target, direction, endpoint, options) => Request::Fade {
                target,
                direction: *direction,
                endpoint: *endpoint,
                options: *options,
            },
            Self::Wait(duration) => Request::Wait {
                duration: *duration,
            },
            Self::Add(target, options) => Request::Add {
                target,
                options: *options,
            },
            Self::Nested(builder) => builder.request(),
            Self::Rotate(target, angle, pivot, options) => Request::ManimRotate {
                target,
                angle: *angle,
                pivot: *pivot,
                options: *options,
            },
            Self::Indicate(target, indication, options) => Request::Indicate {
                target,
                indication: *indication,
                options: *options,
            },
        }
    }
}
#[pyclass(unsendable, skip_from_py_object, module = "_noon_native")]
#[derive(Clone)]
pub struct Composition {
    pub(crate) kind: Kind,
    pub(crate) options: AnimationOptions,
    pub(crate) play_options: AnimationOptions,
    pub(crate) children: Vec<Child>,
}
impl Composition {
    pub(crate) fn new(
        kind: &str,
        duration: Option<f64>,
        lag: f64,
        play_duration: Option<f64>,
    ) -> PyResult<Self> {
        let kind = match kind {
            "parallel" => Kind::Parallel,
            "sequence" => Kind::Sequence,
            _ => return Err(engine_error("unknown composition kind")),
        };
        Ok(Self {
            kind,
            options: AnimationOptions {
                run_time: duration,
                lag_ratio: Some(lag),
                ..AnimationOptions::new()
            },
            play_options: AnimationOptions {
                run_time: play_duration,
                ..AnimationOptions::new()
            },
            children: Vec::new(),
        })
    }
    pub(crate) fn request(&self) -> Request<'_> {
        Request::Composition {
            kind: self.kind,
            children: self.children.iter().map(Child::request).collect(),
            options: self.options,
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn transform(
        &mut self,
        source: &MobjectHandle,
        target: &MobjectHandle,
        point: bool,
        method: bool,
        duration: f64,
        easing: &str,
        arc: f64,
        reverse: bool,
    ) -> PyResult<()> {
        self.children.push(Child::Transform {
            source: source.handle.clone(),
            target: target.handle.clone(),
            point,
            method,
            options: AnimationOptions {
                path_arc: Some(arc),
                reverse_rate_function: Some(reverse),
                ..options(duration, easing)?
            },
        });
        Ok(())
    }
}
#[pymethods]
#[allow(clippy::too_many_arguments)]
impl Composition {
    #[pyo3(name = "setCompositionRateFunction")]
    fn set_rate(&mut self, value: &str) -> PyResult<()> {
        self.options.rate_func = Some(rate(value)?);
        Ok(())
    }
    #[pyo3(name = "setPlayRateFunction")]
    fn set_play_rate(&mut self, value: &str) -> PyResult<()> {
        self.play_options.rate_func = Some(rate(value)?);
        Ok(())
    }
    #[pyo3(name = "appendComposition")]
    fn nested(&mut self, child: &Self) {
        self.children.push(Child::Nested(child.clone()));
    }
    #[pyo3(name="appendMethodTransformTo",signature=(_id,source,target,point,duration,easing,arc,reverse))]
    fn method(
        &mut self,
        _id: Option<&str>,
        source: &MobjectHandle,
        target: &MobjectHandle,
        point: bool,
        duration: f64,
        easing: &str,
        arc: f64,
        reverse: bool,
    ) -> PyResult<()> {
        self.transform(source, target, point, true, duration, easing, arc, reverse)
    }
    #[pyo3(name = "appendTransformTo")]
    fn affine(
        &mut self,
        source: &MobjectHandle,
        target: &MobjectHandle,
        duration: f64,
        easing: &str,
        arc: f64,
        reverse: bool,
    ) -> PyResult<()> {
        self.transform(source, target, false, false, duration, easing, arc, reverse)
    }
    #[pyo3(name = "appendPointTransformTo")]
    fn points(
        &mut self,
        source: &MobjectHandle,
        target: &MobjectHandle,
        duration: f64,
        easing: &str,
        arc: f64,
        reverse: bool,
    ) -> PyResult<()> {
        self.transform(source, target, true, false, duration, easing, arc, reverse)
    }
    #[pyo3(name = "appendEnteringTransformTo")]
    fn entering(
        &mut self,
        _id: &str,
        source: &MobjectHandle,
        target: &MobjectHandle,
        duration: f64,
        easing: &str,
        arc: f64,
        reverse: bool,
    ) -> PyResult<()> {
        self.transform(source, target, false, false, duration, easing, arc, reverse)
    }
    #[pyo3(name = "appendEnteringPointTransformTo")]
    fn entering_points(
        &mut self,
        _id: &str,
        source: &MobjectHandle,
        target: &MobjectHandle,
        duration: f64,
        easing: &str,
        arc: f64,
        reverse: bool,
    ) -> PyResult<()> {
        self.transform(source, target, true, false, duration, easing, arc, reverse)
    }
    #[pyo3(name = "appendCreate")]
    fn create(
        &mut self,
        _id: &str,
        target: &MobjectHandle,
        duration: f64,
        easing: &str,
    ) -> PyResult<()> {
        self.children.push(Child::Create(
            target.handle.clone(),
            options(duration, easing)?,
        ));
        Ok(())
    }
    #[pyo3(name = "appendUncreate")]
    fn uncreate(
        &mut self,
        _id: &str,
        target: &MobjectHandle,
        duration: f64,
        easing: &str,
        remover: bool,
        reverse: bool,
    ) -> PyResult<()> {
        self.children.push(Child::Uncreate(
            target.handle.clone(),
            AnimationOptions {
                remover: Some(remover),
                reverse_rate_function: Some(reverse),
                ..options(duration, easing)?
            },
        ));
        Ok(())
    }
    #[pyo3(name = "appendWait")]
    fn wait(&mut self, duration: f64) {
        self.children.push(Child::Wait(duration));
    }
    #[pyo3(name = "appendAdd")]
    fn add(
        &mut self,
        _id: &str,
        target: &MobjectHandle,
        duration: f64,
        easing: &str,
    ) -> PyResult<()> {
        self.children.push(Child::Add(
            target.handle.clone(),
            options(duration, easing)?,
        ));
        Ok(())
    }
    #[pyo3(name = "appendFade")]
    fn fade(
        &mut self,
        _id: &str,
        target: &MobjectHandle,
        direction: &str,
        scale: f64,
        translation: &str,
        x: f64,
        y: f64,
        duration: f64,
        easing: &str,
    ) -> PyResult<()> {
        let direction = match direction {
            "in" => SemanticFadeDirection::In,
            "out" => SemanticFadeDirection::Out,
            _ => return Err(engine_error("invalid fade direction")),
        };
        let translation = match translation {
            "none" => FadeTranslation::Shift(noon::SemanticVec3::ZERO),
            "shift" => FadeTranslation::Shift(noon::SemanticVec3::new(x, y, 0.0)),
            "point" => FadeTranslation::Point(noon::SemanticVec3::new(x, y, 0.0)),
            _ => return Err(engine_error("unsupported fade translation")),
        };
        self.children.push(Child::Fade(
            target.handle.clone(),
            direction,
            FadeEndpoint {
                scale_factor: scale,
                translation,
            },
            options(duration, easing)?,
        ));
        Ok(())
    }
    #[pyo3(name="appendManimRotate",signature=(_id,target,angle,kind,x,y,duration,easing))]
    fn rotate(
        &mut self,
        _id: Option<&str>,
        target: &MobjectHandle,
        angle: f64,
        kind: &str,
        x: f64,
        y: f64,
        duration: f64,
        easing: &str,
    ) -> PyResult<()> {
        let pivot = match kind {
            "center" => noon::ManimRotationPivot::Center,
            "point" => noon::ManimRotationPivot::Point(x, y),
            "edge" => noon::ManimRotationPivot::Edge(x, y),
            _ => return Err(engine_error("invalid pivot")),
        };
        self.children.push(Child::Rotate(
            target.handle.clone(),
            angle,
            pivot,
            options(duration, easing)?,
        ));
        Ok(())
    }
    #[pyo3(name="appendIndicateMobject",signature=(target,scale,red,green,blue,alpha,duration,easing,lag))]
    #[allow(clippy::too_many_arguments)]
    fn indicate(
        &mut self,
        target: &MobjectHandle,
        scale: f64,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        duration: Option<f64>,
        easing: Option<&str>,
        lag: Option<f64>,
    ) -> PyResult<()> {
        let color = crate::callback_values::callback_color(
            "indication color",
            Some(red),
            Some(green),
            Some(blue),
            Some(alpha),
        )?
        .expect("all channels supplied");
        let options = AnimationOptions {
            run_time: duration,
            rate_func: easing.map(rate).transpose()?,
            lag_ratio: lag,
            ..AnimationOptions::new()
        };
        self.children.push(Child::Indicate(
            target.handle.clone(),
            noon::IndicateOptions::new(scale, color),
            options,
        ));
        Ok(())
    }
}

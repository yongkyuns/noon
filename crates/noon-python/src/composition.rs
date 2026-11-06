//! Inert language-binding arguments. Timing/validation/lifecycle are resolved by Noon.
use crate::callback_values::callback_color;
use crate::{engine_error, family::FamilyHandle, mobject::MobjectHandle};
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
fn parse_affine_lifecycle_direction(value: &str) -> Result<noon::AffineLifecycleDirection, PyErr> {
    match value {
            "introduce-from" => Ok(noon::AffineLifecycleDirection::IntroduceFrom),
            "remove-to" => Ok(noon::AffineLifecycleDirection::RemoveTo),
            _ => Err(engine_error(format!(
                "ordinary affine lifecycle direction must be \"introduce-from\" or \"remove-to\", got {value:?}"
            ))),
        }
}

fn parse_affine_lifecycle_endpoint(
    value: &str,
    x: f64,
    y: f64,
    rotation_offset: f64,
    point_color: Option<noon::Color>,
) -> Result<noon::AffineLifecycleEndpoint, PyErr> {
    match value {
            "point" => Ok(noon::AffineLifecycleEndpoint::Point {
                x,
                y,
                rotation_offset,
                point_color,
            }),
            "effective-center" => Ok(noon::AffineLifecycleEndpoint::EffectiveCenter),
            _ => Err(engine_error(format!(
                "ordinary affine lifecycle endpoint must be \"point\" or \"effective-center\", got {value:?}"
            ))),
        }
}

#[derive(Clone)]
pub(crate) enum DissolveTarget {
    Object(Mobject),
    Family(noon::MobjectFamily),
}
impl DissolveTarget {
    fn target(&self) -> noon::MobjectTarget<'_> {
        match self {
            Self::Object(value) => noon::MobjectTarget::Object(value),
            Self::Family(value) => noon::MobjectTarget::Family(value),
        }
    }
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
    FamilyTransform {
        source: noon::MobjectFamily,
        target: noon::MobjectFamily,
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
    TextWrite(Mobject, bool, AnimationOptions),
    GlowPulse(Mobject, f64, AnimationOptions),
    Dissolve(
        DissolveTarget,
        noon_core::DissolveDirection,
        noon_core::DissolveStyle,
        AnimationOptions,
    ),
    FamilyReveal(noon::MobjectFamily, bool, AnimationOptions),
    MoveAlongPath(Mobject, Mobject, AnimationOptions),
    ValueTracker(noon::ValueTracker, f64, AnimationOptions),
    FamilyFade {
        target: noon::MobjectFamily,
        direction: SemanticFadeDirection,
        options: AnimationOptions,
    },
    FamilyIndicate {
        target: noon::MobjectFamily,
        indication: noon::IndicateOptions,
        options: AnimationOptions,
    },
    FocusOn {
        focus: noon::FocusOnOptions,
        options: AnimationOptions,
    },
    PassingFlash {
        target: Mobject,
        time_width: f64,
        options: AnimationOptions,
    },
    DrawBorderThenFill {
        target: Mobject,
        outline: noon::DrawBorderThenFillOptions,
        options: AnimationOptions,
    },
    FamilyDrawBorderThenFill {
        target: noon::MobjectFamily,
        outline: noon::DrawBorderThenFillOptions,
        options: AnimationOptions,
    },
    FamilyTextWrite {
        target: noon::MobjectFamily,
        reverse_member_order: bool,
        options: AnimationOptions,
    },
    TextReveal {
        target: Mobject,
        reverse: bool,
        options: AnimationOptions,
    },
    AffineLifecycle {
        target: Mobject,
        direction: noon::AffineLifecycleDirection,
        endpoint: noon::AffineLifecycleEndpoint,
        options: AnimationOptions,
    },
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
            Self::FamilyTransform {
                source,
                target,
                method,
                options,
            } => {
                if *method {
                    Request::FamilyMethodTargetTo {
                        source,
                        target_state: target,
                        options: *options,
                    }
                } else {
                    Request::FamilyTransformTo {
                        source,
                        target_state: target,
                        options: *options,
                    }
                }
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
            Self::TextWrite(target, reverse_member_order, options) => Request::TextWrite {
                target,
                reverse_member_order: *reverse_member_order,
                options: *options,
            },
            Self::GlowPulse(target, intensity, options) => Request::GlowPulse {
                target,
                intensity: *intensity,
                options: *options,
            },
            Self::Dissolve(target, direction, style, options) => Request::Dissolve {
                target: target.target(),
                direction: *direction,
                style: *style,
                options: *options,
            },
            Self::FamilyReveal(target, reverse, options) => Request::FamilyReveal {
                target,
                reverse: *reverse,
                options: *options,
            },
            Self::MoveAlongPath(target, path, options) => Request::MoveAlongPath {
                target,
                path,
                options: *options,
            },
            Self::ValueTracker(tracker, target, options) => Request::ValueTracker {
                tracker,
                target: *target,
                options: *options,
            },
            Self::FamilyFade {
                target,
                direction,
                options,
            } => Request::FamilyFade {
                target,
                direction: *direction,
                options: *options,
            },
            Self::FamilyIndicate {
                target,
                indication,
                options,
            } => Request::FamilyIndicate {
                target,
                indication: *indication,
                options: *options,
            },
            Self::FocusOn { focus, options } => Request::FocusOn {
                focus: *focus,
                options: *options,
            },
            Self::PassingFlash {
                target,
                time_width,
                options,
            } => Request::PassingFlash {
                target,
                time_width: *time_width,
                options: *options,
            },
            Self::DrawBorderThenFill {
                target,
                outline,
                options,
            } => Request::DrawBorderThenFill {
                target,
                outline: *outline,
                options: *options,
            },
            Self::FamilyDrawBorderThenFill {
                target,
                outline,
                options,
            } => Request::FamilyDrawBorderThenFill {
                target,
                outline: *outline,
                options: *options,
            },
            Self::FamilyTextWrite {
                target,
                reverse_member_order,
                options,
            } => Request::FamilyTextWrite {
                target,
                reverse_member_order: *reverse_member_order,
                options: *options,
            },
            Self::TextReveal {
                target,
                reverse,
                options,
            } => Request::TextReveal {
                target,
                reverse: *reverse,
                options: *options,
            },
            Self::AffineLifecycle {
                target,
                direction,
                endpoint,
                options,
            } => Request::AffineLifecycle {
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
    // Wrapper provenance for newly entered objects; membership stays in the Rust request.
    pub(crate) bindings: Vec<Mobject>,
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
            bindings: Vec::new(),
        })
    }
    pub(crate) fn request(&self) -> Request<'_> {
        Request::Composition {
            kind: self.kind,
            children: self.children.iter().map(Child::request).collect(),
            options: self.options,
        }
    }
    fn optional_options(
        duration: Option<f64>,
        easing: Option<String>,
    ) -> PyResult<AnimationOptions> {
        Self::family_options(duration, easing, None)
    }
    fn family_options(
        duration: Option<f64>,
        easing: Option<String>,
        lag: Option<f64>,
    ) -> PyResult<AnimationOptions> {
        Ok(AnimationOptions {
            run_time: duration,
            rate_func: easing.as_deref().map(rate).transpose()?,
            lag_ratio: lag,
            ..AnimationOptions::new()
        })
    }
    fn options(duration: f64, easing: &str) -> PyResult<AnimationOptions> {
        options(duration, easing)
    }
    #[allow(clippy::too_many_arguments)]
    fn dissolve(
        &mut self,
        target: DissolveTarget,
        direction: &str,
        seed: Option<f64>,
        softness: Option<f64>,
        introducer: Option<bool>,
        remover: Option<bool>,
        duration: Option<f64>,
        easing: Option<&str>,
        lag: Option<f64>,
        reverse: bool,
    ) -> PyResult<()> {
        let direction = match direction {
            "in" => noon_core::DissolveDirection::In,
            "out" => noon_core::DissolveDirection::Out,
            _ => return Err(engine_error("invalid dissolve direction")),
        };
        let update = noon::integration::mask_parameters(
            noon::effects::EffectUpdate::default(),
            None,
            softness,
            seed,
        )
        .map_err(engine_error)?;
        let style = noon_core::DissolveStyle::new(update)
            .map_err(noon::AuthoringError::from)
            .map_err(engine_error)?;
        self.children.push(Child::Dissolve(
            target,
            direction,
            style,
            AnimationOptions {
                run_time: duration,
                rate_func: easing.map(rate).transpose()?,
                lag_ratio: lag,
                introducer,
                remover,
                reverse_rate_function: Some(reverse),
                ..AnimationOptions::new()
            },
        ));
        Ok(())
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
    #[pyo3(name = "appendTextWrite", signature = (_id, target, reverse_order, introducer, remover, reverse_rate, duration, easing, lag, play_duration, play_easing))]
    fn text_write(
        &mut self,
        _id: &str,
        target: &MobjectHandle,
        reverse_order: bool,
        introducer: bool,
        remover: bool,
        reverse_rate: bool,
        duration: Option<f64>,
        easing: Option<&str>,
        lag: Option<f64>,
        play_duration: Option<f64>,
        play_easing: Option<&str>,
    ) -> PyResult<()> {
        let options = AnimationOptions {
            run_time: duration,
            rate_func: easing.map(rate).transpose()?,
            lag_ratio: lag,
            ..AnimationOptions::new()
        }
        .with_overrides(AnimationOptions {
            run_time: play_duration,
            rate_func: play_easing.map(rate).transpose()?,
            ..AnimationOptions::new()
        })
        .introducer(introducer)
        .remover(remover)
        .reverse_rate_function(reverse_rate);
        self.children.push(Child::TextWrite(
            target.handle.clone(),
            reverse_order,
            options,
        ));
        if !_id.is_empty() {
            self.bindings.push(target.handle.clone());
        }
        Ok(())
    }

    #[pyo3(name="appendGlowPulseMobject", signature=(target,intensity,duration,easing,lag,reverse))]
    fn glow_pulse(
        &mut self,
        target: &MobjectHandle,
        intensity: f64,
        duration: Option<f64>,
        easing: Option<&str>,
        lag: Option<f64>,
        reverse: bool,
    ) -> PyResult<()> {
        self.children.push(Child::GlowPulse(
            target.handle.clone(),
            intensity,
            AnimationOptions {
                run_time: duration,
                rate_func: easing.map(rate).transpose()?,
                lag_ratio: lag,
                reverse_rate_function: Some(reverse),
                ..AnimationOptions::new()
            },
        ));
        Ok(())
    }
    #[pyo3(name="appendDissolveMobject", signature=(_id,target,direction,seed,softness,introducer,remover,duration,easing,lag,reverse))]
    fn dissolve_object(
        &mut self,
        _id: &str,
        target: &MobjectHandle,
        direction: &str,
        seed: Option<f64>,
        softness: Option<f64>,
        introducer: Option<bool>,
        remover: Option<bool>,
        duration: Option<f64>,
        easing: Option<&str>,
        lag: Option<f64>,
        reverse: bool,
    ) -> PyResult<()> {
        self.dissolve(
            DissolveTarget::Object(target.handle.clone()),
            direction,
            seed,
            softness,
            introducer,
            remover,
            duration,
            easing,
            lag,
            reverse,
        )?;
        if !_id.is_empty() {
            self.bindings.push(target.handle.clone());
        }
        Ok(())
    }
    #[pyo3(name="appendDissolveFamily", signature=(target,direction,seed,softness,introducer,remover,duration,easing,lag,reverse))]
    fn dissolve_family(
        &mut self,
        target: &FamilyHandle,
        direction: &str,
        seed: Option<f64>,
        softness: Option<f64>,
        introducer: Option<bool>,
        remover: Option<bool>,
        duration: Option<f64>,
        easing: Option<&str>,
        lag: Option<f64>,
        reverse: bool,
    ) -> PyResult<()> {
        self.dissolve(
            DissolveTarget::Family(target.family.clone()),
            direction,
            seed,
            softness,
            introducer,
            remover,
            duration,
            easing,
            lag,
            reverse,
        )
    }
    #[pyo3(name = "appendDissolveFamilyEntering")]
    fn dissolve_family_entering(&mut self, _id: &str, member: &MobjectHandle) -> PyResult<()> {
        let Some(Child::Dissolve(DissolveTarget::Family(target), ..)) = self.children.last() else {
            return Err(engine_error("family entering member must follow Dissolve"));
        };
        if !std::rc::Rc::ptr_eq(
            target.integration_store(),
            member.handle.integration_store(),
        ) {
            return Err(engine_error(
                "dissolve family member belongs to another authoring store",
            ));
        }
        // The shared composition admits this family's canonical members. The ID
        // is facade binding provenance, never a second native object identity.
        member.handle.validate().map_err(engine_error)?;
        self.bindings.push(member.handle.clone());
        Ok(())
    }
    #[pyo3(name="appendFamilyReveal",signature=(target,reverse,introducer,remover,reverse_rate,duration,easing,lag,play_duration,play_easing))]
    fn family_reveal(
        &mut self,
        target: &FamilyHandle,
        reverse: bool,
        introducer: Option<bool>,
        remover: Option<bool>,
        reverse_rate: Option<bool>,
        duration: Option<f64>,
        easing: Option<&str>,
        lag: Option<f64>,
        play_duration: Option<f64>,
        play_easing: Option<&str>,
    ) -> PyResult<()> {
        let options = AnimationOptions {
            run_time: duration,
            rate_func: easing.map(rate).transpose()?,
            lag_ratio: lag,
            introducer,
            remover,
            reverse_rate_function: reverse_rate,
            ..AnimationOptions::new()
        }
        .with_overrides(AnimationOptions {
            run_time: play_duration,
            rate_func: play_easing.map(rate).transpose()?,
            ..AnimationOptions::new()
        });
        self.children
            .push(Child::FamilyReveal(target.family.clone(), reverse, options));
        Ok(())
    }
    #[pyo3(name = "appendFamilyRevealEntering")]
    fn family_reveal_entering(&mut self, _id: &str, member: &MobjectHandle) -> PyResult<()> {
        let Some(Child::FamilyReveal(target, ..)) = self.children.last() else {
            return Err(engine_error(
                "family entering member must follow FamilyReveal",
            ));
        };
        if !std::rc::Rc::ptr_eq(
            target.integration_store(),
            member.handle.integration_store(),
        ) {
            return Err(engine_error(
                "family member belongs to another authoring store",
            ));
        }
        member.handle.validate().map_err(engine_error)?;
        self.bindings.push(member.handle.clone());
        Ok(())
    }
    #[pyo3(name = "appendMoveAlongPath")]
    fn move_along_path(
        &mut self,
        target: &MobjectHandle,
        path: &MobjectHandle,
        duration: f64,
        easing: &str,
    ) -> PyResult<()> {
        self.children.push(Child::MoveAlongPath(
            target.handle.clone(),
            path.handle.clone(),
            options(duration, easing)?,
        ));
        Ok(())
    }
    #[pyo3(name="appendValueTracker",signature=(tracker,target,duration,easing))]
    fn value_tracker(
        &mut self,
        tracker: &crate::signals::ValueTrackerHandle,
        target: f64,
        duration: Option<f64>,
        easing: Option<&str>,
    ) -> PyResult<()> {
        if !target.is_finite() {
            return Err(engine_error("ValueTracker target must be finite"));
        }
        self.children.push(Child::ValueTracker(
            tracker.value.clone(),
            target,
            AnimationOptions {
                run_time: duration,
                rate_func: easing.map(rate).transpose()?,
                ..AnimationOptions::new()
            },
        ));
        Ok(())
    }
    #[pyo3(name = "appendIndicateFamily")]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(target,scale_factor,red,green,blue,alpha,child_run_time,rate_function,lag_ratio))]
    pub fn append_indicate_family(
        &mut self,
        target: &FamilyHandle,
        scale_factor: f64,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
        child_run_time: Option<f64>,
        rate_function: Option<String>,
        lag_ratio: Option<f64>,
    ) -> Result<(), PyErr> {
        let color = callback_color(
            "indication color",
            Some(red),
            Some(green),
            Some(blue),
            Some(alpha),
        )?
        .expect("all indication color channels were supplied");
        self.children.push(Child::FamilyIndicate {
            target: target.family.clone(),
            indication: noon::IndicateOptions::new(scale_factor, color),
            options: Self::family_options(child_run_time, rate_function, lag_ratio)?,
        });
        Ok(())
    }
    #[pyo3(name = "appendFocusOn")]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(x,y,opacity,red,green,blue,child_run_time,rate_function))]
    pub fn append_focus_on(
        &mut self,
        x: f64,
        y: f64,
        opacity: f64,
        red: f64,
        green: f64,
        blue: f64,
        child_run_time: Option<f64>,
        rate_function: Option<String>,
    ) -> Result<(), PyErr> {
        let color = callback_color("focus color", Some(red), Some(green), Some(blue), Some(1.0))?
            .expect("all focus color channels supplied");
        self.children.push(Child::FocusOn {
            focus: noon::FocusOnOptions {
                point: (x, y),
                opacity,
                color,
            },
            options: Self::optional_options(child_run_time, rate_function)?,
        });
        Ok(())
    }
    #[pyo3(name = "appendPassingFlash")]
    #[pyo3(signature=(_object_id,target,time_width,child_run_time,rate_function))]
    pub fn append_passing_flash(
        &mut self,
        _object_id: &str,
        target: &MobjectHandle,
        time_width: f64,
        child_run_time: Option<f64>,
        rate_function: Option<String>,
    ) -> Result<(), PyErr> {
        self.children.push(Child::PassingFlash {
            target: target.handle.clone(),
            time_width,
            options: Self::optional_options(child_run_time, rate_function)?
                .introducer(true)
                .remover(true),
        });
        if !_object_id.is_empty() {
            self.bindings.push(target.handle.clone());
        }
        Ok(())
    }
    #[pyo3(name = "appendDrawBorderThenFillMobject")]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(_object_id,target,stroke_width,red,green,blue,alpha,phase_rate_function,introducer,child_run_time,rate_function))]
    pub fn append_draw_border_then_fill_mobject(
        &mut self,
        _object_id: &str,
        target: &MobjectHandle,
        stroke_width: f64,
        red: Option<f64>,
        green: Option<f64>,
        blue: Option<f64>,
        alpha: Option<f64>,
        phase_rate_function: &str,
        introducer: bool,
        child_run_time: f64,
        rate_function: &str,
    ) -> Result<(), PyErr> {
        let color = callback_color("outline color", red, green, blue, alpha)?;
        let phase =
            noon_core::RateFunction::from_semantic_id(phase_rate_function).ok_or_else(|| {
                engine_error(format!(
                    "unsupported outline phase rate function {phase_rate_function:?}"
                ))
            })?;
        self.children.push(Child::DrawBorderThenFill {
            target: target.handle.clone(),
            outline: noon::DrawBorderThenFillOptions::new(stroke_width, color)
                .with_phase_rate_function(phase),
            options: Self::options(child_run_time, rate_function)?.introducer(introducer),
        });
        if !_object_id.is_empty() {
            self.bindings.push(target.handle.clone());
        }
        Ok(())
    }
    #[pyo3(name = "appendDrawBorderThenFillFamily")]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(target,stroke_width,red,green,blue,alpha,phase_rate_function,introducer,child_run_time,rate_function,lag_ratio))]
    pub fn append_draw_border_then_fill_family(
        &mut self,
        target: &FamilyHandle,
        stroke_width: f64,
        red: Option<f64>,
        green: Option<f64>,
        blue: Option<f64>,
        alpha: Option<f64>,
        phase_rate_function: &str,
        introducer: bool,
        child_run_time: Option<f64>,
        rate_function: Option<String>,
        lag_ratio: Option<f64>,
    ) -> Result<(), PyErr> {
        let color = callback_color("outline color", red, green, blue, alpha)?;
        let phase =
            noon_core::RateFunction::from_semantic_id(phase_rate_function).ok_or_else(|| {
                engine_error(format!(
                    "unsupported outline phase rate function {phase_rate_function:?}"
                ))
            })?;
        self.children.push(Child::FamilyDrawBorderThenFill {
            target: target.family.clone(),
            outline: noon::DrawBorderThenFillOptions::new(stroke_width, color)
                .with_phase_rate_function(phase),
            options: Self::family_options(child_run_time, rate_function, lag_ratio)?
                .introducer(introducer),
        });
        Ok(())
    }
    #[pyo3(name = "appendFamilyTextWrite")]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(target,reverse_member_order,introducer,remover,reverse_rate_function,child_run_time,rate_function,lag_ratio,play_run_time,play_rate_function))]
    pub fn append_family_text_write(
        &mut self,
        target: &FamilyHandle,
        reverse_member_order: bool,
        introducer: bool,
        remover: bool,
        reverse_rate_function: bool,
        child_run_time: Option<f64>,
        rate_function: Option<String>,
        lag_ratio: Option<f64>,
        play_run_time: Option<f64>,
        play_rate_function: Option<String>,
    ) -> Result<(), PyErr> {
        let options = Self::family_options(child_run_time, rate_function, lag_ratio)?
            .with_overrides(Self::optional_options(play_run_time, play_rate_function)?)
            .introducer(introducer)
            .remover(remover)
            .reverse_rate_function(reverse_rate_function);
        self.children.push(Child::FamilyTextWrite {
            target: target.family.clone(),
            reverse_member_order,
            options,
        });
        Ok(())
    }
    #[pyo3(name = "appendTextReveal")]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(_object_id,target,reverse,introducer,remover,reverse_rate_function,child_run_time,rate_function,lag_ratio,play_run_time,play_rate_function))]
    pub fn append_text_reveal(
        &mut self,
        _object_id: &str,
        target: &MobjectHandle,
        reverse: bool,
        introducer: Option<bool>,
        remover: Option<bool>,
        reverse_rate_function: Option<bool>,
        child_run_time: Option<f64>,
        rate_function: Option<String>,
        lag_ratio: Option<f64>,
        play_run_time: Option<f64>,
        play_rate_function: Option<String>,
    ) -> Result<(), PyErr> {
        let mut options = Self::family_options(child_run_time, rate_function, lag_ratio)?
            .with_overrides(Self::optional_options(play_run_time, play_rate_function)?);
        if let Some(introducer) = introducer {
            options = options.introducer(introducer);
        }
        if let Some(remover) = remover {
            options = options.remover(remover);
        }
        if let Some(reverse_rate_function) = reverse_rate_function {
            options = options.reverse_rate_function(reverse_rate_function);
        }
        self.children.push(Child::TextReveal {
            target: target.handle.clone(),
            reverse,
            options,
        });
        if !_object_id.is_empty() {
            self.bindings.push(target.handle.clone());
        }
        Ok(())
    }
    #[pyo3(name = "appendFamilyFade")]
    #[pyo3(signature=(target,direction,child_run_time,rate_function,lag_ratio))]
    pub fn append_family_fade(
        &mut self,
        target: &FamilyHandle,
        direction: &str,
        child_run_time: Option<f64>,
        rate_function: Option<String>,
        lag_ratio: Option<f64>,
    ) -> Result<(), PyErr> {
        self.children.push(Child::FamilyFade {
            target: target.family.clone(),
            direction: match direction {
                "in" => SemanticFadeDirection::In,
                "out" => SemanticFadeDirection::Out,
                _ => return Err(engine_error("invalid fade direction")),
            },
            options: Self::family_options(child_run_time, rate_function, lag_ratio)?,
        });
        Ok(())
    }
    #[pyo3(name="appendAffineLifecycle",signature=(_object_id,target,direction,endpoint,x,y,rotation_offset,point_red,point_green,point_blue,point_alpha,child_run_time,rate_function))]
    fn append_affine_lifecycle(
        &mut self,
        _object_id: &str,
        target: &MobjectHandle,
        direction: &str,
        endpoint: &str,
        x: f64,
        y: f64,
        rotation_offset: f64,
        point_red: Option<f64>,
        point_green: Option<f64>,
        point_blue: Option<f64>,
        point_alpha: Option<f64>,
        child_run_time: f64,
        rate_function: &str,
    ) -> Result<(), PyErr> {
        let direction = parse_affine_lifecycle_direction(direction)?;
        let endpoint = parse_affine_lifecycle_endpoint(
            endpoint,
            x,
            y,
            rotation_offset,
            callback_color(
                "point_color",
                point_red,
                point_green,
                point_blue,
                point_alpha,
            )?,
        )?;
        let options = Self::options(child_run_time, rate_function)?;
        self.children.push(Child::AffineLifecycle {
            target: target.handle.clone(),
            direction,
            endpoint,
            options,
        });
        if !_object_id.is_empty() {
            self.bindings.push(target.handle.clone());
        }
        Ok(())
    }
    #[pyo3(name = "appendFamilyFadeEntering")]
    fn append_family_fade_entering(&mut self, _id: &str, member: &MobjectHandle) -> PyResult<()> {
        let Some(Child::FamilyFade { target, .. }) = self.children.last() else {
            return Err(engine_error(
                "family entering member must follow FamilyFade",
            ));
        };
        if !std::rc::Rc::ptr_eq(
            target.integration_store(),
            member.handle.integration_store(),
        ) {
            return Err(engine_error(
                "family entering member belongs to another authoring store",
            ));
        }
        member.handle.validate().map_err(engine_error)?;
        self.bindings.push(member.handle.clone());
        Ok(())
    }
    #[pyo3(name = "appendDrawBorderThenFillFamilyEntering")]
    fn append_draw_border_then_fill_family_entering(
        &mut self,
        _id: &str,
        member: &MobjectHandle,
    ) -> PyResult<()> {
        let Some(Child::FamilyDrawBorderThenFill { target, .. }) = self.children.last() else {
            return Err(engine_error(
                "family entering member must follow FamilyDrawBorderThenFill",
            ));
        };
        if !std::rc::Rc::ptr_eq(
            target.integration_store(),
            member.handle.integration_store(),
        ) {
            return Err(engine_error(
                "family entering member belongs to another authoring store",
            ));
        }
        member.handle.validate().map_err(engine_error)?;
        self.bindings.push(member.handle.clone());
        Ok(())
    }
    #[pyo3(name = "appendFamilyTextWriteEntering")]
    fn append_family_text_write_entering(
        &mut self,
        _id: &str,
        member: &MobjectHandle,
    ) -> PyResult<()> {
        let Some(Child::FamilyTextWrite { target, .. }) = self.children.last() else {
            return Err(engine_error(
                "family entering member must follow FamilyTextWrite",
            ));
        };
        if !std::rc::Rc::ptr_eq(
            target.integration_store(),
            member.handle.integration_store(),
        ) {
            return Err(engine_error(
                "family entering member belongs to another authoring store",
            ));
        }
        member.handle.validate().map_err(engine_error)?;
        self.bindings.push(member.handle.clone());
        Ok(())
    }
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
        self.bindings.extend(child.bindings.iter().cloned());
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
    #[pyo3(name = "appendFamilyTransformTo")]
    fn family_transform(
        &mut self,
        source: &FamilyHandle,
        target: &FamilyHandle,
        duration: Option<f64>,
        easing: Option<&str>,
        lag: Option<f64>,
        arc: Option<f64>,
    ) -> PyResult<()> {
        self.children.push(Child::FamilyTransform {
            source: source.family.clone(),
            target: target.family.clone(),
            method: false,
            options: AnimationOptions {
                run_time: duration,
                rate_func: easing.map(rate).transpose()?,
                lag_ratio: lag,
                path_arc: arc,
                ..AnimationOptions::new()
            },
        });
        Ok(())
    }
    #[pyo3(name = "appendFamilyMethodTargetTo")]
    fn family_method_target(
        &mut self,
        source: &FamilyHandle,
        target: &FamilyHandle,
        duration: Option<f64>,
        easing: Option<&str>,
        lag: Option<f64>,
        arc: Option<f64>,
    ) -> PyResult<()> {
        self.family_transform(source, target, duration, easing, lag, arc)?;
        let Some(Child::FamilyTransform { method, .. }) = self.children.last_mut() else {
            unreachable!("family target append always stages one family child");
        };
        *method = true;
        Ok(())
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
        self.transform(source, target, false, false, duration, easing, arc, reverse)?;
        if !_id.is_empty() {
            self.bindings.push(source.handle.clone());
        }
        Ok(())
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
        self.transform(source, target, true, false, duration, easing, arc, reverse)?;
        if !_id.is_empty() {
            self.bindings.push(source.handle.clone());
        }
        Ok(())
    }
    #[pyo3(name = "appendCreate")]
    fn create(
        &mut self,
        _id: &str,
        target: &MobjectHandle,
        duration: f64,
        easing: &str,
        reverse: bool,
    ) -> PyResult<()> {
        self.children.push(Child::Create(
            target.handle.clone(),
            options(duration, easing)?.reverse_rate_function(reverse),
        ));
        if !_id.is_empty() {
            self.bindings.push(target.handle.clone());
        }
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
        if !_id.is_empty() {
            self.bindings.push(target.handle.clone());
        }
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
        if !_id.is_empty() {
            self.bindings.push(target.handle.clone());
        }
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

/// The declaration itself retains its Rust store identity and validates at activation.
#[pyclass(unsendable, module = "_noon_native")]
pub struct DeclaredAnimation {
    pub(crate) value: noon::DeclaredAnimation,
}

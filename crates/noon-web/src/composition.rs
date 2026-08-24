use noon_core::{
    resolve_composition_schedule as resolve_core_composition_schedule,
    resolve_uniform_composition_schedule as resolve_core_uniform_composition_schedule,
    CompositionError, CompositionSchedule,
};

#[derive(Clone, Debug, PartialEq)]
pub struct FrontendCompositionResolution {
    ok: bool,
    schedule: Option<CompositionSchedule>,
    error_kind: Option<String>,
    message: Option<String>,
}

impl FrontendCompositionResolution {
    fn success(schedule: CompositionSchedule) -> Self {
        Self {
            ok: true,
            schedule: Some(schedule),
            error_kind: None,
            message: None,
        }
    }

    fn failure(kind: &str, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            schedule: None,
            error_kind: Some(kind.to_owned()),
            message: Some(message.into()),
        }
    }

    pub const fn ok(&self) -> bool {
        self.ok
    }

    pub fn schedule(&self) -> Option<&CompositionSchedule> {
        self.schedule.as_ref()
    }

    pub fn error_kind(&self) -> Option<String> {
        self.error_kind.clone()
    }

    pub fn message(&self) -> Option<String> {
        self.message.clone()
    }
}

fn failure(error: CompositionError) -> FrontendCompositionResolution {
    let kind = match error {
        CompositionError::Empty => "empty",
        CompositionError::InvalidLagRatio(_) => "invalid_lag_ratio",
        CompositionError::InvalidChildRunTime { .. } => "invalid_child_run_time",
        CompositionError::InvalidRunTime(_) => "invalid_run_time",
    };
    FrontendCompositionResolution::failure(kind, error.to_string())
}

pub fn resolve_frontend_composition_schedule(
    child_run_times: &[f64],
    lag_ratio: f64,
    run_time: Option<f64>,
) -> FrontendCompositionResolution {
    match resolve_core_composition_schedule(child_run_times, lag_ratio, run_time) {
        Ok(schedule) => FrontendCompositionResolution::success(schedule),
        Err(error) => failure(error),
    }
}

pub fn resolve_frontend_uniform_composition_schedule(
    child_count: usize,
    lag_ratio: f64,
    run_time: f64,
) -> FrontendCompositionResolution {
    match resolve_core_uniform_composition_schedule(child_count, lag_ratio, run_time) {
        Ok(schedule) => FrontendCompositionResolution::success(schedule),
        Err(error) => failure(error),
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{
        resolve_frontend_composition_schedule, resolve_frontend_uniform_composition_schedule,
        FrontendCompositionResolution,
    };

    #[wasm_bindgen]
    pub struct WasmCompositionResolution(FrontendCompositionResolution);

    #[wasm_bindgen]
    impl WasmCompositionResolution {
        #[wasm_bindgen(getter)]
        pub fn ok(&self) -> bool {
            self.0.ok()
        }

        #[wasm_bindgen(getter, js_name = runTime)]
        pub fn run_time(&self) -> f64 {
            self.0.schedule().map_or(0.0, |schedule| schedule.run_time)
        }

        #[wasm_bindgen(getter, js_name = intrinsicRunTime)]
        pub fn intrinsic_run_time(&self) -> f64 {
            self.0
                .schedule()
                .map_or(0.0, |schedule| schedule.intrinsic_run_time)
        }

        #[wasm_bindgen(getter)]
        pub fn length(&self) -> usize {
            self.0
                .schedule()
                .map_or(0, |schedule| schedule.intervals.len())
        }

        #[wasm_bindgen(js_name = startTime)]
        pub fn start_time(&self, index: usize) -> f64 {
            self.0
                .schedule()
                .and_then(|schedule| schedule.intervals.get(index))
                .map_or(f64::NAN, |interval| interval.start_time)
        }

        #[wasm_bindgen(js_name = duration)]
        pub fn duration(&self, index: usize) -> f64 {
            self.0
                .schedule()
                .and_then(|schedule| schedule.intervals.get(index))
                .map_or(f64::NAN, |interval| interval.duration)
        }

        #[wasm_bindgen(js_name = endTime)]
        pub fn end_time(&self, index: usize) -> f64 {
            self.0
                .schedule()
                .and_then(|schedule| schedule.intervals.get(index))
                .map_or(f64::NAN, |interval| interval.end_time())
        }

        #[wasm_bindgen(getter, js_name = errorKind)]
        pub fn error_kind(&self) -> Option<String> {
            self.0.error_kind()
        }

        #[wasm_bindgen(getter)]
        pub fn message(&self) -> Option<String> {
            self.0.message()
        }
    }

    #[wasm_bindgen(js_name = resolveCompositionSchedule)]
    pub fn resolve_composition_schedule(
        child_run_times: Box<[f64]>,
        lag_ratio: f64,
        run_time: f64,
    ) -> WasmCompositionResolution {
        let run_time = (!run_time.is_nan()).then_some(run_time);
        WasmCompositionResolution(resolve_frontend_composition_schedule(
            &child_run_times,
            lag_ratio,
            run_time,
        ))
    }

    #[wasm_bindgen(js_name = resolveUniformCompositionSchedule)]
    pub fn resolve_uniform_composition_schedule(
        child_count: usize,
        lag_ratio: f64,
        run_time: f64,
    ) -> WasmCompositionResolution {
        WasmCompositionResolution(resolve_frontend_uniform_composition_schedule(
            child_count,
            lag_ratio,
            run_time,
        ))
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_bridge_preserves_unequal_child_timing() {
        let resolved = resolve_frontend_composition_schedule(&[10.0, 1.0], 0.1, Some(5.0));
        assert!(resolved.ok());
        let schedule = resolved.schedule().unwrap();
        assert_eq!(schedule.intrinsic_run_time, 10.0);
        assert_eq!(schedule.intervals[0].duration, 5.0);
        assert_eq!(schedule.intervals[1].start_time, 0.5);
        assert_eq!(schedule.intervals[1].duration, 0.5);
    }

    #[test]
    fn browser_uniform_bridge_matches_family_lowering() {
        let resolved = resolve_frontend_uniform_composition_schedule(3, 0.5, 1.2);
        assert!(resolved.ok());
        let schedule = resolved.schedule().unwrap();
        assert!((schedule.intervals[0].duration - 0.6).abs() < 1e-12);
        assert!((schedule.intervals[2].start_time - 0.6).abs() < 1e-12);
    }

    #[test]
    fn browser_bridge_preserves_shared_validation_errors() {
        let resolved = resolve_frontend_composition_schedule(&[1.0], -0.1, None);
        assert!(!resolved.ok());
        assert_eq!(resolved.error_kind().as_deref(), Some("invalid_lag_ratio"));
    }
}

use noon_core::{
    resolve_lifecycle_plan as resolve_core_lifecycle_plan,
    validate_presence_transition as validate_core_presence_transition, LifecycleBinding, LifecycleError,
    LifecycleIntent, LifecyclePlan, LifecycleState, PresenceTransitionError,
};

#[derive(Clone, Debug, PartialEq)]
pub struct FrontendLifecycleResolution {
    ok: bool,
    plan: LifecyclePlan,
    error_kind: Option<String>,
    message: Option<String>,
}

impl FrontendLifecycleResolution {
    fn success(plan: LifecyclePlan) -> Self {
        Self {
            ok: true,
            plan,
            error_kind: None,
            message: None,
        }
    }

    fn failure(kind: &str, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            plan: LifecyclePlan::default(),
            error_kind: Some(kind.to_owned()),
            message: Some(message.into()),
        }
    }

    pub const fn ok(&self) -> bool {
        self.ok
    }

    pub const fn plan(&self) -> LifecyclePlan {
        self.plan
    }

    pub fn error_kind(&self) -> Option<String> {
        self.error_kind.clone()
    }

    pub fn message(&self) -> Option<String> {
        self.message.clone()
    }
}

fn parse_intent(value: &str) -> Option<LifecycleIntent> {
    match value {
        "add" => Some(LifecycleIntent::Add),
        "remove" => Some(LifecycleIntent::Remove),
        "introduce" => Some(LifecycleIntent::Introduce),
        "remove_after_animation" => Some(LifecycleIntent::RemoveAfterAnimation),
        "require_present" => Some(LifecycleIntent::RequirePresent),
        "require_available_target" => Some(LifecycleIntent::RequireAvailableTarget),
        _ => None,
    }
}

fn parse_binding(value: &str) -> Option<LifecycleBinding> {
    match value {
        "detached" => Some(LifecycleBinding::Detached),
        "this_scene" => Some(LifecycleBinding::ThisScene),
        "other_scene" => Some(LifecycleBinding::OtherScene),
        _ => None,
    }
}

pub fn resolve_frontend_lifecycle_plan(
    intent: &str,
    binding: &str,
    has_presence_timeline: bool,
    present: bool,
    has_future_event: bool,
    at_time_zero: bool,
) -> FrontendLifecycleResolution {
    let Some(intent) = parse_intent(intent) else {
        return FrontendLifecycleResolution::failure(
            "invalid_intent",
            format!("unsupported lifecycle intent: {intent}"),
        );
    };
    let Some(binding) = parse_binding(binding) else {
        return FrontendLifecycleResolution::failure(
            "invalid_binding",
            format!("unsupported lifecycle binding: {binding}"),
        );
    };

    let state = LifecycleState {
        binding,
        has_presence_timeline,
        present,
        has_future_event,
        at_time_zero,
    };
    match resolve_core_lifecycle_plan(intent, state) {
        Ok(plan) => FrontendLifecycleResolution::success(plan),
        Err(error) => {
            let kind = match error {
                LifecycleError::BelongsToAnotherScene => "other_scene",
                LifecycleError::RequiresBoundObject => "requires_bound",
                LifecycleError::FutureLifecycleEvent => "future_event",
                LifecycleError::RequiresPresent => "requires_present",
                LifecycleError::RequiresAbsent => "requires_absent",
            };
            FrontendLifecycleResolution::failure(kind, error.to_string())
        }
    }
}

pub fn validate_frontend_presence_transition(
    has_previous: bool,
    previous_time: f64,
    previous_to: bool,
    time: f64,
    from: bool,
) -> FrontendLifecycleResolution {
    let previous = has_previous.then_some((previous_time, previous_to));
    match validate_core_presence_transition(previous, time, from) {
        Ok(()) => FrontendLifecycleResolution::success(LifecyclePlan::default()),
        Err(error) => {
            let kind = match error {
                PresenceTransitionError::InvalidTime => "invalid_time",
                PresenceTransitionError::OutOfOrder => "out_of_order",
                PresenceTransitionError::Discontinuous => "discontinuous",
            };
            FrontendLifecycleResolution::failure(kind, error.to_string())
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{
        resolve_frontend_lifecycle_plan, validate_frontend_presence_transition,
        FrontendLifecycleResolution,
    };

    #[wasm_bindgen]
    pub struct WasmLifecycleResolution(FrontendLifecycleResolution);

    #[wasm_bindgen]
    impl WasmLifecycleResolution {
        #[wasm_bindgen(getter)]
        pub fn ok(&self) -> bool {
            self.0.ok()
        }

        #[wasm_bindgen(getter)]
        pub fn bind(&self) -> bool {
            self.0.plan().bind
        }

        #[wasm_bindgen(getter, js_name = showNow)]
        pub fn show_now(&self) -> bool {
            self.0.plan().show_now
        }

        #[wasm_bindgen(getter, js_name = hideNow)]
        pub fn hide_now(&self) -> bool {
            self.0.plan().hide_now
        }

        #[wasm_bindgen(getter, js_name = showAtStart)]
        pub fn show_at_start(&self) -> bool {
            self.0.plan().show_at_start
        }

        #[wasm_bindgen(getter, js_name = hideAtEnd)]
        pub fn hide_at_end(&self) -> bool {
            self.0.plan().hide_at_end
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

    #[wasm_bindgen(js_name = resolveLifecyclePlan)]
    pub fn resolve_lifecycle_plan(
        intent: &str,
        binding: &str,
        has_presence_timeline: bool,
        present: bool,
        has_future_event: bool,
        at_time_zero: bool,
    ) -> WasmLifecycleResolution {
        WasmLifecycleResolution(resolve_frontend_lifecycle_plan(
            intent,
            binding,
            has_presence_timeline,
            present,
            has_future_event,
            at_time_zero,
        ))
    }

    #[wasm_bindgen(js_name = validatePresenceTransition)]
    pub fn validate_presence_transition(
        has_previous: bool,
        previous_time: f64,
        previous_to: bool,
        time: f64,
        from: bool,
    ) -> WasmLifecycleResolution {
        WasmLifecycleResolution(validate_frontend_presence_transition(
            has_previous,
            previous_time,
            previous_to,
            time,
            from,
        ))
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_bridge_uses_shared_reintroduction_rules() {
        let resolved = resolve_frontend_lifecycle_plan(
            "add",
            "this_scene",
            true,
            false,
            false,
            false,
        );
        assert!(resolved.ok());
        assert!(resolved.plan().show_now);
    }

    #[test]
    fn browser_bridge_preserves_lifecycle_errors() {
        let resolved = resolve_frontend_lifecycle_plan(
            "introduce",
            "this_scene",
            true,
            true,
            false,
            false,
        );
        assert!(!resolved.ok());
        assert_eq!(resolved.error_kind().as_deref(), Some("requires_absent"));
    }

    #[test]
    fn browser_bridge_validates_presence_chain_continuity() {
        let resolved = validate_frontend_presence_transition(true, 1.0, false, 2.0, true);
        assert!(!resolved.ok());
        assert_eq!(resolved.error_kind().as_deref(), Some("discontinuous"));
    }
}

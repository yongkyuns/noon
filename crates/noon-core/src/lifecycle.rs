use serde::{Deserialize, Serialize};

/// Where an authoring object currently belongs relative to the scene handling it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleBinding {
    Detached,
    ThisScene,
    OtherScene,
}

/// High-level lifecycle operation requested by an authoring frontend.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifecycleIntent {
    /// `Scene.add`: bind detached objects, or reintroduce an absent bound object.
    Add,
    /// `Scene.remove`: hide a present object immediately; unrelated objects are no-ops.
    Remove,
    /// `Create` / `FadeIn`: bind detached objects and introduce them at animation start.
    Introduce,
    /// `FadeOut` and equivalent removers: require presence and hide at animation end.
    RemoveAfterAnimation,
    /// Transform/replacement source requirement.
    RequirePresent,
    /// Replacement/copy/matching target requirement.
    RequireAvailableTarget,
}

/// Minimal transient lifecycle facts needed to make a cross-language decision.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LifecycleState {
    pub binding: LifecycleBinding,
    pub has_presence_timeline: bool,
    pub present: bool,
    pub has_future_event: bool,
    pub at_time_zero: bool,
}

impl LifecycleState {
    pub const fn detached(at_time_zero: bool) -> Self {
        Self {
            binding: LifecycleBinding::Detached,
            has_presence_timeline: false,
            present: true,
            has_future_event: false,
            at_time_zero,
        }
    }

    pub const fn bound(
        has_presence_timeline: bool,
        present: bool,
        has_future_event: bool,
        at_time_zero: bool,
    ) -> Self {
        Self {
            binding: LifecycleBinding::ThisScene,
            has_presence_timeline,
            present,
            has_future_event,
            at_time_zero,
        }
    }
}

/// Actions emitted by the shared lifecycle planner.
///
/// Frontends remain responsible only for binding their language wrapper to the
/// canonical scene object and writing the indicated presence transition at the
/// current animation boundary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecyclePlan {
    pub bind: bool,
    pub show_now: bool,
    pub hide_now: bool,
    pub show_at_start: bool,
    pub hide_at_end: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LifecycleError {
    BelongsToAnotherScene,
    RequiresBoundObject,
    FutureLifecycleEvent,
    RequiresPresent,
    RequiresAbsent,
}

impl std::fmt::Display for LifecycleError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BelongsToAnotherScene => formatter.write_str("object already belongs to another Scene"),
            Self::RequiresBoundObject => formatter.write_str("lifecycle operation requires an object bound to this Scene"),
            Self::FutureLifecycleEvent => formatter.write_str(
                "object has a future lifecycle event; lifecycle operations must be authored chronologically",
            ),
            Self::RequiresPresent => formatter.write_str("lifecycle operation requires the object to be present"),
            Self::RequiresAbsent => formatter.write_str("lifecycle operation requires the object to be absent"),
        }
    }
}

impl std::error::Error for LifecycleError {}

/// Resolve one lifecycle operation using the shared Manim-compatible authoring rules.
pub fn resolve_lifecycle_plan(
    intent: LifecycleIntent,
    state: LifecycleState,
) -> Result<LifecyclePlan, LifecycleError> {
    use LifecycleBinding::{Detached, OtherScene, ThisScene};
    use LifecycleIntent::{
        Add, Introduce, Remove, RemoveAfterAnimation, RequireAvailableTarget, RequirePresent,
    };

    if state.binding == OtherScene {
        return match intent {
            Remove => Ok(LifecyclePlan::default()),
            _ => Err(LifecycleError::BelongsToAnotherScene),
        };
    }

    if state.binding == ThisScene && state.has_future_event {
        return Err(LifecycleError::FutureLifecycleEvent);
    }

    match intent {
        Add => match state.binding {
            Detached => Ok(LifecyclePlan {
                bind: true,
                show_now: !state.at_time_zero,
                ..LifecyclePlan::default()
            }),
            ThisScene => Ok(LifecyclePlan {
                show_now: state.has_presence_timeline && !state.present,
                ..LifecyclePlan::default()
            }),
            OtherScene => unreachable!(),
        },
        Remove => match state.binding {
            Detached => Ok(LifecyclePlan::default()),
            ThisScene => Ok(LifecyclePlan {
                hide_now: state.present,
                ..LifecyclePlan::default()
            }),
            OtherScene => unreachable!(),
        },
        Introduce => match state.binding {
            Detached => Ok(LifecyclePlan {
                bind: true,
                show_at_start: true,
                ..LifecyclePlan::default()
            }),
            ThisScene => {
                if state.has_presence_timeline && state.present {
                    Err(LifecycleError::RequiresAbsent)
                } else {
                    Ok(LifecyclePlan {
                        show_at_start: true,
                        ..LifecyclePlan::default()
                    })
                }
            }
            OtherScene => unreachable!(),
        },
        RemoveAfterAnimation => match state.binding {
            Detached => Err(LifecycleError::RequiresBoundObject),
            ThisScene if !state.present => Err(LifecycleError::RequiresPresent),
            ThisScene => Ok(LifecyclePlan {
                hide_at_end: true,
                ..LifecyclePlan::default()
            }),
            OtherScene => unreachable!(),
        },
        RequirePresent => match state.binding {
            Detached => Err(LifecycleError::RequiresBoundObject),
            ThisScene if !state.present => Err(LifecycleError::RequiresPresent),
            ThisScene => Ok(LifecyclePlan::default()),
            OtherScene => unreachable!(),
        },
        RequireAvailableTarget => match state.binding {
            Detached => Err(LifecycleError::RequiresBoundObject),
            ThisScene if state.has_presence_timeline && state.present => {
                Err(LifecycleError::RequiresAbsent)
            }
            ThisScene => Ok(LifecyclePlan::default()),
            OtherScene => unreachable!(),
        },
    }
}

/// Validate continuity and chronology before appending a zero-duration presence event.
pub fn validate_presence_transition(
    previous: Option<(f64, bool)>,
    time: f64,
    from: bool,
) -> Result<(), PresenceTransitionError> {
    if !time.is_finite() || time < 0.0 {
        return Err(PresenceTransitionError::InvalidTime);
    }
    if let Some((previous_time, previous_to)) = previous {
        if time < previous_time {
            return Err(PresenceTransitionError::OutOfOrder);
        }
        if previous_to != from {
            return Err(PresenceTransitionError::Discontinuous);
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresenceTransitionError {
    InvalidTime,
    OutOfOrder,
    Discontinuous,
}

impl std::fmt::Display for PresenceTransitionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTime => formatter.write_str("presence event time must be finite and non-negative"),
            Self::OutOfOrder => formatter.write_str("presence events must be scheduled in chronological order"),
            Self::Discontinuous => formatter.write_str("presence event chain must be continuous"),
        }
    }
}

impl std::error::Error for PresenceTransitionError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_binds_detached_and_only_backfills_absence_after_time_zero() {
        assert_eq!(
            resolve_lifecycle_plan(LifecycleIntent::Add, LifecycleState::detached(true)).unwrap(),
            LifecyclePlan {
                bind: true,
                ..LifecyclePlan::default()
            }
        );
        assert_eq!(
            resolve_lifecycle_plan(LifecycleIntent::Add, LifecycleState::detached(false)).unwrap(),
            LifecyclePlan {
                bind: true,
                show_now: true,
                ..LifecyclePlan::default()
            }
        );
    }

    #[test]
    fn readd_and_remove_are_idempotent_membership_operations() {
        let absent = LifecycleState::bound(true, false, false, false);
        assert!(resolve_lifecycle_plan(LifecycleIntent::Add, absent).unwrap().show_now);
        assert!(!resolve_lifecycle_plan(LifecycleIntent::Remove, absent).unwrap().hide_now);

        let present = LifecycleState::bound(true, true, false, false);
        assert!(!resolve_lifecycle_plan(LifecycleIntent::Add, present).unwrap().show_now);
        assert!(resolve_lifecycle_plan(LifecycleIntent::Remove, present).unwrap().hide_now);
    }

    #[test]
    fn introducers_allow_initial_objects_but_reject_present_reintroductions() {
        let initial = LifecycleState::bound(false, true, false, true);
        assert!(
            resolve_lifecycle_plan(LifecycleIntent::Introduce, initial)
                .unwrap()
                .show_at_start
        );
        assert_eq!(
            resolve_lifecycle_plan(
                LifecycleIntent::Introduce,
                LifecycleState::bound(true, true, false, false),
            ),
            Err(LifecycleError::RequiresAbsent)
        );
    }

    #[test]
    fn removers_and_source_target_requirements_share_presence_rules() {
        let present = LifecycleState::bound(true, true, false, false);
        assert!(
            resolve_lifecycle_plan(LifecycleIntent::RemoveAfterAnimation, present)
                .unwrap()
                .hide_at_end
        );
        assert!(resolve_lifecycle_plan(LifecycleIntent::RequirePresent, present).is_ok());

        let absent = LifecycleState::bound(true, false, false, false);
        assert_eq!(
            resolve_lifecycle_plan(LifecycleIntent::RemoveAfterAnimation, absent),
            Err(LifecycleError::RequiresPresent)
        );
        assert!(resolve_lifecycle_plan(LifecycleIntent::RequireAvailableTarget, absent).is_ok());
    }

    #[test]
    fn future_events_are_rejected_before_operation_specific_rules() {
        let state = LifecycleState::bound(true, false, true, false);
        assert_eq!(
            resolve_lifecycle_plan(LifecycleIntent::Add, state),
            Err(LifecycleError::FutureLifecycleEvent)
        );
    }

    #[test]
    fn presence_transition_validation_is_shared() {
        assert!(validate_presence_transition(None, 0.0, false).is_ok());
        assert!(validate_presence_transition(Some((1.0, true)), 1.0, true).is_ok());
        assert_eq!(
            validate_presence_transition(Some((2.0, true)), 1.0, true),
            Err(PresenceTransitionError::OutOfOrder)
        );
        assert_eq!(
            validate_presence_transition(Some((1.0, false)), 2.0, true),
            Err(PresenceTransitionError::Discontinuous)
        );
    }
}

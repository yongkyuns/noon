use serde::{Deserialize, Serialize};

/// Host-owned identity for one callable-table entry in an execution context.
///
/// This is not semantic or globally allocated identity. The callback implementation
/// stays in the host language, and the same ID may appear in multiple semantic
/// registration occurrences and on multiple targets within its owning context.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct HostCallbackId(u64);

impl HostCallbackId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One authored occurrence of a host updater on a semantic object or family.
///
/// Reusing a callback ID creates another occurrence. The interval is inclusive at
/// `active_from` and exclusive at `inactive_from`, matching authored frame-time
/// boundaries without making the host callable table a scheduling authority.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SemanticUpdaterRegistration {
    callback: HostCallbackId,
    active_from: f64,
    inactive_from: Option<f64>,
}

impl SemanticUpdaterRegistration {
    pub fn new(
        callback: HostCallbackId,
        active_from: f64,
        inactive_from: Option<f64>,
    ) -> Result<Self, SemanticUpdaterRegistrationError> {
        let registration = Self {
            callback,
            active_from,
            inactive_from,
        };
        registration.validate()?;
        Ok(registration)
    }

    pub const fn callback(self) -> HostCallbackId {
        self.callback
    }

    pub const fn active_from(self) -> f64 {
        self.active_from
    }

    pub const fn inactive_from(self) -> Option<f64> {
        self.inactive_from
    }

    pub fn is_active_at(self, time: f64) -> bool {
        time.is_finite()
            && time >= self.active_from
            && self.inactive_from.is_none_or(|end| time < end)
    }

    pub(crate) const fn is_open(self) -> bool {
        self.inactive_from.is_none()
    }

    pub(crate) fn close(
        &mut self,
        inactive_from: f64,
    ) -> Result<(), SemanticUpdaterRegistrationError> {
        let replacement = Self::new(self.callback, self.active_from, Some(inactive_from))?;
        *self = replacement;
        Ok(())
    }

    fn validate(self) -> Result<(), SemanticUpdaterRegistrationError> {
        if !self.active_from.is_finite()
            || self.active_from < 0.0
            || self
                .inactive_from
                .is_some_and(|end| !end.is_finite() || end < self.active_from)
        {
            return Err(SemanticUpdaterRegistrationError::InvalidActivationInterval);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticUpdaterRegistrationError {
    InvalidActivationInterval,
}

impl std::fmt::Display for SemanticUpdaterRegistrationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("invalid semantic updater activation interval")
    }
}

impl std::error::Error for SemanticUpdaterRegistrationError {}

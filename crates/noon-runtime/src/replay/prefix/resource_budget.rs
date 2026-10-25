//! Constant-size admission accounting, not another resource owner or cache.
use super::PrefixError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ResourceBudget {
    bytes: usize,
    limit: usize,
}

impl ResourceBudget {
    pub(super) fn new(limit: usize) -> Self {
        Self { bytes: 0, limit }
    }
    pub(super) fn bytes(self) -> usize {
        self.bytes
    }
    pub(super) fn limit(self) -> usize {
        self.limit
    }

    /// Pure preflight: only the successful publication commits the returned value.
    /// Removed membership/rewind never refunds pins required by promised history.
    pub(super) fn with_additional(self, additional: usize) -> Result<Self, PrefixError> {
        self.bytes
            .checked_add(additional)
            .filter(|&total| total <= self.limit)
            .map(|bytes| Self { bytes, ..self })
            .ok_or(PrefixError::ResourceBudgetExceeded {
                retained_bytes: self.bytes,
                additional_bytes: additional,
                limit_bytes: self.limit,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_limit_and_empty_extensions_are_admitted() {
        let budget = ResourceBudget::new(7).with_additional(7).unwrap();
        assert_eq!(budget.bytes(), 7);
        assert_eq!(budget.with_additional(0).unwrap(), budget);
        assert_eq!(ResourceBudget::new(0).with_additional(0).unwrap().bytes(), 0);
    }

    #[test]
    fn rejected_preflight_does_not_charge_and_reports_the_attempt() {
        let budget = ResourceBudget::new(7).with_additional(4).unwrap();
        for _ in 0..2 {
            assert_eq!(
                budget.with_additional(4),
                Err(PrefixError::ResourceBudgetExceeded {
                    retained_bytes: 4,
                    additional_bytes: 4,
                    limit_bytes: 7,
                })
            );
        }
        assert_eq!(budget.with_additional(3).unwrap().bytes(), 7);
        assert_eq!(budget.bytes(), 4);
    }

    #[test]
    fn integer_overflow_is_rejected_even_at_the_largest_limit() {
        let budget = ResourceBudget::new(usize::MAX)
            .with_additional(usize::MAX - 1)
            .unwrap();
        assert!(budget.with_additional(2).is_err());
        assert_eq!(budget.bytes(), usize::MAX - 1);
    }
}

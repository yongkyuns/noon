//! Constant-size accounting only; history remains owned by the existing runtime.
use super::PrefixError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HistoryRecordBudget {
    bytes: usize,
    limit: usize,
}

impl HistoryRecordBudget {
    pub(super) fn new(limit: usize) -> Self {
        Self { bytes: 0, limit }
    }
    pub(super) fn bytes(self) -> usize {
        self.bytes
    }
    pub(super) fn limit(self) -> usize {
        self.limit
    }
    pub(super) fn with_additional(self, additional: usize) -> Result<Self, PrefixError> {
        self.bytes
            .checked_add(additional)
            .filter(|&total| total <= self.limit)
            .map(|bytes| Self { bytes, ..self })
            .ok_or(PrefixError::HistoryRecordBudgetExceeded {
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
    fn exact_limit_zero_extension_and_failed_preflight_preserve_the_charge() {
        let budget = HistoryRecordBudget::new(7).with_additional(7).unwrap();
        assert_eq!(budget.with_additional(0).unwrap(), budget);
        for _ in 0..2 {
            assert_eq!(
                budget.with_additional(1),
                Err(PrefixError::HistoryRecordBudgetExceeded {
                    retained_bytes: 7,
                    additional_bytes: 1,
                    limit_bytes: 7,
                })
            );
        }
        assert_eq!(budget.bytes(), 7);
        assert_eq!(
            HistoryRecordBudget::new(0).with_additional(0).unwrap().bytes(),
            0
        );
    }

    #[test]
    fn cumulative_byte_overflow_is_a_permanent_budget_rejection() {
        let budget = HistoryRecordBudget::new(usize::MAX)
            .with_additional(usize::MAX - 1)
            .unwrap();
        assert!(matches!(
            budget.with_additional(2),
            Err(PrefixError::HistoryRecordBudgetExceeded { .. })
        ));
        assert_eq!(budget.bytes(), usize::MAX - 1);
    }
}

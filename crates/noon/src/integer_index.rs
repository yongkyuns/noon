//! Integer argument precision is independent of scene size and host word size.

/// An arbitrary precision signed authoring index.
///
/// Ordinary Rust integer arguments convert directly. Large coordinates can
/// cancel during a query before the result is checked against its actual span.
pub type IntegerIndex = num_bigint::BigInt;

pub(crate) fn normalize_index(index: IntegerIndex, length: usize) -> Option<usize> {
    let index = if index.sign() == num_bigint::Sign::Minus {
        index + length
    } else {
        index
    };
    usize::try_from(index).ok().filter(|&index| index < length)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_indices_validate_after_exact_arithmetic() {
        let huge = IntegerIndex::from(10).pow(100);
        assert_eq!(normalize_index(-IntegerIndex::from(1), 4), Some(3));
        assert_eq!(normalize_index(-IntegerIndex::from(4), 4), Some(0));
        assert_eq!(normalize_index(-IntegerIndex::from(5), 4), None);
        assert_eq!(normalize_index(huge.clone(), 4), None);
        assert_eq!(normalize_index(huge.clone() - huge + 2, 4), Some(2));
        assert_eq!(normalize_index(0.into(), 0), None);
    }
}

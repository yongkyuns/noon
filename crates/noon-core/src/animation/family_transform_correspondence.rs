/// One member of a transient unequal-family Transform correspondence.
///
/// Indices address the independently traversed source and target family leaf lists.
/// `*_is_derived_copy` records the transparent duplicate introduced only for visual
/// alignment. It is deliberately not a semantic or stable execution identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyTransformCorrespondenceMember {
    pub source_index: u32,
    pub target_index: u32,
    pub source_is_derived_copy: bool,
    pub target_is_derived_copy: bool,
}

/// Renderer-independent correspondence for Manim-style unequal family alignment.
///
/// This plan owns only transient indices and copy markers. Semantic topology,
/// SemanticNodeId values, ObjectId values, authored membership, and persistent state
/// remain outside this type. Runtime/render lowering may realize copy-marked entries as
/// temporary faded display instances for the duration of the animation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyTransformCorrespondence {
    members: Vec<FamilyTransformCorrespondenceMember>,
}

impl FamilyTransformCorrespondence {
    pub fn members(&self) -> &[FamilyTransformCorrespondenceMember] {
        &self.members
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyTransformCorrespondenceError {
    EmptyFamily {
        source_count: usize,
        target_count: usize,
    },
    TooManyMembers(usize),
}

impl std::fmt::Display for FamilyTransformCorrespondenceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyFamily {
                source_count,
                target_count,
            } => write!(
                formatter,
                "unequal family Transform requires non-empty source and target families, found {source_count} and {target_count} leaves"
            ),
            Self::TooManyMembers(count) => write!(
                formatter,
                "unequal family Transform correspondence exceeds u32 indexing with {count} aligned members"
            ),
        }
    }
}

impl std::error::Error for FamilyTransformCorrespondenceError {}

/// Derive ManimCE-style family alignment without creating semantic or execution IDs.
///
/// Manim expands the shorter side to the longer side using
/// `repeat_index = (aligned_index * original_count) / aligned_count`, retaining the
/// first occurrence of each original member and treating later occurrences as faded
/// copies. Applying that rule independently to both sides yields a deterministic pair
/// list for expansion and contraction while leaving both authored families untouched.
pub fn derive_family_transform_correspondence(
    source_count: usize,
    target_count: usize,
) -> Result<FamilyTransformCorrespondence, FamilyTransformCorrespondenceError> {
    if source_count == 0 || target_count == 0 {
        return Err(FamilyTransformCorrespondenceError::EmptyFamily {
            source_count,
            target_count,
        });
    }

    let aligned_count = source_count.max(target_count);
    if u32::try_from(aligned_count).is_err()
        || u32::try_from(source_count).is_err()
        || u32::try_from(target_count).is_err()
    {
        return Err(FamilyTransformCorrespondenceError::TooManyMembers(
            aligned_count,
        ));
    }

    let source = expanded_indices(source_count, aligned_count);
    let target = expanded_indices(target_count, aligned_count);
    let members = source
        .into_iter()
        .zip(target)
        .map(|((source_index, source_is_derived_copy), (target_index, target_is_derived_copy))| {
            FamilyTransformCorrespondenceMember {
                source_index,
                target_index,
                source_is_derived_copy,
                target_is_derived_copy,
            }
        })
        .collect();

    Ok(FamilyTransformCorrespondence { members })
}

fn expanded_indices(original_count: usize, aligned_count: usize) -> Vec<(u32, bool)> {
    let mut seen = vec![false; original_count];
    (0..aligned_count)
        .map(|aligned_index| {
            let original_index = aligned_index * original_count / aligned_count;
            let is_derived_copy = std::mem::replace(&mut seen[original_index], true);
            (
                u32::try_from(original_index).expect("preflighted family index fits u32"),
                is_derived_copy,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(
        source_index: u32,
        target_index: u32,
        source_is_derived_copy: bool,
        target_is_derived_copy: bool,
    ) -> FamilyTransformCorrespondenceMember {
        FamilyTransformCorrespondenceMember {
            source_index,
            target_index,
            source_is_derived_copy,
            target_is_derived_copy,
        }
    }

    #[test]
    fn equal_family_is_identity_correspondence_without_copies() {
        let plan = derive_family_transform_correspondence(3, 3).unwrap();
        assert_eq!(
            plan.members(),
            &[
                member(0, 0, false, false),
                member(1, 1, false, false),
                member(2, 2, false, false),
            ]
        );
    }

    #[test]
    fn expansion_repeats_source_in_manim_order_and_marks_faded_copy() {
        let plan = derive_family_transform_correspondence(2, 3).unwrap();
        assert_eq!(
            plan.members(),
            &[
                member(0, 0, false, false),
                member(0, 1, true, false),
                member(1, 2, false, false),
            ]
        );
    }

    #[test]
    fn contraction_repeats_target_in_manim_order_and_marks_faded_copy() {
        let plan = derive_family_transform_correspondence(3, 2).unwrap();
        assert_eq!(
            plan.members(),
            &[
                member(0, 0, false, false),
                member(1, 0, false, true),
                member(2, 1, false, false),
            ]
        );
    }

    #[test]
    fn repeated_padding_preserves_bucket_order_for_larger_ratio() {
        let plan = derive_family_transform_correspondence(2, 5).unwrap();
        assert_eq!(
            plan.members(),
            &[
                member(0, 0, false, false),
                member(0, 1, true, false),
                member(0, 2, true, false),
                member(1, 3, false, false),
                member(1, 4, true, false),
            ]
        );
    }

    #[test]
    fn empty_family_fails_closed_instead_of_inventing_identity() {
        assert_eq!(
            derive_family_transform_correspondence(0, 2),
            Err(FamilyTransformCorrespondenceError::EmptyFamily {
                source_count: 0,
                target_count: 2,
            })
        );
        assert_eq!(
            derive_family_transform_correspondence(2, 0),
            Err(FamilyTransformCorrespondenceError::EmptyFamily {
                source_count: 2,
                target_count: 0,
            })
        );
    }
}

use std::collections::HashMap;

use crate::MatchingShapeKey;

/// One deterministic equal-key group shared by the source and target families.
///
/// Duplicate shapes stay grouped rather than being paired arbitrarily one-by-one,
/// matching Manim's group-transform semantics for repeated keys.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchingShapeGroup {
    pub key: MatchingShapeKey,
    pub source_indices: Vec<usize>,
    pub target_indices: Vec<usize>,
}

/// Stable matching-shape partition for one source/target family pair.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MatchingShapeCorrespondence {
    pub matched_groups: Vec<MatchingShapeGroup>,
    pub unmatched_source_indices: Vec<usize>,
    pub unmatched_target_indices: Vec<usize>,
}

/// Partition two ordered shape-key families into equal-key groups and leftovers.
///
/// Matched groups follow first source-key occurrence order. Duplicate members retain
/// their authored order inside each group. Leftovers retain their original family
/// order. No unmatched member is paired by position or proximity.
pub fn matching_shape_correspondence(
    source_keys: &[MatchingShapeKey],
    target_keys: &[MatchingShapeKey],
) -> MatchingShapeCorrespondence {
    let mut target_by_key: HashMap<MatchingShapeKey, Vec<usize>> = HashMap::new();
    for (index, key) in target_keys.iter().cloned().enumerate() {
        target_by_key.entry(key).or_default().push(index);
    }

    let mut source_group_positions: HashMap<MatchingShapeKey, usize> = HashMap::new();
    let mut source_groups: Vec<(MatchingShapeKey, Vec<usize>)> = Vec::new();
    for (index, key) in source_keys.iter().cloned().enumerate() {
        if let Some(position) = source_group_positions.get(&key).copied() {
            source_groups[position].1.push(index);
        } else {
            let position = source_groups.len();
            source_group_positions.insert(key.clone(), position);
            source_groups.push((key, vec![index]));
        }
    }

    let mut matched_target = vec![false; target_keys.len()];
    let mut matched_groups = Vec::new();
    let mut unmatched_source_indices = Vec::new();
    for (key, source_indices) in source_groups {
        if let Some(target_indices) = target_by_key.get(&key) {
            for index in target_indices.iter().copied() {
                matched_target[index] = true;
            }
            matched_groups.push(MatchingShapeGroup {
                key,
                source_indices,
                target_indices: target_indices.clone(),
            });
        } else {
            unmatched_source_indices.extend(source_indices);
        }
    }

    let unmatched_target_indices = matched_target
        .into_iter()
        .enumerate()
        .filter_map(|(index, matched)| (!matched).then_some(index))
        .collect();

    MatchingShapeCorrespondence {
        matched_groups,
        unmatched_source_indices,
        unmatched_target_indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{vector_path_matching_shape_key, Transform2D, Vec2, VectorPath};

    fn key(offset: f32) -> MatchingShapeKey {
        let path = VectorPath::new()
            .move_to(Vec2::new(-1.0, -1.0))
            .line_to(Vec2::new(1.0 + offset, -0.5))
            .line_to(Vec2::new(-0.25, 1.0))
            .close();
        vector_path_matching_shape_key(&path, Transform2D::IDENTITY).unwrap()
    }

    #[test]
    fn duplicate_keys_form_one_stable_group() {
        let a = key(0.0);
        let b = key(0.4);
        let correspondence = matching_shape_correspondence(
            &[a.clone(), b.clone(), a.clone()],
            &[a.clone(), a.clone(), b.clone()],
        );

        assert_eq!(correspondence.matched_groups.len(), 2);
        assert_eq!(correspondence.matched_groups[0].key, a);
        assert_eq!(correspondence.matched_groups[0].source_indices, vec![0, 2]);
        assert_eq!(correspondence.matched_groups[0].target_indices, vec![0, 1]);
        assert_eq!(correspondence.matched_groups[1].key, b);
        assert_eq!(correspondence.matched_groups[1].source_indices, vec![1]);
        assert_eq!(correspondence.matched_groups[1].target_indices, vec![2]);
        assert!(correspondence.unmatched_source_indices.is_empty());
        assert!(correspondence.unmatched_target_indices.is_empty());
    }

    #[test]
    fn leftovers_remain_explicit_and_in_family_order() {
        let a = key(0.0);
        let b = key(0.4);
        let c = key(0.8);
        let correspondence = matching_shape_correspondence(
            &[a.clone(), b.clone(), b.clone()],
            &[c.clone(), a.clone(), c],
        );

        assert_eq!(correspondence.matched_groups.len(), 1);
        assert_eq!(correspondence.matched_groups[0].source_indices, vec![0]);
        assert_eq!(correspondence.matched_groups[0].target_indices, vec![1]);
        assert_eq!(correspondence.unmatched_source_indices, vec![1, 2]);
        assert_eq!(correspondence.unmatched_target_indices, vec![0, 2]);
    }
}

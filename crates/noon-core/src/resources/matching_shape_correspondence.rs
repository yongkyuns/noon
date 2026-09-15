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

/// One user-directed source-key to target-key group.
///
/// The source and target keys may differ. Members stay grouped in authored family
/// order; no positional pairing is introduced inside a mapped group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchingShapeMappedGroup {
    pub source_key: MatchingShapeKey,
    pub target_key: MatchingShapeKey,
    pub source_indices: Vec<usize>,
    pub target_indices: Vec<usize>,
}

/// Stable matching-shape partition for one source/target family pair.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MatchingShapeCorrespondence {
    pub matched_groups: Vec<MatchingShapeGroup>,
    pub mapped_groups: Vec<MatchingShapeMappedGroup>,
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
    matching_shape_correspondence_with_key_map(source_keys, target_keys, &[])
}

/// Partition shape-key families with Manim-compatible explicit key redirection.
///
/// Equal-key matched groups are derived first, exactly like ManimCE v0.21 builds its
/// ordinary transform groups before processing `key_map`. Valid mapping entries are
/// then consumed in caller order. A mapping is ignored unless both keys still exist;
/// once consumed, either key is unavailable to a later mapping. Finally, leftovers
/// are derived from the remaining key maps while preserving authored family order.
///
/// Because Manim builds equal-key transform groups before consuming `key_map`, a map
/// that redirects a key which already has an equal-key match can intentionally leave
/// that same group represented in both `matched_groups` and `mapped_groups`. Noon
/// preserves that ordering semantics rather than silently rewriting the request.
pub fn matching_shape_correspondence_with_key_map(
    source_keys: &[MatchingShapeKey],
    target_keys: &[MatchingShapeKey],
    key_map: &[(MatchingShapeKey, MatchingShapeKey)],
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

    let matched_groups = source_groups
        .iter()
        .filter_map(|(key, source_indices)| {
            target_by_key.get(key).map(|target_indices| MatchingShapeGroup {
                key: key.clone(),
                source_indices: source_indices.clone(),
                target_indices: target_indices.clone(),
            })
        })
        .collect();

    let mut remaining_source: HashMap<MatchingShapeKey, Vec<usize>> =
        source_groups.iter().cloned().collect();
    let mut remaining_target = target_by_key;
    let mut mapped_groups = Vec::new();
    for (source_key, target_key) in key_map {
        let Some(source_indices) = remaining_source.get(source_key).cloned() else {
            continue;
        };
        let Some(target_indices) = remaining_target.get(target_key).cloned() else {
            continue;
        };
        remaining_source.remove(source_key);
        remaining_target.remove(target_key);
        mapped_groups.push(MatchingShapeMappedGroup {
            source_key: source_key.clone(),
            target_key: target_key.clone(),
            source_indices,
            target_indices,
        });
    }

    let mut unmatched_source_indices = source_groups
        .iter()
        .filter(|(key, _)| {
            remaining_source.contains_key(key) && !remaining_target.contains_key(key)
        })
        .flat_map(|(_, indices)| indices.iter().copied())
        .collect::<Vec<_>>();
    unmatched_source_indices.sort_unstable();

    let mut unmatched_target_indices = remaining_target
        .iter()
        .filter(|(key, _)| !remaining_source.contains_key(*key))
        .flat_map(|(_, indices)| indices.iter().copied())
        .collect::<Vec<_>>();
    unmatched_target_indices.sort_unstable();

    MatchingShapeCorrespondence {
        matched_groups,
        mapped_groups,
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
        assert!(correspondence.mapped_groups.is_empty());
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
        assert!(correspondence.mapped_groups.is_empty());
        assert_eq!(correspondence.unmatched_source_indices, vec![1, 2]);
        assert_eq!(correspondence.unmatched_target_indices, vec![0, 2]);
    }

    #[test]
    fn key_map_consumes_valid_groups_in_declared_order_and_skips_invalid_entries() {
        let a = key(0.0);
        let b = key(0.4);
        let c = key(0.8);
        let d = key(1.2);
        let missing = key(1.6);
        let correspondence = matching_shape_correspondence_with_key_map(
            &[a.clone(), a.clone(), b.clone(), c.clone()],
            &[d.clone(), b.clone(), d.clone(), c.clone()],
            &[
                (missing.clone(), d.clone()),
                (a.clone(), d.clone()),
                (b.clone(), missing),
                (c.clone(), b.clone()),
                (a.clone(), c.clone()),
            ],
        );

        assert_eq!(correspondence.mapped_groups.len(), 2);
        assert_eq!(correspondence.mapped_groups[0].source_key, a);
        assert_eq!(correspondence.mapped_groups[0].target_key, d);
        assert_eq!(correspondence.mapped_groups[0].source_indices, vec![0, 1]);
        assert_eq!(correspondence.mapped_groups[0].target_indices, vec![0, 2]);
        assert_eq!(correspondence.mapped_groups[1].source_key, c);
        assert_eq!(correspondence.mapped_groups[1].target_key, b);
        assert_eq!(correspondence.mapped_groups[1].source_indices, vec![3]);
        assert_eq!(correspondence.mapped_groups[1].target_indices, vec![1]);
        assert_eq!(correspondence.unmatched_source_indices, vec![2]);
        assert_eq!(correspondence.unmatched_target_indices, vec![3]);
    }

    #[test]
    fn key_map_is_applied_after_equal_key_groups_are_assembled() {
        let a = key(0.0);
        let b = key(0.4);
        let correspondence = matching_shape_correspondence_with_key_map(
            &[a.clone(), b.clone()],
            &[a.clone(), b.clone()],
            &[(a.clone(), b.clone())],
        );

        assert_eq!(correspondence.matched_groups.len(), 2);
        assert_eq!(correspondence.matched_groups[0].source_indices, vec![0]);
        assert_eq!(correspondence.matched_groups[0].target_indices, vec![0]);
        assert_eq!(correspondence.matched_groups[1].source_indices, vec![1]);
        assert_eq!(correspondence.matched_groups[1].target_indices, vec![1]);
        assert_eq!(correspondence.mapped_groups.len(), 1);
        assert_eq!(correspondence.mapped_groups[0].source_indices, vec![0]);
        assert_eq!(correspondence.mapped_groups[0].target_indices, vec![1]);
        assert_eq!(correspondence.unmatched_source_indices, vec![1]);
        assert_eq!(correspondence.unmatched_target_indices, vec![0]);
    }
}

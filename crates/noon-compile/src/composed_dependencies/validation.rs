use super::projection::Projection;
use super::*;

fn error(owner: ObjectId, kind: CompiledComposedSourceErrorKind) -> CompiledComposedSourceError {
    CompiledComposedSourceError { owner, kind }
}

pub(super) fn validate(
    view: &Projection<'_>,
    affected: &BTreeSet<ObjectId>,
) -> Result<ComposedValidationStats, CompiledComposedSourceError> {
    let mut candidates = affected
        .iter()
        .copied()
        .filter(|id| view.source(*id).is_some())
        .collect::<BTreeSet<_>>();
    if candidates.is_empty() {
        return Ok(ComposedValidationStats::default());
    }
    // Visit each distinct changed input's reverse edges once, even for deeply
    // nested equal intervals. No all-scope pairwise containment search is needed.
    let mut inputs = BTreeSet::new();
    for &owner in affected {
        if let Some(source) = view.source(owner) {
            inputs.extend(source.members().iter().copied());
        }
    }
    for input in inputs {
        candidates.extend(view.overlapping(input));
    }
    let mut depths = BTreeMap::new();
    for &start in &candidates {
        let mut current = start;
        let mut path = Vec::new();
        let mut visiting = BTreeSet::new();
        let mut next_depth;
        loop {
            if let Some(depth) = depths.get(&current) {
                next_depth = depth + 1;
                break;
            }
            if !visiting.insert(current) {
                return Err(error(start, CompiledComposedSourceErrorKind::ParentCycle));
            }
            let source = view.source(current).ok_or_else(|| {
                error(
                    start,
                    CompiledComposedSourceErrorKind::InvalidParent(current),
                )
            })?;
            path.push(current);
            if let Some(parent) = source.parent_scope() {
                if view.source(parent).is_none() {
                    return Err(error(
                        current,
                        CompiledComposedSourceErrorKind::InvalidParent(parent),
                    ));
                }
                current = parent;
            } else {
                next_depth = 0;
                break;
            }
        }
        for owner in path.into_iter().rev() {
            depths.insert(owner, next_depth);
            next_depth += 1;
        }
    }
    candidates.extend(depths.keys().copied());
    let mut stats = ComposedValidationStats::default();
    let mut intervals = Vec::with_capacity(candidates.len());
    for owner in candidates {
        let source = view.source(owner).expect("validated scope ancestry");
        if !view.owner_valid(owner) {
            return Err(error(owner, CompiledComposedSourceErrorKind::InvalidOwner));
        }
        let first = source.members()[0];
        let start = view
            .rank(first)
            .ok_or_else(|| error(owner, CompiledComposedSourceErrorKind::UnknownInput(first)))?;
        if affected.contains(&owner) {
            for (offset, &input) in source.members().iter().enumerate() {
                stats.members_examined += 1;
                match view.input_valid(input) {
                    None => {
                        return Err(error(
                            owner,
                            CompiledComposedSourceErrorKind::UnknownInput(input),
                        ))
                    }
                    Some(false) => {
                        return Err(error(
                            owner,
                            CompiledComposedSourceErrorKind::NonDrawableInput(input),
                        ))
                    }
                    Some(true) => {}
                }
                if view.rank(input) != Some(start + offset) {
                    return Err(error(
                        owner,
                        CompiledComposedSourceErrorKind::NonContiguousInput,
                    ));
                }
            }
        }
        stats.scopes_validated += 1;
        intervals.push((
            start,
            start + source.members().len(),
            depths[&owner],
            owner,
            source.parent_scope(),
        ));
    }
    intervals.sort_unstable_by_key(|&(start, end, depth, owner, _)| {
        (start, std::cmp::Reverse(end), depth, owner)
    });
    let mut stack = Vec::<(usize, ObjectId)>::new();
    for (start, end, _, owner, parent) in intervals {
        while stack.last().is_some_and(|&(end, _)| start >= end) {
            stack.pop();
        }
        if stack.last().is_some_and(|&(outer_end, _)| end > outer_end)
            || stack.last().map(|&(_, id)| id) != parent
        {
            return Err(error(
                owner,
                CompiledComposedSourceErrorKind::CrossingOrAliasedScopes,
            ));
        }
        stack.push((end, owner));
    }
    Ok(stats)
}

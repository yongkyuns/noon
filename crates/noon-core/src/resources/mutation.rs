use std::{fmt, sync::Arc};

use crate::{
    GeometryResource, GeometryResourceArena, GeometryResourceError, GeometryResourceHandle,
    TextResource, TextResourceArena, TextResourceError, TextResourceHandle,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeometryResourceMutationError {
    Stale {
        expected: GeometryResourceHandle,
        actual: GeometryResourceHandle,
    },
    Resource(GeometryResourceError),
}

impl From<GeometryResourceError> for GeometryResourceMutationError {
    fn from(value: GeometryResourceError) -> Self {
        Self::Resource(value)
    }
}

impl fmt::Display for GeometryResourceMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stale { expected, actual } => write!(
                formatter,
                "stale geometry resource {}@{}; current version is {}",
                expected.id.get(),
                expected.version,
                actual.version
            ),
            Self::Resource(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for GeometryResourceMutationError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextResourceMutationError {
    Stale {
        expected: TextResourceHandle,
        actual: TextResourceHandle,
    },
    Resource(TextResourceError),
}

impl From<TextResourceError> for TextResourceMutationError {
    fn from(value: TextResourceError) -> Self {
        Self::Resource(value)
    }
}

impl fmt::Display for TextResourceMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stale { expected, actual } => write!(
                formatter,
                "stale text resource {}@{}; current version is {}",
                expected.id.get(),
                expected.version,
                actual.version
            ),
            Self::Resource(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TextResourceMutationError {}

pub fn replace_geometry_if_current(
    arena: &mut GeometryResourceArena,
    expected: GeometryResourceHandle,
    resource: GeometryResource,
) -> Result<GeometryResourceHandle, GeometryResourceMutationError> {
    let actual =
        arena
            .current_handle(expected.id)
            .ok_or(GeometryResourceMutationError::Resource(
                GeometryResourceError::UnknownResource(expected.id),
            ))?;
    if actual != expected {
        return Err(GeometryResourceMutationError::Stale { expected, actual });
    }
    if actual.version == u64::MAX {
        return Err(GeometryResourceMutationError::Resource(
            GeometryResourceError::VersionExhausted(actual.id),
        ));
    }
    arena.replace(expected.id, resource).map_err(Into::into)
}

pub fn remove_geometry_if_current(
    arena: &mut GeometryResourceArena,
    expected: GeometryResourceHandle,
) -> Result<GeometryResource, GeometryResourceMutationError> {
    let actual =
        arena
            .current_handle(expected.id)
            .ok_or(GeometryResourceMutationError::Resource(
                GeometryResourceError::UnknownResource(expected.id),
            ))?;
    if actual != expected {
        return Err(GeometryResourceMutationError::Stale { expected, actual });
    }
    if actual.version == u64::MAX {
        return Err(GeometryResourceMutationError::Resource(
            GeometryResourceError::VersionExhausted(actual.id),
        ));
    }
    arena.remove(expected.id).map_err(Into::into)
}

pub fn replace_text_if_current(
    arena: &mut TextResourceArena,
    expected: TextResourceHandle,
    resource: TextResource,
) -> Result<TextResourceHandle, TextResourceMutationError> {
    let actual = arena
        .current_handle(expected.id)
        .ok_or(TextResourceMutationError::Resource(
            TextResourceError::UnknownResource(expected.id),
        ))?;
    if actual != expected {
        return Err(TextResourceMutationError::Stale { expected, actual });
    }
    if actual.version == u64::MAX {
        return Err(TextResourceMutationError::Resource(
            TextResourceError::VersionExhausted(actual.id),
        ));
    }
    arena.replace(expected.id, resource).map_err(Into::into)
}

pub fn remove_text_if_current(
    arena: &mut TextResourceArena,
    expected: TextResourceHandle,
) -> Result<Arc<TextResource>, TextResourceMutationError> {
    let actual = arena
        .current_handle(expected.id)
        .ok_or(TextResourceMutationError::Resource(
            TextResourceError::UnknownResource(expected.id),
        ))?;
    if actual != expected {
        return Err(TextResourceMutationError::Stale { expected, actual });
    }
    if actual.version == u64::MAX {
        return Err(TextResourceMutationError::Resource(
            TextResourceError::VersionExhausted(actual.id),
        ));
    }
    arena.remove(expected.id).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Rect, TextRenderItem, TextResourceValidationError, TextSourceKind, Vec2, VectorPath,
    };

    fn geometry(end_x: f32) -> GeometryResource {
        GeometryResource::VectorPath(Arc::new(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(end_x, 0.0)),
        ))
    }

    fn text(source: &str) -> TextResource {
        TextResource {
            source: Arc::from(source),
            kind: TextSourceKind::Plain,
            runs: Arc::from([]),
            vector_items: Arc::from([]),
            render_items: Arc::from([]),
            parts: Arc::from([]),
            bounds: Rect::new(Vec2::ZERO, Vec2::ZERO),
            baseline: 0.0,
            layout_artifact: None,
        }
    }

    #[test]
    fn stale_geometry_replace_and_remove_leave_current_resource_unchanged() {
        let mut arena = GeometryResourceArena::new();
        let first = arena.insert(geometry(1.0));
        let replacement = geometry(2.0);
        let second = replace_geometry_if_current(&mut arena, first, replacement.clone()).unwrap();
        let before = arena.stats();

        assert_eq!(
            replace_geometry_if_current(&mut arena, first, geometry(3.0)),
            Err(GeometryResourceMutationError::Stale {
                expected: first,
                actual: second,
            })
        );
        assert_eq!(
            remove_geometry_if_current(&mut arena, first),
            Err(GeometryResourceMutationError::Stale {
                expected: first,
                actual: second,
            })
        );
        assert_eq!(arena.current_handle(first.id), Some(second));
        assert_eq!(arena.get(second), Some(&replacement));
        assert_eq!(arena.stats(), before);

        assert_eq!(
            remove_geometry_if_current(&mut arena, second),
            Ok(replacement)
        );
        assert!(arena.is_empty());
    }

    #[test]
    fn stale_text_replace_and_remove_leave_current_resource_unchanged() {
        let mut arena = TextResourceArena::new();
        let first = arena.insert(text("x")).unwrap();
        let second = replace_text_if_current(&mut arena, first, text("x^2")).unwrap();
        let before = arena.stats();

        assert_eq!(
            replace_text_if_current(&mut arena, first, text("stale")),
            Err(TextResourceMutationError::Stale {
                expected: first,
                actual: second,
            })
        );
        assert!(matches!(
            remove_text_if_current(&mut arena, first),
            Err(TextResourceMutationError::Stale { expected, actual })
                if expected == first && actual == second
        ));
        assert_eq!(arena.current_handle(first.id), Some(second));
        assert_eq!(arena.get(second).unwrap().source.as_ref(), "x^2");
        assert_eq!(arena.stats(), before);

        let removed = remove_text_if_current(&mut arena, second).unwrap();
        assert_eq!(removed.source.as_ref(), "x^2");
        assert!(arena.is_empty());
    }

    #[test]
    fn invalid_text_replace_is_transactional() {
        let mut arena = TextResourceArena::new();
        let current = arena.insert(text("stable")).unwrap();
        let before = arena.stats();
        let mut invalid = text("invalid");
        invalid.render_items = Arc::from([TextRenderItem::GlyphRun(0)]);

        assert_eq!(
            replace_text_if_current(&mut arena, current, invalid),
            Err(TextResourceMutationError::Resource(
                TextResourceError::InvalidResource(TextResourceValidationError::InvalidRenderItem)
            ))
        );
        assert_eq!(arena.current_handle(current.id), Some(current));
        assert_eq!(arena.get(current).unwrap().source.as_ref(), "stable");
        assert_eq!(arena.stats(), before);
    }
}

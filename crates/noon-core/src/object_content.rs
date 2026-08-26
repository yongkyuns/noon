use crate::{GeometryRef, ObjectDefinition, ObjectId, Style, TextResourceHandle, Transform2D};

/// Renderer-independent retained payload referenced by one semantic object.
///
/// Geometry remains inline/reference-backed through `GeometryRef`; text and math
/// reference immutable `TextResource` data by stable versioned handle. Keeping
/// these as distinct variants prevents steady-state text from masquerading as
/// placeholder geometry or eagerly expanding into per-glyph outlines.
#[derive(Clone, Debug, PartialEq)]
pub enum ObjectContentRef {
    Geometry(GeometryRef),
    Text(TextResourceHandle),
}

impl ObjectContentRef {
    pub fn geometry(&self) -> Option<&GeometryRef> {
        match self {
            Self::Geometry(geometry) => Some(geometry),
            Self::Text(_) => None,
        }
    }

    pub const fn text(&self) -> Option<TextResourceHandle> {
        match self {
            Self::Geometry(_) => None,
            Self::Text(handle) => Some(*handle),
        }
    }
}

impl From<GeometryRef> for ObjectContentRef {
    fn from(value: GeometryRef) -> Self {
        Self::Geometry(value)
    }
}

impl From<TextResourceHandle> for ObjectContentRef {
    fn from(value: TextResourceHandle) -> Self {
        Self::Text(value)
    }
}

/// General retained object input used by compiler paths that are not constrained
/// by the legacy geometry-only `SceneDefinition` serialization contract.
///
/// This deliberately shares `ObjectId`, transform, and style semantics with legacy
/// objects so geometry and text can eventually occupy one execution/painter-order
/// domain without introducing fake geometry or a second identity space.
#[derive(Clone, Debug, PartialEq)]
pub struct RetainedObjectDefinition {
    pub id: ObjectId,
    pub content: ObjectContentRef,
    pub transform: Transform2D,
    pub style: Style,
}

impl RetainedObjectDefinition {
    pub fn new(id: ObjectId, content: impl Into<ObjectContentRef>) -> Self {
        Self {
            id,
            content: content.into(),
            transform: Transform2D::default(),
            style: Style::default(),
        }
    }

    pub fn geometry(id: ObjectId, geometry: GeometryRef) -> Self {
        Self::new(id, geometry)
    }

    pub fn text(id: ObjectId, text: TextResourceHandle) -> Self {
        Self::new(id, text)
    }
}

impl From<&ObjectDefinition> for RetainedObjectDefinition {
    fn from(value: &ObjectDefinition) -> Self {
        Self {
            id: value.id,
            content: ObjectContentRef::Geometry(value.geometry.clone()),
            transform: value.transform,
            style: value.style,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextResourceId;

    #[test]
    fn legacy_geometry_converts_without_changing_semantics() {
        let mut legacy = ObjectDefinition::new(ObjectId::new(7), GeometryRef::circle(2.0));
        legacy.transform.translation.x = 3.0;
        legacy.style.opacity = 0.4;

        let retained = RetainedObjectDefinition::from(&legacy);
        assert_eq!(retained.id, legacy.id);
        assert_eq!(retained.content.geometry(), Some(&legacy.geometry));
        assert_eq!(retained.transform, legacy.transform);
        assert_eq!(retained.style, legacy.style);
        assert_eq!(retained.content.text(), None);
    }

    #[test]
    fn text_object_keeps_only_the_versioned_resource_handle() {
        let handle = TextResourceHandle {
            id: TextResourceId::new(11),
            version: 4,
        };
        let retained = RetainedObjectDefinition::text(ObjectId::new(3), handle);
        assert_eq!(retained.content.text(), Some(handle));
        assert_eq!(retained.content.geometry(), None);
    }
}

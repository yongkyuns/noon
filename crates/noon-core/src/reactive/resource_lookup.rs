use crate::{
    FontFaceIdentity, FontResource, FontResourceArena, FontResourceHandle, GeometryId,
    GeometryResource, GeometryResourceArena, GeometryResourceHandle, TextResource,
    TextResourceArena, TextResourceHandle,
};

/// Read-only text-resource resolution shared by authored arenas and compiled snapshots.
pub trait TextResourceLookup {
    fn get(&self, handle: TextResourceHandle) -> Option<&TextResource>;
}

impl TextResourceLookup for TextResourceArena {
    fn get(&self, handle: TextResourceHandle) -> Option<&TextResource> {
        TextResourceArena::get(self, handle)
    }
}

/// Read-only font-resource resolution shared by authored arenas and compiled snapshots.
pub trait FontResourceLookup {
    fn handle_for_face(&self, face: &FontFaceIdentity) -> Option<FontResourceHandle>;

    fn get(&self, handle: FontResourceHandle) -> Option<&FontResource>;

    fn get_for_face(&self, face: &FontFaceIdentity) -> Option<&FontResource> {
        self.get(self.handle_for_face(face)?)
    }
}

impl FontResourceLookup for FontResourceArena {
    fn handle_for_face(&self, face: &FontFaceIdentity) -> Option<FontResourceHandle> {
        FontResourceArena::handle_for_face(self, face)
    }

    fn get(&self, handle: FontResourceHandle) -> Option<&FontResource> {
        FontResourceArena::get(self, handle)
    }
}

/// Read-only geometry-resource resolution shared by authored arenas and compiled snapshots.
pub trait GeometryResourceLookup {
    fn current_handle(&self, id: GeometryId) -> Option<GeometryResourceHandle>;

    fn get(&self, handle: GeometryResourceHandle) -> Option<&GeometryResource>;
}

impl GeometryResourceLookup for GeometryResourceArena {
    fn current_handle(&self, id: GeometryId) -> Option<GeometryResourceHandle> {
        GeometryResourceArena::current_handle(self, id)
    }

    fn get(&self, handle: GeometryResourceHandle) -> Option<&GeometryResource> {
        GeometryResourceArena::get(self, handle)
    }
}

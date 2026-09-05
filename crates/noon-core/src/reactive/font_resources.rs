use std::{collections::BTreeMap, mem::size_of, sync::Arc};

use crate::FontFaceIdentity;

/// Stable identity for one immutable OpenType font buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontResourceId(u64);

impl FontResourceId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Versioned reference to one immutable font buffer.
///
/// Font resources are append-only for the lifetime of an arena, so version zero
/// remains stable for every live handle. The version field keeps the handle shape
/// compatible with Noon's other retained-resource boundaries and leaves room for a
/// future explicit arena-generation model without changing renderer records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontResourceHandle {
    pub id: FontResourceId,
    pub version: u64,
}

/// Font-file identity used to resolve a shaped run to its backing OpenType data.
///
/// `variation_key` intentionally does not participate: multiple variable-font
/// instances share the same immutable font buffer and differ only at shaping /
/// rasterization time.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontResourceKey {
    pub face_key: Arc<str>,
    pub face_index: u32,
}

impl FontResourceKey {
    pub fn from_face(face: &FontFaceIdentity) -> Self {
        Self {
            face_key: face.face_key.clone(),
            face_index: face.face_index,
        }
    }
}

/// Immutable renderer/backend-neutral OpenType resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontResource {
    pub key: FontResourceKey,
    pub data: Arc<[u8]>,
}

impl FontResource {
    pub fn retained_bytes(&self) -> usize {
        size_of::<Self>()
            .saturating_add(self.key.face_key.len())
            .saturating_add(self.data.len())
    }
}

#[derive(Clone, Debug)]
struct FontResourceEntry {
    version: u64,
    value: Arc<FontResource>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FontResourceStats {
    pub live_resources: usize,
    pub retained_bytes: usize,
    pub font_bytes: usize,
}

/// Stable-ID arena for immutable font buffers used by retained text resources.
///
/// The arena provides the missing boundary between backend-neutral glyph IDs in
/// `TextResource` and renderer atlas population. Frame/object snapshots continue
/// to carry only text handles; font buffers are resolved separately and retained
/// once per face.
///
/// Unlike mutable geometry/text arenas, font resources are deliberately append-only
/// for the arena lifetime. A shaped glyph ID is meaningful only relative to the
/// exact font bytes that produced it, so a face key is never rebound to new bytes.
/// Dropping the whole arena is the font-resource invalidation barrier.
#[derive(Clone, Debug, Default)]
pub struct FontResourceArena {
    entries: Vec<FontResourceEntry>,
    handles_by_key: BTreeMap<FontResourceKey, FontResourceHandle>,
    retained_bytes: usize,
    font_bytes: usize,
}

impl FontResourceArena {
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern the backing font data for a shaped face.
    ///
    /// Reusing the same face key/index with identical bytes is allocation-free.
    /// Reusing the identity with different bytes is rejected so retained shaped
    /// text can never silently resolve to different glyph outlines.
    pub fn intern_face(
        &mut self,
        face: &FontFaceIdentity,
        data: impl Into<Arc<[u8]>>,
    ) -> Result<FontResourceHandle, FontResourceError> {
        let key = FontResourceKey::from_face(face);
        let data = data.into();
        if let Some(handle) = self.handles_by_key.get(&key).copied() {
            let existing = self
                .get(handle)
                .expect("font lookup must reference an immutable arena entry");
            if existing.data.as_ref() == data.as_ref() {
                return Ok(handle);
            }
            return Err(FontResourceError::ConflictingResource(key));
        }

        let id = FontResourceId::new(
            u64::try_from(self.entries.len()).expect("Noon font resource ID space exhausted"),
        );
        let handle = FontResourceHandle { id, version: 0 };
        let resource = Arc::new(FontResource {
            key: key.clone(),
            data,
        });
        self.retained_bytes = self
            .retained_bytes
            .saturating_add(resource.retained_bytes());
        self.font_bytes = self.font_bytes.saturating_add(resource.data.len());
        self.entries.push(FontResourceEntry {
            version: 0,
            value: resource,
        });
        self.handles_by_key.insert(key, handle);
        Ok(handle)
    }

    pub fn get(&self, handle: FontResourceHandle) -> Option<&FontResource> {
        let entry = self.entries.get(handle.id.get() as usize)?;
        if entry.version != handle.version {
            return None;
        }
        Some(entry.value.as_ref())
    }

    /// Share one immutable font payload with a derived compiled resource snapshot.
    pub fn get_shared(&self, handle: FontResourceHandle) -> Option<Arc<FontResource>> {
        let entry = self.entries.get(handle.id.get() as usize)?;
        (entry.version == handle.version).then(|| entry.value.clone())
    }

    pub fn handle_for_face(&self, face: &FontFaceIdentity) -> Option<FontResourceHandle> {
        self.handles_by_key
            .get(&FontResourceKey::from_face(face))
            .copied()
    }

    pub fn get_for_face(&self, face: &FontFaceIdentity) -> Option<&FontResource> {
        self.get(self.handle_for_face(face)?)
    }

    pub const fn stats(&self) -> FontResourceStats {
        FontResourceStats {
            live_resources: self.entries.len(),
            retained_bytes: self.retained_bytes,
            font_bytes: self.font_bytes,
        }
    }

    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontResourceError {
    ConflictingResource(FontResourceKey),
}

impl std::fmt::Display for FontResourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ConflictingResource(key) => write!(
                formatter,
                "font face {}#{} was interned with different bytes",
                key.face_key, key.face_index
            ),
        }
    }
}

impl std::error::Error for FontResourceError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(variation: &str) -> FontFaceIdentity {
        keyed_face("test-sans-v1", variation)
    }

    fn keyed_face(face_key: &str, variation: &str) -> FontFaceIdentity {
        FontFaceIdentity {
            family: Arc::from("Test Sans"),
            face_key: Arc::from(face_key),
            face_index: 0,
            variation_key: Arc::from(variation),
        }
    }

    #[test]
    fn repeated_face_interning_reuses_one_font_buffer() {
        let mut arena = FontResourceArena::new();
        let first = arena
            .intern_face(&face(""), Arc::<[u8]>::from([1, 2, 3, 4]))
            .unwrap();
        let second = arena
            .intern_face(&face(""), Arc::<[u8]>::from([1, 2, 3, 4]))
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.stats().font_bytes, 4);
    }

    #[test]
    fn variable_font_runs_share_the_same_backing_resource() {
        let mut arena = FontResourceArena::new();
        let regular = arena
            .intern_face(&face("wght=400"), Arc::<[u8]>::from([5, 6, 7]))
            .unwrap();
        let bold = arena
            .intern_face(&face("wght=700"), Arc::<[u8]>::from([5, 6, 7]))
            .unwrap();

        assert_eq!(regular, bold);
        assert_eq!(arena.handle_for_face(&face("wght=700")), Some(regular));
        assert_eq!(arena.len(), 1);
    }

    #[test]
    fn face_identity_cannot_silently_change_font_bytes() {
        let mut arena = FontResourceArena::new();
        let handle = arena
            .intern_face(&face(""), Arc::<[u8]>::from([1, 2, 3]))
            .unwrap();
        assert!(matches!(
            arena.intern_face(&face(""), Arc::<[u8]>::from([9, 8, 7])),
            Err(FontResourceError::ConflictingResource(_))
        ));
        assert_eq!(arena.get(handle).unwrap().data.as_ref(), &[1, 2, 3]);
        assert_eq!(arena.len(), 1);
    }

    #[test]
    fn font_handles_and_bytes_are_stable_for_the_arena_lifetime() {
        let mut arena = FontResourceArena::new();
        let first = arena
            .intern_face(&face(""), Arc::<[u8]>::from([1, 2, 3]))
            .unwrap();
        let second = arena
            .intern_face(&face("wght=700"), Arc::<[u8]>::from([1, 2, 3]))
            .unwrap();

        assert_eq!(first, second);
        assert_eq!(first.version, 0);
        assert_eq!(arena.get(first).unwrap().data.as_ref(), &[1, 2, 3]);
        assert_eq!(arena.stats().live_resources, 1);
    }

    #[test]
    fn bounded_working_set_churn_reaches_a_stable_resource_plateau() {
        const CYCLES: usize = 1_000;
        const WORKING_SET: usize = 4;

        let mut arena = FontResourceArena::new();
        let mut expected_handles = [None; WORKING_SET];
        let mut plateau = None;

        for cycle in 0..CYCLES {
            for (index, expected_handle) in expected_handles.iter_mut().enumerate() {
                let key = format!("test-sans-{index}");
                let bytes = Arc::<[u8]>::from([index as u8, 17, 29, 43]);
                let handle = arena
                    .intern_face(&keyed_face(&key, "wght=400"), bytes)
                    .unwrap();

                match expected_handle {
                    Some(expected) => assert_eq!(handle, *expected),
                    None => *expected_handle = Some(handle),
                }
            }

            if cycle == 0 {
                plateau = Some(arena.stats());
            } else {
                assert_eq!(arena.stats(), plateau.unwrap());
            }
        }

        let plateau = plateau.unwrap();
        assert_eq!(plateau.live_resources, WORKING_SET);
        assert_eq!(plateau.font_bytes, WORKING_SET * 4);
    }

    #[test]
    fn rejected_churn_does_not_grow_or_mutate_the_live_resource_set() {
        const ATTEMPTS: usize = 1_000;

        let mut arena = FontResourceArena::new();
        let stable_face = keyed_face("stable-face", "");
        let stable_handle = arena
            .intern_face(&stable_face, Arc::<[u8]>::from([1, 2, 3, 4]))
            .unwrap();
        let baseline = arena.stats();

        for attempt in 0..ATTEMPTS {
            let conflicting =
                Arc::<[u8]>::from([9, (attempt & 0xff) as u8, ((attempt >> 8) & 0xff) as u8, 7]);
            assert!(matches!(
                arena.intern_face(&stable_face, conflicting),
                Err(FontResourceError::ConflictingResource(_))
            ));
            assert_eq!(arena.stats(), baseline);
            assert_eq!(
                arena.get(stable_handle).unwrap().data.as_ref(),
                &[1, 2, 3, 4]
            );
        }
    }
}

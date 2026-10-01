mod images;
pub(crate) mod residency;
use crate::TransportImageResourceHandle;
use images::{install_images, ImageHandles, TransportImageEntry};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    sync::Arc,
};

use noon_core::{
    Color, FontFaceIdentity, FontResourceArena, FontResourceLookup, FontVariationSetting,
    GeometryRef, GeometryResource, GeometryResourceArena, GeometryResourceHandle,
    GeometryResourceLookup, GlyphRun, PositionedGlyph, Rect, StrokeCap, StrokeJoin, Style,
    TextAffineTransform, TextClusterIdentity, TextDirection, TextGlyphStroke, TextLayoutArtifact,
    TextLayoutBackend, TextLayoutBackendKind, TextPart, TextRenderItem, TextResource,
    TextResourceArena, TextResourceHandle, TextResourceLookup, TextResourceReplacementStaging,
    TextSourceKind, TextSourceSpan, TextVectorItem, TextVectorStyle, Transform2D, Vec2, VectorPath,
};
use serde::{Deserialize, Serialize};

use crate::TransportTextResourceHandle;

pub(crate) mod incremental_render;

/// One-shot resource channel paired with `noon.execution.retained`.
///
/// Frame deltas carry only small text handles. This bundle transfers the immutable
/// shaped text, vector-decoration geometry, and exact OpenType buffers once when a
/// retained scene is installed. Python never owns or serializes these payloads.
pub const RETAINED_RESOURCE_TRANSPORT_CHANNEL: &str = "noon.execution.retained.resources";
pub const RETAINED_RESOURCE_TRANSPORT_VERSION: u32 = 9;

/// A reusable arena slot qualified by its occupant generation.
pub(crate) fn render_geometry_id(slot: u32, generation: u32) -> u64 {
    (u64::from(generation) << 32) | u64::from(slot)
}

pub(crate) fn render_geometry_parts(id: u64) -> (u32, u32) {
    (id as u32, (id >> 32) as u32)
}

#[derive(Clone, Debug)]
pub(crate) struct RenderGeometrySlot {
    pub(crate) generation: u32,
    pub(crate) geometry: Option<Arc<GeometryRef>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct RenderGeometryUpdate {
    slot: u32,
    generation: u32,
    geometry: Option<GeometryRef>,
}

/// Exact source resources whose final published row reference was released by
/// this delta. Dependency resources are retired by the installed owner after it
/// accounts for the text closure, never by a transport-wide scan.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetainedResourceRetirements {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<TransportImageResourceHandle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub texts: Vec<TransportTextResourceHandle>,
}

impl RetainedResourceRetirements {
    pub(crate) fn is_empty(&self) -> bool {
        self.images.is_empty() && self.texts.is_empty()
    }

    pub(crate) fn validate(&self) -> Result<(), RetainedResourceTransportError> {
        if self.images.iter().collect::<HashSet<_>>().len() != self.images.len() {
            return Err(RetainedResourceTransportError::DuplicateRetiredImage);
        }
        if self.texts.iter().collect::<HashSet<_>>().len() != self.texts.len() {
            return Err(RetainedResourceTransportError::DuplicateRetiredText);
        }
        Ok(())
    }
}

/// Immutable compiled render geometry at the genuine cross-worker boundary.
/// Indices are scoped to the player session and this installed resource bundle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportRenderGeometryResources {
    session: u32,
    updates: Vec<RenderGeometryUpdate>,
    preparations: Vec<RenderGeometryPreparation>,
}

/// Derived renderer inputs, transferred once with the installed geometry table.
/// Actual playback still resolves its full mesh key; these are preparation hints.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct RenderGeometryPreparation {
    pub resource: u32,
    pub style: Style,
    pub transform: Transform2D,
}

impl RenderGeometryPreparation {
    fn is_finite(&self) -> bool {
        let finite_color = |color: Color| {
            [color.red, color.green, color.blue, color.alpha]
                .into_iter()
                .all(f32::is_finite)
        };
        [
            self.transform.translation.x,
            self.transform.translation.y,
            self.transform.scale.x,
            self.transform.scale.y,
            self.transform.rotation,
            self.style.stroke_width,
            self.style.opacity,
        ]
        .into_iter()
        .all(f32::is_finite)
            && self.style.fill.is_none_or(finite_color)
            && self.style.stroke.is_none_or(finite_color)
    }
}

fn group_render_geometry_preparations(
    preparations: Vec<RenderGeometryPreparation>,
) -> BTreeMap<u32, Vec<RenderGeometryPreparation>> {
    let mut grouped = BTreeMap::new();
    for preparation in preparations {
        grouped
            .entry(preparation.resource)
            .or_insert_with(Vec::new)
            .push(preparation);
    }
    grouped
}

#[cfg(test)]
pub(crate) fn compiled_render_geometry_preparations(
    compiled: &noon_compile::CompiledScene,
    geometries: &[Arc<GeometryRef>],
) -> Result<Vec<RenderGeometryPreparation>, RetainedResourceTransportError> {
    let indices = geometries
        .iter()
        .enumerate()
        .map(|(index, geometry)| {
            u32::try_from(index)
                .map(|index| (Arc::as_ptr(geometry) as usize, index))
                .map_err(|_| {
                    RetainedResourceTransportError::Encode("too many render resources".into())
                })
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    Ok(compiled
        .tracks_iter()
        .filter_map(|track| {
            let noon_compile::TransformGeometryPlan::PathPair {
                geometry,
                render_transform,
            } = track.transform_geometry_plan.as_ref()?
            else {
                return None;
            };
            let noon_core::TrackValues::Object { from, to } = &track.values else {
                return None;
            };
            if from.style.stroke_width_mode != to.style.stroke_width_mode
                || (render_transform.is_none()
                    && from.style.stroke_width_mode == noon_core::StrokeWidthMode::ScreenSpace)
            {
                return None;
            }
            Some(RenderGeometryPreparation {
                resource: *indices.get(&(Arc::as_ptr(geometry) as usize))?,
                style: from.style,
                transform: render_transform.unwrap_or(Transform2D::IDENTITY),
            })
        })
        .collect())
}

#[cfg(test)]
pub(crate) fn compiled_render_geometries(
    compiled: &noon_compile::CompiledScene,
) -> Arc<[Arc<GeometryRef>]> {
    let mut seen = std::collections::HashSet::new();
    compiled
        .tracks_iter()
        .filter_map(|track| {
            let noon_compile::TransformGeometryPlan::PathPair {
                geometry,
                render_transform,
            } = track.transform_geometry_plan.as_ref()?
            else {
                return None;
            };
            if render_transform.is_none()
                && matches!(&track.values,
                noon_core::TrackValues::Object { from, to }
                    if from.style.stroke_width_mode == noon_core::StrokeWidthMode::ScreenSpace
                    && to.style.stroke_width_mode == noon_core::StrokeWidthMode::ScreenSpace)
            {
                return None;
            }
            seen.insert(Arc::as_ptr(geometry) as usize)
                .then(|| geometry.clone())
        })
        .collect::<Vec<_>>()
        .into()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TransportGeometryResourceHandle {
    pub arena: u64,
    pub id: u64,
    pub version: u64,
}

impl From<GeometryResourceHandle> for TransportGeometryResourceHandle {
    fn from(value: GeometryResourceHandle) -> Self {
        Self {
            arena: value.arena,
            id: value.id.get(),
            version: value.version,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RetainedResourceBundle {
    pub channel: String,
    pub protocol_version: u32,
    images: Vec<TransportImageEntry>,
    texts: Vec<TransportTextEntry>,
    geometries: Vec<TransportGeometryEntry>,
    fonts: Vec<TransportFontEntry>,
    render_geometry_resources: Option<TransportRenderGeometryResources>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RetainedResourceInventory {
    images: HashSet<TransportImageResourceHandle>,
    texts: HashMap<(u64, u64), TransportTextResourceHandle>,
    geometries: BTreeSet<TransportGeometryResourceHandle>,
    fonts: BTreeSet<(String, u32)>,
}

impl RetainedResourceInventory {
    pub(crate) fn contains_image(&self, handle: TransportImageResourceHandle) -> bool {
        self.images.contains(&handle)
    }
    pub(crate) fn contains_text(&self, handle: TransportTextResourceHandle) -> bool {
        self.texts
            .get(&(handle.arena, handle.id))
            .is_some_and(|installed| *installed == handle)
    }
    pub(crate) fn forget_root_retirements(&mut self, retirements: &RetainedResourceRetirements) {
        for image in &retirements.images {
            self.images.remove(image);
        }
        for text in &retirements.texts {
            let key = (text.arena, text.id);
            if self.texts.get(&key) == Some(text) {
                self.texts.remove(&key);
            }
        }
    }

    pub(crate) fn forget_geometry(&mut self, handle: TransportGeometryResourceHandle) {
        self.geometries.remove(&handle);
    }

    pub(crate) fn forget_font(&mut self, face: &(String, u32)) {
        self.fonts.remove(face);
    }

    #[cfg(test)]
    pub(crate) fn counts(&self) -> [usize; 4] {
        [
            self.images.len(),
            self.texts.len(),
            self.geometries.len(),
            self.fonts.len(),
        ]
    }
}

pub(crate) struct TransportTextDependencyClosure {
    pub text: TransportTextResourceHandle,
    pub geometries: Vec<TransportGeometryResourceHandle>,
    pub fonts: Vec<(String, u32)>,
}

impl RetainedResourceBundle {
    pub(crate) fn inventory(&self) -> RetainedResourceInventory {
        RetainedResourceInventory {
            images: self.images.iter().map(|entry| entry.handle).collect(),
            texts: self
                .texts
                .iter()
                .map(|entry| ((entry.handle.arena, entry.handle.id), entry.handle))
                .collect(),
            geometries: self.geometries.iter().map(|entry| entry.handle).collect(),
            fonts: self
                .fonts
                .iter()
                .map(|entry| (entry.face_key.clone(), entry.face_index))
                .collect(),
        }
    }

    pub(crate) fn text_closures(
        &self,
    ) -> impl Iterator<Item = TransportTextDependencyClosure> + '_ {
        self.texts
            .iter()
            .map(|entry| TransportTextDependencyClosure {
                text: entry.handle,
                geometries: entry
                    .resource
                    .vector_items
                    .iter()
                    .map(|item| item.geometry)
                    .collect(),
                fonts: entry
                    .resource
                    .runs
                    .iter()
                    .map(|run| (run.font.face_key.to_string(), run.font.face_index))
                    .collect(),
            })
    }

    pub(crate) fn retain_additions(&mut self, installed: &mut RetainedResourceInventory) {
        self.images
            .retain(|entry| installed.images.insert(entry.handle));
        self.texts.retain(|entry| {
            let key = (entry.handle.arena, entry.handle.id);
            if installed.texts.get(&key) == Some(&entry.handle) {
                false
            } else {
                installed.texts.insert(key, entry.handle);
                true
            }
        });
        self.geometries
            .retain(|entry| installed.geometries.insert(entry.handle));
        self.fonts.retain(|entry| {
            installed
                .fonts
                .insert((entry.face_key.clone(), entry.face_index))
        });
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.images.is_empty()
            && self.texts.is_empty()
            && self.geometries.is_empty()
            && self.fonts.is_empty()
            && self.render_geometry_resources.is_none()
    }

    pub(crate) fn capture_additions(
        text_handles: impl IntoIterator<Item = TextResourceHandle>,
        texts: &(impl TextResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        installed: &RetainedResourceInventory,
    ) -> Result<Self, RetainedResourceTransportError> {
        Self::capture_filtered(text_handles, texts, geometries, fonts, Some(installed))
    }

    pub fn capture(
        text_handles: impl IntoIterator<Item = TextResourceHandle>,
        texts: &(impl TextResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
    ) -> Result<Self, RetainedResourceTransportError> {
        Self::capture_filtered(text_handles, texts, geometries, fonts, None)
    }

    fn capture_filtered(
        text_handles: impl IntoIterator<Item = TextResourceHandle>,
        texts: &(impl TextResourceLookup + ?Sized),
        geometries: &(impl GeometryResourceLookup + ?Sized),
        fonts: &(impl FontResourceLookup + ?Sized),
        installed: Option<&RetainedResourceInventory>,
    ) -> Result<Self, RetainedResourceTransportError> {
        let text_handles = text_handles
            .into_iter()
            .filter(|handle| {
                installed.is_none_or(|inventory| {
                    !inventory
                        .contains_text(TransportTextResourceHandle::from_source_handle(*handle))
                })
            })
            .collect::<BTreeSet<_>>();
        let mut geometry_handles = BTreeSet::new();
        let mut font_entries = BTreeMap::<(String, u32), TransportFontEntry>::new();
        let mut text_entries = Vec::with_capacity(text_handles.len());

        for handle in text_handles {
            let resource = texts.get(handle).ok_or_else(|| {
                RetainedResourceTransportError::UnknownText(
                    TransportTextResourceHandle::from_source_handle(handle),
                )
            })?;
            for vector in resource.vector_items.iter() {
                let transport = TransportGeometryResourceHandle::from(vector.geometry);
                if installed.is_none_or(|inventory| !inventory.geometries.contains(&transport)) {
                    geometry_handles.insert(vector.geometry);
                }
            }
            for run in resource.runs.iter() {
                let font_key = (run.font.face_key.to_string(), run.font.face_index);
                if installed.is_some_and(|inventory| inventory.fonts.contains(&font_key)) {
                    continue;
                }
                let font = fonts.get_for_face(&run.font).ok_or_else(|| {
                    RetainedResourceTransportError::MissingFont {
                        face_key: run.font.face_key.to_string(),
                        face_index: run.font.face_index,
                    }
                })?;
                let key = (font.key.face_key.to_string(), font.key.face_index);
                font_entries
                    .entry(key.clone())
                    .or_insert_with(|| TransportFontEntry {
                        face_key: key.0,
                        face_index: key.1,
                        data: font.data.as_ref().to_vec(),
                    });
            }
            text_entries.push(TransportTextEntry {
                handle: TransportTextResourceHandle::from_source_handle(handle),
                resource: TransportTextResource::from_core(resource),
            });
        }

        let mut geometry_entries = Vec::with_capacity(geometry_handles.len());
        for handle in geometry_handles {
            let resource = geometries
                .get(handle)
                .ok_or_else(|| RetainedResourceTransportError::UnknownGeometry(handle.into()))?;
            let GeometryResource::VectorPath(path) = resource;
            geometry_entries.push(TransportGeometryEntry {
                handle: handle.into(),
                path: path.as_ref().clone(),
            });
        }

        Ok(Self {
            channel: RETAINED_RESOURCE_TRANSPORT_CHANNEL.to_owned(),
            protocol_version: RETAINED_RESOURCE_TRANSPORT_VERSION,
            images: Vec::new(),
            texts: text_entries,
            geometries: geometry_entries,
            fonts: font_entries.into_values().collect(),
            render_geometry_resources: None,
        })
    }

    pub fn text_count(&self) -> usize {
        self.texts.len()
    }

    #[cfg(test)]
    pub(crate) fn set_render_geometries(
        &mut self,
        session: u32,
        geometries: Arc<[Arc<GeometryRef>]>,
        preparations: Vec<RenderGeometryPreparation>,
    ) {
        self.render_geometry_resources = Some(TransportRenderGeometryResources {
            session,
            updates: geometries
                .iter()
                .enumerate()
                .map(|(slot, geometry)| RenderGeometryUpdate {
                    slot: u32::try_from(slot).expect("render geometry index exceeds u32"),
                    generation: 0,
                    geometry: Some(geometry.as_ref().clone()),
                })
                .collect(),
            preparations,
        });
    }

    pub(crate) fn set_render_geometry_updates(
        &mut self,
        session: u32,
        updates: Vec<(u32, u32, Option<Arc<GeometryRef>>)>,
        preparations: Vec<RenderGeometryPreparation>,
    ) {
        self.render_geometry_resources = Some(TransportRenderGeometryResources {
            session,
            updates: updates
                .into_iter()
                .map(|(slot, generation, geometry)| RenderGeometryUpdate {
                    slot,
                    generation,
                    geometry: geometry.map(|geometry| geometry.as_ref().clone()),
                })
                .collect(),
            preparations,
        });
    }

    pub fn geometry_count(&self) -> usize {
        self.geometries.len()
    }

    pub fn font_count(&self) -> usize {
        self.fonts.len()
    }

    pub fn font_bytes(&self) -> usize {
        self.fonts.iter().map(|font| font.data.len()).sum()
    }

    pub fn encode_binary(&self) -> Result<Vec<u8>, RetainedResourceTransportError> {
        let mut bytes = Vec::new();
        ciborium::ser::into_writer(self, &mut bytes)
            .map_err(|error| RetainedResourceTransportError::Encode(error.to_string()))?;
        Ok(bytes)
    }

    pub fn decode_binary(bytes: &[u8]) -> Result<Self, RetainedResourceTransportError> {
        let bundle: Self = ciborium::de::from_reader(bytes)
            .map_err(|error| RetainedResourceTransportError::Decode(error.to_string()))?;
        bundle.validate_protocol()?;
        Ok(bundle)
    }

    pub fn install(self) -> Result<InstalledRetainedResources, RetainedResourceTransportError> {
        self.validate_protocol()?;
        if let Some(resources) = &self.render_geometry_resources {
            for (index, update) in resources.updates.iter().enumerate() {
                let Some(geometry) = &update.geometry else {
                    return Err(RetainedResourceTransportError::InvalidRenderGeometry(index));
                };
                if update.slot as usize != index
                    || update.generation != 0
                    || !matches!(geometry, GeometryRef::VectorPath(_))
                    || !geometry.is_finite()
                {
                    return Err(RetainedResourceTransportError::InvalidRenderGeometry(index));
                }
            }
            for (index, preparation) in resources.preparations.iter().enumerate() {
                if preparation.resource as usize >= resources.updates.len()
                    || !preparation.is_finite()
                {
                    return Err(RetainedResourceTransportError::InvalidRenderPreparation(
                        index,
                    ));
                }
            }
        }

        let (images, image_handles) = install_images(self.images)?;
        let mut geometries = GeometryResourceArena::new();
        let mut geometry_handles = HashMap::with_capacity(self.geometries.len());
        for entry in self.geometries {
            if geometry_handles.contains_key(&entry.handle) {
                return Err(RetainedResourceTransportError::DuplicateGeometry(
                    entry.handle,
                ));
            }
            let local = geometries.insert_path(entry.path);
            geometry_handles.insert(entry.handle, local);
        }

        let font_bytes = self
            .fonts
            .into_iter()
            .map(|entry| {
                (
                    (entry.face_key, entry.face_index),
                    Arc::<[u8]>::from(entry.data),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut fonts = FontResourceArena::new();
        let mut texts = TextResourceArena::new();
        let mut text_handles = HashMap::with_capacity(self.texts.len());

        for entry in self.texts {
            if text_handles.contains_key(&entry.handle) {
                return Err(RetainedResourceTransportError::DuplicateText(entry.handle));
            }
            let resource = entry.resource.into_core(&geometry_handles)?;
            for run in resource.runs.iter() {
                let key = (run.font.face_key.to_string(), run.font.face_index);
                let bytes = font_bytes.get(&key).ok_or_else(|| {
                    RetainedResourceTransportError::MissingFont {
                        face_key: key.0.clone(),
                        face_index: key.1,
                    }
                })?;
                fonts
                    .intern_face(&run.font, bytes.clone())
                    .map_err(|error| {
                        RetainedResourceTransportError::InvalidFont(error.to_string())
                    })?;
            }
            let local = texts
                .insert(resource)
                .map_err(|error| RetainedResourceTransportError::InvalidText(error.to_string()))?;
            text_handles.insert(entry.handle, local);
        }

        let inventory = RetainedResourceInventory {
            images: image_handles.keys().copied().collect(),
            texts: text_handles
                .keys()
                .copied()
                .map(|handle| ((handle.arena, handle.id), handle))
                .collect(),
            geometries: geometry_handles.keys().copied().collect(),
            fonts: font_bytes.keys().cloned().collect(),
        };
        let mut installed = InstalledRetainedResources {
            images,
            image_handles,
            image_layers: HashMap::new(),
            texts,
            geometries,
            fonts,
            text_handles,
            geometry_transports: geometry_handles
                .iter()
                .map(|(&transport, &local)| (local, transport))
                .collect(),
            geometry_handles,
            render_geometry_session: self
                .render_geometry_resources
                .as_ref()
                .map(|resources| resources.session),
            render_geometry_preparation_count: self
                .render_geometry_resources
                .as_ref()
                .map_or(0, |resources| resources.preparations.len()),
            render_geometry_preparations: self
                .render_geometry_resources
                .as_ref()
                .map(|resources| group_render_geometry_preparations(resources.preparations.clone()))
                .unwrap_or_default(),
            render_geometries: self
                .render_geometry_resources
                .map(|resources| {
                    resources
                        .updates
                        .into_iter()
                        .map(|update| RenderGeometrySlot {
                            generation: update.generation,
                            geometry: update.geometry.map(Arc::new),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            additions: Vec::new(),
            free_addition_layers: Vec::new(),
            free_addition_layer_set: HashSet::new(),
            text_layers: HashMap::new(),
            geometry_layers: HashMap::new(),
            font_layers: HashMap::new(),
            font_layers_by_face: HashMap::new(),
            font_arenas: HashSet::new(),
            inventory,
            text_dependencies: HashMap::new(),
            geometry_references: HashMap::new(),
            font_references: HashMap::new(),
        };
        installed.initialize_text_dependencies();
        Ok(installed)
    }

    fn validate_protocol(&self) -> Result<(), RetainedResourceTransportError> {
        if self.channel != RETAINED_RESOURCE_TRANSPORT_CHANNEL {
            return Err(RetainedResourceTransportError::InvalidChannel(
                self.channel.clone(),
            ));
        }
        if self.protocol_version != RETAINED_RESOURCE_TRANSPORT_VERSION {
            return Err(RetainedResourceTransportError::UnsupportedVersion(
                self.protocol_version,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct TextResourceDependencies {
    geometries: Vec<GeometryResourceHandle>,
    fonts: Vec<noon_core::FontResourceHandle>,
}

#[derive(Clone, Debug)]
pub struct InstalledRetainedResources {
    images: noon_core::RasterImageResourceArena,
    image_handles: ImageHandles,
    image_layers: HashMap<u64, usize>,
    texts: TextResourceArena,
    geometries: GeometryResourceArena,
    fonts: FontResourceArena,
    text_handles: HashMap<TransportTextResourceHandle, TextResourceHandle>,
    geometry_handles: HashMap<TransportGeometryResourceHandle, GeometryResourceHandle>,
    geometry_transports: HashMap<GeometryResourceHandle, TransportGeometryResourceHandle>,
    render_geometry_session: Option<u32>,
    render_geometries: Vec<RenderGeometrySlot>,
    render_geometry_preparations: BTreeMap<u32, Vec<RenderGeometryPreparation>>,
    render_geometry_preparation_count: usize,
    additions: Vec<InstalledRetainedResources>,
    free_addition_layers: Vec<usize>,
    free_addition_layer_set: HashSet<usize>,
    text_layers: HashMap<u64, usize>,
    geometry_layers: HashMap<u64, usize>,
    font_layers: HashMap<u64, usize>,
    font_layers_by_face: HashMap<(String, u32), usize>,
    font_arenas: HashSet<u64>,
    inventory: RetainedResourceInventory,
    text_dependencies: HashMap<TransportTextResourceHandle, TextResourceDependencies>,
    geometry_references: HashMap<GeometryResourceHandle, usize>,
    font_references: HashMap<noon_core::FontResourceHandle, usize>,
}

impl InstalledRetainedResources {
    pub(crate) fn render_geometry_session(&self) -> Option<u32> {
        self.render_geometry_session
    }

    pub(crate) fn render_geometries(&self) -> &[RenderGeometrySlot] {
        &self.render_geometries
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn render_geometry_preparations(
        &self,
    ) -> impl Iterator<Item = &RenderGeometryPreparation> {
        self.render_geometry_preparations
            .values()
            .flat_map(|preparations| preparations.iter())
    }

    pub fn render_geometry_preparation_count(&self) -> usize {
        self.render_geometry_preparation_count
    }

    pub fn texts(&self) -> &dyn TextResourceLookup {
        self
    }

    pub fn geometries(&self) -> &dyn GeometryResourceLookup {
        self
    }

    pub fn fonts(&self) -> &dyn FontResourceLookup {
        self
    }

    pub fn geometry_count(&self) -> usize {
        self.geometries.len()
            + self
                .additions
                .iter()
                .map(InstalledRetainedResources::geometry_count)
                .sum::<usize>()
    }

    pub fn resolve_text_handle(
        &self,
        transport: TransportTextResourceHandle,
    ) -> Option<TextResourceHandle> {
        self.text_handles.get(&transport).copied()
    }

    pub(crate) fn text_handle_remap(
        &self,
    ) -> HashMap<TransportTextResourceHandle, TextResourceHandle> {
        self.text_handles.clone()
    }

    #[cfg(test)]
    pub(crate) fn inventory(&self) -> RetainedResourceInventory {
        self.inventory.clone()
    }

    pub(crate) fn prepare_additions(
        &self,
        bundle: RetainedResourceBundle,
    ) -> Result<PreparedRetainedResourceAdditions, RetainedResourceTransportError> {
        if bundle.render_geometry_resources.is_some() {
            return Err(RetainedResourceTransportError::IncrementalRenderGeometryResources);
        }
        let inventory = bundle.inventory();
        if let Some(handle) = inventory.images.intersection(&self.inventory.images).next() {
            return Err(RetainedResourceTransportError::DuplicateImage(*handle));
        }
        if let Some(handle) = inventory
            .texts
            .values()
            .find(|handle| self.inventory.contains_text(**handle))
        {
            return Err(RetainedResourceTransportError::DuplicateText(*handle));
        }
        if let Some(handle) = inventory
            .geometries
            .intersection(&self.inventory.geometries)
            .next()
        {
            return Err(RetainedResourceTransportError::DuplicateGeometry(*handle));
        }
        if let Some((face_key, face_index)) =
            inventory.fonts.intersection(&self.inventory.fonts).next()
        {
            return Err(RetainedResourceTransportError::DuplicateFont {
                face_key: face_key.clone(),
                face_index: *face_index,
            });
        }
        bundle.validate_protocol()?;
        let (images, image_handles) = install_images(bundle.images)?;
        let mut geometries = GeometryResourceArena::new();
        let mut geometry_handles = HashMap::with_capacity(bundle.geometries.len());
        for entry in bundle.geometries {
            if geometry_handles.contains_key(&entry.handle) {
                return Err(RetainedResourceTransportError::DuplicateGeometry(
                    entry.handle,
                ));
            }
            let local = geometries.insert_path(entry.path);
            geometry_handles.insert(entry.handle, local);
        }
        for entry in &bundle.texts {
            for vector in &entry.resource.vector_items {
                if geometry_handles.contains_key(&vector.geometry) {
                    continue;
                }
                let local = self.geometry_handles.get(&vector.geometry).copied().ok_or(
                    RetainedResourceTransportError::MissingGeometry(vector.geometry),
                )?;
                geometry_handles.insert(vector.geometry, local);
            }
        }

        let mut font_bytes = BTreeMap::new();
        for entry in bundle.fonts {
            let key = (entry.face_key, entry.face_index);
            if font_bytes
                .insert(key.clone(), Arc::<[u8]>::from(entry.data))
                .is_some()
            {
                return Err(RetainedResourceTransportError::DuplicateFont {
                    face_key: key.0,
                    face_index: key.1,
                });
            }
        }
        let mut fonts = FontResourceArena::new();
        let mut font_arenas = HashSet::new();
        let mut texts = TextResourceArena::new();
        let mut staged_texts = StagedTextReplacements::default();
        let mut staged_text_arenas = HashSet::new();
        let mut text_handle_remap = HashMap::with_capacity(bundle.texts.len());
        let mut superseded_text_handles = Vec::new();
        let mut text_dependencies = HashMap::with_capacity(bundle.texts.len());
        for entry in bundle.texts {
            if text_handle_remap.contains_key(&entry.handle) {
                return Err(RetainedResourceTransportError::DuplicateText(entry.handle));
            }
            let resource = entry.resource.into_core(&geometry_handles)?;
            for run in resource.runs.iter() {
                let key = (run.font.face_key.to_string(), run.font.face_index);
                if let Some(bytes) = font_bytes.get(&key) {
                    let handle = fonts
                        .intern_face(&run.font, bytes.clone())
                        .map_err(|error| {
                            RetainedResourceTransportError::InvalidFont(error.to_string())
                        })?;
                    font_arenas.insert(handle.arena);
                } else if self.handle_for_face(&run.font).is_none() {
                    return Err(RetainedResourceTransportError::MissingFont {
                        face_key: key.0,
                        face_index: key.1,
                    });
                }
            }
            let dependencies = TextResourceDependencies {
                geometries: resource
                    .vector_items
                    .iter()
                    .map(|item| item.geometry)
                    .collect(),
                fonts: resource
                    .runs
                    .iter()
                    .map(|run| {
                        fonts
                            .handle_for_face(&run.font)
                            .or_else(|| self.handle_for_face(&run.font))
                            .expect("validated text font remains installed")
                    })
                    .collect(),
            };
            let key = (entry.handle.arena, entry.handle.id);
            if let Some(previous) = self.inventory.texts.get(&key) {
                superseded_text_handles.push(*previous);
            }
            let previous = self
                .inventory
                .texts
                .get(&key)
                .and_then(|previous| self.text_handles.get(previous))
                .copied()
                .and_then(|previous| {
                    self.text_arena_for_handle(previous)
                        .map(|(owner, arena)| (owner, previous, arena))
                });
            let local = match previous {
                Some((owner, previous, arena)) => staged_texts
                    .replace(owner, arena, previous, resource)
                    .map_err(|error| {
                        RetainedResourceTransportError::InvalidText(error.to_string())
                    }),
                None => texts.insert(resource).map_err(|error| {
                    RetainedResourceTransportError::InvalidText(error.to_string())
                }),
            }?;
            if previous.is_none() {
                staged_text_arenas.insert(local.arena);
            }
            text_handle_remap.insert(entry.handle, local);
            text_dependencies.insert(entry.handle, dependencies);
        }
        let mut installed = InstalledRetainedResources {
            images,
            image_handles,
            image_layers: HashMap::new(),
            texts,
            geometries,
            fonts,
            text_handles: HashMap::new(),
            geometry_handles: geometry_handles
                .into_iter()
                .filter(|(transport, _)| inventory.geometries.contains(transport))
                .collect(),
            geometry_transports: HashMap::new(),
            render_geometry_session: None,
            render_geometries: Vec::new(),
            render_geometry_preparations: BTreeMap::new(),
            render_geometry_preparation_count: 0,
            additions: Vec::new(),
            free_addition_layers: Vec::new(),
            free_addition_layer_set: HashSet::new(),
            text_layers: HashMap::new(),
            geometry_layers: HashMap::new(),
            font_layers: HashMap::new(),
            font_layers_by_face: HashMap::new(),
            font_arenas,
            inventory,
            text_dependencies,
            geometry_references: HashMap::new(),
            font_references: HashMap::new(),
        };
        installed.geometry_transports = installed
            .geometry_handles
            .iter()
            .map(|(&transport, &local)| (local, transport))
            .collect();
        Ok(PreparedRetainedResourceAdditions {
            installed,
            staged_texts,
            staged_text_arenas,
            text_handle_remap,
            superseded_text_handles,
        })
    }

    pub(crate) fn commit_additions(&mut self, additions: PreparedRetainedResourceAdditions) {
        let PreparedRetainedResourceAdditions {
            installed,
            staged_texts,
            staged_text_arenas,
            text_handle_remap,
            superseded_text_handles,
        } = additions;
        self.commit_staged_text_replacements(staged_texts)
            .expect("prepared retained text replacements must remain current until commit");
        let owns_payload = !installed.images.is_empty()
            || !installed.texts.is_empty()
            || !installed.geometries.is_empty()
            || !installed.fonts.is_empty();
        // A version replacement may reuse an existing text arena and add no
        // payload layer. It must not consume a free layer reservation.
        let layer = if owns_payload {
            let layer = self
                .free_addition_layers
                .pop()
                .unwrap_or(self.additions.len());
            self.free_addition_layer_set.remove(&layer);
            layer
        } else {
            self.additions.len()
        };
        for content in installed.image_handles.values() {
            self.image_layers.insert(content.resource().arena, layer);
        }
        self.inventory
            .images
            .extend(installed.inventory.images.iter().copied());
        self.image_handles.extend(
            installed
                .image_handles
                .iter()
                .map(|(&key, &value)| (key, value)),
        );
        for &arena in &staged_text_arenas {
            self.text_layers.insert(arena, layer);
        }
        for handle in installed.geometry_handles.values() {
            self.geometry_layers.insert(handle.arena, layer);
        }
        for &arena in &installed.font_arenas {
            self.font_layers.insert(arena, layer);
        }
        for face in &installed.inventory.fonts {
            self.font_layers_by_face.insert(face.clone(), layer);
        }
        for (&key, &handle) in &installed.inventory.texts {
            if let Some(previous) = self.inventory.texts.insert(key, handle) {
                self.text_handles.remove(&previous);
            }
        }
        self.inventory
            .geometries
            .extend(installed.inventory.geometries.iter().copied());
        self.inventory
            .fonts
            .extend(installed.inventory.fonts.iter().cloned());
        self.text_handles.extend(text_handle_remap);
        self.geometry_handles.extend(
            installed
                .geometry_handles
                .iter()
                .map(|(&key, &value)| (key, value)),
        );
        self.geometry_transports.extend(
            installed
                .geometry_transports
                .iter()
                .map(|(&local, &transport)| (local, transport)),
        );
        for (&transport, dependencies) in &installed.text_dependencies {
            self.register_text_dependencies(transport, dependencies.clone());
        }
        for superseded in superseded_text_handles {
            self.release_text_dependencies(superseded);
        }
        if owns_payload {
            if layer == self.additions.len() {
                self.additions.push(installed);
            } else {
                self.additions[layer] = installed;
            }
        }
    }

    pub(crate) fn retire(&mut self, retirements: &RetainedResourceRetirements) {
        for image in &retirements.images {
            self.retire_image(*image);
        }
        for text in &retirements.texts {
            self.retire_text(*text);
        }
    }

    fn initialize_text_dependencies(&mut self) {
        let entries = self
            .text_handles
            .iter()
            .filter_map(|(&transport, &local)| {
                let resource = self.texts.get(local)?;
                let dependencies = TextResourceDependencies {
                    geometries: resource
                        .vector_items
                        .iter()
                        .map(|item| item.geometry)
                        .collect(),
                    fonts: resource
                        .runs
                        .iter()
                        .filter_map(|run| self.fonts.handle_for_face(&run.font))
                        .collect(),
                };
                Some((transport, dependencies))
            })
            .collect::<Vec<_>>();
        for (transport, dependencies) in entries {
            self.register_text_dependencies(transport, dependencies);
        }
    }

    fn register_text_dependencies(
        &mut self,
        transport: TransportTextResourceHandle,
        dependencies: TextResourceDependencies,
    ) {
        for geometry in &dependencies.geometries {
            *self.geometry_references.entry(*geometry).or_default() += 1;
        }
        for font in &dependencies.fonts {
            *self.font_references.entry(*font).or_default() += 1;
        }
        self.text_dependencies.insert(transport, dependencies);
    }

    fn release_text_dependencies(&mut self, transport: TransportTextResourceHandle) {
        let Some(dependencies) = self.text_dependencies.remove(&transport) else {
            return;
        };
        for geometry in dependencies.geometries {
            let Some(count) = self.geometry_references.get_mut(&geometry) else {
                continue;
            };
            *count -= 1;
            if *count == 0 {
                self.geometry_references.remove(&geometry);
                self.retire_geometry(geometry);
            }
        }
        for font in dependencies.fonts {
            let Some(count) = self.font_references.get_mut(&font) else {
                continue;
            };
            *count -= 1;
            if *count == 0 {
                self.font_references.remove(&font);
                self.retire_font(font);
            }
        }
    }

    fn retire_text(&mut self, transport: TransportTextResourceHandle) {
        let Some(local) = self.text_handles.remove(&transport) else {
            return;
        };
        if self.inventory.texts.get(&(transport.arena, transport.id)) == Some(&transport) {
            self.inventory
                .texts
                .remove(&(transport.arena, transport.id));
        }
        let owner = self.text_arena_for_handle(local).map(|(owner, _)| owner);
        if let Some(owner) = owner {
            let _ = self.text_arena_mut(owner).remove(local.id);
            if matches!(owner, InstalledTextArena::Addition(_)) && self.text_arena(owner).is_empty()
            {
                self.text_layers.remove(&local.arena);
            }
            self.recycle_layer_if_empty(owner);
        }
        self.release_text_dependencies(transport);
    }

    fn retire_image(&mut self, transport: TransportImageResourceHandle) {
        let Some(content) = self.image_handles.remove(&transport) else {
            return;
        };
        self.inventory.images.remove(&transport);
        let handle = content.resource();
        if self.images.get(handle).is_some() {
            self.images.remove(handle);
        } else if let Some(&layer) = self.image_layers.get(&handle.arena) {
            self.additions[layer].images.remove(handle);
            if self.additions[layer].images.is_empty() {
                self.image_layers.remove(&handle.arena);
            }
            self.recycle_layer_if_empty(InstalledTextArena::Addition(layer));
        }
    }

    fn retire_geometry(&mut self, handle: GeometryResourceHandle) {
        let removed = if self.geometries.get(handle).is_some() {
            self.geometries.remove(handle.id).is_ok()
        } else if let Some(&layer) = self.geometry_layers.get(&handle.arena) {
            self.additions[layer].geometries.remove(handle.id).is_ok()
        } else {
            false
        };
        if removed {
            if let Some(transport) = self.geometry_transports.remove(&handle) {
                self.geometry_handles.remove(&transport);
                self.inventory.geometries.remove(&transport);
            }
            if let Some(layer) = self.geometry_layers.get(&handle.arena).copied() {
                if self.additions[layer].geometries.is_empty() {
                    self.geometry_layers.remove(&handle.arena);
                }
                self.recycle_layer_if_empty(InstalledTextArena::Addition(layer));
            }
        }
    }

    fn retire_font(&mut self, handle: noon_core::FontResourceHandle) {
        let face = if self.fonts.get(handle).is_some() {
            self.fonts
                .get(handle)
                .map(|font| (font.key.face_key.to_string(), font.key.face_index))
        } else if let Some(&layer) = self.font_layers.get(&handle.arena) {
            self.additions[layer]
                .fonts
                .get(handle)
                .map(|font| (font.key.face_key.to_string(), font.key.face_index))
        } else {
            None
        };
        if self.fonts.get(handle).is_some() {
            self.fonts.remove(handle);
        } else if let Some(&layer) = self.font_layers.get(&handle.arena) {
            self.additions[layer].fonts.remove(handle);
        }
        if let Some(face) = face {
            self.inventory.fonts.remove(&face);
            self.font_layers_by_face.remove(&face);
            if let Some(layer) = self.font_layers.get(&handle.arena).copied() {
                if self.additions[layer].fonts.is_empty() {
                    self.font_layers.remove(&handle.arena);
                }
                self.recycle_layer_if_empty(InstalledTextArena::Addition(layer));
            }
        }
    }

    fn recycle_layer_if_empty(&mut self, owner: InstalledTextArena) {
        let InstalledTextArena::Addition(layer) = owner else {
            return;
        };
        let resources = &self.additions[layer];
        if !resources.images.is_empty()
            || !resources.texts.is_empty()
            || !resources.geometries.is_empty()
            || !resources.fonts.is_empty()
            || !self.free_addition_layer_set.insert(layer)
        {
            return;
        }
        self.free_addition_layers.push(layer);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstalledTextArena {
    Base,
    Addition(usize),
}

#[derive(Default)]
struct StagedTextReplacements {
    stages: HashMap<u64, TextResourceReplacementStaging>,
    owners: HashMap<u64, InstalledTextArena>,
}

impl StagedTextReplacements {
    fn replace(
        &mut self,
        owner: InstalledTextArena,
        arena: &TextResourceArena,
        expected: TextResourceHandle,
        resource: TextResource,
    ) -> Result<TextResourceHandle, noon_core::TextResourceError> {
        if let Some(staging) = self.stages.get_mut(&expected.arena) {
            return staging.replace(arena, expected, resource);
        }
        let mut staging = arena.replacement_staging();
        let next = staging.replace(arena, expected, resource)?;
        self.owners.insert(expected.arena, owner);
        self.stages.insert(expected.arena, staging);
        Ok(next)
    }

    fn get(&self, handle: TextResourceHandle) -> Option<&TextResource> {
        self.stages.get(&handle.arena)?.get(handle)
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.stages
            .values()
            .map(TextResourceReplacementStaging::len)
            .sum()
    }

    fn iter(&self) -> impl Iterator<Item = (InstalledTextArena, &TextResourceReplacementStaging)> {
        self.stages.iter().map(|(&arena, staging)| {
            (
                *self
                    .owners
                    .get(&arena)
                    .expect("each staged text arena has one owner"),
                staging,
            )
        })
    }

    fn into_iter(
        self,
    ) -> impl Iterator<Item = (InstalledTextArena, TextResourceReplacementStaging)> {
        let Self { stages, owners } = self;
        stages.into_iter().map(move |(arena, staging)| {
            (
                owners
                    .get(&arena)
                    .copied()
                    .expect("each staged text arena has one owner"),
                staging,
            )
        })
    }
}

pub(crate) struct PreparedRetainedResourceAdditions {
    installed: InstalledRetainedResources,
    staged_texts: StagedTextReplacements,
    staged_text_arenas: HashSet<u64>,
    text_handle_remap: HashMap<TransportTextResourceHandle, TextResourceHandle>,
    superseded_text_handles: Vec<TransportTextResourceHandle>,
}

impl PreparedRetainedResourceAdditions {
    pub(crate) fn text_handle_remap(
        &self,
    ) -> HashMap<TransportTextResourceHandle, TextResourceHandle> {
        self.text_handle_remap.clone()
    }

    pub(crate) fn superseded_text_handles(&self) -> &[TransportTextResourceHandle] {
        &self.superseded_text_handles
    }

    pub(crate) fn text_lookup<'a>(
        &'a self,
        existing: &'a InstalledRetainedResources,
    ) -> InstalledTextResourceOverlay<'a> {
        InstalledTextResourceOverlay {
            existing,
            additions: &self.installed,
            staged_texts: &self.staged_texts,
        }
    }
}

pub(crate) struct InstalledTextResourceOverlay<'a> {
    existing: &'a InstalledRetainedResources,
    additions: &'a InstalledRetainedResources,
    staged_texts: &'a StagedTextReplacements,
}

impl TextResourceLookup for InstalledTextResourceOverlay<'_> {
    fn get(&self, handle: TextResourceHandle) -> Option<&TextResource> {
        self.staged_texts
            .get(handle)
            .or_else(|| self.additions.get_text(handle))
            .or_else(|| self.existing.get_text(handle))
    }
}

impl InstalledRetainedResources {
    fn text_arena_for_handle(
        &self,
        handle: TextResourceHandle,
    ) -> Option<(InstalledTextArena, &TextResourceArena)> {
        if self.texts.get(handle).is_some() {
            return Some((InstalledTextArena::Base, &self.texts));
        }
        let layer = *self.text_layers.get(&handle.arena)?;
        let arena = &self.additions.get(layer)?.texts;
        arena.get(handle)?;
        Some((InstalledTextArena::Addition(layer), arena))
    }

    fn text_arena(&self, owner: InstalledTextArena) -> &TextResourceArena {
        match owner {
            InstalledTextArena::Base => &self.texts,
            InstalledTextArena::Addition(layer) => &self.additions[layer].texts,
        }
    }

    fn text_arena_mut(&mut self, owner: InstalledTextArena) -> &mut TextResourceArena {
        match owner {
            InstalledTextArena::Base => &mut self.texts,
            InstalledTextArena::Addition(layer) => &mut self.additions[layer].texts,
        }
    }

    fn commit_staged_text_replacements(
        &mut self,
        staged: StagedTextReplacements,
    ) -> Result<(), noon_core::TextResourceReplacementStagingError> {
        for (owner, staging) in staged.iter() {
            self.text_arena(owner)
                .validate_replacement_staging(staging)?;
        }
        for (owner, staging) in staged.into_iter() {
            self.text_arena_mut(owner)
                .commit_replacement_staging(staging)?;
        }
        Ok(())
    }

    fn get_text(&self, handle: TextResourceHandle) -> Option<&TextResource> {
        self.texts.get(handle).or_else(|| {
            self.text_layers
                .get(&handle.arena)
                .and_then(|&layer| self.additions.get(layer))
                .and_then(|resources| resources.texts.get(handle))
        })
    }
}

impl TextResourceLookup for InstalledRetainedResources {
    fn get(&self, handle: TextResourceHandle) -> Option<&TextResource> {
        self.get_text(handle)
    }
}

impl GeometryResourceLookup for InstalledRetainedResources {
    fn current_handle(&self, id: noon_core::GeometryId) -> Option<GeometryResourceHandle> {
        self.geometries.current_handle(id)
    }

    fn get(&self, handle: GeometryResourceHandle) -> Option<&GeometryResource> {
        self.geometries.get(handle).or_else(|| {
            self.geometry_layers
                .get(&handle.arena)
                .and_then(|&layer| self.additions.get(layer))
                .and_then(|resources| resources.geometries.get(handle))
        })
    }
}

impl FontResourceLookup for InstalledRetainedResources {
    fn handle_for_face(&self, face: &FontFaceIdentity) -> Option<noon_core::FontResourceHandle> {
        self.fonts.handle_for_face(face).or_else(|| {
            self.font_layers_by_face
                .get(&(face.face_key.to_string(), face.face_index))
                .and_then(|&layer| self.additions.get(layer))
                .and_then(|resources| resources.fonts.handle_for_face(face))
        })
    }

    fn get(&self, handle: noon_core::FontResourceHandle) -> Option<&noon_core::FontResource> {
        self.fonts.get(handle).or_else(|| {
            self.font_layers
                .get(&handle.arena)
                .and_then(|&layer| self.additions.get(layer))
                .and_then(|resources| resources.fonts.get(handle))
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RetainedResourceTransportError {
    DuplicateRetiredImage,
    DuplicateRetiredText,
    RetiredLiveImage(TransportImageResourceHandle),
    RetiredLiveText(TransportTextResourceHandle),
    UnknownImage(TransportImageResourceHandle),
    DuplicateImage(TransportImageResourceHandle),
    InvalidImage(String),
    InvalidChannel(String),
    UnsupportedVersion(u32),
    UnknownText(TransportTextResourceHandle),
    UnknownGeometryId(noon_core::GeometryId),
    UnknownGeometry(TransportGeometryResourceHandle),
    DuplicateText(TransportTextResourceHandle),
    DuplicateGeometry(TransportGeometryResourceHandle),
    DuplicateFont { face_key: String, face_index: u32 },
    MissingGeometry(TransportGeometryResourceHandle),
    MissingFont { face_key: String, face_index: u32 },
    InvalidText(String),
    InvalidFont(String),
    InvalidRenderGeometry(usize),
    InvalidRenderPreparation(usize),
    IncrementalRenderGeometryResources,
    Encode(String),
    Decode(String),
}

impl fmt::Display for RetainedResourceTransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DuplicateRetiredImage => formatter.write_str("duplicate retired raster image"),
            Self::DuplicateRetiredText => formatter.write_str("duplicate retired text resource"),
            Self::RetiredLiveImage(handle) => {
                write!(formatter, "retired live raster image {handle:?}")
            }
            Self::RetiredLiveText(handle) => {
                write!(formatter, "retired live text resource {handle:?}")
            }
            Self::UnknownImage(handle) => {
                write!(formatter, "unknown raster image resource {handle:?}")
            }
            Self::DuplicateImage(handle) => {
                write!(formatter, "duplicate raster image resource {handle:?}")
            }
            Self::InvalidImage(reason) => {
                write!(formatter, "invalid raster image resource: {reason}")
            }
            Self::InvalidRenderPreparation(index) => {
                write!(formatter, "invalid render geometry preparation {index}")
            }
            Self::InvalidRenderGeometry(index) => write!(
                formatter,
                "invalid compiled render geometry resource {index}"
            ),
            Self::InvalidChannel(channel) => {
                write!(formatter, "invalid retained resource channel {channel}")
            }
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported retained resource version {version}")
            }
            Self::UnknownText(handle) => write!(
                formatter,
                "unknown retained text resource {}@{}",
                handle.id, handle.version
            ),
            Self::UnknownGeometryId(id) => {
                write!(formatter, "unknown retained geometry resource {}", id.get())
            }
            Self::UnknownGeometry(handle) => write!(
                formatter,
                "unknown retained geometry resource {}@{}",
                handle.id, handle.version
            ),
            Self::DuplicateText(handle) => write!(
                formatter,
                "duplicate retained text resource {}@{}",
                handle.id, handle.version
            ),
            Self::DuplicateGeometry(handle) => write!(
                formatter,
                "duplicate retained geometry resource {}@{}",
                handle.id, handle.version
            ),
            Self::DuplicateFont {
                face_key,
                face_index,
            } => write!(formatter, "duplicate retained font {face_key}#{face_index}"),
            Self::MissingGeometry(handle) => write!(
                formatter,
                "missing retained geometry dependency {}@{}",
                handle.id, handle.version
            ),
            Self::MissingFont {
                face_key,
                face_index,
            } => write!(formatter, "missing retained font {face_key}#{face_index}"),
            Self::InvalidText(message) => write!(formatter, "invalid retained text: {message}"),
            Self::InvalidFont(message) => write!(formatter, "invalid retained font: {message}"),
            Self::IncrementalRenderGeometryResources => formatter.write_str(
                "incremental retained resources cannot replace compiled render geometry",
            ),
            Self::Encode(message) => {
                write!(formatter, "retained resource encode failed: {message}")
            }
            Self::Decode(message) => {
                write!(formatter, "retained resource decode failed: {message}")
            }
        }
    }
}

impl std::error::Error for RetainedResourceTransportError {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportTextEntry {
    handle: TransportTextResourceHandle,
    resource: TransportTextResource,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportGeometryEntry {
    handle: TransportGeometryResourceHandle,
    path: VectorPath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportFontEntry {
    face_key: String,
    face_index: u32,
    data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportTextResource {
    source: String,
    kind: TransportTextSourceKind,
    runs: Vec<TransportGlyphRun>,
    vector_items: Vec<TransportTextVectorItem>,
    render_items: Vec<TransportTextRenderItem>,
    parts: Vec<TransportTextPart>,
    bounds: Rect,
    baseline: f32,
    layout_artifact: Option<TransportTextLayoutArtifact>,
}

impl TransportTextResource {
    fn from_core(resource: &TextResource) -> Self {
        Self {
            source: resource.source.to_string(),
            kind: resource.kind.into(),
            runs: resource
                .runs
                .iter()
                .map(TransportGlyphRun::from_core)
                .collect(),
            vector_items: resource
                .vector_items
                .iter()
                .map(TransportTextVectorItem::from_core)
                .collect(),
            render_items: resource
                .render_items
                .iter()
                .copied()
                .map(TransportTextRenderItem::from)
                .collect(),
            parts: resource
                .parts
                .iter()
                .map(TransportTextPart::from_core)
                .collect(),
            bounds: resource.bounds,
            baseline: resource.baseline,
            layout_artifact: resource
                .layout_artifact
                .as_ref()
                .map(TransportTextLayoutArtifact::from_core),
        }
    }

    fn into_core(
        self,
        geometry_handles: &HashMap<TransportGeometryResourceHandle, GeometryResourceHandle>,
    ) -> Result<TextResource, RetainedResourceTransportError> {
        Ok(TextResource {
            source: Arc::from(self.source),
            kind: self.kind.into(),
            runs: self
                .runs
                .into_iter()
                .map(TransportGlyphRun::into_core)
                .collect::<Vec<_>>()
                .into(),
            vector_items: self
                .vector_items
                .into_iter()
                .map(|item| item.into_core(geometry_handles))
                .collect::<Result<Vec<_>, _>>()?
                .into(),
            render_items: self
                .render_items
                .into_iter()
                .map(TextRenderItem::from)
                .collect::<Vec<_>>()
                .into(),
            parts: self
                .parts
                .into_iter()
                .map(TransportTextPart::into_core)
                .collect::<Vec<_>>()
                .into(),
            bounds: self.bounds,
            baseline: self.baseline,
            layout_artifact: self
                .layout_artifact
                .map(TransportTextLayoutArtifact::into_core),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransportTextSourceKind {
    Plain,
    Markup,
    Typst,
    MathTypst,
    Tex,
    MathTex,
}

impl From<TextSourceKind> for TransportTextSourceKind {
    fn from(value: TextSourceKind) -> Self {
        match value {
            TextSourceKind::Plain => Self::Plain,
            TextSourceKind::Markup => Self::Markup,
            TextSourceKind::Typst => Self::Typst,
            TextSourceKind::MathTypst => Self::MathTypst,
            TextSourceKind::Tex => Self::Tex,
            TextSourceKind::MathTex => Self::MathTex,
        }
    }
}

impl From<TransportTextSourceKind> for TextSourceKind {
    fn from(value: TransportTextSourceKind) -> Self {
        match value {
            TransportTextSourceKind::Plain => Self::Plain,
            TransportTextSourceKind::Markup => Self::Markup,
            TransportTextSourceKind::Typst => Self::Typst,
            TransportTextSourceKind::MathTypst => Self::MathTypst,
            TransportTextSourceKind::Tex => Self::Tex,
            TransportTextSourceKind::MathTex => Self::MathTex,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportGlyphRun {
    font: TransportFontFaceIdentity,
    variations: Vec<TransportFontVariationSetting>,
    font_size: f32,
    direction: TransportTextDirection,
    fill: Option<Color>,
    stroke: Option<TransportTextGlyphStroke>,
    transform: TransportTextAffineTransform,
    glyphs: Vec<TransportPositionedGlyph>,
}

impl TransportGlyphRun {
    fn from_core(run: &GlyphRun) -> Self {
        Self {
            font: TransportFontFaceIdentity::from_core(&run.font),
            variations: run
                .variations
                .iter()
                .copied()
                .map(TransportFontVariationSetting::from)
                .collect(),
            font_size: run.font_size,
            direction: run.direction.into(),
            fill: run.fill,
            stroke: run.stroke.as_ref().map(TransportTextGlyphStroke::from_core),
            transform: run.transform.into(),
            glyphs: run
                .glyphs
                .iter()
                .map(TransportPositionedGlyph::from_core)
                .collect(),
        }
    }

    fn into_core(self) -> GlyphRun {
        GlyphRun {
            font: self.font.into_core(),
            variations: self
                .variations
                .into_iter()
                .map(FontVariationSetting::from)
                .collect::<Vec<_>>()
                .into(),
            font_size: self.font_size,
            direction: self.direction.into(),
            fill: self.fill,
            stroke: self.stroke.map(TransportTextGlyphStroke::into_core),
            transform: self.transform.into(),
            glyphs: self
                .glyphs
                .into_iter()
                .map(TransportPositionedGlyph::into_core)
                .collect::<Vec<_>>()
                .into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportFontFaceIdentity {
    family: String,
    face_key: String,
    face_index: u32,
    variation_key: String,
}

impl TransportFontFaceIdentity {
    fn from_core(face: &FontFaceIdentity) -> Self {
        Self {
            family: face.family.to_string(),
            face_key: face.face_key.to_string(),
            face_index: face.face_index,
            variation_key: face.variation_key.to_string(),
        }
    }

    fn into_core(self) -> FontFaceIdentity {
        FontFaceIdentity {
            family: Arc::from(self.family),
            face_key: Arc::from(self.face_key),
            face_index: self.face_index,
            variation_key: Arc::from(self.variation_key),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct TransportFontVariationSetting {
    tag: [u8; 4],
    value: f32,
}

impl From<FontVariationSetting> for TransportFontVariationSetting {
    fn from(value: FontVariationSetting) -> Self {
        Self {
            tag: value.tag,
            value: value.value,
        }
    }
}

impl From<TransportFontVariationSetting> for FontVariationSetting {
    fn from(value: TransportFontVariationSetting) -> Self {
        Self {
            tag: value.tag,
            value: value.value,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransportTextDirection {
    LeftToRight,
    RightToLeft,
}

impl From<TextDirection> for TransportTextDirection {
    fn from(value: TextDirection) -> Self {
        match value {
            TextDirection::LeftToRight => Self::LeftToRight,
            TextDirection::RightToLeft => Self::RightToLeft,
        }
    }
}

impl From<TransportTextDirection> for TextDirection {
    fn from(value: TransportTextDirection) -> Self {
        match value {
            TransportTextDirection::LeftToRight => Self::LeftToRight,
            TransportTextDirection::RightToLeft => Self::RightToLeft,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportTextGlyphStroke {
    paint: Option<Color>,
    width: f32,
    cap: StrokeCap,
    join: StrokeJoin,
    dash_array: Vec<f32>,
    dash_phase: f32,
    miter_limit: f32,
}

impl TransportTextGlyphStroke {
    fn from_core(stroke: &TextGlyphStroke) -> Self {
        Self {
            paint: stroke.paint,
            width: stroke.width,
            cap: stroke.cap,
            join: stroke.join,
            dash_array: stroke.dash_array.as_ref().to_vec(),
            dash_phase: stroke.dash_phase,
            miter_limit: stroke.miter_limit,
        }
    }

    fn into_core(self) -> TextGlyphStroke {
        TextGlyphStroke {
            paint: self.paint,
            width: self.width,
            cap: self.cap,
            join: self.join,
            dash_array: self.dash_array.into(),
            dash_phase: self.dash_phase,
            miter_limit: self.miter_limit,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct TransportTextAffineTransform {
    xx: f32,
    yx: f32,
    xy: f32,
    yy: f32,
    tx: f32,
    ty: f32,
}

impl From<TextAffineTransform> for TransportTextAffineTransform {
    fn from(value: TextAffineTransform) -> Self {
        Self {
            xx: value.xx,
            yx: value.yx,
            xy: value.xy,
            yy: value.yy,
            tx: value.tx,
            ty: value.ty,
        }
    }
}

impl From<TransportTextAffineTransform> for TextAffineTransform {
    fn from(value: TransportTextAffineTransform) -> Self {
        Self {
            xx: value.xx,
            yx: value.yx,
            xy: value.xy,
            yy: value.yy,
            tx: value.tx,
            ty: value.ty,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportPositionedGlyph {
    glyph_id: u32,
    cluster: TransportTextClusterIdentity,
    origin: Vec2,
    advance: Vec2,
    bounds: Rect,
}

impl TransportPositionedGlyph {
    fn from_core(glyph: &PositionedGlyph) -> Self {
        Self {
            glyph_id: glyph.glyph_id,
            cluster: TransportTextClusterIdentity::from_core(&glyph.cluster),
            origin: glyph.origin,
            advance: glyph.advance,
            bounds: glyph.bounds,
        }
    }

    fn into_core(self) -> PositionedGlyph {
        PositionedGlyph {
            glyph_id: self.glyph_id,
            cluster: self.cluster.into_core(),
            origin: self.origin,
            advance: self.advance,
            bounds: self.bounds,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportTextClusterIdentity {
    source_span: TransportTextSourceSpan,
    cluster_ordinal: u32,
    semantic_key: Option<String>,
}

impl TransportTextClusterIdentity {
    fn from_core(cluster: &TextClusterIdentity) -> Self {
        Self {
            source_span: cluster.source_span.into(),
            cluster_ordinal: cluster.cluster_ordinal,
            semantic_key: cluster.semantic_key.as_deref().map(str::to_owned),
        }
    }

    fn into_core(self) -> TextClusterIdentity {
        TextClusterIdentity {
            source_span: self.source_span.into(),
            cluster_ordinal: self.cluster_ordinal,
            semantic_key: self.semantic_key.map(Arc::from),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportTextSourceSpan {
    start: u32,
    end: u32,
}

impl From<TextSourceSpan> for TransportTextSourceSpan {
    fn from(value: TextSourceSpan) -> Self {
        Self {
            start: value.start,
            end: value.end,
        }
    }
}

impl From<TransportTextSourceSpan> for TextSourceSpan {
    fn from(value: TransportTextSourceSpan) -> Self {
        TextSourceSpan::new(value.start, value.end)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct TransportTextVectorItem {
    geometry: TransportGeometryResourceHandle,
    transform: TransportTextAffineTransform,
    style: TransportTextVectorStyle,
    source_span: Option<TransportTextSourceSpan>,
    semantic_key: Option<String>,
}

impl TransportTextVectorItem {
    fn from_core(item: &TextVectorItem) -> Self {
        Self {
            geometry: item.geometry.into(),
            transform: item.transform.into(),
            style: item.style.into(),
            source_span: item.source_span.map(Into::into),
            semantic_key: item.semantic_key.as_deref().map(str::to_owned),
        }
    }

    fn into_core(
        self,
        geometry_handles: &HashMap<TransportGeometryResourceHandle, GeometryResourceHandle>,
    ) -> Result<TextVectorItem, RetainedResourceTransportError> {
        let geometry = geometry_handles.get(&self.geometry).copied().ok_or(
            RetainedResourceTransportError::MissingGeometry(self.geometry),
        )?;
        Ok(TextVectorItem {
            geometry,
            transform: self.transform.into(),
            style: self.style.into(),
            source_span: self.source_span.map(Into::into),
            semantic_key: self.semantic_key.map(Arc::from),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct TransportTextVectorStyle {
    fill: Option<Color>,
    stroke: Option<Color>,
    stroke_width: f32,
    stroke_cap: StrokeCap,
    stroke_join: StrokeJoin,
}

impl From<TextVectorStyle> for TransportTextVectorStyle {
    fn from(value: TextVectorStyle) -> Self {
        Self {
            fill: value.fill,
            stroke: value.stroke,
            stroke_width: value.stroke_width,
            stroke_cap: value.stroke_cap,
            stroke_join: value.stroke_join,
        }
    }
}

impl From<TransportTextVectorStyle> for TextVectorStyle {
    fn from(value: TransportTextVectorStyle) -> Self {
        Self {
            fill: value.fill,
            stroke: value.stroke,
            stroke_width: value.stroke_width,
            stroke_cap: value.stroke_cap,
            stroke_join: value.stroke_join,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransportTextRenderItem {
    GlyphRun(u32),
    Vector(u32),
}

impl From<TextRenderItem> for TransportTextRenderItem {
    fn from(value: TextRenderItem) -> Self {
        match value {
            TextRenderItem::GlyphRun(index) => Self::GlyphRun(index),
            TextRenderItem::Vector(index) => Self::Vector(index),
        }
    }
}

impl From<TransportTextRenderItem> for TextRenderItem {
    fn from(value: TransportTextRenderItem) -> Self {
        match value {
            TransportTextRenderItem::GlyphRun(index) => Self::GlyphRun(index),
            TransportTextRenderItem::Vector(index) => Self::Vector(index),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportTextPart {
    source_span: TransportTextSourceSpan,
    first_cluster: u32,
    cluster_count: u32,
    first_vector: u32,
    vector_count: u32,
    semantic_key: Option<String>,
}

impl TransportTextPart {
    fn from_core(part: &TextPart) -> Self {
        Self {
            source_span: part.source_span.into(),
            first_cluster: part.first_cluster,
            cluster_count: part.cluster_count,
            first_vector: part.first_vector,
            vector_count: part.vector_count,
            semantic_key: part.semantic_key.as_deref().map(str::to_owned),
        }
    }

    fn into_core(self) -> TextPart {
        TextPart {
            source_span: self.source_span.into(),
            first_cluster: self.first_cluster,
            cluster_count: self.cluster_count,
            first_vector: self.first_vector,
            vector_count: self.vector_count,
            semantic_key: self.semantic_key.map(Arc::from),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportTextLayoutArtifact {
    backend: TransportTextLayoutBackend,
    template_fingerprint: String,
    artifact_fingerprint: String,
    backend_payload_key: Option<String>,
}

impl TransportTextLayoutArtifact {
    fn from_core(artifact: &TextLayoutArtifact) -> Self {
        Self {
            backend: TransportTextLayoutBackend::from_core(&artifact.backend),
            template_fingerprint: artifact.template_fingerprint.to_string(),
            artifact_fingerprint: artifact.artifact_fingerprint.to_string(),
            backend_payload_key: artifact.backend_payload_key.as_deref().map(str::to_owned),
        }
    }

    fn into_core(self) -> TextLayoutArtifact {
        TextLayoutArtifact {
            backend: self.backend.into_core(),
            template_fingerprint: Arc::from(self.template_fingerprint),
            artifact_fingerprint: Arc::from(self.artifact_fingerprint),
            backend_payload_key: self.backend_payload_key.map(Arc::from),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct TransportTextLayoutBackend {
    kind: TransportTextLayoutBackendKind,
    version: String,
}

impl TransportTextLayoutBackend {
    fn from_core(backend: &TextLayoutBackend) -> Self {
        Self {
            kind: TransportTextLayoutBackendKind::from_core(&backend.kind),
            version: backend.version.to_string(),
        }
    }

    fn into_core(self) -> TextLayoutBackend {
        TextLayoutBackend {
            kind: self.kind.into_core(),
            version: Arc::from(self.version),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum TransportTextLayoutBackendKind {
    NativeText,
    Typst,
    Latex,
    Other(String),
}

impl TransportTextLayoutBackendKind {
    fn from_core(kind: &TextLayoutBackendKind) -> Self {
        match kind {
            TextLayoutBackendKind::NativeText => Self::NativeText,
            TextLayoutBackendKind::Typst => Self::Typst,
            TextLayoutBackendKind::Latex => Self::Latex,
            TextLayoutBackendKind::Other(name) => Self::Other(name.to_string()),
        }
    }

    fn into_core(self) -> TextLayoutBackendKind {
        match self {
            Self::NativeText => TextLayoutBackendKind::NativeText,
            Self::Typst => TextLayoutBackendKind::Typst,
            Self::Latex => TextLayoutBackendKind::Latex,
            Self::Other(name) => TextLayoutBackendKind::Other(Arc::from(name)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon::{MathTypst, Scene, Typst};
    use noon_core::TextSourceKind;

    fn text_handles(objects: &[noon::Mobject]) -> Vec<TextResourceHandle> {
        objects
            .iter()
            .map(|object| object.state().unwrap().content.text().unwrap())
            .collect()
    }

    #[test]
    fn typst_resource_bundle_round_trips_without_python_or_placeholder_geometry() {
        let mut scene = Scene::new();
        let objects = vec![
            scene.typst(Typst::new("*Hello* from _Typst!_")).unwrap(),
            scene
                .math_typst(MathTypst::new("frac(x, 2)").with_font_size(72.0))
                .unwrap(),
            scene
                .typst(Typst::new(
                    "#line(length: 20pt, stroke: (paint: red, thickness: 2pt, cap: \"round\", join: \"bevel\"))",
                ))
                .unwrap(),
        ];
        let original_handles = text_handles(&objects);
        let source = scene.integration_store().borrow();
        let bundle = RetainedResourceBundle::capture(
            original_handles.iter().copied(),
            source.text_resources(),
            source.geometry_resources(),
            source.font_resources(),
        )
        .unwrap();
        assert_eq!(bundle.text_count(), 3);
        assert!(bundle.geometry_count() >= 1);
        assert!(bundle.font_count() >= 1);
        assert!(bundle.font_bytes() > 0);

        let bytes = bundle.encode_binary().unwrap();
        let decoded = RetainedResourceBundle::decode_binary(&bytes).unwrap();
        let installed = decoded.install().unwrap();

        for original in original_handles {
            let transport = TransportTextResourceHandle::from_source_handle(original);
            let local = installed.resolve_text_handle(transport).unwrap();
            assert_ne!(original.arena, local.arena);
            assert!(installed.texts().get(original).is_none());
            let resource = installed.texts().get(local).unwrap();
            let original_resource = source.text_resources().get(original).unwrap();
            assert_eq!(
                resource.vector_items.len(),
                original_resource.vector_items.len()
            );
            assert!(matches!(
                resource.kind,
                TextSourceKind::Typst | TextSourceKind::MathTypst
            ));
            for run in resource.runs.iter() {
                assert!(installed.fonts().get_for_face(&run.font).is_some());
            }
            for (vector, original_vector) in resource
                .vector_items
                .iter()
                .zip(original_resource.vector_items.iter())
            {
                assert!(installed.geometries().get(vector.geometry).is_some());
                assert_eq!(vector.style, original_vector.style);
            }
        }
    }

    #[test]
    fn capture_is_dependency_closed_and_deduplicates_shared_font_bytes() {
        let mut scene = Scene::new();
        let objects = vec![
            scene.typst(Typst::new("A")).unwrap(),
            scene.typst(Typst::new("B")).unwrap(),
        ];
        let handles = text_handles(&objects);
        let source = scene.integration_store().borrow();

        let bundle = RetainedResourceBundle::capture(
            handles.iter().copied(),
            source.text_resources(),
            source.geometry_resources(),
            source.font_resources(),
        )
        .unwrap();

        assert_eq!(bundle.text_count(), 2);
        assert_eq!(bundle.font_count(), 1);
    }

    #[test]
    fn sparse_addition_reuses_installed_dependencies_and_preserves_handles() {
        let mut scene = Scene::new();
        let objects = vec![
            scene.typst(Typst::new("A")).unwrap(),
            scene.typst(Typst::new("B")).unwrap(),
        ];
        let handles = text_handles(&objects);
        let source = scene.integration_store().borrow();
        let base = RetainedResourceBundle::capture(
            [handles[0]],
            source.text_resources(),
            source.geometry_resources(),
            source.font_resources(),
        )
        .unwrap();
        let mut inventory = base.inventory();
        let first_transport = TransportTextResourceHandle::from_source_handle(handles[0]);
        let second_transport = TransportTextResourceHandle::from_source_handle(handles[1]);
        let mut installed = base.install().unwrap();
        let first_local = installed.resolve_text_handle(first_transport).unwrap();
        let mut addition = RetainedResourceBundle::capture_additions(
            handles.iter().copied(),
            source.text_resources(),
            source.geometry_resources(),
            source.font_resources(),
            &inventory,
        )
        .unwrap();
        assert_eq!(addition.text_count(), 1);
        assert_eq!(addition.font_count(), 0);
        addition.retain_additions(&mut inventory);

        let prepared = installed.prepare_additions(addition).unwrap();
        let second_local = prepared.text_handle_remap()[&second_transport];
        installed.commit_additions(prepared);

        assert_eq!(
            installed.resolve_text_handle(first_transport),
            Some(first_local)
        );
        assert_eq!(
            installed.resolve_text_handle(second_transport),
            Some(second_local)
        );
        assert!(TextResourceLookup::get(&installed, first_local).is_some());
        assert!(TextResourceLookup::get(&installed, second_local).is_some());

        let repeated = RetainedResourceBundle::capture_additions(
            handles,
            source.text_resources(),
            source.geometry_resources(),
            source.font_resources(),
            &inventory,
        )
        .unwrap();
        assert!(
            repeated.is_empty(),
            "installed resources must not be resent"
        );
    }

    #[test]
    fn versioned_text_additions_replace_one_installed_slot_and_lookup_entry() {
        fn resource(source: &str) -> TextResource {
            TextResource {
                source: Arc::from(source),
                kind: TextSourceKind::MathTex,
                runs: Arc::from([]),
                vector_items: Arc::from([]),
                render_items: Arc::from([]),
                parts: Arc::from([]),
                bounds: Rect::new(Vec2::ZERO, Vec2::ONE),
                baseline: 0.0,
                layout_artifact: None,
            }
        }

        let mut source = TextResourceArena::new();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let first = source.insert(resource("1")).unwrap();
        let base = RetainedResourceBundle::capture([first], &source, &geometries, &fonts).unwrap();
        let mut inventory = base.inventory();
        let mut installed = base.install().unwrap();
        let first_transport = TransportTextResourceHandle::from_source_handle(first);
        let first_local = installed.resolve_text_handle(first_transport).unwrap();

        for value in 2..=32 {
            let next = source
                .replace(first.id, resource(&value.to_string()))
                .unwrap();
            let next_transport = TransportTextResourceHandle::from_source_handle(next);
            let mut addition = RetainedResourceBundle::capture_additions(
                [next],
                &source,
                &geometries,
                &fonts,
                &inventory,
            )
            .unwrap();
            addition.retain_additions(&mut inventory);
            let prepared = installed.prepare_additions(addition).unwrap();
            assert_eq!(prepared.superseded_text_handles().len(), 1);
            assert_eq!(prepared.staged_texts.len(), 1);
            let local = prepared.text_handle_remap()[&next_transport];
            installed.commit_additions(prepared);

            assert_eq!(local.id, first_local.id);
            assert_eq!(local.version, (value - 1) as u64);
            assert_eq!(installed.resolve_text_handle(next_transport), Some(local));
            assert!(installed.resolve_text_handle(first_transport).is_none());
            assert_eq!(
                installed.texts.get(local).unwrap().source.as_ref(),
                value.to_string()
            );
            assert_eq!(installed.texts.len(), 1);
            assert_eq!(installed.texts.slot_capacity(), 1);
            assert!(installed.additions.is_empty());
            assert_eq!(installed.inventory.texts.len(), 1);
            assert_eq!(installed.text_handles.len(), 1);
        }
    }

    #[test]
    fn versioned_addition_text_reuses_its_original_layer_slot() {
        fn resource(source: &str) -> TextResource {
            TextResource {
                source: Arc::from(source),
                kind: TextSourceKind::MathTex,
                runs: Arc::from([]),
                vector_items: Arc::from([]),
                render_items: Arc::from([]),
                parts: Arc::from([]),
                bounds: Rect::new(Vec2::ZERO, Vec2::ONE),
                baseline: 0.0,
                layout_artifact: None,
            }
        }

        let mut source = TextResourceArena::new();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let base = RetainedResourceBundle::capture([], &source, &geometries, &fonts).unwrap();
        let mut inventory = base.inventory();
        let mut installed = base.install().unwrap();
        let first = source.insert(resource("1")).unwrap();
        let first_transport = TransportTextResourceHandle::from_source_handle(first);
        let mut addition = RetainedResourceBundle::capture_additions(
            [first],
            &source,
            &geometries,
            &fonts,
            &inventory,
        )
        .unwrap();
        addition.retain_additions(&mut inventory);
        let prepared = installed.prepare_additions(addition).unwrap();
        let first_local = prepared.text_handle_remap()[&first_transport];
        installed.commit_additions(prepared);
        assert!(installed.texts.is_empty());
        assert_eq!(installed.additions.len(), 1);
        assert_eq!(installed.additions[0].texts.slot_capacity(), 1);

        for value in 2..=32 {
            let next = source
                .replace(first.id, resource(&value.to_string()))
                .unwrap();
            let next_transport = TransportTextResourceHandle::from_source_handle(next);
            let mut addition = RetainedResourceBundle::capture_additions(
                [next],
                &source,
                &geometries,
                &fonts,
                &inventory,
            )
            .unwrap();
            addition.retain_additions(&mut inventory);
            let prepared = installed.prepare_additions(addition).unwrap();
            assert_eq!(prepared.staged_texts.len(), 1);
            let local = prepared.text_handle_remap()[&next_transport];
            installed.commit_additions(prepared);

            assert_eq!(local.id, first_local.id);
            assert_eq!(local.version, (value - 1) as u64);
            assert_eq!(installed.resolve_text_handle(next_transport), Some(local));
            assert!(installed.resolve_text_handle(first_transport).is_none());
            assert_eq!(installed.additions.len(), 1);
            assert_eq!(installed.additions[0].texts.len(), 1);
            assert_eq!(installed.additions[0].texts.slot_capacity(), 1);
            assert_eq!(installed.text_handles.len(), 1);
            assert_eq!(installed.inventory.texts.len(), 1);
            assert_eq!(
                installed.get_text(local).unwrap().source.as_ref(),
                value.to_string()
            );
        }
    }

    #[test]
    fn sparse_text_staging_is_local_and_failed_batches_leave_large_tables_unchanged() {
        const UNRELATED_TEXTS: usize = 512;

        fn resource(source: String) -> TextResource {
            TextResource {
                source: Arc::from(source),
                kind: TextSourceKind::MathTex,
                runs: Arc::from([]),
                vector_items: Arc::from([]),
                render_items: Arc::from([]),
                parts: Arc::from([]),
                bounds: Rect::new(Vec2::ZERO, Vec2::ONE),
                baseline: 0.0,
                layout_artifact: None,
            }
        }

        let mut source = TextResourceArena::new();
        let geometries = GeometryResourceArena::new();
        let fonts = FontResourceArena::new();
        let handles = (0..UNRELATED_TEXTS)
            .map(|index| source.insert(resource(format!("seed-{index}"))).unwrap())
            .collect::<Vec<_>>();
        let base =
            RetainedResourceBundle::capture(handles.iter().copied(), &source, &geometries, &fonts)
                .unwrap();
        let mut inventory = base.inventory();
        let installed = base.install().unwrap();
        let original = handles[0];
        let original_transport = TransportTextResourceHandle::from_source_handle(original);
        let original_local = installed.resolve_text_handle(original_transport).unwrap();
        let slot_capacity = installed.texts.slot_capacity();
        let stats = installed.texts.stats();

        let first = source
            .replace(original.id, resource("next-0".to_owned()))
            .unwrap();
        let first_transport = TransportTextResourceHandle::from_source_handle(first);
        let mut first_addition = RetainedResourceBundle::capture_additions(
            [first],
            &source,
            &geometries,
            &fonts,
            &inventory,
        )
        .unwrap();
        let mut first_inventory = inventory.clone();
        first_addition.retain_additions(&mut first_inventory);
        let prepared = installed.prepare_additions(first_addition).unwrap();
        assert_eq!(prepared.staged_texts.len(), 1);
        assert_eq!(
            prepared
                .text_lookup(&installed)
                .get(prepared.text_handle_remap()[&first_transport])
                .unwrap()
                .source
                .as_ref(),
            "next-0"
        );
        assert_eq!(installed.texts.slot_capacity(), slot_capacity);
        assert_eq!(installed.texts.stats(), stats);
        drop(prepared);

        let second = source
            .replace(handles[1].id, resource("next-1".to_owned()))
            .unwrap();
        let mut invalid_addition = RetainedResourceBundle::capture_additions(
            [first, second],
            &source,
            &geometries,
            &fonts,
            &inventory,
        )
        .unwrap();
        invalid_addition.retain_additions(&mut inventory);
        assert_eq!(invalid_addition.texts.len(), 2);
        invalid_addition.texts[1]
            .resource
            .render_items
            .push(TransportTextRenderItem::GlyphRun(0));

        assert!(matches!(
            installed.prepare_additions(invalid_addition),
            Err(RetainedResourceTransportError::InvalidText(_))
        ));
        assert_eq!(
            installed.resolve_text_handle(original_transport),
            Some(original_local)
        );
        assert_eq!(
            installed.texts.get(original_local).unwrap().source.as_ref(),
            "seed-0"
        );
        assert_eq!(installed.texts.slot_capacity(), slot_capacity);
        assert_eq!(installed.texts.stats(), stats);
        for (index, handle) in handles.iter().copied().enumerate() {
            let transport = TransportTextResourceHandle::from_source_handle(handle);
            let local = installed.resolve_text_handle(transport).unwrap();
            assert_eq!(
                installed.texts.get(local).unwrap().source.as_ref(),
                format!("seed-{index}")
            );
        }
    }

    #[test]
    fn protocol_rejects_wrong_channel_before_installing_resources() {
        let bundle = RetainedResourceBundle {
            channel: "noon.execution".to_owned(),
            protocol_version: RETAINED_RESOURCE_TRANSPORT_VERSION,
            images: Vec::new(),
            texts: Vec::new(),
            geometries: Vec::new(),
            fonts: Vec::new(),
            render_geometry_resources: None,
        };
        assert!(matches!(
            bundle.install(),
            Err(RetainedResourceTransportError::InvalidChannel(_))
        ));
    }

    #[test]
    fn compiled_morph_bundle_installs_geometry_once_and_reuses_local_arcs() {
        let geometry = Arc::new(GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::ZERO)
                .line_to(Vec2::new(1.0, 0.0)),
        ));
        let mut bundle = RetainedResourceBundle::capture(
            [],
            &TextResourceArena::new(),
            &GeometryResourceArena::new(),
            &FontResourceArena::new(),
        )
        .unwrap();
        let preparations = vec![
            RenderGeometryPreparation {
                resource: 0,
                style: Style::default(),
                transform: Transform2D::IDENTITY,
            },
            RenderGeometryPreparation {
                resource: 0,
                style: Style {
                    stroke_width: 3.0,
                    ..Style::default()
                },
                transform: Transform2D::IDENTITY,
            },
        ];
        bundle.set_render_geometries(17, vec![geometry.clone()].into(), preparations.clone());
        let installed = RetainedResourceBundle::decode_binary(&bundle.encode_binary().unwrap())
            .unwrap()
            .install()
            .unwrap();
        assert_eq!(installed.render_geometry_session(), Some(17));
        assert_eq!(
            installed
                .render_geometry_preparations()
                .cloned()
                .collect::<Vec<_>>(),
            preparations
        );
        assert_eq!(installed.render_geometry_preparation_count(), 2);
        for invalid in [
            RenderGeometryPreparation {
                resource: 1,
                ..preparations[0].clone()
            },
            RenderGeometryPreparation {
                style: Style {
                    stroke_width: f32::NAN,
                    ..Style::default()
                },
                ..preparations[0].clone()
            },
            RenderGeometryPreparation {
                transform: Transform2D {
                    rotation: f32::INFINITY,
                    ..Transform2D::IDENTITY
                },
                ..preparations[0].clone()
            },
        ] {
            let mut invalid_bundle = bundle.clone();
            invalid_bundle
                .render_geometry_resources
                .as_mut()
                .unwrap()
                .preparations
                .push(invalid);
            let decoded =
                RetainedResourceBundle::decode_binary(&invalid_bundle.encode_binary().unwrap())
                    .unwrap();
            assert!(matches!(
                decoded.install(),
                Err(RetainedResourceTransportError::InvalidRenderPreparation(2))
            ));
        }
        let first = installed.render_geometries();
        let again = installed.render_geometries();
        assert_eq!(first[0].geometry.as_ref(), Some(&geometry));
        assert!(Arc::ptr_eq(
            first[0].geometry.as_ref().unwrap(),
            again[0].geometry.as_ref().unwrap()
        ));
        assert!(
            !Arc::ptr_eq(first[0].geometry.as_ref().unwrap(), &geometry),
            "cross-worker decode owns its local resource allocation"
        );
    }

    #[test]
    fn nonfinite_compiled_resource_is_rejected_before_installation() {
        let mut bundle = RetainedResourceBundle::capture(
            [],
            &TextResourceArena::new(),
            &GeometryResourceArena::new(),
            &FontResourceArena::new(),
        )
        .unwrap();
        bundle.set_render_geometries(
            17,
            vec![Arc::new(GeometryRef::path(
                VectorPath::new().move_to(Vec2::new(f32::NAN, 0.0)),
            ))]
            .into(),
            Vec::new(),
        );
        let decoded =
            RetainedResourceBundle::decode_binary(&bundle.encode_binary().unwrap()).unwrap();
        assert!(matches!(
            decoded.install(),
            Err(RetainedResourceTransportError::InvalidRenderGeometry(0))
        ));
    }
}

#[cfg(test)]
mod morph_tests;

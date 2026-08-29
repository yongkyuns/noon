use noon::{
    polygram_vertex_groups, IntoSnapshot, Polygon, Polygram, RegularPolygon, RegularPolygram, Star,
    Triangle,
};
use noon_core::{ObjectSnapshot, Vec2};

fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() || value.abs() > f64::from(f32::MAX) {
        return Err(format!("{name} must be a finite f32-compatible number"));
    }
    Ok(value as f32)
}

fn positive_f32(name: &str, value: f64) -> Result<f32, String> {
    let value = finite_f32(name, value)?;
    if value <= 0.0 {
        return Err(format!("{name} must be positive"));
    }
    Ok(value)
}

fn vertices_from_flat(name: &str, coordinates: &[f64]) -> Result<Vec<Vec2>, String> {
    if !coordinates.len().is_multiple_of(2) {
        return Err(format!("{name} must contain x/y coordinate pairs"));
    }
    coordinates
        .as_chunks::<2>()
        .0
        .iter()
        .enumerate()
        .map(|(index, pair)| {
            Ok(Vec2::new(
                finite_f32(&format!("{name}[{index}].x"), pair[0])?,
                finite_f32(&format!("{name}[{index}].y"), pair[1])?,
            ))
        })
        .collect()
}

fn vertex_groups_from_flat(
    coordinates: &[f64],
    group_lengths: &[u32],
) -> Result<Vec<Vec<Vec2>>, String> {
    let vertices = vertices_from_flat("vertices", coordinates)?;
    let expected = group_lengths.iter().try_fold(0usize, |total, length| {
        total
            .checked_add(*length as usize)
            .ok_or_else(|| "polygram vertex count overflow".to_owned())
    })?;
    if expected != vertices.len() {
        return Err(format!(
            "polygram group lengths describe {expected} vertices but {} were supplied",
            vertices.len()
        ));
    }

    let mut groups = Vec::with_capacity(group_lengths.len());
    let mut start = 0usize;
    for (group_index, length) in group_lengths.iter().copied().enumerate() {
        let length = length as usize;
        if length == 0 {
            return Err(format!(
                "polygram vertex group {group_index} must contain at least one vertex"
            ));
        }
        let end = start + length;
        groups.push(vertices[start..end].to_vec());
        start = end;
    }
    Ok(groups)
}

fn optional_angle(name: &str, value: Option<f64>) -> Result<Option<f32>, String> {
    value.map(|value| finite_f32(name, value)).transpose()
}

pub(crate) fn polygon_snapshot(coordinates: &[f64]) -> Result<ObjectSnapshot, String> {
    Ok(Polygon::new(vertices_from_flat("vertices", coordinates)?).into_snapshot())
}

pub(crate) fn polygram_snapshot(
    coordinates: &[f64],
    group_lengths: &[u32],
) -> Result<ObjectSnapshot, String> {
    Polygram::new(vertex_groups_from_flat(coordinates, group_lengths)?)
        .map(IntoSnapshot::into_snapshot)
        .map_err(|error| error.to_string())
}

pub(crate) fn regular_polygon_snapshot(
    num_vertices: u32,
    radius: f64,
    start_angle: Option<f64>,
) -> Result<ObjectSnapshot, String> {
    RegularPolygon::with_options(
        num_vertices as usize,
        positive_f32("radius", radius)?,
        optional_angle("start_angle", start_angle)?,
    )
    .map(IntoSnapshot::into_snapshot)
    .map_err(|error| error.to_string())
}

pub(crate) fn regular_polygram_snapshot(
    num_vertices: u32,
    density: u32,
    radius: f64,
    start_angle: Option<f64>,
) -> Result<ObjectSnapshot, String> {
    RegularPolygram::with_options(
        num_vertices as usize,
        density as usize,
        positive_f32("radius", radius)?,
        optional_angle("start_angle", start_angle)?,
    )
    .map(IntoSnapshot::into_snapshot)
    .map_err(|error| error.to_string())
}

pub(crate) fn star_snapshot(
    points: u32,
    outer_radius: f64,
    inner_radius: Option<f64>,
    density: u32,
    start_angle: Option<f64>,
) -> Result<ObjectSnapshot, String> {
    Star::with_options(
        points as usize,
        positive_f32("outer_radius", outer_radius)?,
        inner_radius
            .map(|value| positive_f32("inner_radius", value))
            .transpose()?,
        density as usize,
        optional_angle("start_angle", start_angle)?,
    )
    .map(IntoSnapshot::into_snapshot)
    .map_err(|error| error.to_string())
}

pub(crate) fn triangle_snapshot() -> ObjectSnapshot {
    Triangle::new().into_snapshot()
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct EncodedVertexGroups {
    coordinates: Vec<f64>,
    group_lengths: Vec<u32>,
}

pub(crate) fn encode_vertex_groups(
    snapshot: &ObjectSnapshot,
) -> Result<EncodedVertexGroups, String> {
    let groups = polygram_vertex_groups(snapshot);
    let mut coordinates = Vec::new();
    let mut group_lengths = Vec::with_capacity(groups.len());
    for group in groups {
        group_lengths.push(
            u32::try_from(group.len())
                .map_err(|_| "polygram vertex group exceeds JS index range".to_owned())?,
        );
        coordinates.reserve(group.len() * 2);
        for vertex in group {
            coordinates.push(f64::from(vertex.x));
            coordinates.push(f64::from(vertex.y));
        }
    }
    Ok(EncodedVertexGroups {
        coordinates,
        group_lengths,
    })
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use crate::{WasmAuthoringMobjectHandle, WasmAuthoringStore};

    use super::{
        encode_vertex_groups, polygon_snapshot, polygram_snapshot, regular_polygon_snapshot,
        regular_polygram_snapshot, star_snapshot, triangle_snapshot, EncodedVertexGroups,
    };

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    fn attach_snapshot(
        store: &WasmAuthoringStore,
        snapshot: noon_core::ObjectSnapshot,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        // The public host boundary remains typed. This serialization is contained
        // inside Rust only because the established WASM store intentionally keeps
        // its semantic arena private; no snapshot crosses Python/JS for construction.
        let json = serde_json::to_string(&snapshot).map_err(|error| js_error(error.to_string()))?;
        store.create_mobject(&json)
    }

    fn snapshot_from_handle(
        handle: &WasmAuthoringMobjectHandle,
    ) -> Result<noon_core::ObjectSnapshot, JsValue> {
        // Likewise, query results cross the host boundary as typed numeric arrays.
        // Geometry/path traversal and transforms remain owned by shared Rust.
        let json = handle.snapshot_json()?;
        serde_json::from_str(&json).map_err(|error| js_error(error.to_string()))
    }

    #[wasm_bindgen]
    pub struct WasmPolygramVertexGroups(EncodedVertexGroups);

    impl WasmPolygramVertexGroups {
        pub(crate) fn from_snapshot(snapshot: &noon_core::ObjectSnapshot) -> Result<Self, String> {
            encode_vertex_groups(snapshot).map(Self)
        }
    }

    #[wasm_bindgen]
    impl WasmPolygramVertexGroups {
        #[wasm_bindgen(js_name = coordinates)]
        pub fn coordinates(&self) -> Box<[f64]> {
            self.0.coordinates.clone().into_boxed_slice()
        }

        #[wasm_bindgen(js_name = groupLengths)]
        pub fn group_lengths(&self) -> Box<[u32]> {
            self.0.group_lengths.clone().into_boxed_slice()
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringStore {
        #[wasm_bindgen(js_name = createManimPolygon)]
        pub fn create_manim_polygon(
            &self,
            coordinates: &[f64],
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            attach_snapshot(self, polygon_snapshot(coordinates).map_err(js_error)?)
        }

        #[wasm_bindgen(js_name = createManimPolygram)]
        pub fn create_manim_polygram(
            &self,
            coordinates: &[f64],
            group_lengths: &[u32],
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            attach_snapshot(
                self,
                polygram_snapshot(coordinates, group_lengths).map_err(js_error)?,
            )
        }

        #[wasm_bindgen(js_name = createManimRegularPolygon)]
        pub fn create_manim_regular_polygon(
            &self,
            num_vertices: u32,
            radius: f64,
            start_angle: Option<f64>,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            attach_snapshot(
                self,
                regular_polygon_snapshot(num_vertices, radius, start_angle).map_err(js_error)?,
            )
        }

        #[wasm_bindgen(js_name = createManimRegularPolygram)]
        pub fn create_manim_regular_polygram(
            &self,
            num_vertices: u32,
            density: u32,
            radius: f64,
            start_angle: Option<f64>,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            attach_snapshot(
                self,
                regular_polygram_snapshot(num_vertices, density, radius, start_angle)
                    .map_err(js_error)?,
            )
        }

        #[wasm_bindgen(js_name = createManimTriangle)]
        pub fn create_manim_triangle(&self) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            attach_snapshot(self, triangle_snapshot())
        }

        #[wasm_bindgen(js_name = createManimStar)]
        pub fn create_manim_star(
            &self,
            points: u32,
            outer_radius: f64,
            inner_radius: Option<f64>,
            density: u32,
            start_angle: Option<f64>,
        ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
            attach_snapshot(
                self,
                star_snapshot(points, outer_radius, inner_radius, density, start_angle)
                    .map_err(js_error)?,
            )
        }
    }

    #[wasm_bindgen]
    impl WasmAuthoringMobjectHandle {
        #[wasm_bindgen(js_name = manimVertexGroups)]
        pub fn manim_vertex_groups(&self) -> Result<WasmPolygramVertexGroups, JsValue> {
            let snapshot = snapshot_from_handle(self)?;
            WasmPolygramVertexGroups::from_snapshot(&snapshot).map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use noon_core::GeometryRef;

    use super::*;

    #[test]
    fn polygon_bridge_uses_shared_constructor_and_query_order() {
        let snapshot = polygon_snapshot(&[-1.0, -1.0, 2.0, -1.0, 0.0, 3.0]).unwrap();
        let encoded = encode_vertex_groups(&snapshot).unwrap();
        assert_eq!(encoded.group_lengths, vec![3]);
        assert_eq!(encoded.coordinates, vec![-1.0, -1.0, 2.0, -1.0, 0.0, 3.0]);
        assert!(matches!(snapshot.geometry, GeometryRef::VectorPath(_)));
    }

    #[test]
    fn polygram_bridge_preserves_disconnected_groups() {
        let snapshot = polygram_snapshot(
            &[
                0.0, 2.0, -1.0, -1.0, 1.0, -1.0, 0.0, -2.0, -1.0, 1.0, 1.0, 1.0,
            ],
            &[3, 3],
        )
        .unwrap();
        let encoded = encode_vertex_groups(&snapshot).unwrap();
        assert_eq!(encoded.group_lengths, vec![3, 3]);
        assert_eq!(encoded.coordinates.len(), 12);
    }

    #[test]
    fn regular_polygon_default_even_orientation_matches_zero_angle() {
        let default = regular_polygon_snapshot(4, 1.0, None).unwrap();
        let zero_angle = regular_polygon_snapshot(4, 1.0, Some(0.0)).unwrap();
        let quarter_turn =
            regular_polygon_snapshot(4, 1.0, Some(f64::from(FRAC_PI_2))).unwrap();

        let default_vertices = encode_vertex_groups(&default).unwrap().coordinates;
        let zero_vertices = encode_vertex_groups(&zero_angle).unwrap().coordinates;
        let quarter_vertices = encode_vertex_groups(&quarter_turn).unwrap().coordinates;

        assert_eq!(default_vertices, zero_vertices);
        assert_ne!(default_vertices, quarter_vertices);
        assert!((default_vertices[0] - 1.0).abs() < 1e-6);
        assert!(default_vertices[1].abs() < 1e-6);
        assert!(quarter_vertices[0].abs() < 1e-6);
        assert!((quarter_vertices[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn regular_polygram_and_star_delegate_density_validation() {
        assert!(regular_polygram_snapshot(6, 2, 1.0, None).is_ok());
        assert!(regular_polygram_snapshot(5, 0, 1.0, None).is_err());
        assert!(star_snapshot(7, 2.0, None, 3, Some(f64::from(FRAC_PI_2))).is_ok());
        assert!(star_snapshot(5, 1.0, None, 3, None).is_err());
    }

    #[test]
    fn triangle_bridge_uses_shared_triangle_constructor() {
        let snapshot = triangle_snapshot();
        let encoded = encode_vertex_groups(&snapshot).unwrap();
        assert_eq!(encoded.group_lengths, vec![3]);
        assert_eq!(encoded.coordinates.len(), 6);
    }

    #[test]
    fn malformed_flat_inputs_are_rejected_before_shared_construction() {
        assert!(polygon_snapshot(&[0.0, 1.0, 2.0]).is_err());
        assert!(polygram_snapshot(&[0.0, 0.0, 1.0, 0.0], &[3]).is_err());
        assert!(polygram_snapshot(&[0.0, 0.0], &[0, 1]).is_err());
    }
}

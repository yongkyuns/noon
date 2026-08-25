#![no_main]

use libfuzzer_sys::fuzz_target;
use noon_core::{StrokeCap, StrokeJoin, Vec2, VectorPath};
use noon_geometry::tessellate_styled_with_fill;

const MAX_COMMANDS: usize = 64;
const MAX_VERTICES: usize = 1_000_000;

fn scalar(lo: u8, hi: u8) -> f32 {
    let raw = i16::from_le_bytes([lo, hi]);
    f32::from(raw) / 2048.0
}

fn point(bytes: &[u8]) -> Vec2 {
    Vec2::new(scalar(bytes[0], bytes[1]), scalar(bytes[2], bytes[3]))
}

fn path_from_bytes(data: &[u8]) -> VectorPath {
    let mut path = VectorPath::new();
    if data.len() < 4 {
        return path;
    }
    path = path.move_to(point(&data[..4]));
    let mut cursor = 4;
    let mut commands = 0;
    while cursor < data.len() && commands < MAX_COMMANDS {
        match data[cursor] % 5 {
            0 if cursor + 5 <= data.len() => {
                path = path.move_to(point(&data[cursor + 1..cursor + 5]));
                cursor += 5;
            }
            1 if cursor + 5 <= data.len() => {
                path = path.line_to(point(&data[cursor + 1..cursor + 5]));
                cursor += 5;
            }
            2 if cursor + 9 <= data.len() => {
                path = path.quadratic_to(
                    point(&data[cursor + 1..cursor + 5]),
                    point(&data[cursor + 5..cursor + 9]),
                );
                cursor += 9;
            }
            3 if cursor + 13 <= data.len() => {
                path = path.cubic_to(
                    point(&data[cursor + 1..cursor + 5]),
                    point(&data[cursor + 5..cursor + 9]),
                    point(&data[cursor + 9..cursor + 13]),
                );
                cursor += 13;
            }
            4 => {
                path = path.close();
                cursor += 1;
            }
            _ => break,
        }
        commands += 1;
    }
    path
}

fuzz_target!(|data: &[u8]| {
    if data.len() > 4096 {
        return;
    }
    let path = path_from_bytes(data);
    let width = data.first().map_or(1.0, |value| f32::from(*value) / 16.0);
    let join = match data.get(1).copied().unwrap_or_default() % 3 {
        0 => StrokeJoin::Round,
        1 => StrokeJoin::Bevel,
        _ => StrokeJoin::Miter,
    };
    let cap = match data.get(2).copied().unwrap_or_default() % 3 {
        0 => StrokeCap::Round,
        1 => StrokeCap::Butt,
        _ => StrokeCap::Square,
    };
    let fill = data.get(3).copied().unwrap_or_default() & 1 != 0;

    if let Ok(mesh) = tessellate_styled_with_fill(&path, width, join, cap, fill) {
        assert!(mesh.vertices.len() <= MAX_VERTICES, "bounded input produced an unreasonable mesh");
        assert!(mesh.indices.iter().all(|&index| (index as usize) < mesh.vertices.len()));
        assert!(mesh.vertices.iter().all(|vertex| {
            vertex.position.x.is_finite()
                && vertex.position.y.is_finite()
                && vertex.target_position.x.is_finite()
                && vertex.target_position.y.is_finite()
                && vertex.path_distance.is_finite()
                && vertex.path_progress.is_finite()
        }));
        assert!(mesh.stroke_length.is_finite());
    }
});

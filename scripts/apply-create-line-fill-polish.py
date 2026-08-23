from pathlib import Path


def replace_once(path, old, new):
    path = Path(path)
    text = path.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one match, found {count}: {old[:100]!r}")
    path.write_text(text.replace(old, new, 1))


# Keep Line Create on the analytic capsule pipeline and encode reveal in the
# otherwise-unused PackedTransform padding float. This avoids per-frame
# tessellation and gives the moving endpoint a round cap for the whole animation.
lib = "crates/noon-render-wgpu/src/lib.rs"
replace_once(
    lib,
    "                    let packed = pack_line(object);",
    "                    let packed = pack_line(object, frame.reveal(object_index));",
)
replace_once(
    lib,
    "                    self.lines.push(pack_line(object));",
    "                    self.lines\n                        .push(pack_line(object, frame.reveal(object_index)));",
)
replace_once(
    lib,
    '''fn pack_line(object: &FrameObjectState) -> LineInstance {
    let GeometryRef::Line { start, end } = &object.geometry else {
        unreachable!("line slot must retain line geometry")
    };
    LineInstance {
        transform: object.transform.into(),
        style: pack_style(object),
        start: [start.x, start.y],
        end: [end.x, end.y],
    }
}''',
    '''fn pack_line(object: &FrameObjectState, reveal: f32) -> LineInstance {
    let GeometryRef::Line { start, end } = &object.geometry else {
        unreachable!("line slot must retain line geometry")
    };
    let mut transform: PackedTransform = object.transform.into();
    transform.padding = reveal.clamp(0.0, 1.0);
    LineInstance {
        transform,
        style: pack_style(object),
        start: [start.x, start.y],
        end: [end.x, end.y],
    }
}''',
)
replace_once(
    lib,
    '''    #[test]
    fn every_analytic_shape_uses_path_pipeline_for_partial_reveal() {
        let mut circle = object(1, GeometryRef::circle(1.0));
        let mut rectangle = object(2, GeometryRef::rectangle(2.0, 1.0));
        let mut line = object(
            3,
            GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
        );
        for state in [&mut circle, &mut rectangle, &mut line] {
            state.style.fill = None;
            state.style.stroke = Some(Color::WHITE);
            state.style.stroke_width = 0.05;
        }
        let mut frame = frame(vec![circle, rectangle, line]);
        frame.reveals.fill(0.5);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);
        assert!(prepared.circles.is_empty());
        assert!(prepared.rectangles.is_empty());
        assert!(prepared.lines.is_empty());
        assert_eq!(prepared.paths.len(), 3);
        assert_eq!(prepared.stats.instance_count, 3);
        assert_eq!(prepared.stats.unsupported_count, 0);
    }''',
    '''    #[test]
    fn closed_analytic_create_uses_paths_while_line_reveal_stays_analytic() {
        let mut circle = object(1, GeometryRef::circle(1.0));
        let mut rectangle = object(2, GeometryRef::rectangle(2.0, 1.0));
        let mut line = object(
            3,
            GeometryRef::line(Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)),
        );
        for state in [&mut circle, &mut rectangle, &mut line] {
            state.style.fill = None;
            state.style.stroke = Some(Color::WHITE);
            state.style.stroke_width = 0.05;
        }
        let mut frame = frame(vec![circle, rectangle, line]);
        frame.reveals.fill(0.5);
        let mut preparer = FramePreparer::new();

        let prepared = preparer.prepare(&frame);
        assert!(prepared.circles.is_empty());
        assert!(prepared.rectangles.is_empty());
        assert_eq!(prepared.lines.len(), 1);
        assert_eq!(prepared.lines[0].transform.padding, 0.5);
        assert_eq!(prepared.paths.len(), 2);
        assert_eq!(prepared.stats.instance_count, 3);
        assert_eq!(prepared.stats.unsupported_count, 0);
        assert_eq!(prepared.stats.geometry_cache_misses, 2);

        frame.reveals[2] = 0.8;
        let advanced = preparer.prepare_incremental(&frame, &FrameChanges::objects(vec![2]));
        assert_eq!(advanced.lines.len(), 1);
        assert_eq!(advanced.lines[0].transform.padding, 0.8);
        assert_eq!(advanced.stats.geometry_cache_misses, 0);
        assert_eq!(advanced.stats.instances_repacked, 1);
        assert_eq!(advanced.line_dirty_ranges, &[0..1]);
        assert!(!advanced.path_geometry_dirty);
    }''',
)
replace_once(
    lib,
    '''        assert_eq!(instance.end, [3.0, -0.5]);
        assert_eq!(instance.style.stroke, [0.2, 0.8, 0.4, 1.0]);''',
    '''        assert_eq!(instance.end, [3.0, -0.5]);
        assert_eq!(instance.transform.padding, 1.0);
        assert_eq!(instance.style.stroke, [0.2, 0.8, 0.4, 1.0]);''',
)

# Line vertex data exposes PackedTransform::padding as a dedicated reveal value.
gpu = "crates/noon-render-wgpu/src/gpu.rs"
replace_once(
    gpu,
    "const LINE_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 9] = [",
    "const LINE_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 10] = [",
)
replace_once(
    gpu,
    '''    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 80,
        shader_location: 9,
    },
];''',
    '''    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 80,
        shader_location: 9,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32,
        offset: 20,
        shader_location: 10,
    },
];''',
)

# Analytic Line reveal shortens the capsule itself, preserving a round moving cap.
analytic = Path("crates/noon-render-wgpu/src/analytic.wgsl")
text = analytic.read_text()
text = text.replace(
    '''    @location(9) end: vec2<f32>,
};''',
    '''    @location(9) end: vec2<f32>,
    @location(10) reveal: f32,
};''',
    1,
)
text = text.replace(
    '''fn vs_line(input: LineVertexInput) -> VertexOutput {
    let delta = input.end - input.start;
    let segment_length = length(delta);''',
    '''fn vs_line(input: LineVertexInput) -> VertexOutput {
    let reveal = clamp(input.reveal, 0.0, 1.0);
    let revealed_end = mix(input.start, input.end, reveal);
    let delta = revealed_end - input.start;
    let segment_length = length(delta);''',
    1,
)
text = text.replace(
    '''    let width = max(input.metrics.x, 0.0);
    let half_width = width * 0.5;''',
    '''    let width = select(0.0, max(input.metrics.x, 0.0), reveal > 0.0);
    let half_width = width * 0.5;''',
    1,
)
text = text.replace(
    '''    let center = (input.start + input.end) * 0.5;''',
    '''    let center = (input.start + revealed_end) * 0.5;''',
    1,
)
analytic.write_text(text)

# Only closed analytic primitives need temporary path outlines. Lines can reveal
# exactly and more cheaply as analytic capsules.
Path("crates/noon-render-wgpu/src/reveal.rs").write_text('''use noon_core::{GeometryRef, VectorPath};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AnalyticRevealKey {
    Circle(u32),
    Rectangle(u32, u32),
}

pub(crate) fn analytic_reveal_key(geometry: &GeometryRef) -> Option<AnalyticRevealKey> {
    match geometry {
        GeometryRef::Circle { radius } => Some(AnalyticRevealKey::Circle(radius.to_bits())),
        GeometryRef::Rectangle { size } => Some(AnalyticRevealKey::Rectangle(
            size.x.to_bits(),
            size.y.to_bits(),
        )),
        GeometryRef::Line { .. } | GeometryRef::VectorPath(_) | GeometryRef::External(_) => None,
    }
}

pub(crate) fn temporary_reveal_path(
    geometry: &GeometryRef,
    reveal: f32,
) -> Option<(AnalyticRevealKey, VectorPath)> {
    if reveal >= 1.0 {
        return None;
    }
    let key = analytic_reveal_key(geometry)?;
    let path = noon_geometry::canonical_outline_path(geometry)?;
    Some((key, path))
}
''')

# Filled paths now fade their complete fill in smoothly while the border is
# revealed. A fill-only object still gets a temporary creation outline, which
# fades away near completion instead of popping off on the final frame.
Path("crates/noon-render-wgpu/src/path.wgsl").write_text('''struct Camera {
    center: vec2<f32>,
    clip_scale: vec2<f32>,
    viewport_size: vec2<f32>,
    padding: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> camera: Camera;

struct PathVertexInput {
    @location(0) local: vec2<f32>,
    @location(1) target_local: vec2<f32>,
    @location(2) surface_and_progress: u32,
    @location(3) translation: vec2<f32>,
    @location(4) scale: vec2<f32>,
    @location(5) rotation: f32,
    @location(6) fill: vec4<f32>,
    @location(7) stroke: vec4<f32>,
    @location(8) metrics: vec2<f32>,
    @location(9) flags: vec2<u32>,
    @location(10) path_params: vec2<f32>,
};

struct PathVertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) path_progress: f32,
    @location(2) reveal: f32,
    @location(3) is_stroke: f32,
};

fn premultiplied(color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(color.rgb * color.a, color.a);
}

@vertex
fn vs_path(input: PathVertexInput) -> PathVertexOutput {
    let is_stroke = (input.surface_and_progress & 1u) == 1u;
    let encoded_progress = input.surface_and_progress >> 1u;
    let path_progress = f32(encoded_progress) / 16777215.0;
    let morph = clamp(input.path_params.y, 0.0, 1.0);
    let reveal = clamp(input.path_params.x, 0.0, 1.0);
    let local = mix(input.local, input.target_local, morph);

    let c = cos(input.rotation);
    let s = sin(input.rotation);
    let scaled = local * input.scale;
    let world = vec2<f32>(
        c * scaled.x - s * scaled.y,
        s * scaled.x + c * scaled.y,
    ) + input.translation;

    var output: PathVertexOutput;
    output.position = vec4<f32>((world - camera.center) * camera.clip_scale, 0.0, 1.0);

    let fill_enabled = input.flags.x != 0u;
    let stroke_enabled = input.flags.y != 0u;
    let derive_creation_stroke = reveal < 1.0 && fill_enabled && !stroke_enabled;
    let authored_enabled = select(fill_enabled, stroke_enabled, is_stroke);
    let enabled = authored_enabled || (is_stroke && derive_creation_stroke);
    let authored_color = select(input.fill, input.stroke, is_stroke);
    let color = select(authored_color, input.fill, is_stroke && derive_creation_stroke);
    var creation_outline_alpha = 1.0;
    if is_stroke && derive_creation_stroke {
        creation_outline_alpha = 1.0 - smoothstep(0.75, 1.0, reveal);
    }
    output.color = select(
        vec4<f32>(0.0),
        premultiplied(color) * (input.metrics.y * creation_outline_alpha),
        enabled,
    );
    output.path_progress = path_progress;
    output.reveal = reveal;
    output.is_stroke = select(0.0, 1.0, is_stroke);
    return output;
}

@fragment
fn fs_path(input: PathVertexOutput) -> @location(0) vec4<f32> {
    // Fragment derivatives must execute in uniform control flow. `reveal` is an
    // interpolated input, so evaluate fwidth before any reveal-dependent branch.
    let edge = max(fwidth(input.path_progress), 0.00001);

    if input.reveal <= 0.0 {
        return vec4<f32>(0.0);
    }
    if input.reveal >= 1.0 {
        return input.color;
    }

    if input.is_stroke < 0.5 {
        // Manim-like Create polish: reveal the border while smoothly bringing in
        // the authored fill instead of popping the complete fill on the last frame.
        let fill_alpha = smoothstep(0.0, 1.0, input.reveal);
        return input.color * fill_alpha;
    }

    let coverage = 1.0 - smoothstep(input.reveal, input.reveal + edge, input.path_progress);
    return input.color * coverage;
}
''')

Path("crates/noon-render-wgpu/tests/path_shader_uniformity.rs").write_text('''#[test]
fn reveal_derivative_runs_before_reveal_control_flow() {
    let shader = include_str!("../src/path.wgsl");
    let derivative = shader
        .find("let edge = max(fwidth(input.path_progress)")
        .expect("path shader must evaluate a reveal derivative");
    let hidden_branch = shader
        .find("if input.reveal <= 0.0")
        .expect("path shader must handle a fully hidden reveal");
    let complete_branch = shader
        .find("if input.reveal >= 1.0")
        .expect("path shader must handle a complete reveal");

    assert!(
        derivative < hidden_branch && derivative < complete_branch,
        "fragment derivatives must execute before reveal-dependent control flow"
    );
}

#[test]
fn fill_only_partial_reveal_derives_a_visible_outline() {
    let shader = include_str!("../src/path.wgsl");
    assert!(shader.contains(
        "let derive_creation_stroke = reveal < 1.0 && fill_enabled && !stroke_enabled;"
    ));
    assert!(shader.contains(
        "let enabled = authored_enabled || (is_stroke && derive_creation_stroke);"
    ));
    assert!(shader.contains(
        "creation_outline_alpha = 1.0 - smoothstep(0.75, 1.0, reveal);"
    ));
}

#[test]
fn partial_reveal_smoothly_fades_fill_instead_of_waiting_for_completion() {
    let shader = include_str!("../src/path.wgsl");
    assert!(shader.contains("if input.is_stroke < 0.5"));
    assert!(shader.contains("let fill_alpha = smoothstep(0.0, 1.0, input.reveal);"));
    assert!(shader.contains("return input.color * fill_alpha;"));
}
''')

Path("web/python/examples/create_shapes.py").write_text('''from noon import (
    BLUE,
    DOWN,
    LEFT,
    PINK,
    RIGHT,
    UP,
    WHITE,
    Circle,
    Create,
    Line,
    Path,
    Scene,
    Square,
    VectorPath,
)

scene = Scene()

circle = Circle(0.9).set_fill(BLUE).set_stroke(WHITE, 0.055).shift(LEFT * 3 + UP)
square = Square(1.7).set_fill(PINK).set_stroke(WHITE, 0.055).shift(UP)
line = Line(LEFT, RIGHT).set_stroke(BLUE, 0.055).scale(1.25).shift(RIGHT * 3 + UP)

wave_path = (
    VectorPath()
    .move_to(LEFT * 2.4 + DOWN)
    .cubic_to(LEFT * 1.2 + DOWN * 2.0, RIGHT * 1.2, RIGHT * 2.4 + DOWN)
)
wave = Path(wave_path).set_fill(None).set_stroke(PINK, 0.05).shift(DOWN * 0.6)

scene.add(circle, square, line, wave)
scene.play(
    Create(circle),
    Create(square),
    Create(line),
    Create(wave),
    run_time=3.2,
    easing="ease_in_out_cubic",
)

result = scene
''')

# Add a browser-level endpoint regression in the upper-right line region. At
# 40ms before completion an analytic line is already visually indistinguishable
# from its endpoint; the old path-clipped line still had a late cap transition.
smoke = "scripts/browser-smoke.mjs"
replace_once(
    smoke,
    '''function latestSceneEnd(document) {''',
    '''function differingPixelCount(beforeBuffer, afterBuffer, region) {
  const before = PNG.sync.read(beforeBuffer);
  const after = PNG.sync.read(afterBuffer);
  assert.equal(before.width, after.width, "pixel diff width mismatch");
  assert.equal(before.height, after.height, "pixel diff height mismatch");
  let differing = 0;
  const minX = Math.max(0, Math.floor(region.minX * before.width));
  const maxX = Math.min(before.width - 1, Math.ceil(region.maxX * before.width));
  const minY = Math.max(0, Math.floor(region.minY * before.height));
  const maxY = Math.min(before.height - 1, Math.ceil(region.maxY * before.height));
  for (let y = minY; y <= maxY; y += 1) {
    for (let x = minX; x <= maxX; x += 1) {
      const offset = (y * before.width + x) * 4;
      const distance =
        Math.abs(before.data[offset] - after.data[offset]) +
        Math.abs(before.data[offset + 1] - after.data[offset + 1]) +
        Math.abs(before.data[offset + 2] - after.data[offset + 2]) +
        Math.abs(before.data[offset + 3] - after.data[offset + 3]);
      if (distance >= 32) {
        differing += 1;
      }
    }
  }
  return differing;
}

function latestSceneEnd(document) {''',
)
replace_once(
    smoke,
    '''      console.log(`✓ ${example.name}: Create-to-analytic bounds continuous within ${boundDelta}px`);
    }
  }''',
    '''      console.log(`✓ ${example.name}: Create-to-analytic bounds continuous within ${boundDelta}px`);

      const lineBeforePath = path.join(
        artifactDir,
        artifactName(index, example.name, "line-end-before"),
      );
      const lineEndPath = path.join(
        artifactDir,
        artifactName(index, example.name, "line-end-final"),
      );
      const lineBefore = await renderAndCapture(page, latestEnd - 0.04, lineBeforePath);
      const lineEnd = await renderAndCapture(page, latestEnd, lineEndPath);
      const lineEndpointDiff = differingPixelCount(
        lineBefore.screenshot,
        lineEnd.screenshot,
        { minX: 0.64, maxX: 0.98, minY: 0.12, maxY: 0.48 },
      );
      assert.ok(
        lineEndpointDiff <= 24,
        `${example.name}: line endpoint changed across ${lineEndpointDiff} pixels near completion`,
      );
      console.log(
        `✓ ${example.name}: line endpoint continuous near completion (${lineEndpointDiff} changed pixels)`,
      );
    }
  }''',
)

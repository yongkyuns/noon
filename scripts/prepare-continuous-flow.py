"""One-use, exact-branch integration of reviewed shared Rust stroke code.

Removed before the resulting production commit. This is not a runtime, build
input, permanent code generator, or animation implementation.
"""
from pathlib import Path
import re

changed = set()

def edit(path, before, after, count=1):
    p = Path(path)
    text = p.read_text()
    if text.count(before) != count:
        raise RuntimeError(f'{path}: unexpected source anchor {before[:90]!r}')
    p.write_text(text.replace(before, after))
    changed.add(path)

def block_span(text, marker):
    a = text.index('{', text.index(marker))
    depth = 0
    for b in range(a, len(text)):
        if text[b] == '{': depth += 1
        elif text[b] == '}':
            depth -= 1
            if depth == 0: return a, b + 1
    raise ValueError(marker)

def block_edit(path, marker, before, after):
    p = Path(path); text = p.read_text()
    a, b = block_span(text, marker)
    part = text[a:b]
    if part.count(before) != 1:
        raise RuntimeError(f'{path}: unexpected block anchor {marker}')
    p.write_text(text[:a] + part.replace(before, after) + text[b:])
    changed.add(path)

edit('crates/noon-core/src/lib.rs', 'mod spatial3d;',
     'mod spatial3d;\nmod stroke_pattern;\npub use stroke_pattern::*;')
edit('crates/noon/src/lib.rs', 'mod dashed_line_authoring;',
     'mod dashed_line_authoring;\npub use noon_core::{StrokePattern, StrokePatternError};')
edit('crates/noon/src/lib.rs', '        StoredGeometry, StyleUpdate,',
     '        StoredGeometry, StrokePattern, StrokePatternError, StyleUpdate,')
edit('crates/noon/src/semantic_mobject.rs', 'mod style;', 'mod style;\nmod stroke_pattern;')
edit('crates/noon/src/example_scenes.rs', 'pub mod line_passing_flash;',
     'pub mod line_passing_flash;\npub mod continuous_flow;')
block_edit('crates/noon-core/src/lib.rs', 'pub struct Style {', 'pub opacity: f32,',
    '#[serde(default, skip_serializing_if = "Option::is_none")]\n    pub stroke_pattern: Option<StrokePattern>,\n    pub opacity: f32,')
block_edit('crates/noon-core/src/lib.rs', 'impl Default for Style',
    '        Self {', '        Self {\n            stroke_pattern: None,')
model = 'crates/noon-core/src/semantic_store/semantic_model.rs'
block_edit(model, 'pub struct SemanticStyle {', 'pub object_opacity: f64,',
    '#[serde(default, skip_serializing_if = "Option::is_none")]\n    pub stroke_pattern: Option<crate::StrokePattern>,\n    pub object_opacity: f64,')
block_edit(model, 'impl Default for SemanticStyle', '        Self {',
    '        Self {\n            stroke_pattern: None,')
block_edit(model, 'pub fn from_compact(style: Style)', 'Self {',
    'Self {\n            stroke_pattern: style.stroke_pattern,')
edit(model, '&& self.object_opacity.is_finite()',
    '&& self.object_opacity.is_finite()\n            && self.stroke_pattern.is_none_or(|p| p.validate().is_ok())')
edit('crates/noon-core/src/object_state.rs', '&& style.opacity.is_finite();',
    '&& style.opacity.is_finite()\n        && style.stroke_pattern.is_none_or(|p| p.validate().is_ok());')

# Propagate only through explicit style literals; preserve conversions. Strings,
# docs, nested expressions, type definitions, and ..Default literals are skipped.
def code_mask(text):
    chars = list(text); i = 0; n = len(text)
    raw_pattern = re.compile(r'(?:br|r)(#{0,64})"')
    def blank(a, b):
        for j in range(a, b):
            if chars[j] != '\n': chars[j] = ' '
    while i < n:
        if text.startswith('//', i):
            j = text.find('\n', i)
            if j < 0: j = n
            blank(i, j); i = j
        elif text.startswith('/*', i):
            j = i + 2; depth = 1
            while j < n and depth:
                if text.startswith('/*', j): depth += 1; j += 2
                elif text.startswith('*/', j): depth -= 1; j += 2
                else: j += 1
            blank(i, j); i = j
        else:
            raw = raw_pattern.match(text, i) if text[i] in 'br' else None
            if raw:
                ending = '"' + raw[1]
                j = text.find(ending, i + len(raw[0]))
                j = n if j < 0 else j + len(ending)
                blank(i, j); i = j
            elif text[i] == '"':
                j = i + 1
                while j < n:
                    if text[j] == '\\': j += 2
                    elif text[j] == '"': j += 1; break
                    else: j += 1
                blank(i, min(j, n)); i = j
            elif text[i] == "'" and (char := re.match(r"'(?:\\.|[^'\\])'", text[i:i+16])):
                j = i + len(char[0]); blank(i, j); i = j
            else: i += 1
    return ''.join(chars)

for p in sorted(Path('crates').rglob('*.rs')):
    text = p.read_text()
    if not re.search(r'\b(?:SemanticStyle|Style)\s*\{', text): continue
    mask = code_mask(text); insertions = []
    for match in re.finditer(r'\b(?:SemanticStyle|Style)\s*\{', mask):
        a = mask.index('{', match.start()); depth = 1; b = a + 1
        while b < len(mask) and depth:
            depth += (mask[b] == '{') - (mask[b] == '}'); b += 1
        body = mask[a+1:b-1]
        flat = []; depth = 0
        for char in body:
            if char in '{[(': depth += 1; flat.append(' ')
            elif char in '}])': depth -= 1; flat.append(' ')
            else: flat.append(char if depth == 0 else ' ')
        flat = ''.join(flat)
        if '..' in flat or re.search(r'\bstroke_pattern\s*:', flat): continue
        fields = re.findall(r'(?:^|,)\s*(\w+)\s*(?::|(?=,|$))', flat)
        if 'fill' not in fields or 'stroke_width' not in fields or 'pub ' in flat: continue
        if any(token in flat for token in ['fn ', 'let ', 'return ', '=>']): continue
        expression = 'None'
        copied = re.search(r'\bstroke_(?:cap|join|width_mode)\s*:\s*([A-Za-z_]\w*(?:\.[A-Za-z_]\w*)*)\.stroke_(?:cap|join|width_mode)', body)
        if copied: expression = copied[1] + '.stroke_pattern'
        indent = re.search(r'\n([ \t]+)\S', text[a+1:b])
        pad = indent[1] if indent else '        '
        insertions.append((a+1, '\n' + pad + 'stroke_pattern: ' + expression + ','))
    for position, addition in reversed(insertions):
        text = text[:position] + addition + text[position:]
    if insertions:
        p.write_text(text); changed.add(str(p))

runtime = Path('crates/noon-runtime/src/lib.rs')
text = runtime.read_text(); a,b = block_span(text, 'fn interpolate_style(')
part = text[a:b]
new = re.sub(r'stroke_pattern:\s*[^,]+,',
    'stroke_pattern: noon_core::StrokePattern::interpolate(from.stroke_pattern, to.stroke_pattern, progress),', part, count=1)
assert new != part
runtime.write_text(text[:a] + new + text[b:])
changed.add(str(runtime))

# Explicitly fail unsupported profiles through the same lowering error model.
projection = Path('crates/noon-compile/src/semantic_lowering/projection.rs')
text = projection.read_text()
text = text.replace('pub enum SemanticLoweringError {',
    'pub enum SemanticLoweringError {\n    UnsupportedStrokePattern { node: SemanticNodeId },', 1)
text = text.replace('pub enum SemanticExecutionValueError {',
    'pub enum SemanticExecutionValueError {\n    UnsupportedStrokePattern,', 1)
for marker, arm in [
    ('impl std::fmt::Display for SemanticLoweringError', '\n            Self::UnsupportedStrokePattern { node } => write!(formatter, "stroke pattern on {node:?} requires a finite planar analytic Line"),'),
    ('impl std::fmt::Display for SemanticExecutionValueError', '\n            Self::UnsupportedStrokePattern => formatter.write_str("stroke pattern requires a finite planar analytic Line"),'),
    ('pub(super) fn with_node', '\n            Self::UnsupportedStrokePattern => SemanticLoweringError::UnsupportedStrokePattern { node },'),
]:
    start = text.index(marker)
    at = text.index('        match self {', start) + len('        match self {')
    text = text[:at] + arm + text[at:]
start = text.index('pub(crate) fn lower_semantic_style_value(')
at = text.index('    let fill = lower_paint(', start)
text = text[:at] + '''    if let Some(pattern) = state.style.stroke_pattern {
        if pattern.validate().is_err()
            || state.transform.planar_rotation().is_none()
            || state.transform.translation.z != 0.0
            || !matches!(state.content, SemanticObjectContent::Geometry(noon_core::StoredGeometry::Line { .. }))
        {
            return Err(SemanticExecutionValueError::UnsupportedStrokePattern);
        }
    }
''' + text[at:]
projection.write_text(text); changed.add(str(projection))
edit('crates/noon-compile/src/semantic_lowering/animation_payload/transform_payload.rs',
    '        || source.style.stroke_cap != target.style.stroke_cap',
    '        || source.style.stroke_cap != target.style.stroke_cap\n        || source.style.stroke_pattern != target.style.stroke_pattern')

renderer = 'crates/noon-render-wgpu/src/lib.rs'
edit(renderer, '((self.fill_enabled != 0 && self.fill[3] != 0.0)',
    '(((self.fill_enabled & 1) != 0 && self.fill[3] != 0.0)')
block_edit(renderer, 'fn pack_line(', '    LineInstance {',
    '''    if let Some(pattern) = object.style.stroke_pattern {
        style.fill = pattern.packed();
        style.fill_enabled = 4; // explicit line-only payload in unused fill lanes
    }
    LineInstance {''')
shader = 'crates/noon-render-wgpu/src/analytic.wgsl'
block_edit(shader, 'struct VertexOutput {', '@location(7) line_stroke_enabled: f32,',
    '@location(7) line_stroke_enabled: f32,\n    @location(8) @interpolate(flat) stroke_pattern: vec4<f32>,')
block_edit(shader, 'fn make_output(', 'output.line_stroke_enabled = 0.0;',
    'output.line_stroke_enabled = 0.0;\n    output.stroke_pattern = vec4<f32>(0.0);')
block_edit(shader, 'fn vs_line(',
    'output.line_stroke_enabled = select(0.0, 1.0, stroke_is_enabled(input.flags));',
    '''output.line_stroke_enabled = select(0.0, 1.0, stroke_is_enabled(input.flags));
    output.stroke_pattern = vec4<f32>(0.0);
    if (input.flags.x & 4u) != 0u {
        var units = 1.0;
        if screen_space {
            let local_length = length(input.end - input.start);
            let world_start = transform_point(input.start, input.translation, input.scale, input.rotation);
            let world_end = transform_point(input.end, input.translation, input.scale, input.rotation);
            units = length(world_end - world_start) / max(local_length, 0.000001);
        }
        output.stroke_pattern = vec4<f32>(input.fill.xyz * units, input.fill.w);
    }''')
helper = '''// Periodic ink on one retained Line. The shader owns no animation time.
fn periodic_stroke_mask(pattern: vec4<f32>, local: vec2<f32>, half_length: f32,
                        radius: f32, footprint: vec2<f32>) -> f32 {
    let period = max(pattern.x, 0.000001);
    let centered = local.x + half_length - pattern.z - pattern.y * 0.5;
    let q = centered - period * floor(centered / period + 0.5);
    let half_pixel = max(abs(footprint.x) * 0.5, 0.000001);
    let dash = 1.0 - smoothstep(-half_pixel, half_pixel, abs(q) - pattern.y * 0.5);
    let delta = vec2<f32>(q, local.y);
    let distance = length(delta);
    let circle_pixel = max(dot(abs(delta) / max(distance, 0.000001), abs(footprint)) * 0.5, 0.000001);
    let dot_mask = 1.0 - smoothstep(-circle_pixel, circle_pixel, distance - radius);
    let ink = select(dash, dot_mask, pattern.y == 0.0);
    return pattern.w + (1.0 - pattern.w) * ink;
}

'''
edit(shader, '@fragment\nfn fs_line(', helper + '@fragment\nfn fs_line(')
block_edit(shader, 'fn fs_line(', 'return styled_line_color(input, coverage) * visible;',
    '''let footprint = fwidth(input.local);
    var mask = 1.0;
    if input.stroke_pattern.x > 0.0 {
        mask = periodic_stroke_mask(input.stroke_pattern, input.local, half_length, radius, footprint);
    }
    return styled_line_color(input, coverage * mask) * visible;''')

capture = Path('crates/noon-export/tests/capture.rs')
capture.write_text(capture.read_text() + '''
#[test]
#[ignore = "requires software Vulkan; run by native output qualification"]
fn native_capture_continuous_stroke_pattern_replays_retained_pixels() {
    let mut session = noon::example_scenes::continuous_flow::session().unwrap();
    let mut capture = noon_export::SessionCapture::new(&session, software(640, 360)).unwrap();
    let times = [0.0, 0.125, 0.25, 0.5, 1.0, 2.0, 3.75, 4.0, 4.5];
    let mut images = Vec::new();
    for (index, time) in times.into_iter().enumerate() {
        session.advance_to(time).unwrap();
        assert_eq!(session.frame().objects.len(), 2, "no object per dash");
        let image = capture.capture(&mut session).unwrap();
        assert_eq!(image.receipt.published_time, time);
        let rgb = image.rgba.as_chunks::<4>().0;
        assert!(rgb.iter().all(|pixel| pixel[3] == 255));
        assert!(rgb.iter().filter(|p| p[0] < 64 && p[1] > 160 && p[2] > 160).count() > 40,
            "missing cyan dashes at t={time}");
        assert!(rgb.iter().filter(|p| p[0] > 160 && p[1] > 120 && p[2] < 64).count() > 20,
            "missing yellow dots at t={time}");
        proof(&format!("continuous-flow-{index:02}"), image.rgba, 640, 360);
        images.push(image.rgba.to_vec());
    }
    assert_ne!(images[0], images[1], "pattern must visibly move");
    assert_eq!(images[7], images[8], "stop must freeze the patterns");
    assert_eq!(images[0], images[7], "complete cycles recover the initial pixels");
    for index in [7, 2, 0, 6, 4, 1, 8, 3, 5, 0] {
        session.seek(times[index]).unwrap();
        assert_eq!(capture.capture(&mut session).unwrap().rgba, images[index],
            "retained seek at t={}", times[index]);
    }
}
''')
changed.add(str(capture))
print('Integrated production stroke implementation into:', *sorted(changed), sep='\n  ')

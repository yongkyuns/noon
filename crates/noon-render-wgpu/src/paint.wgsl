struct GradientStopUniform {
    color: vec4<f32>,
    position: vec4<f32>,
};
struct PaintUniform {
    emission: vec4<f32>,
    bounds: vec4<f32>,
    endpoints: vec4<f32>,
    metadata: vec4<u32>,
    transform: vec4<f32>,
    rotation: vec4<f32>,
    stops: array<GradientStopUniform, 16>,
    image_u: vec4<f32>,
    image_v: vec4<f32>,
};
@group(1) @binding(0) var<uniform> paint: PaintUniform;
@group(2) @binding(0) var paint_image: texture_2d<f32>;

fn paint_local_from_pixel(pixel: vec2<f32>) -> vec2<f32> {
    let clip = vec2<f32>(pixel.x * 2.0 / camera.viewport_size.x - 1.0,
        1.0 - pixel.y * 2.0 / camera.viewport_size.y);
    let delta = clip / camera.clip_scale + camera.center - paint.transform.xy;
    let c = paint.rotation.x;
    let s = paint.rotation.y;
    return vec2<f32>(c * delta.x + s * delta.y, -s * delta.x + c * delta.y) / paint.transform.zw;
}

fn noise_hash(cell: vec2<u32>) -> f32 {
    var h = paint.metadata.z ^ (cell.x * 0x9e3779b9u) ^ (cell.y * 0x85ebca6bu);
    h = (h ^ (h >> 16u)) * 0x7feb352du;
    h = (h ^ (h >> 15u)) * 0x846ca68bu;
    h = h ^ (h >> 16u);
    return f32(h >> 16u) / 65535.0;
}
struct NoiseAxis {
    cell: i32,
    weights: vec3<f32>,
};
fn noise_integral(t: f32) -> f32 {
    return t*t*t*t*(t*t - 3.0*t + 2.5);
}
fn noise_axis(center: f32, width: f32) -> NoiseAxis {
    if width < 1.0 / 256.0 {
        let t = fract(center);
        let weight = clamp(t*t*t*(t*(t*6.0-15.0)+10.0), 0.0, 1.0);
        return NoiseAxis(i32(floor(center)), vec3(1.0-weight, weight, 0.0));
    }
    let base = floor(center - width * 0.5);
    let lo = center - width * 0.5 - base;
    let hi = center + width * 0.5 - base;
    let a = clamp(lo, 0.0, 1.0);
    let b = clamp(hi, 0.0, 1.0);
    let c = clamp(lo-1.0, 0.0, 1.0);
    let d = clamp(hi-1.0, 0.0, 1.0);
    let first = noise_integral(b) - noise_integral(a);
    let second = noise_integral(d) - noise_integral(c);
    let weights = max(vec3(b-a-first, first+d-c-second, second) / width, vec3(0.0));
    return NoiseAxis(i32(base), weights / (weights.x+weights.y+weights.z));
}
fn noise_field(point: vec2<f32>) -> f32 {
    // Physical pixel -> inverse object pose -> normalized box -> lattice.
    // The lattice-aligned bounding box is the declared reconstruction filter.
    let pixel = vec2(2.0) / (camera.viewport_size * camera.clip_scale);
    let c = paint.rotation.x;
    let s = paint.rotation.y;
    let dx = vec2(c,-s) * pixel.x / paint.transform.zw / paint.bounds.zw * paint.image_u.xy;
    let dy = vec2(s,c) * pixel.y / paint.transform.zw / paint.bounds.zw * paint.image_u.xy;
    let width = abs(dx) + abs(dy);
    let coarse = max(width.x, width.y);
    if coarse >= 2.0 { return 0.5; }
    let q = clamp(point, vec2(0.0), vec2(1.0)) * paint.image_u.xy + paint.image_u.zw;
    let x = noise_axis(q.x, min(width.x, 1.0));
    let y = noise_axis(q.y, min(width.y, 1.0));
    let phase = vec2(paint.metadata.w & 65535u, paint.metadata.w >> 16u);
    let base = bitcast<vec2<u32>>(vec2(x.cell, y.cell)) + phase;
    var value = 0.0;
    for (var i = 0u; i < 3u; i += 1u) {
        for (var j = 0u; j < 3u; j += 1u) {
            let cell = (base + vec2(i,j)) & vec2(65535u);
            value += noise_hash(cell) * x.weights[i] * y.weights[j];
        }
    }
    let u = clamp(coarse - 1.0, 0.0, 1.0);
    let fade = u*u*(3.0-2.0*u);
    return mix(value, 0.5, fade);
}

// Exact periodic-band overlap for an interval shorter than one cell. Working
// in units of that interval keeps a tiny footprint meaningful at a band edge.
fn hatch_partial_band(center: f32, width: f32, duty: f32) -> f32 {
    let wrapped = center - floor(center + 0.5);
    var coverage = 0.0;
    for (var i = -1; i <= 1; i += 1) {
        let offset = f32(i) - wrapped;
        let hi = clamp((offset + duty * 0.5) / width + 0.5, 0.0, 1.0);
        let lo = clamp((offset - duty * 0.5) / width + 0.5, 0.0, 1.0);
        coverage += hi - lo;
    }
    return clamp(coverage, 0.0, 1.0);
}

fn hatch_band(center: f32, width: f32, duty: f32) -> f32 {
    if duty <= 0.0 { return 0.0; }
    if duty >= 1.0 { return 1.0; }
    if width >= 256.0 { return duty; }
    if width <= 0.0 {
        return select(0.0, 1.0, fract(center + duty * 0.5) < duty);
    }
    if width < 1.0 { return hatch_partial_band(center, width, duty); }
    let whole = floor(width);
    let remainder = width - whole;
    if remainder == 0.0 { return duty; }
    // Complete periods contribute exactly their duty. Only the parity of that
    // count changes the center of the remaining interval.
    let residual_center = center - f32(u32(whole) & 1u) * 0.5;
    let residual = hatch_partial_band(residual_center, remainder, duty);
    return clamp((whole * duty + remainder * residual) / width, 0.0, 1.0);
}

fn hatch_field(local: vec2<f32>) -> f32 {
    let pixel = vec2(2.0) / (camera.viewport_size * camera.clip_scale);
    let c = paint.rotation.x;
    let s = paint.rotation.y;
    let dx = vec2(c, -s) * pixel.x / paint.transform.zw;
    let dy = vec2(s, c) * pixel.y / paint.transform.zw;
    // The reconstruction domain is the physical pixel's axis-aligned box in
    // the perpendicular line coordinates, including inverse object pose.
    let width = vec2(
        abs(dot(paint.image_u.xy, dx)) + abs(dot(paint.image_u.xy, dy)),
        abs(dot(paint.image_v.xy, dx)) + abs(dot(paint.image_v.xy, dy)),
    );
    // Reduce the local coordinate before adding phase, so a large integral
    // cell does not erase the independently authored subcell displacement.
    let q = fract(vec2(dot(paint.image_u.xy, local), dot(paint.image_v.xy, local)))
        + vec2(paint.image_u.z, paint.image_v.z);
    let first = hatch_band(q.x, width.x, paint.image_u.w);
    if paint.metadata.z == 0u { return first; }
    let second = hatch_band(q.y, width.y, paint.image_u.w);
    return first + second - first * second;
}

// Return straight encoded-sRGB for the shared source quantizer. Interpolation
// itself is premultiplied so invisible RGB cannot leak into visible samples.
fn base_paint_fill(local: vec2<f32>, carrier: vec4<f32>) -> vec4<f32> {
    let point = (local - paint.bounds.xy) / paint.bounds.zw;
    if paint.metadata.y == 1u {
        let uv = vec2(dot(paint.image_u.xyz, vec3(point, 1.0)), dot(paint.image_v.xyz, vec3(point, 1.0)));
        if paint.image_u.w == 1.0 && (any(uv < vec2(0.0)) || any(uv > vec2(1.0))) { return vec4(0.0); }
        let color = raster_sample(paint_image, uv, textureDimensions(paint_image), paint.metadata.z, paint.metadata.w, 1.0);
        if color.a <= 0.0 { return vec4(0.0); }
        return vec4(color.rgb / color.a, color.a * carrier.a);
    }
    var t: f32;
    if paint.metadata.y == 4u {
        t = hatch_field(local);
    } else if paint.metadata.y == 3u {
        t = noise_field(point);
    } else if paint.metadata.y == 2u {
        let delta = abs(point - paint.endpoints.xy);
        let radii = paint.endpoints.zw;
        // Outside either radius the field is already in the pad region.
        // Divide only interior components, avoiding overflow when squaring
        // a large finite radial coordinate in length().
        if any(delta >= radii) { t = 1.0; }
        else { t = length(delta / radii); }
    } else {
        let axis = paint.endpoints.zw - paint.endpoints.xy;
        let extent = max(abs(axis.x), abs(axis.y));
        let scaled = axis / extent;
        t = (dot(point - paint.endpoints.xy, scaled) / dot(scaled, scaled)) / extent;
    }
    let count = paint.metadata.x;
    var color = paint.stops[0].color;
    if t >= paint.stops[0].position.x {
        color = paint.stops[count - 1u].color;
        for (var i = 1u; i < count; i += 1u) {
            if t < paint.stops[i].position.x {
                let a = paint.stops[i - 1u];
                let b = paint.stops[i];
                color = mix(a.color, b.color, (t - a.position.x) / (b.position.x - a.position.x));
                break;
            }
        }
    }
    if color.a <= 0.0 { return vec4<f32>(0.0); }
    return vec4<f32>(color.rgb / color.a, color.a * carrier.a);
}

// The base sampler has already decoded authored inputs in an HDR view.
fn paint_fill(local: vec2<f32>, carrier: vec4<f32>) -> vec4<f32> {
    let value = base_paint_fill(local, carrier);
    return vec4(value.rgb * (1.0 + paint.emission.x), value.a);
}

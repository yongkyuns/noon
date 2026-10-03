fn cairo_source_color(color: vec4<f32>, opacity: f32) -> vec4<f32> {
    // Cairo stores solid-pattern channels through a rounded 16-bit intermediate,
    // then takes the high byte of the premultiplied result. Quantize the source
    // before fixed-function source-over so WebGPU/WebGL match Cairo's ARGB32
    // compositing instead of applying the render target's nearest-UNORM rounding
    // directly to full-precision float colors.
    let alpha = clamp(color.a * opacity, 0.0, 1.0);
    let rgb16 = floor(clamp(color.rgb * alpha, vec3<f32>(0.0), vec3<f32>(1.0)) * 65535.0 + vec3<f32>(0.5));
    let alpha16 = floor(alpha * 65535.0 + 0.5);
    let rgb8 = floor(rgb16 / 256.0);
    let alpha8 = floor(alpha16 / 256.0);
    return vec4<f32>(rgb8 / 255.0, alpha8 / 255.0);
}

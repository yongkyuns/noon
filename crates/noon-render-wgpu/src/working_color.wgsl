// Input is straight authored sRGB. Alpha never passes through a transfer.
fn srgb_decode_rgb(encoded: vec3<f32>) -> vec3<f32> {
    return select(encoded / 12.92,
        pow((encoded + vec3(0.055)) / 1.055, vec3(2.4)),
        encoded > vec3(0.04045));
}

fn working_color_rgb(encoded: vec3<f32>) -> vec3<f32> {
    if !LINEAR_WORKING_COLOR { return encoded; }
    return srgb_decode_rgb(encoded);
}

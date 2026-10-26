@fragment
fn fs_gradient_circle(input: VertexOutput) -> @location(0) vec4<f32> {
    var painted = input;
    painted.fill = gradient_fill(input.local, input.fill);
    return circle_color(painted);
}

@fragment
fn fs_gradient_rectangle(input: RectangleVertexOutput) -> @location(0) vec4<f32> {
    var painted = input;
    painted.fill = gradient_fill(input.local, input.fill);
    return rectangle_color(painted);
}

from noon import Color, Scene, VectorPath

scene = Scene()

# One semantic path can contain several contours. Noon precomputes a single
# ordered arc-length domain, so the reveal advances through these contours in
# authoring order without rebuilding the mesh each frame.
constellation = (
    VectorPath()
    .move_to((-3.0, 1.55))
    .cubic_to((-2.4, 2.35), (-1.5, 2.35), (-1.05, 1.55))
    .cubic_to((-0.6, 0.75), (0.2, 0.75), (0.7, 1.55))
    .cubic_to((1.25, 2.35), (2.25, 2.35), (3.0, 1.45))
    .move_to((-2.65, 0.35))
    .quadratic_to((-1.75, -1.15), (-0.75, 0.1))
    .quadratic_to((0.1, 1.1), (0.85, -0.15))
    .quadratic_to((1.7, -1.35), (2.75, 0.25))
    .move_to((-2.9, -1.5))
    .line_to((-1.45, -0.8))
    .line_to((0.0, -1.8))
    .line_to((1.45, -0.8))
    .line_to((2.9, -1.5))
)

hero = scene.path(
    constellation,
    fill=None,
    stroke=Color(0.66, 0.58, 1.0),
    stroke_width=0.115,
    key="hero-path",
)
scene.animate_reveal(
    hero,
    duration=4.8,
    easing="ease_in_out_cubic",
    key="hero-path.reveal",
)

# A second path begins later, demonstrating that the reveal track's `from`
# value is also the deterministic pre-start state.
orbit = (
    VectorPath()
    .move_to((-2.4, 0.0))
    .cubic_to((-2.0, -2.1), (2.0, -2.1), (2.4, 0.0))
    .cubic_to((2.0, 2.1), (-2.0, 2.1), (-2.4, 0.0))
)
ring = scene.path(
    orbit,
    fill=None,
    stroke=Color(0.25, 0.82, 0.92),
    stroke_width=0.045,
    opacity=0.72,
    key="orbit",
)
scene.animate_reveal(
    ring,
    duration=3.4,
    start_time=0.8,
    easing="ease_in_out_cubic",
    key="orbit.reveal",
)

# Transform animation composes with reveal while the geometry stays cached.
scene.animate_rotation(
    ring,
    0.0,
    0.7,
    duration=4.2,
    start_time=0.8,
    easing="ease_in_out_cubic",
    key="orbit.rotation",
)

result = scene

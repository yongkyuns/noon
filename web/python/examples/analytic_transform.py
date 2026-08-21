from noon import Circle, Color, Line, Rectangle, Scene, Transform

scene = Scene()

# These stay analytic all the way through the runtime: radius, rectangle size,
# and line endpoints interpolate directly in semantic FrameState geometry.
circle = scene.add(
    Circle(
        0.55,
        position=(-2.5, 0.65),
        fill=Color(0.95, 0.38, 0.42),
        stroke=Color(1.0, 0.86, 0.88),
        stroke_width=0.07,
    ),
    key="analytic-circle",
)
rectangle = scene.add(
    Rectangle(
        1.1,
        0.8,
        position=(0.0, 0.6),
        rotation=-0.2,
        fill=Color(0.35, 0.64, 0.98),
        stroke=Color(0.78, 0.9, 1.0),
        stroke_width=0.07,
    ),
    key="analytic-rectangle",
)
line = scene.add(
    Line(
        (-0.65, 0.0),
        (0.65, 0.0),
        position=(2.45, 0.65),
        stroke=Color(0.32, 0.9, 0.62),
        stroke_width=0.14,
    ),
    key="analytic-line",
)

scene.play(
    Transform(
        circle,
        Circle(
            1.05,
            position=(-2.05, -0.55),
            scale=(1.15, 0.8),
            fill=Color(0.98, 0.7, 0.22),
            stroke=Color(1.0, 0.92, 0.7),
            stroke_width=0.12,
            opacity=0.82,
        ),
        key="analytic-circle.expand",
    ),
    Transform(
        rectangle,
        Rectangle(
            2.1,
            1.35,
            position=(0.0, -0.55),
            rotation=0.65,
            fill=Color(0.68, 0.42, 0.96),
            stroke=Color(0.9, 0.8, 1.0),
            stroke_width=0.13,
            opacity=0.85,
        ),
        key="analytic-rectangle.expand",
    ),
    Transform(
        line,
        Line(
            (-1.0, -0.65),
            (1.0, 0.65),
            position=(2.05, -0.55),
            rotation=-0.25,
            stroke=Color(0.22, 0.82, 0.96),
            stroke_width=0.22,
            opacity=0.9,
        ),
        key="analytic-line.expand",
    ),
    duration=2.0,
    easing="ease_in_out_cubic",
)

# A second atomic Transform demonstrates exact snapshot chaining at t=2 s.
scene.play(
    Transform(
        circle,
        Circle(
            0.55,
            position=(-2.5, 0.65),
            fill=Color(0.95, 0.38, 0.42),
            stroke=Color(1.0, 0.86, 0.88),
            stroke_width=0.07,
        ),
        key="analytic-circle.return",
    ),
    Transform(
        rectangle,
        Rectangle(
            1.1,
            0.8,
            position=(0.0, 0.6),
            rotation=-0.2,
            fill=Color(0.35, 0.64, 0.98),
            stroke=Color(0.78, 0.9, 1.0),
            stroke_width=0.07,
        ),
        key="analytic-rectangle.return",
    ),
    Transform(
        line,
        Line(
            (-0.65, 0.0),
            (0.65, 0.0),
            position=(2.45, 0.65),
            stroke=Color(0.32, 0.9, 0.62),
            stroke_width=0.14,
        ),
        key="analytic-line.return",
    ),
    duration=2.0,
    start_time=2.0,
    easing="ease_in_out_cubic",
)

result = scene

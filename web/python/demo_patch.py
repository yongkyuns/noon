from noon import Color, PatchBatch

palette = context["palette"]
result = (
    PatchBatch(context["sequence"])
    .set_style(
        0,
        fill=Color(*palette["circle"]),
        stroke=Color(1.0, 1.0, 1.0),
        stroke_width=0.04,
    )
    .set_style(
        1,
        fill=Color(*palette["rectangle"]),
        stroke=Color(1.0, 1.0, 1.0),
        stroke_width=0.04,
    )
    .set_style(
        2,
        fill=None,
        stroke=Color(*palette["line"]),
        stroke_width=0.10,
    )
)

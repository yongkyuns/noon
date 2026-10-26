"""Paired with noon::example_scenes::presentation_presets.

Theme values construct ordinary Text and shapes; standard family layout places
them. Explicit text arguments override the selected role for that object only.
"""
from noon import *


class PresentationPresets(Scene):
    def construct(self):
        for theme, name, x in ((Theme.dark(), "Dark", -3.35), (Theme.light(), "Light", 3.35)):
            panel = Rectangle(width=6.2, height=5.8, stroke_width=0)
            panel.set_fill(theme.color("background"), opacity=1).shift(x * RIGHT)
            title = theme.text(name, role="title")
            body = theme.text("Meaning before styling")
            caption = theme.text("Ordinary Text, shared defaults", role="caption")
            override = theme.text("Explicit override", role="label", font_size=24,
                                  color=theme.color("accent"))
            swatches = VGroup(*[
                Circle(radius=0.16, stroke_width=0).set_fill(theme.categorical(index), opacity=1)
                for index in range(theme.categorical_count)
            ]).arrange(RIGHT, buff=theme.spacing("normal"))
            content = VGroup(title, body, caption, override, swatches)
            content.arrange(DOWN, buff=theme.spacing("large")).shift(x * RIGHT)
            self.add(panel, content)
        self.wait(1.0)

from noon import *


def caption(words, size, position, color=None):
    item = Text(words).scale(size)
    if color is not None:
        item.set_color(color)
    item.move_to(position)
    return item


class TrigonometryTutorial(Scene):
    """A human-friendly visual primer on right-triangle trigonometry."""

    def construct(self):
        # 1. Hook: tell the learner what problem trigonometry solves.
        title = caption("TRIGONOMETRY, MADE HUMAN", 0.85, (0, 2.35, 0), BLUE)
        subtitle = caption("Use one angle and two side lengths to find the missing piece", 0.42, (0, 1.45, 0))
        hook = caption("No memorization first — just draw, name, and choose.", 0.52, (0, 0.25, 0), GREEN)
        invitation = caption("In this lesson: the sides, the ratios, and one complete example", 0.42, (0, -1.0, 0))
        self.play(Write(title), Write(subtitle), run_time=1.2)
        self.play(Write(hook), Write(invitation), run_time=1.0)
        self.wait(2.8)
        self.play(FadeOut(title), FadeOut(subtitle), FadeOut(hook), FadeOut(invitation), run_time=0.8)

        # 2. Give the learner a stable picture before introducing formulas.
        section = caption("Step 1: name the sides from your angle", 0.68, (0, 2.7, 0), BLUE)
        a = (-4.8, -1.35, 0)
        b = (-0.5, -1.35, 0)
        c = (-0.5, 1.75, 0)
        base = Line(a, b).set_color(BLUE)
        vertical = Line(b, c).set_color(GREEN)
        diagonal = Line(a, c).set_color(YELLOW)
        theta_dot = Dot(point=a, radius=0.14, color=RED)
        theta = caption("theta", 0.42, (-4.25, -0.8, 0), RED)
        base_label = caption("adjacent", 0.38, (-2.7, -1.95, 0), BLUE)
        vertical_label = caption("opposite", 0.38, (0.1, 0.2, 0), GREEN)
        diagonal_label = caption("hypotenuse", 0.38, (-2.55, 0.55, 0), YELLOW)
        explanation_1 = caption("Pick the angle you care about.", 0.52, (3.65, 1.55, 0))
        explanation_2 = caption("That angle decides the names.", 0.52, (3.65, 0.9, 0), BLUE)
        explanation_3 = caption("opposite = across from theta", 0.42, (3.65, 0.05, 0), GREEN)
        explanation_4 = caption("adjacent = next to theta", 0.42, (3.65, -0.7, 0), BLUE)
        explanation_5 = caption("hypotenuse = longest side", 0.42, (3.65, -1.45, 0), YELLOW)
        explanation_6 = caption("It always faces the 90 deg corner.", 0.38, (3.65, -2.1, 0))
        self.play(Write(section), run_time=0.8)
        self.play(Create(base), Create(vertical), Create(diagonal), FadeIn(theta_dot), Write(theta), run_time=1.5)
        self.play(
            Write(base_label),
            Write(vertical_label),
            Write(diagonal_label),
            Write(explanation_1),
            Write(explanation_2),
            run_time=1.1,
        )
        self.play(Write(explanation_3), Write(explanation_4), Write(explanation_5), Write(explanation_6), run_time=1.5)
        self.wait(4.0)
        self.play(
            FadeOut(section), FadeOut(base), FadeOut(vertical), FadeOut(diagonal), FadeOut(theta_dot),
            FadeOut(theta), FadeOut(base_label), FadeOut(vertical_label), FadeOut(diagonal_label),
            FadeOut(explanation_1), FadeOut(explanation_2), FadeOut(explanation_3), FadeOut(explanation_4),
            FadeOut(explanation_5), FadeOut(explanation_6), run_time=0.9,
        )

        # 3. Put the Pythagorean theorem in context before the trig ratios.
        section = caption("Step 2: use the Pythagorean theorem when it fits", 0.62, (0, 2.55, 0), BLUE)
        context = caption("If you know two sides of a right triangle, find the third.", 0.48, (0, 1.65, 0))
        theorem = caption("a^2 + b^2 = c^2", 0.9, (0, 0.55, 0), YELLOW)
        example = caption("3^2 + 4^2 = 5^2", 0.7, (0, -0.65, 0), GREEN)
        arithmetic = caption("9 + 16 = 25", 0.56, (0, -1.45, 0))
        lesson = caption("This finds a missing side — but not a missing angle.", 0.48, (0, -2.45, 0), RED)
        self.play(Write(section), Write(context), run_time=1.0)
        self.play(Write(theorem), run_time=0.8)
        self.play(Write(example), Write(arithmetic), run_time=0.9)
        self.play(Write(lesson), run_time=0.8)
        self.wait(3.6)
        self.play(FadeOut(section), FadeOut(context), FadeOut(theorem), FadeOut(example), FadeOut(arithmetic), FadeOut(lesson), run_time=0.9)

        # 4. Introduce SOH-CAH-TOA with a plain-language gloss for each ratio.
        section = caption("Step 3: choose a ratio for the angle", 0.7, (0, 2.65, 0), BLUE)
        reminder = caption("Use the ratio that contains the sides you know.", 0.48, (0, 1.9, 0))
        sine = caption("sin(theta) = opposite / hypotenuse", 0.58, (0, 1.15, 0), GREEN)
        sine_gloss = caption("across divided by longest", 0.38, (0, 0.65, 0), GREEN)
        cosine = caption("cos(theta) = adjacent / hypotenuse", 0.58, (0, -0.05, 0), BLUE)
        cosine_gloss = caption("next to divided by longest", 0.38, (0, -0.55, 0), BLUE)
        tangent = caption("tan(theta) = opposite / adjacent", 0.58, (0, -1.25, 0), YELLOW)
        tangent_gloss = caption("across divided by next to", 0.38, (0, -1.75, 0), YELLOW)
        mnemonic = caption("SOH — CAH — TOA", 0.62, (0, -2.7, 0), RED)
        self.play(Write(section), Write(reminder), run_time=0.9)
        self.play(Write(sine), Write(sine_gloss), run_time=0.9)
        self.play(Write(cosine), Write(cosine_gloss), run_time=0.9)
        self.play(Write(tangent), Write(tangent_gloss), run_time=0.9)
        self.play(Write(mnemonic), run_time=0.8)
        self.wait(4.2)
        self.play(
            FadeOut(section), FadeOut(reminder), FadeOut(sine), FadeOut(sine_gloss), FadeOut(cosine),
            FadeOut(cosine_gloss), FadeOut(tangent), FadeOut(tangent_gloss), FadeOut(mnemonic), run_time=0.9,
        )

        # 5. Work through a complete, calculator-friendly example.
        section = caption("Step 4: a complete example", 0.76, (0, 2.7, 0), BLUE)
        a = (-4.8, -1.35, 0)
        b = (-1.0, -1.35, 0)
        c = (-1.0, 1.5, 0)
        base = Line(a, b).set_color(BLUE)
        vertical = Line(b, c).set_color(GREEN)
        diagonal = Line(a, c).set_color(YELLOW)
        theta_dot = Dot(point=a, radius=0.14, color=RED)
        theta = caption("theta", 0.42, (-4.25, -0.8, 0), RED)
        four = caption("4", 0.54, (-2.9, -1.9, 0), BLUE)
        three = caption("3", 0.54, (-0.55, 0.15, 0), GREEN)
        five = caption("5", 0.54, (-3.1, 0.25, 0), YELLOW)
        known = caption("Known: opposite = 3, hypotenuse = 5", 0.43, (3.25, 1.4, 0))
        choose = caption("Those sides match sine.", 0.48, (3.25, 0.7, 0), GREEN)
        calculation = caption("sin(theta) = 3 / 5 = 0.60", 0.56, (3.25, -0.05, 0), GREEN)
        calculator = caption("Use inverse sine: theta = sin^-1(0.60)", 0.36, (3.25, -0.85, 0), BLUE)
        answer = caption("theta is about 36.9 degrees", 0.48, (3.25, -1.6, 0), YELLOW)
        check = caption("Check: the angle is acute.", 0.42, (0, -2.65, 0), RED)
        self.play(Write(section), run_time=0.8)
        self.play(Create(base), Create(vertical), Create(diagonal), FadeIn(theta_dot), Write(theta), run_time=1.4)
        self.play(Write(four), Write(three), Write(five), run_time=0.8)
        self.play(Write(known), Write(choose), run_time=0.9)
        self.play(Write(calculation), run_time=0.7)
        self.play(Write(calculator), Write(answer), run_time=1.0)
        self.play(Write(check), run_time=0.7)
        self.wait(4.5)
        self.play(
            FadeOut(section), FadeOut(base), FadeOut(vertical), FadeOut(diagonal), FadeOut(theta_dot), FadeOut(theta),
            FadeOut(four), FadeOut(three), FadeOut(five), FadeOut(known), FadeOut(choose), FadeOut(calculation),
            FadeOut(calculator), FadeOut(answer), FadeOut(check), run_time=0.9,
        )

        # 6. Address the mistakes that make first attempts feel confusing.
        section = caption("Three checks before you press =", 0.78, (0, 2.55, 0), BLUE)
        check_1 = caption("1. Hypotenuse is always opposite the 90 deg corner.", 0.45, (0, 1.4, 0), YELLOW)
        check_2 = caption("2. Opposite and adjacent depend on your chosen angle.", 0.45, (0, 0.45, 0), GREEN)
        check_3 = caption("3. Set your calculator to degrees for this lesson.", 0.45, (0, -0.5, 0), RED)
        check_4 = caption("4. Sketch first — the picture catches many mistakes.", 0.45, (0, -1.45, 0), BLUE)
        reassurance = caption("Surprised? Re-check the labels before the math.", 0.4, (0, -2.45, 0))
        self.play(Write(section), run_time=0.8)
        self.play(Write(check_1), Write(check_2), run_time=1.0)
        self.play(Write(check_3), Write(check_4), run_time=1.0)
        self.play(Write(reassurance), run_time=0.8)
        self.wait(3.0)
        self.play(FadeOut(section), FadeOut(check_1), FadeOut(check_2), FadeOut(check_3), FadeOut(check_4), FadeOut(reassurance), run_time=0.9)

        # 7. Close with a repeatable workflow the learner can use immediately.
        section = caption("Your repeatable workflow", 0.82, (0, 2.5, 0), BLUE)
        step_1 = caption("1. Draw the right triangle and mark theta.", 0.52, (0, 1.35, 0), GREEN)
        step_2 = caption("2. Label opposite, adjacent, and hypotenuse.", 0.52, (0, 0.55, 0), BLUE)
        step_3 = caption("3. Pick SOH, CAH, or TOA.", 0.52, (0, -0.25, 0), YELLOW)
        step_4 = caption("4. Substitute, calculate, and check the result.", 0.52, (0, -1.05, 0), RED)
        final = caption("You do not need to memorize the picture — redraw it.", 0.56, (0, -2.2, 0), GREEN)
        closing = caption("One triangle at a time.", 0.64, (0, -3.0, 0), BLUE)
        self.play(Write(section), run_time=0.8)
        self.play(Write(step_1), Write(step_2), run_time=1.0)
        self.play(Write(step_3), Write(step_4), run_time=1.0)
        self.play(Write(final), Write(closing), run_time=1.0)
        self.wait(3.0)

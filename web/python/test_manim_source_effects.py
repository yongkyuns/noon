import unittest
from hashlib import sha256
from pathlib import Path
from textwrap import dedent

from _manim_source_execution import source_execution_proof, source_is_scene_only


class SourceEffectsTests(unittest.TestCase):
    def test_linear_scene_requires_closed_base_initializer_and_text(self):
        source = Path(__file__).with_name("examples").joinpath("showcase_linear_algebra.py").read_text()
        self.assertEqual(source_execution_proof(source), {"admitted": True, "reason": None})
        variants = (
            source.replace("from noon import *", "from noon import Scene, LinearTransformationScene, prepare_latex, YELLOW"),
            source.replace("from noon import *", "from manim import *"),
            source.replace("class LinearAlgebra", "LinearTransformationScene = Scene\nclass LinearAlgebra"),
            source.replace("leave_ghost_vectors=True", "leave_ghost_vectors=context.get('value')"),
            source.replace("leave_ghost_vectors=True", "leave_ghost_vectors=1"),
            source + "\n    def apply_matrix(self, matrix): pass\n",
            source.replace('        caption = Typst', '        Typst = self.wait\n        caption = Typst'),
            source.replace('        caption = Typst', '        Mesh3D.sphere(radius=1)\n        caption = Typst'),
            source.replace('        caption = Typst("Swap the coordinates")', '        caption = Typst(context.get("caption"))'),
        )
        for unsafe in variants:
            with self.subTest(source=unsafe):
                self.assertFalse(source_is_scene_only(unsafe))

    def test_moving_camera_scene_requires_the_canonical_closed_wrapper(self):
        examples = Path(__file__).with_name("examples")
        bloom = (examples / "showcase_bloom_emission.py").read_text()
        self.assertEqual(source_execution_proof(bloom), {"admitted": True, "reason": None})
        camera_callbacks = (examples / "showcase_camera_follows_path.py").read_text()
        self.assertFalse(source_execution_proof(camera_callbacks)["admitted"])
        self.assertTrue(source_is_scene_only(
            'from noon import MovingCameraScene\n'
            'class Example(MovingCameraScene):\n'
            '    def construct(self): self.wait(0.1)\n'))
        self.assertTrue(source_is_scene_only(
            'from noon import MovingCameraScene\n'
            'class Example(MovingCameraScene):\n'
            '    async def construct(self): await self.wait(0.1)\n'))

        unsafe_sources = (
            'from noon import *\nMovingCameraScene = Scene\n'
            'class Example(MovingCameraScene):\n    def construct(self): pass\n',
            'from manim import MovingCameraScene\n'
            'class Example(MovingCameraScene):\n    def construct(self): pass\n',
            'from manim import *\n'
            'class Example(MovingCameraScene):\n    def construct(self): pass\n',
            'from noon import MovingCameraScene as CameraScene\n'
            'class Example(CameraScene):\n    def construct(self): pass\n',
            'from noon import *\nclass Example(MovingCameraScene):\n'
            '    def construct(self): pass\n    def add(self): pass\n',
            'from noon import *\nclass Example(MovingCameraScene):\n'
            '    def __init__(self): pass\n    def construct(self): pass\n',
            'from noon import *\nclass Example(MovingCameraScene):\n    pass\n',
        )
        for source in unsafe_sources:
            with self.subTest(source=source):
                self.assertFalse(source_execution_proof(source)["admitted"])

    def test_coordinate_plot_uses_only_preparation_time_scalar_math(self):
        source = Path(__file__).with_name("examples").joinpath(
            "showcase_coordinate_plotting.py"
        ).read_text()
        self.assertEqual(source_execution_proof(source), {"admitted": True, "reason": None})

        for callback in ("lambda t: print(t)", "lambda t: self.wait(t)",
                         "lambda t: open(str(t))", "sin", "lambda t: context.sample(t)"):
            with self.subTest(callback=callback):
                self.assertFalse(source_is_scene_only(source.replace("lambda t: sin(0.8 * t)", callback)))
        self.assertFalse(source_is_scene_only(source.replace(
            "        model = axes.plot", "        sin = self.wait\n        model = axes.plot")))
        self.assertFalse(source_is_scene_only(source.replace(
            "        model = axes.plot", "        factory = axes.plot\n        model = factory")))

    def test_spatial_showcases_use_the_closed_three_d_scene_wrappers(self):
        examples = Path(__file__).with_name("examples")
        for filename in ("showcase_three_d_axes.py", "showcase_spatial_scene.py"):
            with self.subTest(filename=filename):
                self.assertEqual(source_execution_proof((examples / filename).read_text()),
                                 {"admitted": True, "reason": None})

        self.assertTrue(source_is_scene_only(dedent('''
            from noon import ThreeDScene, ThreeDAxes, Typst
            class Example(ThreeDScene):
                def construct(self):
                    axes = ThreeDAxes()
                    Typst("caption")
        ''')))
        self.assertTrue(source_is_scene_only(dedent('''
            from noon import Scene, Circle, Color
            class Example(Scene):
                def construct(self):
                    Color(0.1, 0.2, 0.3)
                    Circle()
        ''')))

        unsafe_sources = (
            # A wildcard import cannot authorize a subsequently shadowed base.
            ('from noon import *\nThreeDScene = Scene\n'
             'class Example(ThreeDScene):\n    def construct(self): pass\n'),
            ('from manim import ThreeDScene\nclass Example(ThreeDScene):\n'
             '    def construct(self): pass\n'),
            # Additional overrides could replace the wrapper's closed methods.
            ('from noon import *\nclass Example(ThreeDScene):\n'
             '    def construct(self): pass\n    def move_camera(self): pass\n'),
            # Spatial constructors are callable only directly in the proven class.
            ('from noon import *\nclass Example(ThreeDScene):\n'
             '    def construct(self):\n        factory = Mesh3D.parametric\n'),
            ('from noon import *\nclass Example(ThreeDScene):\n'
             '    def construct(self):\n'
             '        Mesh3D.parametric(lambda u, v: (self.wait(), v, 0), '
             'u_range=(0, 1), v_range=(0, 1), resolution=(2, 2), '
             'color=RED, point_lit=True)\n'),
            # The same wrapper names are not enabled on ordinary Scene classes.
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        ThreeDAxes()\n'),
        )
        for source in unsafe_sources:
            with self.subTest(source=source):
                self.assertFalse(source_execution_proof(source)["admitted"])

        for filename, names in (("showcase_spatial_scene.py", ("Mesh3D", "Color")),
                                ("showcase_three_d_axes.py", ("RED",))):
            spatial = (examples / filename).read_text()
            for shadowed in names:
                with self.subTest(filename=filename, shadowed=shadowed):
                    poisoned = spatial.replace(
                        "    async def construct(self):\n",
                        f"    async def construct(self):\n        {shadowed} = None\n",
                        1,
                    )
                    self.assertFalse(source_execution_proof(poisoned)["admitted"])
    def test_builtin_audio_source_is_confined_to_initial_scene_audio(self):
        source = dedent('''
            from noon import *

            class AudioWithOrdinaryPlay(Scene):
                def construct(self):
                    self.audio(
                        AudioSource.small_discovery(),
                        duration=3.0,
                        gain=0.5,
                        fade_in=0.5,
                        fade_out=0.5,
                    )
                    self.play(Create(Circle(radius=0.5)), run_time=1.0)
                    self.wait(2.0)
        ''').lstrip()
        self.assertEqual(sha256(source.encode()).hexdigest(),
                         "a898089e0534e31a5a149daaff4fec95ffc7ba5d108125d8ba93b4e252e5a60b")
        self.assertEqual(source_execution_proof(source), {"admitted": True, "reason": None})

        rejected = (
            # Module or local rebinding cannot change the trusted factory.
            'from noon import *\nAudioSource = Circle\n'
            'class Example(Scene):\n    def construct(self):\n'
            '        self.audio(AudioSource.small_discovery(), duration=3)\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self, AudioSource):\n'
            '        self.audio(AudioSource.small_discovery(), duration=3)\n',
            # Factory/member aliases and escaped resources are not a proof.
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        make = AudioSource.small_discovery\n'
            '        self.audio(make(), duration=3)\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        source = AudioSource.small_discovery()\n'
            '        self.audio(source, duration=3)\n',
            'from noon import *\nAudioSource.small_discovery()\n',
            # A source cannot add or replace audio after execution has started.
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        self.play(Create(Circle()), run_time=1)\n'
            '        self.audio(AudioSource.small_discovery(), duration=3)\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        if True:\n            self.play(Create(Circle()), run_time=1)\n'
            '        self.audio(AudioSource.small_discovery(), duration=3)\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        self.audio(AudioSource.small_discovery(channels=1), duration=3)\n',
        )
        for unsafe in rejected:
            with self.subTest(source=unsafe):
                self.assertFalse(source_execution_proof(unsafe)["admitted"])

    def test_matching_shapes_showcase_uses_only_proven_scene_data_calls(self):
        source = Path(__file__).with_name("examples").joinpath(
            "showcase_shape_matching.py"
        ).read_text()
        self.assertEqual(source_execution_proof(source), {"admitted": True, "reason": None})

        unsafe = dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    source, target = VGroup(Circle()), VGroup(Square())
                    self.play(TransformMatchingShapes(source, target, key_map=load_keys()))
        ''')
        proof = source_execution_proof(unsafe)
        self.assertFalse(proof["admitted"])
        self.assertEqual(proof["reason"]["code"], "unknown_name")

    def test_text_math_showcase_uses_shared_native_text_and_typst_resources(self):
        source = Path(__file__).with_name("examples").joinpath(
            "showcase_text_math.py"
        ).read_text()
        self.assertEqual(source_execution_proof(source), {"admitted": True, "reason": None})

        for constructor in ("MathTex", "prepare_latex"):
            with self.subTest(constructor=constructor):
                proof = source_execution_proof(
                    f"from noon import *\n{constructor}('x')\n"
                )
                self.assertFalse(proof["admitted"])
                self.assertEqual(proof["reason"]["code"], "unproven_text_compiler"
                                 if constructor == "MathTex" else "unproven_resource_preparation")

    def test_latex_showcase_requires_direct_controlled_preparation(self):
        source = Path(__file__).with_name("examples").joinpath(
            "showcase_latex_create.py"
        ).read_text()
        self.assertEqual(source_execution_proof(source), {"admitted": True, "reason": None})

        safe = '''from noon import *
class Example(Scene):
    async def construct(self):
        await prepare_latex()
        MathTex("x^2", font_size=24, color=BLUE, opacity=0.8)
'''
        self.assertEqual(source_execution_proof(safe), {"admitted": True, "reason": None})

        unsafe_sources = (
            # Preparation must be awaited before compilation in the same scope.
            safe.replace('        await prepare_latex()\n', ''),
            safe.replace('        await prepare_latex()', '        prepare_latex()'),
            safe.replace('        await prepare_latex()', '        await prepare_latex(context)'),
            safe.replace('        await prepare_latex()', '        await prep()').replace(
                'from noon import *', 'from noon import *\nprep = prepare_latex'
            ),
            safe.replace('await prepare_latex()', 'await prepare()').replace(
                'from noon import *', 'from noon import prepare_latex as prepare, MathTex'
            ),
            safe.replace('MathTex("x^2"', 'MathTex(context'),
            safe.replace('MathTex("x^2", font_size=24', 'MathTex("x^2", font_size=context'),
            safe.replace('await prepare_latex()', 'if True:\n            await prepare_latex()'),
            safe.replace('await prepare_latex()', 'await prepare_latex()\n        prepare_latex = None'),
            safe.replace('MathTex("x^2"', 'Compiler("x^2"').replace(
                'from noon import *', 'from noon import *\nCompiler = MathTex'
            ),
        )
        for unsafe in unsafe_sources:
            with self.subTest(source=unsafe):
                self.assertFalse(source_execution_proof(unsafe)["admitted"])

    def test_raster_image_source_requires_fresh_numeric_lists(self):
        source = Path(__file__).with_name("examples").joinpath(
            "showcase_raster_images.py"
        ).read_text()
        self.assertEqual(source_execution_proof(source), {"admitted": True, "reason": None})
        glass = Path(__file__).with_name("examples").joinpath(
            "showcase_glass_lens.py"
        ).read_text()
        self.assertEqual(source_execution_proof(glass), {"admitted": True, "reason": None})

        direct_pixels = '''from noon import *
class Example(Scene):
    def construct(self):
        pixels = [[[x, y, 0, 255] for x in range(2)] for y in range(3)]
        ImageMobject(pixels)
'''
        self.assertEqual(source_execution_proof(direct_pixels), {"admitted": True, "reason": None})

        for source in (
            'from noon import *\ndef pixels():\n    return "image.png"\n'
            'class Example(Scene):\n    def construct(self):\n        data = pixels()\n        ImageMobject(data)\n',
            'from noon import *\ndef landscape(width=1):\n'
            '    if width:\n        pixels = [[1]]\n    return pixels\n'
            'class Example(Scene):\n    def construct(self):\n        data = landscape()\n        ImageMobject(data)\n',
            'from noon import *\ndef pixels():\n    return [context]\n'
            'class Example(Scene):\n    def construct(self):\n        data = pixels()\n        ImageMobject(data)\n',
            'from noon import *\ndef pixels(flag=1):\n'
            '    if flag:\n        row = [1]\n    else:\n        row = [context]\n'
            '    return [row]\nclass Example(Scene):\n'
            '    def construct(self):\n        data = pixels()\n        ImageMobject(data)\n',
            'from noon import *\ndef pixels():\n    row = [1]\n    return [row]\n'
            'class Example(Scene):\n    def construct(self):\n        data = pixels()\n        ImageMobject(data)\n',
            'from noon import *\ndef pixels():\n    return [[1]]\n'
            'class Example(Scene):\n    def construct(self):\n        data = pixels()\n'
            '        pixels = lambda: "file.png"\n        ImageMobject(data)\n',
            'from noon import *\ndef pixels():\n    return [[[1, 2, 3, 255]]]\n'
            'class Example(Scene):\n    def construct(self):\n'
            "        images = [ImageMobject(pixels()) for pixels in [context.get('factory')]]\n",
            'from noon import *\ndef pixels():\n    return [[1]]\n'
            'class Example(Scene):\n    def construct(self):\n        factory = pixels\n'
            '        data = pixels()\n        factory()\n        ImageMobject(data)\n',
            'from noon import *\ndef pixels():\n    return [[1]]\n'
            'class Example(Scene):\n    def construct(self):\n        data = pixels()\n'
            '        alias = data\n        alias.append(context)\n        ImageMobject(data)\n',
            'from noon import *\nImageMobject("image.png")\n',
            'from noon import *\nImageMobject(context)\n',
            'from noon import *\nfactory = ImageMobject\n',
            'from noon import ImageMobject as Raster\n',
            'from noon import *\ndef pixels():\n    return [[1]]\n'
            'class Example(Scene):\n    def construct(self, ImageMobject):\n'
            '        data = pixels()\n        ImageMobject(data)\n',
            'from noon import *\ndef pixels():\n    return [[1]]\n'
            'class Example(Scene):\n    def construct(self):\n        data = pixels()\n'
            '        ImageMobject(*[data])\n',
            'from noon import *\ndef landscape():\n    return [[1]]\n'
            'class Example(Scene):\n    def construct(self):\n        pixels = landscape()\n'
            '        images = [ImageMobject(pixels) for pixels in ["file.png"]]\n',
            'from noon import *\ndef landscape():\n    return [[1]]\n'
            'class Example(Scene):\n    def construct(self):\n        pixels = landscape()\n'
            '        images = {0: ImageMobject(pixels) for pixels in ["file.png"]}\n',
            'from noon import *\ndef landscape():\n    return [[1]]\n'
            'class Example(Scene):\n    def construct(self):\n        pixels = landscape()\n'
            '        images = (ImageMobject(pixels) for pixels in ["file.png"])\n',
            'from noon import *\nImageMobject.from_url("https://example.test/image.png")\n',
            'from noon import *\ndef pixels():\n    return [[1]]\n'
            'class Example(Scene):\n    def construct(self):\n        data = pixels()\n'
            '        ImageMobject(data, height=context)\n',
            'from noon import *\ndef pixels():\n    return open("image.png")\n'
            'class Example(Scene):\n    def construct(self):\n        data = pixels()\n        ImageMobject(data)\n',
            direct_pixels.replace('        ImageMobject(pixels)\n',
                                  '        alias = pixels\n        ImageMobject(pixels)\n'),
            direct_pixels.replace('        ImageMobject(pixels)\n',
                                  '        pixels[0][0][0] = 7\n        ImageMobject(pixels)\n'),
            direct_pixels.replace('range(2)', 'range(1000000)'),
            direct_pixels.replace('pixels = [[[x, y, 0, 255] for x in range(2)] for y in range(3)]',
                                  'pixels = [[x, y, 0, 255] for x in range(2)]'),
        ):
            with self.subTest(source=source):
                self.assertFalse(source_execution_proof(source)["admitted"])

    def test_painted_image_uses_only_a_closed_local_pixel_expression(self):
        example = Path(__file__).with_name("examples").joinpath(
            "showcase_painted_surfaces.py"
        ).read_text()
        self.assertEqual(source_execution_proof(example), {"admitted": True, "reason": None})

        for source in (
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n        pixels = "texture.png"\n'
            '        paint = ImagePaint(pixels)\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n        palette = [(1, 2, 3)]\n'
            '        palette_alias = palette\n'
            '        pixels = [[[*palette_alias[0], 255] for x in range(1)] for y in range(1)]\n'
            '        paint = ImagePaint(pixels)\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n        palette = [(1, 2, 3)]\n'
            '        pixels = [[[*palette[0], 255] for x in range(1)] for y in range(1)]\n'
            '        palette.append((4, 5, 6))\n        paint = ImagePaint(pixels)\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n        pixels = [[[1, 2, 3, 255]]]\n'
            '        paint = ImagePaint(pixels, sampling="nearest")\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n        pixels = [[[1, 2, 3, 255]]]\n'
            '        paint = ImagePaint(*[pixels])\n',
        ):
            with self.subTest(source=source):
                self.assertFalse(source_execution_proof(source)["admitted"])

    def test_procedural_paint_examples_use_resource_free_rust_values(self):
        examples = Path(__file__).with_name("examples")
        for name in ("radial_gradient.py", "noise_paint.py", "hatch_paint.py"):
            with self.subTest(name=name):
                source = examples.joinpath(name).read_text()
                self.assertEqual(source_execution_proof(source), {"admitted": True, "reason": None})

        external_image = source_execution_proof(
            'from noon import *\n'
            'class Example(Scene):\n'
            '    def construct(self):\n'
            '        paint = ImagePaint("https://example.test/texture.png")\n'
        )
        self.assertFalse(external_image["admitted"])
        self.assertEqual(external_image["reason"]["code"], "unproven_image_paint_source")

    def test_shader_showcases_use_closed_wgsl_schemas_and_bound_parameter_keys(self):
        examples = Path(__file__).with_name("examples")
        for filename in ("showcase_shader_material.py", "showcase_wave_interference.py"):
            with self.subTest(filename=filename):
                self.assertEqual(source_execution_proof((examples / filename).read_text()),
                                 {"admitted": True, "reason": None})
        self.assertTrue(source_is_scene_only(dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    self.play(Rotate(Square(), angle=PI))
        ''')))

        unsafe_sources = (
            'from noon import *\nSOURCE = "fn shade() {}"\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        SOURCE = context\n'
            '        program = WgslEffectProgram(SOURCE, parameters=[])\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        program = WgslEffectProgram(open("shader.wgsl").read(), parameters=[])\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        program = WgslEffectProgram("fn shade() {}", parameters=[], authored_time=True)\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        program = WgslEffectProgram("fn shade() {}", parameters=[], seed=context)\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        program = WgslEffectProgram("fn shade() {}", parameters=['
            'ShaderParameter.scalar("phase", default=context, minimum=0, maximum=1)])\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        program = WgslEffectProgram("fn shade() {}", parameters=['
            'ShaderParameter.scalar("phase", default=0, minimum=0, maximum=1)])\n'
            '        key = program.parameter("other")\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        a = WgslEffectProgram("a", parameters=['
            'ShaderParameter.scalar("phase", default=0, minimum=0, maximum=1)])\n'
            '        b = WgslEffectProgram("b", parameters=['
            'ShaderParameter.scalar("phase", default=0, minimum=0, maximum=1)])\n'
            '        key = a.parameter("phase")\n'
            '        Rectangle().add_effect(ShaderEffect(b, parameters={key: 1}))\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        factory = WgslEffectProgram\n',
            'from noon import ShaderEffect as Apply\n',
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            '        paint = LinearGradient([(0, RED), (1, external_color())])\n',
        )
        for source in unsafe_sources:
            with self.subTest(source=source):
                self.assertFalse(source_execution_proof(source)["admitted"])

    def test_shader_program_and_key_collections_cannot_be_mutated_or_escape(self):
        source = Path(__file__).with_name("examples").joinpath(
            "showcase_shader_material.py"
        ).read_text()
        insertion = '        phase = [program.parameter("phase") for program in programs]\n'
        program_mutations = (
            '        programs.append(context.get("proxy"))\n',
            '        alias = programs\n        alias.append(context.get("proxy"))\n',
            '        programs.extend([material("scene")])\n',
            '        programs.insert(0, material("scene"))\n',
            '        programs.pop()\n',
            '        programs.clear()\n',
            '        programs.reverse()\n',
            '        programs.sort()\n',
            '        programs[0] = context.get("proxy")\n',
            '        alias = programs\n        alias[0] = context.get("proxy")\n',
            '        programs += [material("scene")]\n',
            '        programs = context.get("proxy")\n',
            '        programs = []\n',
            '        consume(programs)\n',
            '        consume(zip(programs))\n',
            '        zipped = zip(programs)\n',
            '        alias = programs if True else []\n        alias.append(context.get("proxy"))\n',
            '        if True:\n            alias = programs\n            alias.append(context.get("proxy"))\n',
            '        def expose():\n            return programs\n        alias = expose()\n'
            '        alias.append(context.get("proxy"))\n',
        )
        for mutation in program_mutations:
            with self.subTest(mutation=mutation):
                self.assertFalse(source_execution_proof(
                    source.replace(insertion, mutation + insertion, 1)
                )["admitted"])

        key_insertion = '        self.wait(0.5)\n'
        key_mutations = (
            '        phase.append(context.get("proxy"))\n',
            '        alias = phase\n        alias[0] = context.get("proxy")\n',
            '        phase += [phase[0]]\n',
        )
        for mutation in key_mutations:
            with self.subTest(mutation=mutation):
                self.assertFalse(source_execution_proof(
                    source.replace(key_insertion, mutation + key_insertion, 1)
                )["admitted"])

    def test_shader_zip_proof_requires_builtin_and_qualified_play_generator(self):
        source = Path(__file__).with_name("examples").joinpath(
            "showcase_shader_material.py"
        ).read_text()
        shadowed = source.replace(
            '        programs = [material("local"), material("scene")]\n',
            '        zip = self.add\n        programs = [material("local"), material("scene")]\n',
            1,
        )
        nested_escape = source.replace(
            "for panel, key in zip(panels, phase))",
            "for panel, key in zip(panels, zip(programs, phase)))",
            1,
        )
        wrong_generator = source.replace(
            "for panel, key in zip(panels, phase))",
            "for panel in zip(panels, programs))",
            1,
        )
        for candidate in (shadowed, nested_escape, wrong_generator):
            with self.subTest(candidate=candidate[-250:]):
                self.assertFalse(source_execution_proof(candidate)["admitted"])

        huge = "9" * 400
        overflow = (
            'from noon import *\nclass Example(Scene):\n'
            '    def construct(self):\n'
            f'        WgslEffectProgram("shader", parameters=[ShaderParameter.scalar("phase", '
            f'default={huge} * {huge}, minimum=0, maximum={huge})])\n'
        )
        proof = source_execution_proof(overflow)
        self.assertFalse(proof["admitted"])
        self.assertEqual(proof["reason"]["code"], "unproven_shader_schema")

    def test_ordered_reveal_showcase_uses_typed_subset_lifecycle_constructors(self):
        source = Path(__file__).with_name("examples").joinpath(
            "showcase_ordered_reveal.py"
        ).read_text()
        self.assertEqual(source_execution_proof(source), {"admitted": True, "reason": None})

        unsafe = dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    group = VGroup(Circle())
                    self.play(ShowIncreasingSubsets(group, int_func=load_int_func()))
        ''')
        proof = source_execution_proof(unsafe)
        self.assertFalse(proof["admitted"])
        self.assertEqual(proof["reason"]["code"], "unknown_name")

    def test_source_proof_reports_the_rejected_construct_and_exact_span(self):
        for expression, code, construct in (
            ('print("effect")', "unknown_name", "print"),
            ('circle.set_x(1)', "unsupported_member", "circle.set_x"),
            ('Code(code_file="input.py")', "unproven_resource_input", 'Code(code_file="input.py")'),
            ('__builtins__', "private_name", "__builtins__"),
        ):
            with self.subTest(expression=expression):
                source = f'from noon import *\ncircle = Circle()\n{expression}\n'
                proof = source_execution_proof(source)
                self.assertFalse(proof["admitted"])
                reason = proof["reason"]
                self.assertEqual(reason["code"], code)
                span = reason["span"]
                self.assertEqual(span["line"], 3)
                self.assertEqual(span["endLine"], 3)
                self.assertEqual(source.splitlines()[2].encode()[span["column"]:span["endColumn"]].decode(), construct)
        self.assertEqual(source_execution_proof('from noon import *\nCircle()'),
                         {"admitted": True, "reason": None})
        syntax = source_execution_proof('π =\n')["reason"]
        self.assertEqual(syntax["code"], "invalid_syntax")
        self.assertEqual(syntax["span"]["line"], 1)
        self.assertGreaterEqual(syntax["span"]["column"], len('π'.encode()))

    def test_module_binding_and_owned_write_failures_keep_their_origin(self):
        for source, code, line in (
            ('print = None\n', "unproven_binding", 1),
            ('from noon import *\nversion = 3\n', "unproven_wildcard_binding", 2),
            ('from noon import *\nvalues = [1]\nvalues[0] = 2\n', "unproven_ownership", 3),
        ):
            with self.subTest(source=source):
                reason = source_execution_proof(source)["reason"]
                self.assertEqual(reason["code"], code)
                self.assertEqual(reason["span"]["line"], line)

    def test_lesson_navigation_uses_the_shared_prepared_cue_contract(self):
        self.assertTrue(source_is_scene_only(dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    self.wait(1)
                    self.next_section("Detail")
                    self.wait(1)
        ''')))

    def test_closed_noon_operations_and_owned_helper_alias_remain_admissible(self):
        self.assertTrue(source_is_scene_only(dedent('''
            from noon import *
            import math
            def offset(value):
                return math.sin(value) * RIGHT
            class Example(Scene):
                def construct(self):
                    circle = Circle(radius=0.5)
                    self.add(circle)
                    self.wait(1)
                    calculate = offset
                    for index in range(3):
                        circle.shift(calculate(index))
                    self.wait(1)
        ''')))

    def test_divmod_is_an_ordinary_builtin_with_module_rebinding_rejected(self):
        self.assertTrue(source_is_scene_only(dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    row, column = divmod(17, 4)
                    self.wait(row + column)
        ''')))
        for source, code in (
            ('from noon import *\ndivmod = print\n', "unproven_binding"),
            ('from noon import *\nfrom math import divmod\n', "unproven_binding"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        divmod = print\n', "unknown_name"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        divide = print\n        divide(17, 4)\n',
             "unknown_name"),
        ):
            with self.subTest(source=source):
                proof = source_execution_proof(source)
                self.assertFalse(proof["admitted"])
                self.assertEqual(proof["reason"]["code"], code)

    def test_discard_store_is_local_and_never_read_or_mutated(self):
        self.assertTrue(source_is_scene_only(dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    quotient, _ = divmod(17, 4)
                    self.wait(quotient)
        ''')))
        for source, code in (
            ('from noon import *\n_ = 1\n', "unproven_binding"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        _ = 1\n        self.wait(_)\n', "private_name"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        _other = 1\n', "private_name"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        _ += 1\n', "private_name"),
            ('from noon import *\nclass Example(Scene):\n'
             '    _ = 1\n    def construct(self):\n        pass\n', "unsupported_class_contract"),
            ('from noon import *\ndef helper():\n    global _\n    _ = 1\n',
             "unproven_scope_effect"),
            ('from noon import *\ndef outer():\n    _ = 1\n    def inner():\n'
             '        nonlocal _\n        _ = 2\n', "unproven_scope_effect"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        values = context\n        values[0] = 2\n',
             "unproven_ownership"),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n        values = [1]\n        alias = values\n'
             '        alias[0] = 2\n', "unproven_ownership"),
        ):
            with self.subTest(source=source):
                proof = source_execution_proof(source)
                self.assertFalse(proof["admitted"])
                self.assertEqual(proof["reason"]["code"], code)

    def test_subscript_writes_require_fresh_local_container_bindings(self):
        self.assertTrue(source_is_scene_only(dedent('''
            from noon import *
            def trace():
                tile, loads = {}, []
                tile[0] = 7
                output = [None] * 4
                output[0] = tile[0]
                counts = [0 for _ in range(4)]
                counts[0] += 1
                return output
            class Example(Scene):
                def construct(self):
                    self.wait(trace()[0])
        ''')))
        for body in (
            'data = context; data[0] = 1',
            'data = []; alias = data; alias[0] = 1',
            'data = []; data = context; data[0] = 1',
            'data = [];\n        if True: data = context\n        data[0] = 1',
            'data = [[]]; data[0][0] = 1',
            'data = [0] * self; data[0] = 1',
            'data = [];\n        match context:\n            case data: data[0] = 1',
            'data = [];\n        try: self.wait(0)\n        except ValueError as data: data[0] = 1',
        ):
            with self.subTest(body=body):
                proof = source_execution_proof('from noon import *\nclass Example(Scene):\n'
                    '    def construct(self):\n        ' + body + '\n')
                self.assertFalse(proof['admitted'])
                self.assertEqual(proof['reason']['code'], 'unproven_ownership')
        for source in (
            'from noon import *\ndata = []; data[0] = 1\n',
            'from noon import *\ndef edit(data):\n    data[0] = 1\n',
            'from noon import *\ndef edit(data):\n    data[0] = 1\n    data = []\n',
            'from noon import *\ndef outer():\n    data = []\n'
            '    def edit(data):\n        data[0] = 1\n',
        ):
            with self.subTest(source=source):
                self.assertFalse(source_is_scene_only(source))

    def test_list_repetition_requires_a_dominating_builtin_integer_guard(self):
        template = 'from noon import *\ndef histogram(bins):\n%s\n    counts = [0] * bins\n    counts[0] += 1\n'
        self.assertTrue(source_is_scene_only(template %
            '    if isinstance(bins, bool) or not isinstance(bins, int) or bins < 1:\n        raise ValueError("bins")'))
        for guard in (
            '    if bins < 1 or not isinstance(bins, int):\n        raise ValueError("bins")',
            '    if bins.count() or not isinstance(bins, int):\n        raise ValueError("bins")',
            '    if not isinstance(bins, int) and bins < 1:\n        raise ValueError("bins")',
            '    if not isinstance(bins, int):\n        pass',
            '    if not isinstance(bins, int):\n        counts = [0] * bins\n        raise ValueError("bins")',
            '    bins = context\n    if not isinstance(bins, int):\n        raise ValueError("bins")',
            '    isinstance = lambda value, kind: False\n    if not isinstance(bins, int):\n        raise ValueError("bins")',
        ):
            with self.subTest(guard=guard):
                self.assertFalse(source_is_scene_only(template % guard))
        self.assertFalse(source_is_scene_only('from noon import *\n'
            'def outer(isinstance):\n    def histogram(bins):\n'
            '        if not isinstance(bins, int):\n            raise ValueError("bins")\n'
            '        counts = [0] * bins\n        counts[0] = 1\n'))

    def test_unknown_host_work_and_dispatch_cannot_be_proven(self):
        for statement in (
            'print("external work")', 'sink = print; sink("external work")',
            'getattr(circle, "shift")(RIGHT)', 'circle.__class__',
            'circle.shift = lambda x: None', 'RIGHT[0] = 5',
            'from js import postMessage', 'import os', 'from noon import np',
            'str.format("{0.__class__}", circle)',
            'def generic[T: print("external work")](): pass',
        ):
            with self.subTest(statement=statement):
                self.assertFalse(source_is_scene_only('from noon import *\n'
                    'class Example(Scene):\n    def construct(self):\n'
                    '        circle = Circle()\n        self.wait(1)\n'
                    + '\n'.join('        ' + line for line in statement.splitlines())))

    def test_nested_bindings_do_not_hide_an_outer_builtin_effect(self):
        for hidden in ('def helper(print):\n    pass',
                       'unused = [print for print in ()]',
                       'class Unrelated(Scene):\n    def helper(self, print):\n        pass'):
            with self.subTest(hidden=hidden):
                self.assertFalse(source_is_scene_only('from noon import *\n' + hidden
                    + '\nclass Example(Scene):\n    def construct(self):\n'
                    '        self.wait(1)\n        print("external work")\n'))
        self.assertFalse(source_is_scene_only('print("external work")\nprint = Circle\n'))
        self.assertFalse(source_is_scene_only('from noon import *\n'
            'version("package")\nversion = None\n'))

    def test_user_destructors_and_definition_hooks_keep_observed_execution(self):
        for method in ('__del__', '__getattribute__', '__getattr__', '__init_subclass__'):
            with self.subTest(method=method):
                self.assertFalse(source_is_scene_only(f'from noon import *\n'
                    f'class Example(Scene):\n    def {method}(self):\n        pass\n'
                    '    def construct(self):\n        self.wait(1)\n'))

    def test_code_file_input_is_not_hidden_by_the_noon_constructor_or_alias(self):
        for expression in ('Code("input.py")', 'Code(code_file="input.py")',
                           'Code(code_string="x", **options)', 'factory = Code',
                           'Display(code_file="input.py")'):
            with self.subTest(expression=expression):
                self.assertFalse(source_is_scene_only('from noon import *\n'
                    'from noon import Code as Display\noptions = {}\n' + expression))
        self.assertTrue(source_is_scene_only('from noon import Code as Display\n'
            'display = Display(code_string="print(1)", language="python")\n'))

    def test_compound_suite_imports_keep_observed_execution(self):
        for statement in ('from noon import Code as Display', 'import math'):
            with self.subTest(statement=statement):
                expression = 'Display(code_file="input.py")' if statement.startswith('from') else 'math.sin(1)'
                self.assertFalse(source_is_scene_only('from noon import Scene\n'
                    'if True:\n    ' + statement + '\n'
                    'class Example(Scene):\n    def construct(self):\n'
                    '        self.wait(1)\n        ' + expression + '\n'))

    def test_later_special_namespace_binding_cannot_admit_the_original_builtins(self):
        self.assertFalse(source_is_scene_only(dedent('''
            from noon import *
            saved = __builtins__
            __builtins__ = saved
            class Example(Scene):
                def construct(self):
                    self.wait(1)
                    saved["print"]("opaque work")
        ''')))
        self.assertFalse(source_is_scene_only('from noon import Code as __builtins__\n'))

    def test_unchanged_gallery_and_stress_sources_use_the_closed_subset(self):
        examples = Path(__file__).parent / 'examples'
        for name in ('manim_parity_create_circle.py', 'showcase_code_blocks.py',
                     'showcase_gpu_glow.py', 'showcase_dynamic_scene.py',
                     'manim_parity_stress_grid.py', 'showcase_layered_effects.py'):
            with self.subTest(name=name):
                self.assertTrue(source_is_scene_only((examples / name).read_text()))

    def test_affine_lifecycle_wrappers_are_trusted_as_exact_closed_imports(self):
        examples = Path(__file__).parent / 'examples'
        for name in ('manim_parity_affine_lifecycle.py', 'showcase_entrances_exits.py'):
            with self.subTest(name=name):
                source = (examples / name).read_text()
                self.assertTrue(source_is_scene_only(source))

        alias = dedent('''
            from noon import *
            intro = SpinInFromNothing
            class Example(Scene):
                def construct(self):
                    square = Square()
                    self.play(intro(square))
                    self.play(ShrinkToCenter(square))
        ''')
        self.assertTrue(source_is_scene_only(alias))

        for source, code, span_text in (
            ('from noon import *\nSpinInFromNothing = print\n',
             'unknown_name', 'print'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        SpinInFromNothing.__call__ = Circle\n',
             'unproven_member_write', 'SpinInFromNothing.__call__'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        SpinInFromNothing(context.target)\n',
             'unsupported_member', 'context.target'),
        ):
            with self.subTest(source=source):
                reason = source_execution_proof(source)['reason']
                self.assertEqual(reason['code'], code)
                span = reason['span']
                self.assertEqual(source.splitlines()[span['line'] - 1].encode()
                                 [span['column']:span['endColumn']].decode(), span_text)

    def test_lagged_start_map_requires_proven_factories_and_scene_only_callbacks(self):
        examples = Path(__file__).parent / 'examples'
        timing = (examples / 'showcase_timing.py').read_text()
        self.assertTrue(source_is_scene_only(timing))

        alias = dedent('''
            from noon import *
            animation_factory = FadeIn
            class Example(Scene):
                def construct(self):
                    group = VGroup(Square(), Square())
                    self.play(LaggedStartMap(
                        animation_factory, group,
                        arg_creator=lambda member: (member,),
                    ))
        ''')
        alias_proof = source_execution_proof(alias)
        self.assertFalse(alias_proof["admitted"])
        self.assertEqual(alias_proof["reason"]["code"], "unproven_callback_factory")

        identity = dedent('''
            from noon import *
            class Example(Scene):
                def construct(self):
                    group = VGroup(Square(), Square())
                    self.play(LaggedStartMap(FadeIn, group, arg_creator=lambda member: member))
        ''')
        self.assertTrue(source_is_scene_only(identity))

        blocked = (
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(Code, group))\n',
             'unproven_resource_reference', 'Code'),
            ('from noon import *\ndef make(member):\n'
             '    return Code(code_file="input.py")\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, arg_creator=make))\n',
             'unproven_resource_input', 'Code(code_file="input.py")'),
            ('from noon import *\nimport os as host\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(host.system, group))\n',
             'unsupported_import', 'import os as host'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, '
             'arg_creator=lambda member: print(member)))\n',
             'unproven_callback_argument', 'lambda member: print(member)'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, '
             'arg_creator=lambda member: (self.wait(0.1) and member,)))\n',
             'unproven_callback_argument',
             'lambda member: (self.wait(0.1) and member,)'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, arg_creator=self.wait))\n',
             'unproven_callback_argument', 'self.wait'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, arg_creator=self.play))\n',
             'unproven_callback_argument', 'self.play'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, '
             'lambda member: (member, Code(code_file="input.py"))))\n',
             'unproven_callback_argument', 'lambda member: (member, Code(code_file="input.py"))'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, self.wait))\n',
             'unproven_callback_argument', 'self.wait'),
            ('from noon import *\nfactory = LaggedStartMap\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(factory(FadeIn, group, arg_creator=self.wait))\n',
             'unproven_callback_factory', 'LaggedStartMap'),
            ('from noon import *\nfactory = LaggedStartMap\nwrapped = factory\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(wrapped(FadeIn, group, arg_creator=self.play))\n',
             'unproven_callback_factory', 'LaggedStartMap'),
            ('from noon import LaggedStartMap as Make\nfrom noon import *\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(Make(FadeIn, group, arg_creator=self.wait))\n',
             'unproven_callback_factory', 'from noon import LaggedStartMap as Make'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(LaggedStartMap, group))\n',
             'unproven_callback_factory', 'LaggedStartMap'),
            ('from noon import *\nanimation_factory = FadeIn\n'
             'def helper(animation_factory, group):\n'
             '    return LaggedStartMap(animation_factory, group, '
             'arg_creator=lambda member: (member,))\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(helper(self.wait, group))\n',
             'unproven_callback_factory', 'animation_factory'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(*[FadeIn, group]))\n',
             'unproven_callback_arguments', 'LaggedStartMap(*[FadeIn, group])'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(self.wait, group))\n',
             'unproven_callback_factory', 'self.wait'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self, LaggedStartMap):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group))\n',
             'unproven_shadowed_constructor', 'LaggedStartMap'),
            ('from noon import *\n'
             'def FadeIn(member):\n    member.wait(0.1)\n    return Circle()\n'
             'animation_factory = FadeIn\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(animation_factory, group))\n',
             'unproven_callback_factory', 'animation_factory'),
            ('from noon import *\nanimation_factory = FadeIn\n'
             'match {"x": ValueTracker}:\n'
             '    case {"x": animation_factory}:\n        pass\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(animation_factory, group))\n',
             'unproven_callback_factory', 'animation_factory'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(Scene, group))\n',
             'unproven_callback_factory', 'Scene'),
            ('from noon import *\nclass Example(Scene):\n'
             '    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        animation_factory = self.play\n'
             '        self.play(LaggedStartMap(animation_factory, group))\n',
             'unproven_callback_factory', 'animation_factory'),
            ('from noon import *\n'
             'def make(scene, member):\n    scene.wait(0.1)\n    return (member,)\n'
             'class Example(Scene):\n    def construct(self):\n'
             '        group = VGroup(Square())\n'
             '        self.play(LaggedStartMap(FadeIn, group, arg_creator=make))\n',
             'unproven_callback_argument', 'make'),
        )
        for source, code, span_text in blocked:
            with self.subTest(source=source):
                reason = source_execution_proof(source)['reason']
                self.assertEqual(reason['code'], code)
                span = reason['span']
                self.assertEqual(source.splitlines()[span['line'] - 1].encode()
                                 [span['column']:span['endColumn']].decode(), span_text)

        # This paired showcase uses only already-qualified map/composition
        # structure; its separate tutorial pair remains blocked at Add.
        paired = (examples / 'ordinary_timed_composition.py').read_text()
        reason = source_execution_proof(paired)['reason']
        self.assertEqual(reason['code'], 'unknown_name')
        self.assertEqual(reason['span']['line'], 11)


if __name__ == '__main__':
    unittest.main()

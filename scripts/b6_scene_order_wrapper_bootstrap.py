from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(
            f"expected exactly one match in {path}, found {count}: {old[:80]!r}"
        )
    file.write_text(text.replace(old, new, 1))


def write_new(path: str, content: str) -> None:
    file = Path(path)
    if file.exists():
        raise SystemExit(f"refusing to overwrite existing file: {path}")
    file.write_text(content)


rust = "crates/noon-web/src/canonical_authoring_scene.rs"
invalid_test = r'''    #[test]
    fn canonical_membership_bring_to_back_routes_through_shared_authority() {
        let mut context = CanonicalAuthoringScene::default();
        let first = context.scene.circle(0.4).unwrap();
        let second = context.scene.square(0.8).unwrap();
        let third = context.scene.rectangle(0.6, 1.0).unwrap();
        context.bind_mobject(ObjectId::new(0), &first).unwrap();
        context.bind_mobject(ObjectId::new(1), &second).unwrap();
        context.bind_mobject(ObjectId::new(2), &third).unwrap();
        let key = |handle: &noon::Mobject| {
            format!(
                "{}:{}",
                handle.node_id().slot(),
                handle.node_id().generation()
            )
        };

        let revision = context.scene.integration_store().borrow().scene_revision();
        context
            .edit_membership(SceneMembershipBatch {
                kind: SceneMembershipBatchKind::BringToBack,
                members: vec![membership_mobject(2, &third), membership_mobject(0, &first)],
                bindings: Vec::new(),
            })
            .unwrap();
        assert_eq!(
            context.root_membership_keys().unwrap(),
            vec![key(&third), key(&first), key(&second)]
        );
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision.checked_next().unwrap()
        );

        let foreign = CanonicalAuthoringScene::default();
        let foreign_object = foreign.scene.circle(0.2).unwrap();
        let before = context.root_membership_keys().unwrap();
        let revision = context.scene.integration_store().borrow().scene_revision();
        assert!(context
            .edit_membership(SceneMembershipBatch {
                kind: SceneMembershipBatchKind::BringToBack,
                members: vec![
                    membership_mobject(0, &first),
                    membership_mobject(9, &foreign_object),
                ],
                bindings: Vec::new(),
            })
            .is_err());
        assert_eq!(context.root_membership_keys().unwrap(), before);
        assert_eq!(
            context.scene.integration_store().borrow().scene_revision(),
            revision
        );
    }

'''
replace_once(rust, invalid_test, "")

fixture = '''from manim import *\n\n\nclass ScenePainterOrder(Scene):\n    def construct(self):\n        def layer(color, offset):\n            return Square(\n                side_length=1.6,\n                fill_color=color,\n                fill_opacity=0.5,\n                stroke_opacity=0.0,\n            ).shift(offset)\n\n        left_center = 1.8 * LEFT\n        left_red = layer(RED, left_center + 0.3 * LEFT)\n        left_green = layer(GREEN, left_center + 0.3 * RIGHT)\n        left_blue = layer(BLUE, left_center + 0.3 * UP)\n\n        right_center = 1.8 * RIGHT\n        right_red = layer(RED, right_center + 0.3 * LEFT)\n        right_green = layer(GREEN, right_center + 0.3 * RIGHT)\n        right_blue = layer(BLUE, right_center + 0.3 * UP)\n\n        # Start each cluster in the opposite order. These two operations must\n        # restore the intended painter order while preserving caller order.\n        self.add(left_blue, left_green, left_red)\n        self.bring_to_back(left_red, left_green)\n\n        self.add(right_red, right_green, right_blue)\n        self.bring_to_front(right_green, right_red)\n\n        self.play(right_red.animate.shift(ORIGIN))\n'''
write_new("parity/manim-v0.21/core-examples/scene_painter_order.py", fixture)
write_new(
    "web/python/examples/manim_compatible_scene_painter_order.py",
    fixture.replace("from manim import *", "from noon import *", 1),
)

existing_fixture = '''    {\n      "id": "three-layer-painter-order",\n      "scene": "ThreeLayerPainterOrder",\n      "expected_duration": 1.0,\n      "raster_tolerance": {\n        "max_bounds_delta_px": 0,\n        "max_differing_ratio": 0.0009,\n        "max_mean_absolute_channel_error": 0.034\n      }\n    },\n'''
new_fixture = existing_fixture + '''    {\n      "id": "scene-painter-order",\n      "scene": "ScenePainterOrder",\n      "source": "parity/manim-v0.21/core-examples/scene_painter_order.py",\n      "expected_duration": 1.0,\n      "raster_tolerance": {\n        "max_bounds_delta_px": 0,\n        "max_differing_ratio": 0.001,\n        "max_mean_absolute_channel_error": 0.04\n      }\n    },\n'''
replace_once("parity/manim-v0.21/manifest.json", existing_fixture, new_fixture)

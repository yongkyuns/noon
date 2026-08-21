from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    file = Path(path)
    text = file.read_text()
    if old not in text:
        raise SystemExit(f"expected text missing from {path}: {old[:120]!r}")
    file.write_text(text.replace(old, new, 1))


# The browser editor owns stable semantic identities, but Rust owns semantic
# scene reconciliation. A full Python rerun must not duplicate the scene diff in
# JavaScript and then issue hundreds of structural patches one-by-one.
replace_once(
    "web/main.js",
    'import { diffSceneDocuments, SceneIdentityMap } from "./scene-identity.js";\n',
    'import { SceneIdentityMap } from "./scene-identity.js";\n',
)
replace_once("web/main.js", "  let authoredScene = null;\n", "")
replace_once("web/main.js", "        authoredScene = null;\n", "")
replace_once(
    "web/main.js",
    '''      const patches =\n        authoredScene === null\n          ? null\n          : diffSceneDocuments(authoredScene, stableDocument);\n      let operation;\n      if (patches === null) {\n        const incremental = player.reconcileScene(JSON.stringify(stableDocument));\n        operation = incremental ? "Scene reconciled" : "Scene replaced safely";\n      } else if (patches.length > 0) {\n        const sequence = Number(player.nextSequence());\n        if (!Number.isSafeInteger(sequence)) {\n          throw new Error("Patch sequence exceeds JavaScript's safe integer range");\n        }\n        player.applyPatchBatch(\n          JSON.stringify({ version: 1, sequence, patches }),\n        );\n        operation = `Scene reconciled with ${patches.length} patch${patches.length === 1 ? "" : "es"}`;\n      } else {\n        operation = "Scene already current";\n      }\n      authoredScene = stableDocument;\n''',
    '''      const incremental = player.reconcileScene(JSON.stringify(stableDocument));\n      const operation = incremental\n        ? "Scene updated incrementally"\n        : "Scene rebuilt atomically";\n''',
)
replace_once("web/main.js", "      authoredScene = null;\n", "")

# The identity layer now has one job: stabilize authoring-local IDs across
# complete Python reruns. Semantic equality belongs to Rust, where the actual IR
# types live and evolve.
Path("web/scene-identity.js").write_text(r'''export class SceneIdentityMap {
  #objects = new IdentityNamespace("object");
  #tracks = new IdentityNamespace("track");

  stabilize(document, identities) {
    const objectIds = this.#objects.resolve(identities.objects);
    const trackIds = this.#tracks.resolve(identities.tracks);
    return {
      ...document,
      objects: document.objects.map((object) => {
        const id = requiredId(objectIds, object.id, "object");
        return id === object.id ? object : { ...object, id };
      }),
      tracks: document.tracks.map((track) => {
        const id = requiredId(trackIds, track.id, "track");
        const object = requiredId(objectIds, track.object, "track object");
        return id === track.id && object === track.object
          ? track
          : { ...track, id, object };
      }),
    };
  }
}

class IdentityNamespace {
  #kind;
  #keyToId = new Map();
  #idToKey = new Map();
  #nextId = 0;

  constructor(kind) {
    this.#kind = kind;
  }

  resolve(entries) {
    const localToStable = new Map();
    for (const { id: localId, key } of entries) {
      let stableId = this.#keyToId.get(key);
      if (stableId === undefined) {
        stableId = this.#claim(localId, key);
      }
      localToStable.set(localId, stableId);
    }
    return localToStable;
  }

  #claim(preferredId, key) {
    let id = preferredId;
    if (this.#idToKey.has(id)) {
      id = this.#nextId;
      while (this.#idToKey.has(id)) {
        id += 1;
      }
    }
    if (!Number.isSafeInteger(id)) {
      throw new Error(`No safe ${this.#kind} identity IDs remain`);
    }
    this.#keyToId.set(key, id);
    this.#idToKey.set(id, key);
    this.#nextId = Math.max(this.#nextId, id + 1);
    return id;
  }
}

function requiredId(ids, localId, kind) {
  const stableId = ids.get(localId);
  if (stableId === undefined) {
    throw new Error(`Scene ${kind} ${localId} has no authoring identity`);
  }
  return stableId;
}
''')

Path("web/scene-identity.test.mjs").write_text(r'''import assert from "node:assert/strict";
import test from "node:test";

import { SceneIdentityMap } from "./scene-identity.js";

test("preserves runtime IDs when Python insertion order changes", () => {
  const identities = new SceneIdentityMap();
  const first = identities.stabilize(
    { version: 1, objects: [{ id: 0 }, { id: 1 }], tracks: [] },
    {
      objects: [
        { id: 0, key: "circle" },
        { id: 1, key: "line" },
      ],
      tracks: [],
    },
  );
  const second = identities.stabilize(
    { version: 1, objects: [{ id: 0 }, { id: 1 }, { id: 2 }], tracks: [] },
    {
      objects: [
        { id: 0, key: "new" },
        { id: 1, key: "circle" },
        { id: 2, key: "line" },
      ],
      tracks: [],
    },
  );

  assert.deepEqual(first.objects.map(({ id }) => id), [0, 1]);
  assert.deepEqual(second.objects.map(({ id }) => id), [2, 0, 1]);
});

test("rewrites track IDs and object references by stable keys", () => {
  const identities = new SceneIdentityMap();
  identities.stabilize(
    {
      version: 1,
      objects: [{ id: 0 }],
      tracks: [{ id: 0, object: 0 }],
    },
    {
      objects: [{ id: 0, key: "hero" }],
      tracks: [{ id: 0, key: "hero.move" }],
    },
  );
  const result = identities.stabilize(
    {
      version: 1,
      objects: [{ id: 0 }, { id: 1 }],
      tracks: [{ id: 0, object: 1 }, { id: 1, object: 0 }],
    },
    {
      objects: [
        { id: 0, key: "other" },
        { id: 1, key: "hero" },
      ],
      tracks: [
        { id: 0, key: "hero.move" },
        { id: 1, key: "other.move" },
      ],
    },
  );

  assert.deepEqual(result.tracks, [
    { id: 0, object: 0 },
    { id: 1, object: 1 },
  ]);
});

function keyedScene(count) {
  return {
    document: {
      version: 1,
      objects: Array.from({ length: count }, (_, id) => ({ id })),
      tracks: [],
    },
    identities: {
      objects: Array.from({ length: count }, (_, id) => ({ id, key: `dot.${id}` })),
      tracks: [],
    },
  };
}

test("grid expansion and shrink preserve surviving semantic IDs", () => {
  const identities = new SceneIdentityMap();
  const initial = keyedScene(180);
  const expanded = keyedScene(200);
  const shrunk = keyedScene(96);

  const first = identities.stabilize(initial.document, initial.identities);
  const second = identities.stabilize(expanded.document, expanded.identities);
  const third = identities.stabilize(shrunk.document, shrunk.identities);

  assert.deepEqual(
    second.objects.slice(0, 180).map(({ id }) => id),
    first.objects.map(({ id }) => id),
  );
  assert.deepEqual(
    third.objects.map(({ id }) => id),
    first.objects.slice(0, 96).map(({ id }) => id),
  );
  assert.deepEqual(second.objects.slice(180).map(({ id }) => id), [180, 181, 182, 183, 184, 185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199]);
});
''')

# Structural patch batches are atomic: mutate a semantic clone, compile the final
# scene once, construct runtime state once, seek once, then commit the transaction.
replace_once(
    "crates/noon-web/src/lib.rs",
    '''pub enum ReconcileOutcome {\n    Incremental { patch_count: usize },\n    Replaced,\n}\n''',
    '''pub enum ReconcileOutcome {\n    Incremental { patch_count: usize },\n    Rebuilt { patch_count: usize },\n    Replaced,\n}\n''',
)
replace_once(
    "crates/noon-web/src/lib.rs",
    '''        let patch_count = patches.len();\n        self.apply_patches_transactionally(&patches)?;\n        self.next_sequence = 0;\n        Ok(ReconcileOutcome::Incremental { patch_count })\n''',
    '''        let patch_count = patches.len();\n        let value_only = patches.iter().all(is_value_patch);\n        self.apply_patches_transactionally(&patches)?;\n        self.next_sequence = 0;\n        Ok(if value_only {\n            ReconcileOutcome::Incremental { patch_count }\n        } else {\n            ReconcileOutcome::Rebuilt { patch_count }\n        })\n''',
)
replace_once(
    "crates/noon-web/src/lib.rs",
    '''        let mut definition = self.definition.clone();\n        let mut instance = self.instance.clone();\n        for patch in patches {\n            definition.apply_patch(patch.clone())?;\n            instance.apply_patch(patch)?;\n        }\n        self.definition = definition;\n        self.instance = instance;\n        Ok(())\n''',
    '''        let playhead = self.instance.frame().time;\n        let mut definition = self.definition.clone();\n        for patch in patches {\n            definition.apply_patch(patch.clone())?;\n        }\n        let compiled = CompiledScene::compile(&definition)?;\n        let mut instance = SceneInstance::new(compiled);\n        instance.seek(playhead)?;\n\n        self.definition = definition;\n        self.instance = instance;\n        Ok(())\n''',
)
replace_once(
    "crates/noon-web/src/lib.rs",
    '''    use noon_core::{GeometryRef, ObjectId, ScenePatch, Style, Transform2D, Vec2};\n''',
    '''    use noon_core::{\n        Easing, GeometryRef, ObjectDefinition, ObjectId, ObjectSnapshot, Property, ScenePatch,\n        StrokeCap, StrokeJoin, Style, TrackDefinition, TrackId, TrackTiming, TrackValues,\n        Transform2D, Vec2,\n    };\n''',
)

lib = Path("crates/noon-web/src/lib.rs")
text = lib.read_text()
insert_at = text.rfind("\n}")
if insert_at < 0:
    raise SystemExit("test module closing brace not found")
new_tests = r'''

    fn grid_scene(columns: usize, rows: usize) -> SceneDefinition {
        let mut scene = SceneDefinition::new();
        for row in 0..rows {
            for column in 0..columns {
                let x = column as f32 * 0.1;
                let y = row as f32 * 0.1;
                let object = scene.add(GeometryRef::circle(0.05));
                scene.object_mut(object).expect("grid object exists").transform.translation =
                    Vec2::new(x, y);
                scene
                    .animate_position(
                        object,
                        Vec2::new(x, y),
                        Vec2::new(x * 0.8 - y * 0.1, y * 0.8 + x * 0.1),
                        TrackTiming::new(0.0, 3.0, Easing::EaseInOutCubic),
                    )
                    .expect("grid track must be valid");
            }
        }
        scene
    }

    #[test]
    fn dense_grid_edit_rebuilds_atomically_and_preserves_playhead() {
        let initial = grid_scene(18, 10);
        let json = encode_scene(&initial).expect("initial grid must serialize");
        let mut player = ScenePlayer::from_scene_json(&json).expect("grid must load");
        player.seek(1.75).expect("seek must succeed");

        let desired = grid_scene(20, 10);
        let json = encode_scene(&desired).expect("expanded grid must serialize");
        let outcome = player
            .reconcile_scene_json(&json)
            .expect("grid reconciliation must succeed");

        let ReconcileOutcome::Rebuilt { patch_count } = outcome else {
            panic!("dense structural edit must use one atomic rebuild: {outcome:?}");
        };
        assert!(patch_count > 180, "grid edit should contain many semantic changes");
        assert_eq!(player.object_count(), 200);
        assert_eq!(player.frame().time, 1.75);
        assert_eq!(player.next_sequence(), 0);
    }

    #[test]
    fn no_op_scene_rerun_remains_incremental_without_mutation() {
        let mut player = player();
        player.seek(0.625).expect("seek must succeed");
        let json = player.scene_json().expect("scene must serialize");
        assert_eq!(
            player
                .reconcile_scene_json(&json)
                .expect("no-op reconcile must succeed"),
            ReconcileOutcome::Incremental { patch_count: 0 }
        );
        assert_eq!(player.frame().time, 0.625);
    }

    #[test]
    fn join_and_cap_only_edit_uses_value_only_reconciliation() {
        let mut player = player();
        let mut desired = SceneDefinition::new();
        let object = desired.add(GeometryRef::circle(1.0));
        desired.object_mut(object).expect("object exists").style = Style {
            stroke_join: StrokeJoin::Bevel,
            stroke_cap: StrokeCap::Square,
            ..Style::default()
        };
        let json = encode_scene(&desired).expect("scene must serialize");

        assert_eq!(
            player
                .reconcile_scene_json(&json)
                .expect("style reconcile must succeed"),
            ReconcileOutcome::Incremental { patch_count: 1 }
        );
        assert_eq!(player.frame().objects[0].style.stroke_join, StrokeJoin::Bevel);
        assert_eq!(player.frame().objects[0].style.stroke_cap, StrokeCap::Square);
    }

    fn transform_scene(target_x: f32) -> SceneDefinition {
        let object = ObjectDefinition::new(ObjectId::new(0), GeometryRef::circle(1.0));
        let from = ObjectSnapshot::new(GeometryRef::circle(1.0));
        let mut to = ObjectSnapshot::new(GeometryRef::circle(1.0));
        to.transform.translation = Vec2::new(target_x, 0.0);
        let track = TrackDefinition {
            id: TrackId::new(0),
            object: ObjectId::new(0),
            property: Property::Transform,
            values: TrackValues::Object { from, to },
            timing: TrackTiming::new(0.0, 2.0, Easing::Linear),
        };
        SceneDefinition::from_parts(vec![object], vec![track]).expect("transform scene is valid")
    }

    #[test]
    fn generic_transform_target_edit_is_detected_by_rust_reconciliation() {
        let initial = transform_scene(1.0);
        let json = encode_scene(&initial).expect("scene must serialize");
        let mut player = ScenePlayer::from_scene_json(&json).expect("scene must load");
        player.seek(0.5).expect("seek must succeed");

        let desired = transform_scene(3.0);
        let json = encode_scene(&desired).expect("scene must serialize");
        assert_eq!(
            player
                .reconcile_scene_json(&json)
                .expect("transform edit must reconcile"),
            ReconcileOutcome::Rebuilt { patch_count: 1 }
        );
        assert_eq!(player.frame().time, 0.5);
        assert!((player.frame().objects[0].transform.translation.x - 0.75).abs() < 1.0e-6);
    }
'''
text = text[:insert_at] + new_tests + text[insert_at:]
lib.write_text(text)

# Make the grid demo truly parameterized by the edited column count and avoid a
# divide-by-zero surprise for the smallest valid 1x1 grid.
replace_once(
    "web/python/examples/instanced_field.py",
    '''spacing_y = 0.34\n\n# A dense analytic circle field.''',
    '''spacing_y = 0.34\nif isinstance(columns, bool) or not isinstance(columns, int) or columns <= 0:\n    raise ValueError("columns must be a positive integer")\nif isinstance(rows, bool) or not isinstance(rows, int) or rows <= 0:\n    raise ValueError("rows must be a positive integer")\ncolor_denominator = max(rows + columns - 2, 1)\n\n# A dense analytic circle field.''',
)
replace_once(
    "web/python/examples/instanced_field.py",
    "        color_mix = (row + column) / (rows + columns - 2)\n",
    "        color_mix = (row + column) / color_denominator\n",
)
replace_once(
    "web/python/examples/instanced_field.py",
    "            start_time=(index % 18) * 0.018,\n",
    "            start_time=(index % columns) * 0.018,\n",
)

//! One Rust-authored semantic fixture and paused input trace shared by the
//! native collector and browser player adapter tests.
use noon_core::{
    NativeEventSource, NativeStateSource, SemanticMutationTransaction, SemanticNativeInputSource,
    SemanticNodeCreation, SemanticNodeId, SemanticObjectProperty, SemanticObjectState,
    SemanticSignalValue, SemanticStore, SemanticVec3, StoredGeometry,
};

pub struct Fixture {
    pub store: SemanticStore,
    pub root: SemanticNodeId,
    pub target: SemanticNodeId,
    pub unrelated: SemanticNodeId,
    pub position: SemanticNodeId,
    pub button: SemanticNodeId,
    pub down: SemanticNodeId,
    pub up: SemanticNodeId,
    pub viewport: SemanticNodeId,
    pub key_pressed: SemanticNodeId,
    pub key_press: SemanticNodeId,
    pub key_release: SemanticNodeId,
}

/// Paused Space key lifecycle, shared by native and browser/WASM qualification.
pub const KEY_TRACE: &[&str] = &["press", "release"];

#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub kind: &'static str,
    pub x: f32,
    pub y: f32,
    pub button: Option<u8>,
}

// A press on A followed by movement to B must not select A. Repeated press
// and release edges remain discrete occurrences, even while paused.
pub const TRACE: &[Step] = &[
    Step {
        kind: "move",
        x: 400.0,
        y: 200.0,
        button: None,
    },
    Step {
        kind: "press",
        x: 400.0,
        y: 200.0,
        button: Some(0),
    },
    Step {
        kind: "move",
        x: 600.0,
        y: 300.0,
        button: None,
    },
    Step {
        kind: "release",
        x: 600.0,
        y: 300.0,
        button: Some(0),
    },
    Step {
        kind: "press",
        x: 600.0,
        y: 300.0,
        button: Some(0),
    },
    Step {
        kind: "press",
        x: 600.0,
        y: 300.0,
        button: Some(0),
    },
    Step {
        kind: "release",
        x: 600.0,
        y: 300.0,
        button: Some(0),
    },
    Step {
        kind: "release",
        x: 600.0,
        y: 300.0,
        button: Some(0),
    },
];

pub const EXPECTED_POSITION: (f32, f32) = (4.0, -2.0);
pub const EXPECTED_DOWN_COUNT: f32 = 3.0;
pub const EXPECTED_UP_COUNT: f32 = 3.0;
pub const EXPECTED_SEQUENCE: u64 = TRACE.len() as u64;
pub const EXPECTED_SELECTED: bool = false;
pub const EXPECTED_FRAME_TIME: f64 = 0.0;

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

impl Fixture {
    pub fn new() -> Self {
        let mut fixture = Self::signals_only();
        fixture
            .store
            .node_mut(fixture.unrelated)
            .and_then(|node| node.semantic_object_state_mut())
            .expect("shared key projection target remains an authored object")
            .style
            .object_opacity = 0.0;
        fixture
            .store
            .bind_semantic_signal(
                fixture.down,
                fixture.target,
                SemanticObjectProperty::RotationZ,
            )
            .unwrap();
        fixture
            .store
            .bind_semantic_signal(
                fixture.key_pressed,
                fixture.unrelated,
                SemanticObjectProperty::Presence,
            )
            .unwrap();
        fixture
            .store
            .bind_semantic_signal(
                fixture.up,
                fixture.unrelated,
                SemanticObjectProperty::RotationZ,
            )
            .unwrap();
        // Project the ordered key event counters into otherwise-unused style
        // channels on the offscreen unrelated object for paired host checks.
        fixture
            .store
            .bind_semantic_signal(
                fixture.key_press,
                fixture.unrelated,
                SemanticObjectProperty::ObjectOpacity,
            )
            .unwrap();
        fixture
            .store
            .bind_semantic_signal(
                fixture.key_release,
                fixture.unrelated,
                SemanticObjectProperty::StrokeWidth,
            )
            .unwrap();
        fixture
    }

    /// Input/replay qualification without a property-driven execution domain.
    pub fn signals_only() -> Self {
        let mut store = SemanticStore::new();
        let root = store.insert_family();
        let target =
            store.insert_semantic_object(SemanticObjectState::new(StoredGeometry::Circle {
                radius: 0.5,
            }));
        let mut unrelated_state = SemanticObjectState::new(StoredGeometry::Circle { radius: 0.5 });
        unrelated_state.transform.translation = SemanticVec3::new(-50.0, -30.0, 0.0);
        let unrelated = store.insert_semantic_object(unrelated_state);
        for object in [target, unrelated] {
            store.add_semantic_family_member(root, object).unwrap();
        }
        let position = signal(
            &mut store,
            root,
            SemanticNativeInputSource::State(NativeStateSource::PointerPosition),
            SemanticSignalValue::Vec3(SemanticVec3::ZERO),
        );
        let button = signal(
            &mut store,
            root,
            SemanticNativeInputSource::State(NativeStateSource::PointerButton { button: 0 }),
            SemanticSignalValue::Bool(false),
        );
        let down = signal(
            &mut store,
            root,
            SemanticNativeInputSource::Event(NativeEventSource::PointerDown { button: 0 }),
            SemanticSignalValue::Scalar(0.0),
        );
        let up = signal(
            &mut store,
            root,
            SemanticNativeInputSource::Event(NativeEventSource::PointerUp { button: 0 }),
            SemanticSignalValue::Scalar(0.0),
        );
        let viewport = signal(
            &mut store,
            root,
            SemanticNativeInputSource::State(NativeStateSource::ViewportSize),
            SemanticSignalValue::Vec3(SemanticVec3::ZERO),
        );
        let key_pressed = signal(
            &mut store,
            root,
            SemanticNativeInputSource::State(NativeStateSource::Key {
                code: "Space".to_owned(),
            }),
            SemanticSignalValue::Bool(false),
        );
        let key_press = signal(
            &mut store,
            root,
            SemanticNativeInputSource::Event(NativeEventSource::KeyPress {
                code: "Space".to_owned(),
            }),
            SemanticSignalValue::Scalar(0.0),
        );
        let key_release = signal(
            &mut store,
            root,
            SemanticNativeInputSource::Event(NativeEventSource::KeyRelease {
                code: "Space".to_owned(),
            }),
            SemanticSignalValue::Scalar(0.0),
        );
        Self {
            store,
            root,
            target,
            unrelated,
            position,
            button,
            down,
            up,
            viewport,
            key_pressed,
            key_press,
            key_release,
        }
    }
}

fn signal(
    store: &mut SemanticStore,
    root: SemanticNodeId,
    source: SemanticNativeInputSource,
    initial: SemanticSignalValue,
) -> SemanticNodeId {
    let mut tx = SemanticMutationTransaction::new();
    let pending =
        tx.create_node(SemanticNodeCreation::native_input_signal(initial, source).unwrap());
    tx.scope_signal(root, pending);
    tx.apply(store).unwrap().resolve(pending).unwrap()
}

use std::collections::BTreeMap;

use noon::legacy::{Circle, ValueTracker, Mobject};
use noon::ReactiveTimelineScene;
use noon_core::{RateFunction, Vec2};

fn finite_f32(name: &str, value: f64) -> Result<f32, String> {
    if !value.is_finite() {
        return Err(format!("{name} must be finite"));
    }
    let lowered = value as f32;
    if !lowered.is_finite() {
        return Err(format!("{name} is outside the supported render range"));
    }
    Ok(lowered)
}

fn vec2(name: &str, x: f64, y: f64) -> Result<Vec2, String> {
    Ok(Vec2::new(
        finite_f32(&format!("{name}.x"), x)?,
        finite_f32(&format!("{name}.y"), y)?,
    ))
}

fn rate_function(value: &str) -> Result<RateFunction, String> {
    if value.is_empty() {
        return Ok(RateFunction::Smooth);
    }
    RateFunction::from_semantic_id(value)
        .ok_or_else(|| format!("unsupported rate function semantic id: {value}"))
}

/// Minimal browser-facing reactive authoring state backed directly by
/// `noon::ReactiveTimelineScene`.
///
/// The browser layer owns only small local handles. Signal identities, derived
/// expressions, bindings, timeline scheduling and interpolation all remain in
/// shared Rust semantics.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrontendReactiveAuthoringScene {
    scene: ReactiveTimelineScene,
    objects: BTreeMap<u32, Mobject>,
    trackers: BTreeMap<u32, ValueTracker>,
    next_object_handle: u32,
    next_tracker_handle: u32,
}

impl FrontendReactiveAuthoringScene {
    pub fn new() -> Self {
        Self::default()
    }

    fn object(&self, handle: u32) -> Result<Mobject, String> {
        self.objects
            .get(&handle)
            .copied()
            .ok_or_else(|| format!("unknown reactive mobject handle {handle}"))
    }

    fn tracker(&self, handle: u32) -> Result<ValueTracker, String> {
        self.trackers
            .get(&handle)
            .copied()
            .ok_or_else(|| format!("unknown ValueTracker handle {handle}"))
    }

    pub fn add_circle(&mut self, radius: f64) -> Result<u32, String> {
        let handle = self.next_object_handle;
        self.next_object_handle = self
            .next_object_handle
            .checked_add(1)
            .ok_or_else(|| "reactive mobject handle space exhausted".to_owned())?;
        let object = self.scene.add(Circle::new(finite_f32("radius", radius)?));
        self.objects.insert(handle, object);
        Ok(handle)
    }

    pub fn value_tracker(&mut self, value: f64) -> Result<u32, String> {
        let handle = self.next_tracker_handle;
        self.next_tracker_handle = self
            .next_tracker_handle
            .checked_add(1)
            .ok_or_else(|| "ValueTracker handle space exhausted".to_owned())?;
        let tracker = self.scene.value_tracker(finite_f32("value", value)?);
        self.trackers.insert(handle, tracker);
        Ok(handle)
    }

    pub fn bind_position_from_tracker(
        &mut self,
        object_handle: u32,
        tracker_handle: u32,
        direction_x: f64,
        direction_y: f64,
        offset_x: f64,
        offset_y: f64,
    ) -> Result<(), String> {
        let object = self.object(object_handle)?;
        let tracker = self.tracker(tracker_handle)?;
        let position = self.scene.position_from_tracker(
            tracker,
            vec2("direction", direction_x, direction_y)?,
            vec2("offset", offset_x, offset_y)?,
        );
        self.scene.bind_position(object, position);
        Ok(())
    }

    pub fn play_value(
        &mut self,
        tracker_handle: u32,
        to: f64,
        run_time: f64,
        rate_func: &str,
    ) -> Result<(), String> {
        let tracker = self.tracker(tracker_handle)?;
        let play = self
            .scene
            .play_value(tracker, finite_f32("target value", to)?)
            .rate_func(rate_function(rate_func)?);
        play.run_time(run_time).map_err(|error| error.to_string())
    }

    pub fn wait(&mut self, duration: f64) -> Result<(), String> {
        self.scene
            .wait(duration)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }

    pub fn time(&self) -> f64 {
        self.scene.time()
    }

    pub fn timed_scene_json(&self) -> Result<String, String> {
        let scene = self
            .scene
            .timed_semantic_scene()
            .map_err(|error| error.to_string())?;
        noon_ir::encode_timed_semantic_scene(&scene).map_err(|error| error.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::FrontendReactiveAuthoringScene;

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    #[wasm_bindgen]
    pub struct ReactiveAuthoringSceneCore(FrontendReactiveAuthoringScene);

    #[wasm_bindgen]
    impl ReactiveAuthoringSceneCore {
        #[wasm_bindgen(constructor)]
        pub fn new() -> ReactiveAuthoringSceneCore {
            ReactiveAuthoringSceneCore(FrontendReactiveAuthoringScene::new())
        }

        #[wasm_bindgen(js_name = addCircle)]
        pub fn add_circle(&mut self, radius: f64) -> Result<u32, JsValue> {
            self.0.add_circle(radius).map_err(js_error)
        }

        #[wasm_bindgen(js_name = valueTracker)]
        pub fn value_tracker(&mut self, value: f64) -> Result<u32, JsValue> {
            self.0.value_tracker(value).map_err(js_error)
        }

        #[wasm_bindgen(js_name = bindPositionFromTracker)]
        pub fn bind_position_from_tracker(
            &mut self,
            object_handle: u32,
            tracker_handle: u32,
            direction_x: f64,
            direction_y: f64,
            offset_x: f64,
            offset_y: f64,
        ) -> Result<(), JsValue> {
            self.0
                .bind_position_from_tracker(
                    object_handle,
                    tracker_handle,
                    direction_x,
                    direction_y,
                    offset_x,
                    offset_y,
                )
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = playValue)]
        pub fn play_value(
            &mut self,
            tracker_handle: u32,
            to: f64,
            run_time: f64,
            rate_func: &str,
        ) -> Result<(), JsValue> {
            self.0
                .play_value(tracker_handle, to, run_time, rate_func)
                .map_err(js_error)
        }

        pub fn wait(&mut self, duration: f64) -> Result<(), JsValue> {
            self.0.wait(duration).map_err(js_error)
        }

        #[wasm_bindgen(getter)]
        pub fn time(&self) -> f64 {
            self.0.time()
        }

        #[wasm_bindgen(js_name = timedSceneJson)]
        pub fn timed_scene_json(&self) -> Result<String, JsValue> {
            self.0.timed_scene_json().map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{Property, RateFunction, RIGHT, UP};

    use super::*;

    #[test]
    fn browser_reactive_facade_reuses_native_tracker_semantics() {
        let mut scene = FrontendReactiveAuthoringScene::new();
        let circle = scene.add_circle(0.3).unwrap();
        let tracker = scene.value_tracker(0.0).unwrap();
        scene
            .bind_position_from_tracker(
                circle,
                tracker,
                f64::from(RIGHT.x),
                f64::from(RIGHT.y),
                f64::from(UP.x),
                f64::from(UP.y),
            )
            .unwrap();
        scene.play_value(tracker, 2.0, 2.0, "linear").unwrap();

        assert_eq!(scene.time(), 2.0);
        let timed = scene.scene.timed_semantic_scene().unwrap();
        assert_eq!(
            timed.semantic().reactive().bindings()[0].property,
            Property::Position
        );
        let track = &timed.signal_timeline().tracks()[0];
        assert_eq!(track.from, 0.0);
        assert_eq!(track.to, 2.0);
        assert_eq!(track.timing.easing, RateFunction::Linear);
        assert_eq!(track.timing.duration, 2.0);
    }

    #[test]
    fn browser_reactive_facade_rejects_unknown_handles_and_nonfinite_values() {
        let mut scene = FrontendReactiveAuthoringScene::new();
        assert!(scene
            .bind_position_from_tracker(99, 42, 1.0, 0.0, 0.0, 0.0)
            .is_err());
        assert!(scene.value_tracker(f64::NAN).is_err());
        assert!(scene.add_circle(f64::INFINITY).is_err());
    }
}

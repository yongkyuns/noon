use std::collections::BTreeMap;

use noon::{
    Animate, Animation, Circle, Create, FadeIn, FadeOut, IntoSnapshot, Line, Mobject, Rectangle,
    Rotate, Scene, Square, Transform,
};
use noon_core::{Color, RateFunction, Vec2};

use crate::FrontendMobjectHandle;

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

fn unit_f32(name: &str, value: f64) -> Result<f32, String> {
    let value = finite_f32(name, value)?;
    if !(0.0..=1.0).contains(&value) {
        return Err(format!("{name} must be between 0 and 1"));
    }
    Ok(value)
}

fn vec2(name: &str, x: f64, y: f64) -> Result<Vec2, String> {
    Ok(Vec2::new(
        finite_f32(&format!("{name}.x"), x)?,
        finite_f32(&format!("{name}.y"), y)?,
    ))
}

fn color(red: f64, green: f64, blue: f64, alpha: f64) -> Result<Color, String> {
    Ok(Color::rgba(
        unit_f32("color.red", red)?,
        unit_f32("color.green", green)?,
        unit_f32("color.blue", blue)?,
        unit_f32("color.alpha", alpha)?,
    ))
}

fn optional_rate_function(value: &str) -> Result<Option<RateFunction>, String> {
    if value.is_empty() {
        return Ok(None);
    }
    RateFunction::from_semantic_id(value)
        .map(Some)
        .ok_or_else(|| format!("unsupported rate function semantic id: {value}"))
}

/// Detached object used by thin browser-language adapters before scene insertion.
///
/// Geometry defaults come from the public `noon` Rust facade while mutations and
/// layout queries delegate to `FrontendMobjectHandle`, so JavaScript does not own
/// a second bounds/style/transform implementation.
#[derive(Clone, Debug, PartialEq)]
pub struct FrontendDetachedMobject {
    handle: FrontendMobjectHandle,
}

impl FrontendDetachedMobject {
    fn from_snapshot(snapshot: noon_core::ObjectSnapshot) -> Self {
        Self {
            handle: FrontendMobjectHandle::from_snapshot(snapshot),
        }
    }

    pub fn circle(radius: f64) -> Result<Self, String> {
        Ok(Self::from_snapshot(
            Circle::new(finite_f32("radius", radius)?).into_snapshot(),
        ))
    }

    pub fn square(side_length: f64) -> Result<Self, String> {
        Ok(Self::from_snapshot(
            Square::new(finite_f32("side_length", side_length)?).into_snapshot(),
        ))
    }

    pub fn rectangle(width: f64, height: f64) -> Result<Self, String> {
        Ok(Self::from_snapshot(
            Rectangle::new(finite_f32("width", width)?, finite_f32("height", height)?)
                .into_snapshot(),
        ))
    }

    pub fn line(start_x: f64, start_y: f64, end_x: f64, end_y: f64) -> Result<Self, String> {
        Ok(Self::from_snapshot(
            Line::new(vec2("start", start_x, start_y)?, vec2("end", end_x, end_y)?).into_snapshot(),
        ))
    }

    pub fn shift(&mut self, x: f64, y: f64) -> Result<&mut Self, String> {
        self.handle.shift(x, y)?;
        Ok(self)
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> Result<&mut Self, String> {
        self.handle.move_to(x, y)?;
        Ok(self)
    }

    pub fn scale(&mut self, factor: f64) -> Result<&mut Self, String> {
        self.handle.scale(factor, factor)?;
        Ok(self)
    }

    pub fn rotate(&mut self, angle: f64) -> Result<&mut Self, String> {
        self.handle.rotate(angle)?;
        Ok(self)
    }

    pub fn set_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<&mut Self, String> {
        self.handle.set_color(red, green, blue, alpha)?;
        Ok(self)
    }

    pub fn set_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<&mut Self, String> {
        self.handle.set_fill_color(red, green, blue, 1.0)?;
        self.handle.set_fill_opacity(opacity)?;
        Ok(self)
    }

    pub fn set_opacity(&mut self, opacity: f64) -> Result<&mut Self, String> {
        self.handle.set_opacity(opacity)?;
        Ok(self)
    }

    pub fn next_to(
        &mut self,
        other: &Self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> Result<&mut Self, String> {
        self.handle
            .next_to_handle(&other.handle, direction_x, direction_y, buff)?;
        Ok(self)
    }

    pub fn center(&self) -> (f64, f64) {
        self.handle.center()
    }

    pub fn width(&self) -> f64 {
        self.handle.width()
    }

    pub fn height(&self) -> f64 {
        self.handle.height()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrontendAnimate {
    animation: Animate,
}

impl FrontendAnimate {
    pub fn shift(&mut self, x: f64, y: f64) -> Result<&mut Self, String> {
        self.animation = self.animation.clone().shift(vec2("shift", x, y)?);
        Ok(self)
    }

    pub fn move_to(&mut self, x: f64, y: f64) -> Result<&mut Self, String> {
        self.animation = self.animation.clone().move_to(vec2("move_to", x, y)?);
        Ok(self)
    }

    pub fn scale(&mut self, factor: f64) -> Result<&mut Self, String> {
        self.animation = self.animation.clone().scale(finite_f32("scale", factor)?);
        Ok(self)
    }

    pub fn rotate(&mut self, angle: f64) -> Result<&mut Self, String> {
        self.animation = self
            .animation
            .clone()
            .rotate(finite_f32("rotation", angle)?);
        Ok(self)
    }

    pub fn set_color(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<&mut Self, String> {
        self.animation = self
            .animation
            .clone()
            .set_color(color(red, green, blue, alpha)?);
        Ok(self)
    }

    pub fn set_fill(
        &mut self,
        red: f64,
        green: f64,
        blue: f64,
        opacity: f64,
    ) -> Result<&mut Self, String> {
        self.animation = self.animation.clone().set_fill(
            Some(color(red, green, blue, 1.0)?),
            Some(unit_f32("fill opacity", opacity)?),
        );
        Ok(self)
    }

    pub fn set_opacity(&mut self, opacity: f64) -> Result<&mut Self, String> {
        self.animation = self
            .animation
            .clone()
            .set_opacity(unit_f32("opacity", opacity)?);
        Ok(self)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrontendPlayBatch {
    animations: Vec<Animation>,
}

impl FrontendPlayBatch {
    pub const fn new() -> Self {
        Self {
            animations: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.animations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.animations.is_empty()
    }
}

/// Browser-facing authoring state backed directly by the public Rust `noon::Scene`.
///
/// Frontends receive small stable local handles. Canonical object identity,
/// target-state lowering, lifecycle and timing remain owned by `noon`/`noon-core`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrontendAuthoringScene {
    scene: Scene,
    objects: BTreeMap<u32, Mobject>,
    next_handle: u32,
}

impl FrontendAuthoringScene {
    pub fn new() -> Self {
        Self::default()
    }

    fn object(&self, handle: u32) -> Result<Mobject, String> {
        self.objects
            .get(&handle)
            .copied()
            .ok_or_else(|| format!("unknown authoring mobject handle {handle}"))
    }

    pub fn add(&mut self, object: &FrontendDetachedMobject) -> Result<u32, String> {
        let handle = self.next_handle;
        self.next_handle = self
            .next_handle
            .checked_add(1)
            .ok_or_else(|| "authoring mobject handle space exhausted".to_owned())?;
        let mobject = self.scene.add(object.handle.snapshot().clone());
        self.objects.insert(handle, mobject);
        Ok(handle)
    }

    pub fn shift(&mut self, handle: u32, x: f64, y: f64) -> Result<(), String> {
        let object = self.object(handle)?;
        self.scene
            .edit(object)
            .map_err(|error| error.to_string())?
            .shift(vec2("shift", x, y)?)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn move_to(&mut self, handle: u32, x: f64, y: f64) -> Result<(), String> {
        let object = self.object(handle)?;
        self.scene
            .edit(object)
            .map_err(|error| error.to_string())?
            .move_to(vec2("move_to", x, y)?)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn scale(&mut self, handle: u32, factor: f64) -> Result<(), String> {
        let object = self.object(handle)?;
        self.scene
            .edit(object)
            .map_err(|error| error.to_string())?
            .scale(finite_f32("scale", factor)?)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn rotate(&mut self, handle: u32, angle: f64) -> Result<(), String> {
        let object = self.object(handle)?;
        self.scene
            .edit(object)
            .map_err(|error| error.to_string())?
            .rotate(finite_f32("rotation", angle)?)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn set_color(
        &mut self,
        handle: u32,
        red: f64,
        green: f64,
        blue: f64,
        alpha: f64,
    ) -> Result<(), String> {
        let object = self.object(handle)?;
        self.scene
            .edit(object)
            .map_err(|error| error.to_string())?
            .set_color(color(red, green, blue, alpha)?)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn set_opacity(&mut self, handle: u32, opacity: f64) -> Result<(), String> {
        let object = self.object(handle)?;
        self.scene
            .edit(object)
            .map_err(|error| error.to_string())?
            .set_opacity(unit_f32("opacity", opacity)?)
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn next_to(
        &mut self,
        handle: u32,
        target: u32,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
    ) -> Result<(), String> {
        let object = self.object(handle)?;
        let target = self.object(target)?;
        self.scene
            .edit(object)
            .map_err(|error| error.to_string())?
            .next_to(
                target,
                vec2("direction", direction_x, direction_y)?,
                finite_f32("buff", buff)?,
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn animate(&self, handle: u32) -> Result<FrontendAnimate, String> {
        Ok(FrontendAnimate {
            animation: self.object(handle)?.animate(),
        })
    }

    pub fn create_play_batch(&self) -> FrontendPlayBatch {
        FrontendPlayBatch::new()
    }

    pub fn append_animate(&self, batch: &mut FrontendPlayBatch, animation: &FrontendAnimate) {
        batch.animations.push(animation.animation.clone().into());
    }

    pub fn append_create(&self, batch: &mut FrontendPlayBatch, handle: u32) -> Result<(), String> {
        batch
            .animations
            .push(Create::new(self.object(handle)?).into());
        Ok(())
    }

    pub fn append_fade_out(
        &self,
        batch: &mut FrontendPlayBatch,
        handle: u32,
    ) -> Result<(), String> {
        batch
            .animations
            .push(FadeOut::new(self.object(handle)?).into());
        Ok(())
    }

    pub fn append_fade_in(&self, batch: &mut FrontendPlayBatch, handle: u32) -> Result<(), String> {
        batch
            .animations
            .push(FadeIn::new(self.object(handle)?).into());
        Ok(())
    }

    pub fn append_transform(
        &self,
        batch: &mut FrontendPlayBatch,
        handle: u32,
        target: &FrontendDetachedMobject,
    ) -> Result<(), String> {
        batch
            .animations
            .push(Transform::new(self.object(handle)?, target.handle.snapshot().clone()).into());
        Ok(())
    }

    pub fn append_rotate(
        &self,
        batch: &mut FrontendPlayBatch,
        handle: u32,
        angle: f64,
    ) -> Result<(), String> {
        batch.animations.push(
            Rotate::new(self.object(handle)?, finite_f32("rotation angle", angle)?).into(),
        );
        Ok(())
    }

    pub fn play_batch(
        &mut self,
        batch: &FrontendPlayBatch,
        run_time: f64,
        rate_func: &str,
    ) -> Result<(), String> {
        if batch.is_empty() {
            return Err("Scene.play requires at least one animation".to_owned());
        }
        if batch.len() > 4 {
            return Err(
                "browser authoring currently supports at most four animations per play call, matching the current Rust tuple authoring facade"
                    .to_owned(),
            );
        }
        let rate_func = optional_rate_function(rate_func)?;
        let play = match batch.animations.as_slice() {
            [a] => self.scene.play(a.clone()),
            [a, b] => self.scene.play((a.clone(), b.clone())),
            [a, b, c] => self.scene.play((a.clone(), b.clone(), c.clone())),
            [a, b, c, d] => self
                .scene
                .play((a.clone(), b.clone(), c.clone(), d.clone())),
            _ => unreachable!("batch size was validated above"),
        };
        let play = if let Some(rate_func) = rate_func {
            play.rate_func(rate_func)
        } else {
            play
        };
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

    pub fn scene_json(&self) -> Result<String, String> {
        noon_ir::encode_scene(self.scene.definition()).map_err(|error| error.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;

    use super::{
        FrontendAnimate, FrontendAuthoringScene, FrontendDetachedMobject, FrontendPlayBatch,
    };

    fn js_error(error: String) -> JsValue {
        JsValue::from_str(&error)
    }

    #[wasm_bindgen]
    pub struct DetachedMobjectCore(FrontendDetachedMobject);

    #[wasm_bindgen]
    impl DetachedMobjectCore {
        #[wasm_bindgen(js_name = cloneHandle)]
        pub fn clone_handle(&self) -> DetachedMobjectCore {
            DetachedMobjectCore(self.0.clone())
        }

        pub fn shift(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.0.shift(x, y).map(|_| ()).map_err(js_error)
        }

        #[wasm_bindgen(js_name = moveTo)]
        pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.0.move_to(x, y).map(|_| ()).map_err(js_error)
        }

        pub fn scale(&mut self, factor: f64) -> Result<(), JsValue> {
            self.0.scale(factor).map(|_| ()).map_err(js_error)
        }

        pub fn rotate(&mut self, angle: f64) -> Result<(), JsValue> {
            self.0.rotate(angle).map(|_| ()).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setColor)]
        pub fn set_color(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            self.0
                .set_color(red, green, blue, alpha)
                .map(|_| ())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setFill)]
        pub fn set_fill(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            opacity: f64,
        ) -> Result<(), JsValue> {
            self.0
                .set_fill(red, green, blue, opacity)
                .map(|_| ())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setOpacity)]
        pub fn set_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.0.set_opacity(opacity).map(|_| ()).map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextTo)]
        pub fn next_to(
            &mut self,
            other: &DetachedMobjectCore,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
        ) -> Result<(), JsValue> {
            self.0
                .next_to(&other.0, direction_x, direction_y, buff)
                .map(|_| ())
                .map_err(js_error)
        }

        #[wasm_bindgen(getter, js_name = centerX)]
        pub fn center_x(&self) -> f64 {
            self.0.center().0
        }

        #[wasm_bindgen(getter, js_name = centerY)]
        pub fn center_y(&self) -> f64 {
            self.0.center().1
        }

        #[wasm_bindgen(getter)]
        pub fn width(&self) -> f64 {
            self.0.width()
        }

        #[wasm_bindgen(getter)]
        pub fn height(&self) -> f64 {
            self.0.height()
        }
    }

    #[wasm_bindgen(js_name = authoringCircle)]
    pub fn authoring_circle(radius: f64) -> Result<DetachedMobjectCore, JsValue> {
        FrontendDetachedMobject::circle(radius)
            .map(DetachedMobjectCore)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = authoringSquare)]
    pub fn authoring_square(side_length: f64) -> Result<DetachedMobjectCore, JsValue> {
        FrontendDetachedMobject::square(side_length)
            .map(DetachedMobjectCore)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = authoringRectangle)]
    pub fn authoring_rectangle(width: f64, height: f64) -> Result<DetachedMobjectCore, JsValue> {
        FrontendDetachedMobject::rectangle(width, height)
            .map(DetachedMobjectCore)
            .map_err(js_error)
    }

    #[wasm_bindgen(js_name = authoringLine)]
    pub fn authoring_line(
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
    ) -> Result<DetachedMobjectCore, JsValue> {
        FrontendDetachedMobject::line(start_x, start_y, end_x, end_y)
            .map(DetachedMobjectCore)
            .map_err(js_error)
    }

    #[wasm_bindgen]
    pub struct AnimateCore(FrontendAnimate);

    #[wasm_bindgen]
    impl AnimateCore {
        pub fn shift(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.0.shift(x, y).map(|_| ()).map_err(js_error)
        }

        #[wasm_bindgen(js_name = moveTo)]
        pub fn move_to(&mut self, x: f64, y: f64) -> Result<(), JsValue> {
            self.0.move_to(x, y).map(|_| ()).map_err(js_error)
        }

        pub fn scale(&mut self, factor: f64) -> Result<(), JsValue> {
            self.0.scale(factor).map(|_| ()).map_err(js_error)
        }

        pub fn rotate(&mut self, angle: f64) -> Result<(), JsValue> {
            self.0.rotate(angle).map(|_| ()).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setColor)]
        pub fn set_color(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            self.0
                .set_color(red, green, blue, alpha)
                .map(|_| ())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setFill)]
        pub fn set_fill(
            &mut self,
            red: f64,
            green: f64,
            blue: f64,
            opacity: f64,
        ) -> Result<(), JsValue> {
            self.0
                .set_fill(red, green, blue, opacity)
                .map(|_| ())
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setOpacity)]
        pub fn set_opacity(&mut self, opacity: f64) -> Result<(), JsValue> {
            self.0.set_opacity(opacity).map(|_| ()).map_err(js_error)
        }
    }

    #[wasm_bindgen]
    pub struct PlayBatchCore(FrontendPlayBatch);

    #[wasm_bindgen]
    pub struct AuthoringSceneCore(FrontendAuthoringScene);

    #[wasm_bindgen]
    impl AuthoringSceneCore {
        #[wasm_bindgen(constructor)]
        pub fn new() -> AuthoringSceneCore {
            AuthoringSceneCore(FrontendAuthoringScene::new())
        }

        pub fn add(&mut self, object: &DetachedMobjectCore) -> Result<u32, JsValue> {
            self.0.add(&object.0).map_err(js_error)
        }

        pub fn shift(&mut self, handle: u32, x: f64, y: f64) -> Result<(), JsValue> {
            self.0.shift(handle, x, y).map_err(js_error)
        }

        #[wasm_bindgen(js_name = moveTo)]
        pub fn move_to(&mut self, handle: u32, x: f64, y: f64) -> Result<(), JsValue> {
            self.0.move_to(handle, x, y).map_err(js_error)
        }

        pub fn scale(&mut self, handle: u32, factor: f64) -> Result<(), JsValue> {
            self.0.scale(handle, factor).map_err(js_error)
        }

        pub fn rotate(&mut self, handle: u32, angle: f64) -> Result<(), JsValue> {
            self.0.rotate(handle, angle).map_err(js_error)
        }

        #[wasm_bindgen(js_name = setColor)]
        pub fn set_color(
            &mut self,
            handle: u32,
            red: f64,
            green: f64,
            blue: f64,
            alpha: f64,
        ) -> Result<(), JsValue> {
            self.0
                .set_color(handle, red, green, blue, alpha)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = setOpacity)]
        pub fn set_opacity(&mut self, handle: u32, opacity: f64) -> Result<(), JsValue> {
            self.0.set_opacity(handle, opacity).map_err(js_error)
        }

        #[wasm_bindgen(js_name = nextTo)]
        pub fn next_to(
            &mut self,
            handle: u32,
            target: u32,
            direction_x: f64,
            direction_y: f64,
            buff: f64,
        ) -> Result<(), JsValue> {
            self.0
                .next_to(handle, target, direction_x, direction_y, buff)
                .map_err(js_error)
        }

        pub fn animate(&self, handle: u32) -> Result<AnimateCore, JsValue> {
            self.0.animate(handle).map(AnimateCore).map_err(js_error)
        }

        #[wasm_bindgen(js_name = createPlayBatch)]
        pub fn create_play_batch(&self) -> PlayBatchCore {
            PlayBatchCore(self.0.create_play_batch())
        }

        #[wasm_bindgen(js_name = appendAnimate)]
        pub fn append_animate(&self, batch: &mut PlayBatchCore, animation: &AnimateCore) {
            self.0.append_animate(&mut batch.0, &animation.0);
        }

        #[wasm_bindgen(js_name = appendCreate)]
        pub fn append_create(&self, batch: &mut PlayBatchCore, handle: u32) -> Result<(), JsValue> {
            self.0.append_create(&mut batch.0, handle).map_err(js_error)
        }

        #[wasm_bindgen(js_name = appendFadeOut)]
        pub fn append_fade_out(
            &self,
            batch: &mut PlayBatchCore,
            handle: u32,
        ) -> Result<(), JsValue> {
            self.0
                .append_fade_out(&mut batch.0, handle)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = appendFadeIn)]
        pub fn append_fade_in(
            &self,
            batch: &mut PlayBatchCore,
            handle: u32,
        ) -> Result<(), JsValue> {
            self.0
                .append_fade_in(&mut batch.0, handle)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = appendTransform)]
        pub fn append_transform(
            &self,
            batch: &mut PlayBatchCore,
            handle: u32,
            target: &DetachedMobjectCore,
        ) -> Result<(), JsValue> {
            self.0
                .append_transform(&mut batch.0, handle, &target.0)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = appendRotate)]
        pub fn append_rotate(
            &self,
            batch: &mut PlayBatchCore,
            handle: u32,
            angle: f64,
        ) -> Result<(), JsValue> {
            self.0
                .append_rotate(&mut batch.0, handle, angle)
                .map_err(js_error)
        }

        #[wasm_bindgen(js_name = playBatch)]
        pub fn play_batch(
            &mut self,
            batch: &PlayBatchCore,
            run_time: f64,
            rate_func: &str,
        ) -> Result<(), JsValue> {
            self.0
                .play_batch(&batch.0, run_time, rate_func)
                .map_err(js_error)
        }

        pub fn wait(&mut self, duration: f64) -> Result<(), JsValue> {
            self.0.wait(duration).map_err(js_error)
        }

        #[wasm_bindgen(getter)]
        pub fn time(&self) -> f64 {
            self.0.time()
        }

        #[wasm_bindgen(js_name = sceneJson)]
        pub fn scene_json(&self) -> Result<String, JsValue> {
            self.0.scene_json().map_err(js_error)
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::*;

#[cfg(test)]
mod tests {
    use noon_core::{Property, RateFunction};

    use super::*;

    #[test]
    fn browser_facade_reuses_shared_static_layout_semantics() {
        let mut scene = FrontendAuthoringScene::new();
        let mut circle = FrontendDetachedMobject::circle(0.5).unwrap();
        circle.shift(-1.0, 0.0).unwrap();
        let square = FrontendDetachedMobject::square(1.0).unwrap();
        let circle = scene.add(&circle).unwrap();
        let square = scene.add(&square).unwrap();
        scene.next_to(square, circle, 1.0, 0.0, 0.25).unwrap();

        let circle_snapshot = scene.scene.snapshot(scene.object(circle).unwrap()).unwrap();
        let square_snapshot = scene.scene.snapshot(scene.object(square).unwrap()).unwrap();
        let gap = square_snapshot.world_bounds().unwrap().min.x
            - circle_snapshot.world_bounds().unwrap().max.x;
        assert!((gap - 0.25).abs() < 1e-6);
    }

    #[test]
    fn browser_facade_lowers_parallel_animate_through_noon_scene() {
        let mut scene = FrontendAuthoringScene::new();
        let left = scene
            .add(&FrontendDetachedMobject::circle(0.4).unwrap())
            .unwrap();
        let right = scene
            .add(&FrontendDetachedMobject::square(0.8).unwrap())
            .unwrap();
        scene.shift(left, -1.0, 0.0).unwrap();
        scene.shift(right, 1.0, 0.0).unwrap();

        let mut left_animation = scene.animate(left).unwrap();
        left_animation.shift(0.0, 1.0).unwrap();
        let mut right_animation = scene.animate(right).unwrap();
        right_animation
            .shift(0.0, -1.0)
            .unwrap()
            .rotate(0.25)
            .unwrap();

        let mut batch = scene.create_play_batch();
        scene.append_animate(&mut batch, &left_animation);
        scene.append_animate(&mut batch, &right_animation);
        scene.play_batch(&batch, 2.0, "linear").unwrap();

        assert_eq!(scene.time(), 2.0);
        assert_eq!(scene.scene.definition().tracks().len(), 2);
        for track in scene.scene.definition().tracks() {
            assert_eq!(track.property, Property::Transform);
            assert_eq!(track.timing.start_time, 0.0);
            assert_eq!(track.timing.duration, 2.0);
            assert_eq!(track.timing.easing, RateFunction::Linear);
        }
    }

    #[test]
    fn browser_facade_lifecycle_uses_same_noon_animation_types() {
        let mut scene = FrontendAuthoringScene::new();
        let circle = scene
            .add(&FrontendDetachedMobject::circle(0.5).unwrap())
            .unwrap();

        let mut create = scene.create_play_batch();
        scene.append_create(&mut create, circle).unwrap();
        scene.play_batch(&create, 1.0, "").unwrap();

        let mut fade_out = scene.create_play_batch();
        scene.append_fade_out(&mut fade_out, circle).unwrap();
        scene.play_batch(&fade_out, 0.5, "").unwrap();

        let mut fade_in = scene.create_play_batch();
        scene.append_fade_in(&mut fade_in, circle).unwrap();
        scene.play_batch(&fade_in, 0.5, "").unwrap();

        assert_eq!(scene.time(), 2.0);
        assert!(scene.scene_json().unwrap().contains("\"tracks\""));
    }
}

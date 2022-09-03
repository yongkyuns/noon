use super::*;

pub struct AnimBuilder<'a> {
    scene: &'a mut Scene,
    animations: Vec<EntityAnimations>,
    run_time: f32,
    rate_func: EaseType,
    lag: f32,
    #[allow(dead_code)]
    repeat: usize, // Not implemented yet
    start_time: Option<f32>,
}

impl<'a> AnimBuilder<'a> {
    pub fn new(scene: &'a mut Scene, animations: Vec<EntityAnimations>) -> Self {
        AnimBuilder {
            scene,
            animations,
            run_time: 1.0,
            rate_func: EaseType::Quad,
            lag: 0.0,
            repeat: 0,
            start_time: None,
        }
    }
    pub fn start_time(mut self, time: f32) -> Self {
        self.start_time = Some(time);
        self
    }
    pub fn run_time(mut self, duration: f32) -> Self {
        self.run_time = duration;
        self
    }
    pub fn rate_func(mut self, rate_func: EaseType) -> Self {
        self.rate_func = rate_func;
        self
    }
    pub fn lag(mut self, lag: f32) -> Self {
        self.lag = lag;
        self
    }
}

impl<'a> Drop for AnimBuilder<'a> {
    fn drop(&mut self) {
        let Self {
            run_time,
            animations,
            rate_func,
            lag,
            start_time,
            ..
        } = self;

        let mut t = if let Some(time) = start_time {
            *time
        } else {
            self.scene.event_time
        };
        for animation in animations.into_iter() {
            animation.set_properties(t, *run_time, *rate_func);
            animation.clone().insert_animation(&mut self.scene.world);
            t += *lag;
        }
        self.scene.event_time = t - *lag + *run_time;
    }
}

// pub struct GroupBuilder<'a> {
//     scene: &'a mut Scene,
//     animations: Vec<EntityAnimations>,
//     run_time: f32,
//     rate_func: EaseType,
//     lag: f32,
//     #[allow(dead_code)]
//     repeat: usize, // Not implemented yet
//     start_time: Option<f32>,
// }

// impl<'a> GroupBuilder<'a> {
//     pub fn new(scene: &'a mut Scene, animations: Vec<EntityAnimations>) -> Self {
//         GroupBuilder {
//             scene,
//             animations,
//             run_time: 1.0,
//             rate_func: EaseType::Quad,
//             lag: 0.0,
//             repeat: 0,
//             start_time: None,
//         }
//     }
//     pub fn start_time(mut self, time: f32) -> Self {
//         self.start_time = Some(time);
//         self
//     }
//     pub fn run_time(mut self, duration: f32) -> Self {
//         self.run_time = duration;
//         self
//     }
//     pub fn rate_func(mut self, rate_func: EaseType) -> Self {
//         self.rate_func = rate_func;
//         self
//     }
//     pub fn lag(mut self, lag: f32) -> Self {
//         self.lag = lag;
//         self
//     }
// }

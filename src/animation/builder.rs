use super::*;

pub struct AnimBuilder<'a> {
    scene: &'a mut Scene,
    animations: Vec<EntityAnimations>,
    run_time: f32,
    rate_func: EaseType,
    lag: f32,
    repeat: usize,
}

impl<'a> AnimBuilder<'a> {
    pub fn new(scene: &'a mut Scene, animations: Vec<EntityAnimations>) -> Self {
        let mut rate_func = EaseType::Quad;
        // for ta in animations.iter() {
        //     if ta.action == Action::ShowCreation {
        //         rate_func = EaseType::Quad;
        //         break;
        //     }
        // }
        AnimBuilder {
            scene,
            animations,
            run_time: 1.0,
            rate_func,
            lag: 0.0,
            repeat: 0,
        }
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
            scene,
            lag,
            repeat,
        } = self;

        let mut t = self.scene.event_time;
        for animation in animations.into_iter() {
            animation.set_properties(t, *run_time, *rate_func);
            animation.clone().insert_animation(&mut self.scene.world);
            t += *lag;
        }
        self.scene.event_time = t - *lag + *run_time;
    }
}

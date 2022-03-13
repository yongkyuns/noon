use pennereq::*;

type EaseFn<S = f32> = fn(t: S, b: S, c: S, d: S) -> S;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum EaseType {
    Linear,
    Quad,
    QuadIn,
    QuadOut,
    Cubic,
    CubicIn,
    CubicOut,
    Quart,
    QuartIn,
    QuartOut,
    Quint,
    QuintIn,
    QuintOut,
    Sine,
    SineIn,
    SineOut,
    Expo,
    ExpoIn,
    ExpoOut,
    Circ,
    CircIn,
    CircOut,
    Elastic,
    ElasticIn,
    ElasticOut,
    Back,
    BackIn,
    BackOut,
    Bounce,
    BounceIn,
    BounceOut,
}

impl EaseType {
    pub fn calculate(&self, t: f32) -> f32 {
        let ease_func: EaseFn = match self {
            EaseType::Linear => linear::ease,
            EaseType::Quad => quad::ease_in_out,
            EaseType::QuadIn => quad::ease_in,
            EaseType::QuadOut => quad::ease_out,
            EaseType::Cubic => cubic::ease_in_out,
            EaseType::CubicIn => cubic::ease_in,
            EaseType::CubicOut => cubic::ease_out,
            EaseType::Quart => quart::ease_in_out,
            EaseType::QuartIn => quart::ease_in,
            EaseType::QuartOut => quart::ease_out,
            EaseType::Quint => quint::ease_in_out,
            EaseType::QuintIn => quint::ease_in,
            EaseType::QuintOut => quint::ease_out,
            EaseType::Sine => sine::ease_in_out,
            EaseType::SineIn => sine::ease_in,
            EaseType::SineOut => sine::ease_out,
            EaseType::Expo => expo::ease_in_out,
            EaseType::ExpoIn => expo::ease_in,
            EaseType::ExpoOut => expo::ease_out,
            EaseType::Circ => circ::ease_in_out,
            EaseType::CircIn => circ::ease_in,
            EaseType::CircOut => circ::ease_out,
            EaseType::Elastic => elastic::ease_in_out,
            EaseType::ElasticIn => elastic::ease_in,
            EaseType::ElasticOut => elastic::ease_out,
            EaseType::Back => back::ease_in_out,
            EaseType::BackIn => back::ease_in,
            EaseType::BackOut => back::ease_out,
            EaseType::Bounce => bounce::ease_in_out,
            EaseType::BounceIn => bounce::ease_in,
            EaseType::BounceOut => bounce::ease_out,
        };
        ease_func(t, 0.0, 1.0, 1.0)
    }
}
// Linear takes normalized time and returns unmodified value.
// This is the default ease function in addition to pennereq crate.
pub mod linear {
    #[inline]
    pub fn ease<T>(t: T, _b: T, _c: T, _d: T) -> T {
        t
    }
}

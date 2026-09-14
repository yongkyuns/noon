use crate::{
    Bounds2D64, GeometryResource, PathCommand, SemanticObjectContent, SemanticObjectState,
    SemanticStore, StoredGeometry, Transform2D, Vec2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectiveLayoutError {
    MissingGeometry,
    MissingText,
    Empty,
}
impl std::fmt::Display for EffectiveLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "effective layout unavailable: {self:?}")
    }
}
impl std::error::Error for EffectiveLayoutError {}

fn xy(t: Transform2D, x: f64, y: f64) -> (f64, f64) {
    let x = x * f64::from(t.scale.x);
    let y = y * f64::from(t.scale.y);
    let r = f64::from(t.rotation);
    let (s, c) = r.sin_cos();
    (
        x * c - y * s + f64::from(t.translation.x),
        x * s + y * c + f64::from(t.translation.y),
    )
}
fn include(b: &mut Option<Bounds2D64>, q: (f64, f64)) {
    if let Some(b) = b {
        b.include(q.0, q.1)
    } else {
        *b = Some(Bounds2D64::point(q.0, q.1))
    }
}
fn path_bounds(path: &crate::VectorPath, t: Transform2D) -> Option<Bounds2D64> {
    let mut b = None;
    let mut cur = None;
    let mut start = None;
    for cmd in path.commands() {
        match *cmd {
            PathCommand::MoveTo { to } => {
                let q = xy(t, to.x.into(), to.y.into());
                include(&mut b, q);
                cur = Some(q);
                start = Some(q)
            }
            PathCommand::LineTo { to } => {
                let q = xy(t, to.x.into(), to.y.into());
                if let Some(c) = cur {
                    include(&mut b, c)
                }
                include(&mut b, q);
                cur = Some(q)
            }
            PathCommand::QuadraticTo { control, to } => {
                let q = xy(t, to.x.into(), to.y.into());
                let c = xy(t, control.x.into(), control.y.into());
                if let Some(a) = cur {
                    include(&mut b, a);
                    include(
                        &mut b,
                        (a.0 + (c.0 - a.0) * 2.0 / 3.0, a.1 + (c.1 - a.1) * 2.0 / 3.0),
                    );
                    include(
                        &mut b,
                        (q.0 + (c.0 - q.0) * 2.0 / 3.0, q.1 + (c.1 - q.1) * 2.0 / 3.0),
                    );
                }
                include(&mut b, q);
                cur = Some(q)
            }
            PathCommand::CubicTo {
                control1,
                control2,
                to,
            } => {
                let q = xy(t, to.x.into(), to.y.into());
                if let Some(a) = cur {
                    include(&mut b, a)
                }
                include(&mut b, xy(t, control1.x.into(), control1.y.into()));
                include(&mut b, xy(t, control2.x.into(), control2.y.into()));
                include(&mut b, q);
                cur = Some(q)
            }
            PathCommand::Close => {
                if let Some(c) = cur {
                    include(&mut b, c)
                }
                if let Some(a) = start {
                    include(&mut b, a);
                    cur = Some(a)
                }
            }
        }
    }
    b
}
pub fn effective_layout_bounds(
    store: &SemanticStore,
    state: &SemanticObjectState,
    t: Transform2D,
) -> Result<Bounds2D64, EffectiveLayoutError> {
    let mut b = None;
    match state.content {
        SemanticObjectContent::Text(h) => {
            let r = store
                .text_resources()
                .get(h)
                .ok_or(EffectiveLayoutError::MissingText)?;
            for q in [
                r.bounds.min,
                Vec2::new(r.bounds.min.x, r.bounds.max.y),
                r.bounds.max,
                Vec2::new(r.bounds.max.x, r.bounds.min.y),
            ] {
                include(&mut b, xy(t, q.x.into(), q.y.into()))
            }
        }
        SemanticObjectContent::Geometry(g) => match g {
            StoredGeometry::Circle { radius } => {
                let r = f64::from(radius);
                let k = (4.0 / 3.0) * (std::f64::consts::PI / 16.0).tan();
                for i in 0..8 {
                    let a = i as f64 * std::f64::consts::PI / 4.0;
                    let e = (i + 1) as f64 * std::f64::consts::PI / 4.0;
                    let (sa, ca) = a.sin_cos();
                    let (se, ce) = e.sin_cos();
                    for (x, y) in [
                        (ca, sa),
                        (ca - k * sa, sa + k * ca),
                        (ce + k * se, se - k * ce),
                        (ce, se),
                    ] {
                        include(&mut b, xy(t, r * x, r * y))
                    }
                }
            }
            StoredGeometry::Rectangle { size } => {
                let x = f64::from(size.x) / 2.0;
                let y = f64::from(size.y) / 2.0;
                for q in [(-x, -y), (-x, y), (x, -y), (x, y)] {
                    include(&mut b, xy(t, q.0, q.1))
                }
            }
            StoredGeometry::Line { start, end } => {
                include(&mut b, xy(t, start.x.into(), start.y.into()));
                include(&mut b, xy(t, end.x.into(), end.y.into()))
            }
            StoredGeometry::Resource(h) => match store
                .geometry_resources()
                .get(h)
                .ok_or(EffectiveLayoutError::MissingGeometry)?
            {
                GeometryResource::VectorPath(path) => b = path_bounds(path, t),
            },
        },
    }
    b.ok_or(EffectiveLayoutError::Empty)
}
pub fn effective_layout_center(
    store: &SemanticStore,
    state: &SemanticObjectState,
    t: Transform2D,
) -> Result<Vec2, EffectiveLayoutError> {
    let b = effective_layout_bounds(store, state, t)?;
    Ok(Vec2::new(
        ((b.min_x + b.max_x) * 0.5) as f32,
        ((b.min_y + b.max_y) * 0.5) as f32,
    ))
}

//! Shared native counterparts of the pinned Special Camera Settings scenes.
//! The cases share setup; camera motion stays in the ordinary live timeline.
use crate::{
    AnimationOptions, CameraRotationAxis, Color, ContinuationStep, LiveContinuation, LiveProgram,
    LiveSession, ManimThreeDAxesOptions, Mobject, MobjectTarget, RateFunction, Scene,
    SemanticSpatialCompositionDomain, SemanticVec3, StyleUpdate, SurfaceOptions, SurfaceSample,
    Text, UvSurfacePlan, WorldAffineEdit,
};
use noon_core::ManimCamera3DProfile;
use std::f64::consts::{FRAC_PI_2, PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraCase {
    FixedFrame,
    Ambient,
    Illusion,
    Light,
    Surface,
}

impl CameraCase {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "fixed-frame" => Some(Self::FixedFrame),
            "ambient" => Some(Self::Ambient),
            "illusion" => Some(Self::Illusion),
            "light" => Some(Self::Light),
            "surface" => Some(Self::Surface),
            _ => None,
        }
    }
    pub fn duration(self) -> f64 {
        match self {
            Self::FixedFrame => 1.,
            Self::Ambient => 3.,
            Self::Illusion => FRAC_PI_2,
            Self::Light | Self::Surface => 0.,
        }
    }
}

fn profile(case: CameraCase) -> ManimCamera3DProfile {
    ManimCamera3DProfile {
        phi: 75. * PI / 180.,
        theta: match case {
            CameraCase::FixedFrame => -45.,
            CameraCase::Surface => -30.,
            _ => 30.,
        } * PI
            / 180.,
        gamma: 0.,
        focal_distance: 20.,
        zoom: 1.,
        frame_height: 8.,
        frame_center: SemanticVec3::ZERO,
    }
}

pub fn scene(case: CameraCase) -> Result<(Scene, Mobject), String> {
    let mut scene = Scene::new();
    let camera = scene
        .camera_3d_profile(profile(case), 0.1, 100.)
        .map_err(|e| e.to_string())?;
    let axes = scene
        .three_d_axes(&ManimThreeDAxesOptions::default())
        .map_err(|e| e.to_string())?;
    match case {
        CameraCase::FixedFrame => {
            let mut label = scene
                .text(Text::new("This is a 3D text"))
                .map_err(|e| e.to_string())?;
            label
                .align_on_frame(-1., 1., 0.5)
                .map_err(|e| e.to_string())?;
            scene
                .add_in_spatial_composition_domain(
                    MobjectTarget::Object(&label),
                    SemanticSpatialCompositionDomain::FixedFrame,
                )
                .map_err(|e| e.to_string())?;
        }
        CameraCase::Ambient | CameraCase::Illusion => {
            // The qualified World path profile uses object-scaled strokes.
            let mut options = crate::ManimGeometryOptions::circle(1.).map_err(|e| e.to_string())?;
            options
                .set_stroke_width_mode("scale_with_object")
                .map_err(|e| e.to_string())?;
            let circle = scene.geometry(options).map_err(|e| e.to_string())?;
            scene
                .add_in_spatial_composition_domain(
                    MobjectTarget::Object(&circle),
                    SemanticSpatialCompositionDomain::World,
                )
                .map_err(|e| e.to_string())?;
        }
        CameraCase::Light | CameraCase::Surface => {
            let light = scene
                .point_light_3d(
                    if case == CameraCase::Light {
                        SemanticVec3::new(0., 0., -3.)
                    } else {
                        SemanticVec3::new(-7., -9., 10.)
                    },
                    Color::WHITE,
                    1.,
                )
                .map_err(|e| e.to_string())?;
            scene.add(&light).map_err(|e| e.to_string())?;
            // Both pinned examples add axes before their surface. Preserve
            // that order in shared runtime state as well as rendered pixels.
            scene
                .add_in_spatial_composition_domain(
                    MobjectTarget::Family(axes.family()),
                    SemanticSpatialCompositionDomain::World,
                )
                .map_err(|e| e.to_string())?;
            let plan = if case == CameraCase::Light {
                UvSurfacePlan::new([-FRAC_PI_2, FRAC_PI_2], [0., TAU], [15, 32])
            } else {
                UvSurfacePlan::new([-2., 2.], [-2., 2.], [24, 24])
            }
            .map_err(|e| e.to_string())?;
            let grid = plan
                .sample(|u, v| {
                    SurfaceSample::position(if case == CameraCase::Light {
                        SemanticVec3::new(
                            1.5 * u.cos() * v.cos(),
                            1.5 * u.cos() * v.sin(),
                            1.5 * u.sin(),
                        )
                    } else {
                        SemanticVec3::new(u, v, (-(u * u + v * v) / (2. * 0.4 * 0.4)).exp())
                    })
                })
                .map_err(|e| e.to_string())?;
            let mut options = SurfaceOptions::default();
            if case == CameraCase::Light {
                options.fill_colors = [Color::RED_D, Color::RED_E];
            }
            let surface = scene
                .surface_family(grid, options)
                .map_err(|e| e.to_string())?;
            if case == CameraCase::Surface {
                scene
                    .world_affine(
                        MobjectTarget::Family(surface.family()),
                        WorldAffineEdit::Scale {
                            factor: 2.,
                            about: Some(SemanticVec3::ZERO),
                        },
                    )
                    .map_err(|e| e.to_string())?;
                surface
                    .set_style(StyleUpdate {
                        fill_opacity: Some(1.),
                        stroke_color: Some(Color::GREEN),
                        ..Default::default()
                    })
                    .map_err(|e| e.to_string())?;
                scene
                    .set_surface_checkerboard(&surface, [Color::ORANGE, Color::BLUE], 0.5)
                    .map_err(|e| e.to_string())?;
            }
            scene
                .add_in_spatial_composition_domain(
                    MobjectTarget::Family(surface.family()),
                    SemanticSpatialCompositionDomain::World,
                )
                .map_err(|e| e.to_string())?;
        }
    }
    if !matches!(case, CameraCase::Light | CameraCase::Surface) {
        scene
            .add_in_spatial_composition_domain(
                MobjectTarget::Family(axes.family()),
                SemanticSpatialCompositionDomain::World,
            )
            .map_err(|e| e.to_string())?;
    }
    Ok((scene, camera))
}

pub struct CameraContinuation {
    case: CameraCase,
    camera: Mobject,
    stage: u8,
}
impl LiveContinuation for CameraContinuation {
    type Error = String;
    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        let step = match (self.case, self.stage) {
            (CameraCase::FixedFrame, 0) => live.wait_segment(1.),
            (CameraCase::Ambient, 0) => {
                live.begin_ambient_camera_rotation(&self.camera, CameraRotationAxis::Theta, 0.1)
                    .map_err(|e| e.to_string())?;
                live.wait_segment(1.)
            }
            (CameraCase::Ambient, 1) => {
                live.stop_ambient_camera_rotation(&self.camera)
                    .map_err(|e| e.to_string())?;
                live.move_camera_profile(
                    &self.camera,
                    profile(self.case),
                    AnimationOptions::new()
                        .run_time(1.)
                        .rate_func(RateFunction::Smooth),
                )
            }
            (CameraCase::Ambient, 2) => live.wait_segment(1.),
            (CameraCase::Illusion, 0) => {
                live.begin_3dillusion_camera_rotation(&self.camera, 2., None, None)
                    .map_err(|e| e.to_string())?;
                live.wait_segment(FRAC_PI_2)
            }
            (CameraCase::Illusion, 1) => {
                live.stop_3dillusion_camera_rotation(&self.camera)
                    .map_err(|e| e.to_string())?;
                self.stage += 1;
                return Ok(ContinuationStep::Finished);
            }
            _ => return Ok(ContinuationStep::Finished),
        };
        self.stage += 1;
        step.map(ContinuationStep::Await).map_err(|e| e.to_string())
    }
}

pub fn program(case: CameraCase) -> Result<LiveProgram<CameraContinuation>, String> {
    let (scene, camera) = scene(case)?;
    scene
        .into_live_program(CameraContinuation {
            case,
            camera,
            stage: 0,
        })
        .map_err(|e| e.to_string())
}

pub fn static_session(case: CameraCase) -> Result<crate::ExecutionSession, String> {
    if case.duration() != 0. {
        return Err("static camera session requires a static case".into());
    }
    scene(case)?
        .0
        .execution_session()
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use noon_compile::CompiledSpatialDrawKind::{Mesh, Planar};

    #[test]
    fn static_camera_examples_preserve_authored_axes_surface_order() {
        for (case, first, last) in [
            (CameraCase::Light, Planar, Mesh),
            (CameraCase::Surface, Planar, Mesh),
        ] {
            let session = static_session(case).unwrap();
            let draws: Vec<_> = session
                .painter_order()
                .iter()
                .map(|&index| {
                    session.frame().objects[index as usize]
                        .spatial
                        .as_deref()
                        .unwrap()
                })
                .filter(|state| state.camera_projection.is_none() && !state.point_light)
                .map(|state| state.draw_kind)
                .collect();
            assert_eq!(*draws.first().unwrap(), first);
            assert_eq!(*draws.last().unwrap(), last);
        }
    }
}

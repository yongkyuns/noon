use js_sys::Array;
use noon::integration::AuthoringFailure;
use noon::{
    Joint2d, JointAttachment2d, JointMapping2d, JointTrack2d, RetargetedRig2d, RigArtwork2d,
    RigArtworkTarget2d, RigPose2d,
};
use wasm_bindgen::prelude::*;

use super::{
    js_error, typed_js_error, CanonicalAuthoringSceneContext, WasmSceneMembershipBatch,
    WasmValueTrackerHandle,
};
use crate::{WasmAuthoringFamilyHandle, WasmAuthoringMobjectHandle, WasmFamilyCopy};

#[derive(Default)]
#[wasm_bindgen]
pub struct WasmRig2dBatch {
    joints: Vec<Joint2d>,
    tracks: Vec<JointTrack2d>,
    attachments: Vec<JointAttachment2d>,
    mapping: Vec<JointMapping2d>,
}

#[wasm_bindgen]
impl WasmRig2dBatch {
    pub fn joint(&mut self, name: String, x: f64, y: f64, rest_angle: f64, parent: Option<String>) {
        self.joints.push(Joint2d {
            name,
            parent,
            offset: [x, y],
            rest_angle,
        });
    }

    pub fn track(&mut self, joint: String, keys: Vec<f32>) -> Result<(), JsValue> {
        let (keys, remainder) = keys.as_chunks::<2>();
        if !remainder.is_empty() {
            return Err(js_error("2D rig keys require pairs"));
        }
        self.tracks.push(JointTrack2d {
            joint,
            keys: keys.to_vec(),
        });
        Ok(())
    }

    #[wasm_bindgen(js_name = mapTrack)]
    pub fn map_track(&mut self, source: String, target: String, gain: f32) {
        self.mapping.push(JointMapping2d {
            source,
            target,
            gain,
        });
    }

    pub fn prepare(self) -> Result<WasmRetargetedRig2d, JsValue> {
        RetargetedRig2d::new(self.joints, &self.tracks, &self.mapping)
            .map(WasmRetargetedRig2d)
            .map_err(typed_js_error)
    }

    pub fn attach(&mut self, joint: String, object: &WasmAuthoringMobjectHandle, x: f64, y: f64) {
        self.attachments.push(JointAttachment2d {
            joint,
            object: object.semantic_mobject().clone(),
            offset: [x, y],
        });
    }
}

#[wasm_bindgen]
impl CanonicalAuthoringSceneContext {
    #[wasm_bindgen(js_name = beginRig2d)]
    pub fn begin_rig_2d(&self) -> WasmRig2dBatch {
        WasmRig2dBatch::default()
    }

    #[wasm_bindgen(js_name = rigPoseTarget)]
    pub fn rig_pose_target(
        &mut self,
        pose: &WasmRigPose2d,
        source: &WasmAuthoringMobjectHandle,
        chain: Option<Array>,
    ) -> Result<WasmAuthoringMobjectHandle, JsValue> {
        let names = chain.map(chain_names).transpose()?;
        let chain = names
            .as_ref()
            .map(|names| names.iter().map(String::as_str).collect::<Vec<_>>());
        let scene = self.inner.local_authoring_scene().map_err(typed_js_error)?;
        pose.0
            .target_for(scene, source.semantic_mobject(), chain.as_deref())
            .map(WasmAuthoringMobjectHandle::from_semantic_mobject)
            .map_err(AuthoringFailure::from_scene_capture)
            .map_err(typed_js_error)
    }

    #[wasm_bindgen(js_name = rigPoseFamilyTarget)]
    pub fn rig_pose_family_target(
        &mut self,
        pose: &WasmRigPose2d,
        source: &WasmAuthoringFamilyHandle,
        references: WasmSceneMembershipBatch,
        chain: Option<Array>,
    ) -> Result<WasmFamilyCopy, JsValue> {
        let names = chain.map(chain_names).transpose()?;
        let chain = names
            .as_ref()
            .map(|names| names.iter().map(String::as_str).collect::<Vec<_>>());
        let source = source.semantic_family()?;
        let scene = self.inner.local_authoring_scene().map_err(typed_js_error)?;
        pose.0
            .target_for_family_with_references(
                scene,
                &source,
                &references.copy_references().map_err(typed_js_error)?,
                chain.as_deref(),
            )
            .map(WasmFamilyCopy::from_copy)
            .map_err(AuthoringFailure::from_scene_capture)
            .map_err(typed_js_error)
    }

    #[wasm_bindgen(js_name = rigArtworkTarget)]
    pub fn rig_artwork_target(
        &mut self,
        artwork: &WasmRigArtworkTarget2d,
        phase: f32,
        references: WasmSceneMembershipBatch,
    ) -> Result<WasmFamilyCopy, JsValue> {
        let scene = self.inner.local_authoring_scene().map_err(typed_js_error)?;
        artwork
            .0
            .target_at_with_references(
                scene,
                phase,
                &references.copy_references().map_err(typed_js_error)?,
            )
            .map(WasmFamilyCopy::from_entry_copy)
            .map_err(AuthoringFailure::from_scene_capture)
            .map_err(typed_js_error)
    }

    #[wasm_bindgen(js_name = bindRig2d)]
    pub fn bind_rig_2d(
        &mut self,
        tracker: &WasmValueTrackerHandle,
        batch: WasmRig2dBatch,
    ) -> Result<(), JsValue> {
        let tracker = tracker.tracker_in(self.inner.scene.integration_store())?;
        self.inner
            .local_authoring_scene()
            .map_err(typed_js_error)?
            .bind_rig_2d(tracker, &batch.joints, &batch.tracks, &batch.attachments)
            .map_err(typed_js_error)
    }
}

#[wasm_bindgen]
pub struct WasmRetargetedRig2d(RetargetedRig2d);

#[wasm_bindgen]
impl WasmRetargetedRig2d {
    pub fn pose(&self, phase: f32) -> Result<WasmRigPose2d, JsValue> {
        self.0
            .pose(phase)
            .map(WasmRigPose2d)
            .map_err(typed_js_error)
    }

    #[wasm_bindgen(js_name = pointingTracks)]
    pub fn pointing_tracks(
        &self,
        wrist: &str,
        targets: Array,
        fingertip: f64,
        bend_left: bool,
    ) -> Result<Array, JsValue> {
        let targets = targets
            .iter()
            .map(|row| {
                let row = row
                    .dyn_into::<Array>()
                    .ok()
                    .filter(|row| row.length() == 3)
                    .ok_or_else(|| js_error("2D pointing requires (phase, x, y) rows"))?;
                let number = |index| {
                    row.get(index).as_f64().ok_or_else(|| {
                        js_error("2D pointing phases and coordinates require numbers")
                    })
                };
                Ok((number(0)? as f32, [number(1)?, number(2)?]))
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        let tracks = self
            .0
            .pointing_tracks(wrist, &targets, fingertip, bend_left)
            .map_err(typed_js_error)?;
        let rows = Array::new();
        for track in tracks {
            let keys = Array::new();
            for [phase, angle] in track.keys {
                keys.push(&Array::of2(&phase.into(), &angle.into()));
            }
            rows.push(&Array::of2(&track.joint.into(), &keys));
        }
        Ok(rows)
    }

    #[wasm_bindgen(js_name = artworkTrack)]
    pub fn artwork_track(
        &self,
        joint: &str,
        poses: Array,
        cues: Array,
    ) -> Result<WasmRigArtwork2d, JsValue> {
        let poses = poses
            .iter()
            .map(|row| {
                let row = artwork_pair(row)?;
                let name = row
                    .get(0)
                    .as_string()
                    .ok_or_else(|| js_error("2D artwork pose requires a name"))?;
                let points = row
                    .get(1)
                    .dyn_into::<Array>()
                    .map_err(|_| js_error("2D artwork pose requires points"))?;
                let points = points
                    .iter()
                    .map(|point| {
                        let point = artwork_pair(point)?;
                        Ok([artwork_number(point.get(0))?, artwork_number(point.get(1))?])
                    })
                    .collect::<Result<_, JsValue>>()?;
                Ok((name, points))
            })
            .collect::<Result<_, JsValue>>()?;
        self.0
            .artwork_track(joint, poses, &artwork_cues(cues)?)
            .map(WasmRigArtwork2d)
            .map_err(typed_js_error)
    }

    #[wasm_bindgen(js_name = artworkObjectTrack)]
    pub fn artwork_object_track(
        &self,
        joint: &str,
        names: Array,
        poses: WasmSceneMembershipBatch,
        cues: Array,
    ) -> Result<WasmRigArtworkTarget2d, JsValue> {
        let targets = poses.copy_references().map_err(typed_js_error)?;
        if names.length() as usize != targets.len() {
            return Err(js_error(
                "2D artwork drawings require corresponding pose names",
            ));
        }
        let poses = names
            .iter()
            .zip(targets)
            .map(|(name, target)| {
                Ok((
                    name.as_string()
                        .ok_or_else(|| js_error("2D artwork pose requires a name"))?,
                    target,
                ))
            })
            .collect::<Result<Vec<_>, JsValue>>()?;
        self.0
            .artwork_track_for(joint, &poses, &artwork_cues(cues)?)
            .map(WasmRigArtworkTarget2d)
            .map_err(typed_js_error)
    }
}

#[wasm_bindgen]
pub struct WasmRigArtwork2d(RigArtwork2d);

#[wasm_bindgen]
impl WasmRigArtwork2d {
    #[wasm_bindgen(js_name = pointsAt)]
    pub fn points_at(&self, phase: f32) -> Result<Vec<f64>, JsValue> {
        self.0
            .points_at(phase)
            .map(|points| points.into_flattened())
            .map_err(typed_js_error)
    }
}

#[wasm_bindgen]
pub struct WasmRigArtworkTarget2d(RigArtworkTarget2d);

fn artwork_cues(cues: Array) -> Result<Vec<(f32, String)>, JsValue> {
    cues.iter()
        .map(|row| {
            let row = artwork_pair(row)?;
            let phase = artwork_number(row.get(0))? as f32;
            let name = row
                .get(1)
                .as_string()
                .ok_or_else(|| js_error("2D artwork cue requires a pose name"))?;
            Ok((phase, name))
        })
        .collect()
}

fn artwork_pair(row: JsValue) -> Result<Array, JsValue> {
    let row = row
        .dyn_into::<Array>()
        .map_err(|_| js_error("2D artwork rows require pairs"))?;
    if row.length() != 2 {
        return Err(js_error("2D artwork rows require pairs"));
    }
    Ok(row)
}

fn artwork_number(value: JsValue) -> Result<f64, JsValue> {
    value
        .as_f64()
        .ok_or_else(|| js_error("2D artwork coordinates and phases require numbers"))
}

#[wasm_bindgen]
pub struct WasmRigPose2d(RigPose2d);

#[wasm_bindgen]
impl WasmRigPose2d {
    pub fn joint(&self, name: &str) -> Result<Vec<f64>, JsValue> {
        self.0
            .joint(name)
            .map(|joint| joint.to_vec())
            .map_err(typed_js_error)
    }

    #[wasm_bindgen(js_name = deformPoints)]
    pub fn deform_points(&self, chain: Array, points: Vec<f64>) -> Result<Vec<f64>, JsValue> {
        let (points, remainder) = points.as_chunks::<2>();
        if !remainder.is_empty() {
            return Err(js_error("2D rig points require pairs"));
        }
        let names = chain_names(chain)?;
        self.0
            .deform_points(&names, points)
            .map(|points| points.into_flattened())
            .map_err(typed_js_error)
    }
}

fn chain_names(chain: Array) -> Result<Vec<String>, JsValue> {
    chain
        .iter()
        .map(|name| {
            name.as_string()
                .ok_or_else(|| js_error("2D rig chain requires joint names"))
        })
        .collect()
}

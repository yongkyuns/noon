//! Retained member queries select one shared scene through live publications.
use crate::{
    Color, CompositeEntryHandle, ContinuationStep, LiveContinuation, LiveProgram, LiveSession,
    LiveSessionError, MobjectFamily, Scene,
};

pub struct FamilyMemberQueries {
    root: MobjectFamily,
    visible: MobjectFamily,
    children: Vec<CompositeEntryHandle>,
    shown: Vec<CompositeEntryHandle>,
    points: Vec<(f64, f64)>,
    stage: u8,
}

impl FamilyMemberQueries {
    fn show(
        &mut self,
        live: &mut LiveSession<'_>,
        selected: Vec<CompositeEntryHandle>,
    ) -> Result<(), LiveSessionError> {
        let old = self
            .shown
            .iter()
            .map(CompositeEntryHandle::as_target)
            .collect::<Vec<_>>();
        live.remove_family_members(&self.visible, &old)?;
        let new = selected
            .iter()
            .map(CompositeEntryHandle::as_target)
            .collect::<Vec<_>>();
        live.add_family_members(&self.visible, &new)?;
        self.shown = selected;
        Ok(())
    }
}

impl LiveContinuation for FamilyMemberQueries {
    type Error = String;

    fn resume(&mut self, live: &mut LiveSession<'_>) -> Result<ContinuationStep, String> {
        if self.stage == 3 {
            return Ok(ContinuationStep::Finished);
        }
        let step = (|| -> Result<_, LiveSessionError> {
            match self.stage {
                0 => {}
                1 => {
                    let first = self.root.split_at(0)?.expect("three current children");
                    let second = self.root.split_at(1)?.expect("three current children");
                    assert_eq!([&first, &second], [&self.children[0], &self.children[1]]);
                    self.show(live, vec![first, second])?;
                }
                2 => {
                    live.set_family_points_f64(&self.root, &self.points)?;
                    let own = CompositeEntryHandle::Family(self.root.clone());
                    assert_eq!(self.root.get_num_points()?, 16);
                    assert_eq!(self.root.split_count()?, 4);
                    let mut expected = vec![own.clone()];
                    expected.extend(self.children.iter().cloned());
                    assert_eq!(self.root.split_members()?, expected);
                    assert_eq!(self.root.split_at(0)?, Some(own.clone()));
                    self.show(live, vec![own])?;
                }
                _ => unreachable!("three held query selections"),
            }
            self.stage += 1;
            live.wait_segment(0.25).map(ContinuationStep::Await)
        })();
        step.map_err(|error| error.to_string())
    }
}

/// The same typed program feeds native capture and direct Rust/WASM rendering.
pub fn program() -> Result<LiveProgram<FamilyMemberQueries>, String> {
    let build = || -> Result<_, Box<dyn std::error::Error>> {
        let mut scene = Scene::new();
        let mut red = scene.circle(0.3)?;
        let mut blue = scene.square(0.6)?;
        let mut yellow = scene.circle(0.3)?;
        let mut extra = scene.circle(0.3)?;
        red.shift(-3.0, 0.0)?;
        blue.shift(-1.0, 0.0)?;
        yellow.shift(1.0, 0.0)?;
        extra.shift(3.0, 0.0)?;
        for (shape, color) in [
            (&mut red, (1.0, 0.0, 0.0)),
            (&mut blue, (0.0, 0.0, 1.0)),
            (&mut yellow, (1.0, 1.0, 0.0)),
            (&mut extra, (0.0, 1.0, 0.0)),
        ] {
            shape.set_fill(color.0, color.1, color.2, 1.0)?;
            shape.set_stroke_width(0.0)?;
        }
        let root = scene.family_with_presentation(
            &[(&red).into(), (&blue).into(), (&yellow).into()],
            0.0,
            Color::WHITE,
            false,
        )?;
        let captured = root.split_iter()?;
        let plan = root.slice_plan()?;
        root.add((&extra).into())?;
        root.remove((&red).into())?;
        let shown = plan
            .select_slice(Some(0), None, 2)?
            .expect("captured list has three members");
        let red_entry = CompositeEntryHandle::Mobject(red);
        let blue_entry = CompositeEntryHandle::Mobject(blue);
        let yellow_entry = CompositeEntryHandle::Mobject(yellow);
        assert_eq!(shown, vec![red_entry.clone(), yellow_entry.clone()]);
        assert_eq!(
            captured.collect::<Result<Vec<_>, _>>()?,
            vec![red_entry, blue_entry.clone(), yellow_entry.clone()]
        );
        let children = vec![
            blue_entry,
            yellow_entry,
            CompositeEntryHandle::Mobject(extra),
        ];
        assert_eq!(root.split_members()?, children);
        let targets = shown
            .iter()
            .map(CompositeEntryHandle::as_target)
            .collect::<Vec<_>>();
        let visible = scene.family_with_presentation(&targets, 0.0, Color::WHITE, false)?;
        let mut donor = scene.square(0.5)?;
        donor.shift(0.0, -2.0)?;
        let points = donor.path_query()?.points()?;
        scene.add_many(&[(&visible).into()])?;
        Ok(scene.into_live_program(FamilyMemberQueries {
            root,
            visible,
            children,
            shown,
            points,
            stage: 0,
        })?)
    };
    build().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiveProgramStatus, RustHostCallbackTable};

    #[test]
    fn captured_queries_and_current_ranks_share_live_scene_membership() {
        let mut program = program().unwrap();
        let mut callbacks = RustHostCallbackTable::new();
        for end in [0.25, 0.5, 0.75] {
            assert!(matches!(
                program.resume().unwrap(),
                LiveProgramStatus::Awaiting(_)
            ));
            match program.drive_to(&mut callbacks, end).unwrap() {
                LiveProgramStatus::PublicationPending(expected) => {
                    let actual = program.take_renderer_publication().context();
                    assert_eq!(actual, expected);
                    program.admit_publication(actual).unwrap();
                }
                LiveProgramStatus::ReadyToResume => {}
                other => panic!("member query completion: {other:?}"),
            }
        }
        assert_eq!(program.resume().unwrap(), LiveProgramStatus::Finished);
    }
}

//! Read-only navigation over the Scene's existing authored section boundaries.

use super::{Scene, SceneSection};

/// Direction for finding a section boundary relative to an authored time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionNavigation {
    Previous,
    Next,
}

/// Immutable navigation metadata copied from one authored Scene.
///
/// Hosts may retain this small value when they move the execution session into a
/// player. It contains no playhead and never seeks or schedules execution.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneSectionNavigation {
    sections: Vec<SceneSection>,
}

impl SceneSectionNavigation {
    pub fn sections(&self) -> &[SceneSection] {
        &self.sections
    }

    /// The same strict, non-wrapping query as [`Scene::section_target`].
    pub fn target(
        &self,
        time: f64,
        direction: SectionNavigation,
    ) -> Result<Option<(usize, &SceneSection)>, String> {
        target(&self.sections, time, direction)
    }
}

impl Scene {
    /// Capture immutable section labels for a host without retaining the Scene
    /// or constructing another timeline. Later authored declarations do not
    /// change this value; replace it explicitly when replacing the scene.
    pub fn section_navigation(&self) -> SceneSectionNavigation {
        SceneSectionNavigation {
            sections: self.sections.clone(),
        }
    }

    /// Find the nearest section boundary strictly before or after `time`.
    ///
    /// The returned index refers to [`Self::sections`]. When several declarations
    /// share the selected time, the first declaration is returned. All section
    /// types participate, including [`super::SectionType::Skip`]; their export
    /// intent is unchanged. Names need not be unique.
    ///
    /// This query neither seeks execution nor changes the authored cursor. It
    /// uses O(log n) time and O(1) extra space over the existing ordered sections,
    /// with no wrapping or tolerance that could merge nearby distinct times.
    /// Returns `None` when there is no boundary in the requested direction.
    ///
    /// ```
    /// use noon::{Scene, SectionNavigation, SectionType};
    ///
    /// let mut scene = Scene::new();
    /// scene.next_section("Introduction", SectionType::Normal)?;
    /// scene.wait(3.0)?;
    /// scene.next_section("Example", SectionType::Normal)?;
    /// let (index, section) = scene
    ///     .section_target(0.0, SectionNavigation::Next)?
    ///     .unwrap();
    /// assert_eq!(index, 1);
    /// assert_eq!(section.time, 3.0);
    /// # Ok::<(), String>(())
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if `time` is negative or nonfinite.
    pub fn section_target(
        &self,
        time: f64,
        direction: SectionNavigation,
    ) -> Result<Option<(usize, &SceneSection)>, String> {
        target(&self.sections, time, direction)
    }
}

fn target(
    sections: &[SceneSection],
    time: f64,
    direction: SectionNavigation,
) -> Result<Option<(usize, &SceneSection)>, String> {
    if !time.is_finite() || time < 0.0 {
        return Err("section navigation time must be finite and non-negative".into());
    }

    let index = match direction {
        SectionNavigation::Next => sections.partition_point(|section| section.time <= time),
        SectionNavigation::Previous => {
            let before = sections.partition_point(|section| section.time < time);
            let Some(last) = before.checked_sub(1) else {
                return Ok(None);
            };
            let target_time = sections[last].time;
            sections.partition_point(|section| section.time < target_time)
        }
    };

    Ok(sections.get(index).map(|section| (index, section)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SectionType;

    fn target(scene: &Scene, time: f64, direction: SectionNavigation) -> Option<(usize, f64)> {
        scene
            .section_target(time, direction)
            .unwrap()
            .map(|(index, section)| {
                assert!(std::ptr::eq(section, &scene.sections()[index]));
                (index, section.time)
            })
    }

    #[test]
    fn empty_sections_have_no_navigation_target() {
        let scene = Scene::new();
        for time in [0.0, 1.0, f64::MAX] {
            for direction in [SectionNavigation::Previous, SectionNavigation::Next] {
                assert_eq!(target(&scene, time, direction), None);
            }
        }
    }

    #[test]
    fn host_navigation_retains_only_its_captured_declarations() {
        let mut scene = Scene::new();
        scene
            .next_section_at("begin", SectionType::Normal, 0.0)
            .unwrap();
        scene
            .next_section_at("discover", SectionType::Normal, 10.0)
            .unwrap();
        let navigation = scene.section_navigation();
        scene
            .next_section_at("resolve", SectionType::Normal, 20.0)
            .unwrap();
        assert_eq!(navigation.sections().len(), 2);
        assert_eq!(
            navigation
                .target(0.0, SectionNavigation::Next)
                .unwrap()
                .unwrap()
                .1
                .time,
            10.0
        );
        assert!(navigation
            .target(10.0, SectionNavigation::Next)
            .unwrap()
            .is_none());
        assert!(navigation
            .target(f64::NAN, SectionNavigation::Next)
            .is_err());
        assert_eq!(
            scene
                .section_target(10.0, SectionNavigation::Next)
                .unwrap()
                .unwrap()
                .1
                .time,
            20.0
        );
    }

    #[test]
    fn navigation_is_strict_and_does_not_wrap() {
        let mut scene = Scene::new();
        for (name, time) in [("intro", 1.0), ("body", 2.0), ("end", 4.0)] {
            scene
                .next_section_at(name, SectionType::Normal, time)
                .unwrap();
        }

        for (time, previous, next) in [
            (0.0, None, Some((0, 1.0))),
            (1.0, None, Some((1, 2.0))),
            (1.5, Some((0, 1.0)), Some((1, 2.0))),
            (2.0, Some((0, 1.0)), Some((2, 4.0))),
            (3.0, Some((1, 2.0)), Some((2, 4.0))),
            (4.0, Some((1, 2.0)), None),
            (5.0, Some((2, 4.0)), None),
        ] {
            assert_eq!(target(&scene, time, SectionNavigation::Previous), previous);
            assert_eq!(target(&scene, time, SectionNavigation::Next), next);
        }
    }

    #[test]
    fn equal_time_targets_use_the_first_declaration_and_include_skip_sections() {
        let mut scene = Scene::new();
        for (time, section_type) in [
            (0.0, SectionType::Skip),
            (0.0, SectionType::Normal),
            (1.0, SectionType::Normal),
            (1.0, SectionType::Skip),
            (2.0, SectionType::Skip),
            (2.0, SectionType::Normal),
        ] {
            scene
                .next_section_at("same name", section_type, time)
                .unwrap();
        }

        for (time, previous, next) in [
            (0.0, None, Some((2, 1.0))),
            (0.5, Some((0, 0.0)), Some((2, 1.0))),
            (1.0, Some((0, 0.0)), Some((4, 2.0))),
            (1.5, Some((2, 1.0)), Some((4, 2.0))),
            (2.0, Some((2, 1.0)), None),
            (3.0, Some((4, 2.0)), None),
        ] {
            assert_eq!(target(&scene, time, SectionNavigation::Previous), previous);
            assert_eq!(target(&scene, time, SectionNavigation::Next), next);
        }
        assert_eq!(scene.sections().len(), 6);
    }

    #[test]
    fn adjacent_representable_times_remain_distinct() {
        let mut scene = Scene::new();
        let first = 1.0_f64;
        let second = f64::from_bits(first.to_bits() + 1);
        let third = f64::from_bits(first.to_bits() + 2);
        for time in [first, second, third] {
            scene
                .next_section_at("close boundary", SectionType::Normal, time)
                .unwrap();
        }

        assert_eq!(
            target(&scene, first, SectionNavigation::Next),
            Some((1, second))
        );
        assert_eq!(
            target(&scene, second, SectionNavigation::Previous),
            Some((0, first))
        );
        assert_eq!(
            target(&scene, second, SectionNavigation::Next),
            Some((2, third))
        );
        assert_eq!(
            target(&scene, third, SectionNavigation::Previous),
            Some((1, second))
        );
    }

    #[test]
    fn queries_preserve_sections_authored_time_and_execution_publication() {
        let mut scene = Scene::new();
        let square = scene.square(1.0).unwrap();
        scene.add(&square).unwrap();
        scene.next_section("intro", SectionType::Normal).unwrap();
        scene.wait(2.0).unwrap();
        scene.next_section("end", SectionType::Skip).unwrap();

        let mut session = scene.execution_session().unwrap();
        session.take_frame_changes();
        let sections = scene.sections().to_vec();
        let time = scene.time();
        let revision = scene.revision();
        let publication = session.publication_context();
        let frame_time = session.frame().time;

        for direction in [SectionNavigation::Previous, SectionNavigation::Next] {
            for invalid in [
                f64::NAN,
                f64::INFINITY,
                f64::NEG_INFINITY,
                -1.0,
                -f64::from_bits(1),
            ] {
                assert!(scene.section_target(invalid, direction).is_err());
            }
            assert!(scene.section_target(1.0, direction).unwrap().is_some());
        }

        assert_eq!(scene.sections(), sections.as_slice());
        assert_eq!(scene.time(), time);
        assert_eq!(scene.revision(), revision);
        assert_eq!(session.publication_context(), publication);
        assert_eq!(session.frame().time, frame_time);
        assert!(session.take_frame_changes().is_empty());
    }
}

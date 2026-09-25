//! SampleSpace mutations borrow the current live session; they never create a
//! second runtime or publish directly into its semantic store.
use super::*;

impl SemanticExecutionPlayer {
    pub(crate) fn live_create_sample_space(
        &mut self,
        options: &noon::SampleSpaceOptions,
    ) -> Result<noon::SampleSpace, AuthoringFailure> {
        self.with_live_session(|live| {
            noon::SampleSpace::new_live(live, options).map_err(Into::into)
        })
    }

    pub(crate) fn live_get_sample_space_division(
        &mut self,
        sample_space: &noon::SampleSpace,
        probabilities: &[f64],
        colors: &[noon::Color],
        vertical: bool,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        self.with_live_session(|live| {
            if vertical {
                sample_space.get_vertical_division_live(live, probabilities.iter().copied(), colors)
            } else {
                sample_space.get_horizontal_division_live(
                    live,
                    probabilities.iter().copied(),
                    colors,
                )
            }
            .map_err(Into::into)
        })
    }

    pub(crate) fn live_divide_sample_space(
        &mut self,
        sample_space: &mut noon::SampleSpace,
        probabilities: &[f64],
        colors: &[noon::Color],
        vertical: bool,
    ) -> Result<noon::MobjectFamily, AuthoringFailure> {
        self.with_live_session(|live| {
            if vertical {
                sample_space.divide_vertically_live(live, probabilities.iter().copied(), colors)
            } else {
                sample_space.divide_horizontally_live(live, probabilities.iter().copied(), colors)
            }
            .map_err(Into::into)
        })
    }
}

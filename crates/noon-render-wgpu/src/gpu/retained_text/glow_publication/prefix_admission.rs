//! Reject unsupported effective inputs before any renderer work is recorded.
//! Timeline evaluation and playback stay owned by Runtime. This guard borrows
//! the resulting typed publication and shares the compiler/worker capability
//! rule, including the first physical member of an isolated target.
use super::*;
use noon_compile::{PrefixAdmission, PrefixAdmissionChanges, PrefixAdmissionView};

struct View<'a, 'b> {
    publication: &'a RendererPublication<'b>,
}
impl PrefixAdmissionView for View<'_, '_> {
    fn prefix_owner(&self, owner: ObjectId) -> Option<(ObjectId, Option<usize>)> {
        let index = self.publication.object_index(owner)?;
        let row = &self.publication.frame().objects[index];
        if row.effects.as_ref()?.input_role() != noon_core::EffectInputRole::PainterPrefix {
            return None;
        }
        let anchor = row
            .content
            .composed_source()
            .map_or(owner, |source| source.members()[0]);
        let rank = self
            .publication
            .object_index(anchor)
            .and_then(|index| self.publication.painter_rank(index));
        Some((anchor, rank))
    }
    fn unsupported_prefix_rank(&self, input: ObjectId) -> Option<usize> {
        let index = self.publication.object_index(input)?;
        let row = &self.publication.frame().objects[index];
        noon_compile::unsupported_prefix_input(
            &row.content,
            row.spatial.is_some(),
            row.draw_enabled,
        )
        .then(|| self.publication.painter_rank(index))
        .flatten()
    }
}
impl RetainedEffectPublication {
    pub(super) fn sync_prefix_admission(
        &mut self,
        publication: &RendererPublication<'_>,
        full: bool,
    ) -> Result<(), EffectPrepareError> {
        let view = View { publication };
        let changes = if full {
            self.prefix_admission = PrefixAdmission::from_view(
                &view,
                publication.frame().objects.iter().map(|row| row.id),
            );
            self.prefix_admission_rows += publication.frame().objects.len();
            PrefixAdmissionChanges::default()
        } else {
            let rows = publication
                .changes()
                .object_indices()
                .iter()
                .filter_map(|&index| publication.frame().objects.get(index))
                .map(|row| row.id);
            let order = publication
                .changes()
                .painter_order_range()
                .into_iter()
                .flat_map(|range| {
                    publication
                        .painter_order()
                        .iter()
                        .take(range.end)
                        .skip(range.start)
                })
                .map(|&index| publication.frame().objects[index as usize].id);
            let changes = self.prefix_admission.prepare(&view, rows, order);
            self.prefix_admission_rows += changes.rows_examined;
            changes
        };
        if let Some((owner, input)) = self.prefix_admission.conflict(&changes) {
            return Err(EffectPrepareError::UnsupportedBackdropInput { owner, input });
        }
        self.prefix_admission.commit(changes);
        Ok(())
    }
}

#[cfg(test)]
mod tests;

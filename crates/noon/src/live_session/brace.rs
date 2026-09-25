//! Brace preparation observes the same coherent live layout as other placements.
use super::*;

impl LiveSession<'_> {
    pub(crate) fn prepare_brace_geometry(
        &self,
        target: &crate::LayoutAnchor,
        label: &crate::LayoutAnchor,
        options: crate::BraceOptions,
    ) -> Result<crate::geometry_authoring::PreparedBraceGeometry, LiveSessionError> {
        self.require_brace_label_placement(label)?;
        if !Rc::ptr_eq(self.store, target.integration_store()) {
            return Err(crate::AuthoringError::ForeignStore.into());
        }
        crate::geometry_authoring::prepare_brace_geometry_with_transform(
            target, options.direction, options.buff, options.sharpness,
            |node, authored| {
                let mut transform = authored.transform;
                if self.session.semantic_object_is_reachable(node) {
                    let store = self.store.borrow();
                    let effective = self.session.effective_semantic_object(&store, node)?;
                    if !effective.authored_content_layout_applicable() {
                        return Err(crate::AuthoringError::Unsupported(
                            crate::UnsupportedAuthoringOperation::EffectiveFamilyLayoutRenderOverride,
                        ));
                    }
                    let value = effective.object.transform;
                    let retain_precision = |old: f64, new: f32| if old as f32 == new { old } else { f64::from(new) };
                    transform.translation.x = retain_precision(transform.translation.x, value.translation.x);
                    transform.translation.y = retain_precision(transform.translation.y, value.translation.y);
                    transform.scale.x = retain_precision(transform.scale.x, value.scale.x);
                    transform.scale.y = retain_precision(transform.scale.y, value.scale.y);
                    transform.rotation_z = retain_precision(transform.rotation_z, value.rotation);
                }
                Ok(transform)
            },
        ).map_err(Into::into)
    }

    /// Placement preserves the normal policy: observe live targets, and reject
    /// persistent edits to a label whose affine driver is still unresolved.
    pub(crate) fn require_brace_label_placement(
        &self,
        label: &crate::LayoutAnchor,
    ) -> Result<(), LiveSessionError> {
        if !Rc::ptr_eq(self.store, label.integration_store()) {
            return Err(crate::AuthoringError::ForeignStore.into());
        }
        self.session.require_published_store(&self.store.borrow())?;
        for &node in label.layout()?.leaves() {
            self.placement_authored_transform(&Mobject::from_node(Rc::clone(self.store), node)?)?;
        }
        Ok(())
    }
}

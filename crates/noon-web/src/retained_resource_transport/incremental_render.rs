use std::sync::Arc;

use noon_core::GeometryRef;

use super::{
    InstalledRetainedResources, InstalledTextResourceOverlay, PreparedRetainedResourceAdditions,
    RenderGeometryPreparation, RetainedResourceBundle, RetainedResourceTransportError,
};

/// Borrowed, validated incremental renderer resources before they are installed.
/// Preparation indices are local to `geometries`, exactly as encoded on the wire.
#[cfg(target_arch = "wasm32")]
pub(crate) struct RenderGeometryAdditionView<'a> {
    pub(crate) session: u32,
    pub(crate) geometries: &'a [GeometryRef],
    pub(crate) preparations: &'a [RenderGeometryPreparation],
}

/// One prepared additive resource transaction. The combined render table is built
/// exactly once, then shared by the resource owner and wire mirror on commit.
pub(crate) struct PreparedRetainedResourceAdditionsWithRender {
    ordinary: PreparedRetainedResourceAdditions,
    render_geometry_session: Option<u32>,
    render_geometries: Option<Arc<[Arc<GeometryRef>]>>,
    #[cfg(any(target_arch = "wasm32", test))]
    render_geometry_suffix_start: usize,
    render_geometry_preparations: Vec<RenderGeometryPreparation>,
}

impl RetainedResourceBundle {
    pub(crate) fn render_geometry_count(&self) -> usize {
        self.render_geometry_resources
            .as_ref()
            .map_or(0, |resources| resources.geometries.len())
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) fn render_geometry_addition(
        &self,
    ) -> Result<Option<RenderGeometryAdditionView<'_>>, RetainedResourceTransportError> {
        self.validate_protocol()?;
        let Some(resources) = &self.render_geometry_resources else {
            return Ok(None);
        };
        validate_render_geometry_resources(resources)?;
        Ok(Some(RenderGeometryAdditionView {
            session: resources.session,
            geometries: &resources.geometries,
            preparations: &resources.preparations,
        }))
    }
}

impl InstalledRetainedResources {
    pub(crate) fn prepare_additions_with_render(
        &self,
        mut bundle: RetainedResourceBundle,
    ) -> Result<PreparedRetainedResourceAdditionsWithRender, RetainedResourceTransportError> {
        bundle.validate_protocol()?;
        let render_resources = bundle.render_geometry_resources.take();
        if let Some(resources) = &render_resources {
            validate_render_geometry_resources(resources)?;
            if let Some(installed_session) = self.render_geometry_session {
                if installed_session != resources.session {
                    return Err(RetainedResourceTransportError::Decode(format!(
                        "retained render geometry session mismatch: installed {installed_session}, addition {}",
                        resources.session
                    )));
                }
            }
        }

        // Delegate all ordinary text/font/vector dependency checks to the existing
        // transaction after removing the render suffix that it intentionally rejects.
        let ordinary = self.prepare_additions(bundle)?;
        let render_geometry_suffix_start = self.render_geometries.len();
        let Some(resources) = render_resources else {
            return Ok(PreparedRetainedResourceAdditionsWithRender {
                ordinary,
                render_geometry_session: None,
                render_geometries: None,
                #[cfg(any(target_arch = "wasm32", test))]
                render_geometry_suffix_start,
                render_geometry_preparations: Vec::new(),
            });
        };

        let mut next = Vec::with_capacity(
            render_geometry_suffix_start.saturating_add(resources.geometries.len()),
        );
        next.extend(self.render_geometries.iter().cloned());
        next.extend(resources.geometries.into_iter().map(Arc::new));
        let next: Arc<[Arc<GeometryRef>]> = next.into();

        let base = u32::try_from(render_geometry_suffix_start).map_err(|_| {
            RetainedResourceTransportError::Encode(
                "retained render geometry resource index exhausted".into(),
            )
        })?;
        let mut preparations = resources.preparations;
        for (index, preparation) in preparations.iter_mut().enumerate() {
            preparation.resource = preparation.resource.checked_add(base).ok_or(
                RetainedResourceTransportError::InvalidRenderPreparation(index),
            )?;
        }

        Ok(PreparedRetainedResourceAdditionsWithRender {
            ordinary,
            render_geometry_session: Some(resources.session),
            render_geometries: Some(next),
            #[cfg(any(target_arch = "wasm32", test))]
            render_geometry_suffix_start,
            render_geometry_preparations: preparations,
        })
    }

    pub(crate) fn commit_additions_with_render(
        &mut self,
        additions: PreparedRetainedResourceAdditionsWithRender,
    ) {
        let PreparedRetainedResourceAdditionsWithRender {
            ordinary,
            render_geometry_session,
            render_geometries,
            #[cfg(any(target_arch = "wasm32", test))]
                render_geometry_suffix_start: _,
            render_geometry_preparations,
        } = additions;
        self.commit_additions(ordinary);
        if let Some(render_geometries) = render_geometries {
            debug_assert!(render_geometry_session.is_some());
            self.render_geometry_session = render_geometry_session;
            self.render_geometries = render_geometries;
            self.render_geometry_preparations
                .extend(render_geometry_preparations);
        }
    }
}

impl PreparedRetainedResourceAdditionsWithRender {
    pub(crate) fn image_handle_remap(&self) -> super::images::ImageHandles {
        self.ordinary.image_handle_remap()
    }
    pub(crate) fn text_handle_remap(
        &self,
    ) -> std::collections::HashMap<crate::TransportTextResourceHandle, noon_core::TextResourceHandle>
    {
        self.ordinary.text_handle_remap()
    }

    pub(crate) fn text_lookup<'a>(
        &'a self,
        existing: &'a InstalledRetainedResources,
    ) -> InstalledTextResourceOverlay<'a> {
        self.ordinary.text_lookup(existing)
    }

    pub(crate) fn render_geometry_session(&self) -> Option<u32> {
        self.render_geometry_session
    }

    pub(crate) fn render_geometries(&self) -> Option<Arc<[Arc<GeometryRef>]>> {
        self.render_geometries.clone()
    }

    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn render_geometry_suffix(&self) -> &[Arc<GeometryRef>] {
        self.render_geometries
            .as_deref()
            .map(|geometries| &geometries[self.render_geometry_suffix_start..])
            .unwrap_or_default()
    }
}

fn validate_render_geometry_resources(
    resources: &super::TransportRenderGeometryResources,
) -> Result<(), RetainedResourceTransportError> {
    for (index, geometry) in resources.geometries.iter().enumerate() {
        if !matches!(geometry, GeometryRef::VectorPath(_)) || !geometry.is_finite() {
            return Err(RetainedResourceTransportError::InvalidRenderGeometry(index));
        }
    }
    for (index, preparation) in resources.preparations.iter().enumerate() {
        if preparation.resource as usize >= resources.geometries.len() || !preparation.is_finite() {
            return Err(RetainedResourceTransportError::InvalidRenderPreparation(
                index,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use noon_core::{
        FontResourceArena, GeometryResourceArena, Style, TextResourceArena, Transform2D, Vec2,
        VectorPath,
    };

    use super::*;

    fn empty_bundle() -> RetainedResourceBundle {
        RetainedResourceBundle::capture(
            [],
            &TextResourceArena::new(),
            &GeometryResourceArena::new(),
            &FontResourceArena::new(),
        )
        .unwrap()
    }

    fn path(x: f32) -> Arc<GeometryRef> {
        Arc::new(GeometryRef::path(
            VectorPath::new()
                .move_to(Vec2::new(x, 0.0))
                .line_to(Vec2::new(x + 1.0, 1.0)),
        ))
    }

    #[test]
    fn incremental_render_append_reuses_prefix_and_rebases_preparations_once() {
        let first = path(0.0);
        let second = path(2.0);
        let mut base = empty_bundle();
        base.set_render_geometries(
            7,
            vec![first].into(),
            vec![RenderGeometryPreparation {
                resource: 0,
                style: Style::default(),
                transform: Transform2D::IDENTITY,
            }],
        );
        let mut installed = base.install().unwrap();
        let installed_prefix = installed.render_geometries();

        let mut addition = empty_bundle();
        addition.set_render_geometries(
            7,
            vec![second].into(),
            vec![RenderGeometryPreparation {
                resource: 0,
                style: Style::default(),
                transform: Transform2D::IDENTITY,
            }],
        );
        let prepared = installed.prepare_additions_with_render(addition).unwrap();
        assert_eq!(prepared.render_geometry_suffix().len(), 1);
        let combined = prepared.render_geometries().unwrap();
        assert_eq!(combined.len(), 2);
        assert!(Arc::ptr_eq(&installed_prefix[0], &combined[0]));
        assert_eq!(installed.render_geometries().len(), 1);

        installed.commit_additions_with_render(prepared);
        let committed = installed.render_geometries();
        assert_eq!(committed.len(), 2);
        assert!(Arc::ptr_eq(&installed_prefix[0], &committed[0]));
        assert_eq!(installed.render_geometry_preparations().len(), 2);
        assert_eq!(installed.render_geometry_preparations()[0].resource, 0);
        assert_eq!(installed.render_geometry_preparations()[1].resource, 1);
    }

    #[test]
    fn mismatched_render_session_is_rejected_without_mutating_installed_table() {
        let mut base = empty_bundle();
        base.set_render_geometries(
            7,
            vec![path(0.0)].into(),
            vec![RenderGeometryPreparation {
                resource: 0,
                style: Style::default(),
                transform: Transform2D::IDENTITY,
            }],
        );
        let installed = base.install().unwrap();
        let before = installed.render_geometries();

        let mut addition = empty_bundle();
        addition.set_render_geometries(
            8,
            vec![path(2.0)].into(),
            vec![RenderGeometryPreparation {
                resource: 0,
                style: Style::default(),
                transform: Transform2D::IDENTITY,
            }],
        );
        assert!(installed.prepare_additions_with_render(addition).is_err());
        let after = installed.render_geometries();
        assert_eq!(after.len(), 1);
        assert!(Arc::ptr_eq(&before[0], &after[0]));
    }
}

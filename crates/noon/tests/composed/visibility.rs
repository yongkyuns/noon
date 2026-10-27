//! Exact full/empty visibility through the production retained renderer. A
//! silhouette glow added later is the visible wrong-order witness.
use super::*;
use noon::effects::{Glow, NoiseDissolve};
use noon_core::SemanticMutationTransaction;

#[test]
#[ignore = "requires a real native raster adapter; visibility ordering and reuse"]
fn visibility_masks_remain_final_after_authored_glow_and_reuse_warmed_captures() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("visibility role adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut images = 0;
    for composed in [false, true] {
        let mut scene = Scene::new();
        let mut a = scene.rectangle(1.2, 0.8).unwrap();
        let mut b = scene.rectangle(1.2, 0.8).unwrap();
        for (source, x) in [(&mut a, -0.2), (&mut b, 0.2)] {
            source.disable_stroke().unwrap();
            source.set_fill(1.0, 1.0, 1.0, 1.0).unwrap();
            source.set_object_opacity(0.6).unwrap();
            source.set_translation(x, 0.0).unwrap();
        }
        let family = scene.family(&[(&a).into(), (&b).into()]).unwrap();
        if composed {
            scene.add_many(&[(&family).into()]).unwrap();
        } else {
            scene.add(&a).unwrap();
        }
        let owner = if composed {
            family.node_id()
        } else {
            a.node_id()
        };
        let target = || -> noon::MobjectTarget<'_> {
            if composed {
                (&family).into()
            } else {
                (&a).into()
            }
        };
        let mut session = scene.execution_session().unwrap();
        let mut tx = SemanticMutationTransaction::new();
        scene
            .integration_store()
            .borrow()
            .stage_visibility_mask(
                owner,
                NoiseDissolve::new(7, 0.08)
                    .unwrap()
                    .with_progress(1.0)
                    .unwrap(),
                &mut tx,
            )
            .unwrap();
        scene.live(&mut session).apply(tx).unwrap();
        let visibility = scene
            .integration_store()
            .borrow()
            .visibility_effect(owner)
            .unwrap()
            .unwrap();
        // A later silhouette glow must be inserted below the visibility mask.
        scene
            .live(&mut session)
            .add_effect(
                target(),
                Glow::new(
                    EffectUpdate::default()
                        .radius(Pixels(2.0))
                        .intensity(1.25)
                        .color(Color::RED)
                        .source(noon_core::GlowSource::Silhouette),
                )
                .unwrap(),
                "visibility",
            )
            .unwrap();
        let mut surface = Surface::new(&device, &queue);
        let mut ordinary = None;
        let mut warmed_bytes = 0;
        for cycle in 0..16 {
            for progress in [1.0, 0.0] {
                let mut tx = SemanticMutationTransaction::new();
                scene
                    .integration_store()
                    .borrow()
                    .stage_visibility_mask(
                        owner,
                        NoiseDissolve::new(7, 0.08)
                            .unwrap()
                            .with_progress(progress)
                            .unwrap(),
                        &mut tx,
                    )
                    .unwrap();
                scene.live(&mut session).apply(tx).unwrap();
                let pixels =
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
                images += 1;
                if progress == 1.0 {
                    assert!(
                        pixels.iter().all(|&v| v == 0),
                        "a later authored glow resurrected a hidden source"
                    );
                } else {
                    assert!(
                        pixels.iter().step_by(4).any(|&v| v > 128),
                        "ordinary source and glow must be visible"
                    );
                    if let Some(reference) = &ordinary {
                        assert_eq!(&pixels, reference, "neutral visibility must be exact");
                    } else {
                        ordinary = Some(pixels.clone());
                    }
                }
                assert_eq!(
                    scene
                        .integration_store()
                        .borrow()
                        .visibility_effect(owner)
                        .unwrap(),
                    Some(visibility)
                );
                if cycle == 0 && progress == 1.0 {
                    warmed_bytes = surface.effect_stats.retained_texture_bytes;
                } else {
                    assert_eq!(surface.effect_stats.retained_texture_bytes, warmed_bytes);
                    assert_eq!(surface.effect_stats.texture_allocations, 0);
                    assert_eq!(
                        surface.effect_stats.source_passes, 0,
                        "progress changes must reuse captured source pixels"
                    );
                }
                let clean =
                    surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
                images += 1;
                assert_eq!(clean, pixels);
                assert_eq!(
                    surface.effect_stats.rows_examined
                        + surface.effect_stats.sources_prepared
                        + surface.effect_stats.source_passes
                        + surface.effect_stats.blur_passes
                        + surface.effect_stats.composite_passes
                        + surface.effect_stats.bytes_uploaded,
                    0
                );
                assert_eq!(surface.effect_stats.texture_allocations, 0);
            }
        }
        let mut tx = SemanticMutationTransaction::new();
        tx.remove_node(visibility);
        scene.live(&mut session).apply(tx).unwrap();
        assert_eq!(
            surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT),
            ordinary.unwrap()
        );
        images += 1;
    }
    eprintln!("VISIBILITY_ROLE_PASS images={images} endpoints_exact=true clean_exact=true warmed_allocations=0 named_glow_cannot_bypass=true");
}

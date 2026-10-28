//! Shared public live controls and membership, checked against literal source fields
//! and an independently normalized full two-dimensional Gaussian.
use super::*;
use noon::{effects::Blur, MobjectFamily};

mod animation;
mod visibility;

fn soften(scene: &mut Scene, session: &mut ExecutionSession, family: &MobjectFamily) {
    scene
        .live(session)
        .add_effect(
            family,
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "soften",
        )
        .unwrap();
}

fn over(destination: &mut [u8], source: [f64; 4]) {
    for c in 0..4 {
        destination[c] = (255.0 * source[c] + f64::from(destination[c]) * (1.0 - source[3]))
            .round()
            .clamp(0.0, 255.0) as u8;
    }
}
fn compose(layers: &[Vec<u8>]) -> Vec<u8> {
    let mut output = vec![0; (SIZE[0] * SIZE[1] * 4) as usize];
    for layer in layers {
        for (out, source) in output
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(layer.as_chunks::<4>().0)
        {
            over(out, source.map(|v| f64::from(v) / 255.0));
        }
    }
    output
}
fn field(bounds: [usize; 4], source: [f64; 4]) -> Vec<u8> {
    let mut output = vec![0; (SIZE[0] * SIZE[1] * 4) as usize];
    for y in bounds[1]..bounds[3] {
        for x in bounds[0]..bounds[2] {
            over(&mut output[(y * SIZE[0] as usize + x) * 4..][..4], source);
        }
    }
    output
}
fn reference(step: usize, root: bool) -> Vec<u8> {
    let mut layers = Vec::new();
    if step < 9 {
        let alpha = 128.0 / 255.0 * 0.125;
        let mut image = field(
            [27, 24, 43, 36],
            [
                192.0 / 255.0 * alpha,
                48.0 / 255.0 * alpha,
                16.0 / 255.0 * alpha,
                alpha,
            ],
        );
        if step <= 5 {
            image = expected_rgba_blur(&image, 2.0, 1.0);
        }
        layers.push(image);
    }
    layers.push(field([33, 26, 47, 34], [0.0, 0.0, 0.375, 0.375]));
    if step >= 8 {
        layers.push(field([58, 27, 62, 33], [0.0, 0.2, 0.0, 0.2]));
    }
    let mut inner = compose(&layers);
    if step == 1 || step == 2 {
        inner = expected_rgba_blur(&inner, 2.0, 1.0);
    }
    let alpha = 192.0 / 255.0 * 0.75;
    let image = field(
        [37, 24, 53, 36],
        [
            16.0 / 255.0 * alpha,
            192.0 / 255.0 * alpha,
            96.0 / 255.0 * alpha,
            alpha,
        ],
    );
    let mut source = compose(&[inner, image]);
    if root {
        source = expected_rgba_blur(&source, 2.0, 1.0);
    }
    if step == 10 {
        source.fill(0);
    }
    compose(&[source, field([37, 28, 43, 32], [0.6, 0.45, 0.075, 0.6])])
}

#[test]
#[ignore = "requires a real native raster adapter; public nested group lifecycle oracle"]
fn public_composed_group_pixels_cover_live_attachment_membership_and_retirement() {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("public group raster adapter");
    assert_ne!(adapter.get_info().backend, wgpu::Backend::Noop);
    eprintln!("public composed group adapter: {:?}", adapter.get_info());
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default())).unwrap();
    let mut scene = Scene::new();
    let mut images = Vec::new();
    for (i, rgba) in [[192, 48, 16, 128], [16, 192, 96, 192]]
        .into_iter()
        .enumerate()
    {
        let mut options = noon::ImageMobjectOptions::rgba8(1, 1, rgba).unwrap();
        options.set_height(1.0).unwrap();
        options.set_opacity(if i == 0 { 0.5 } else { 1.0 }).unwrap();
        options.set_sampling(noon::RasterImageSampling::Nearest);
        let mut image = scene.image(options).unwrap();
        image.scale(1.6, 1.2).unwrap();
        image
            .set_translation(if i == 0 { -0.5 } else { 0.5 }, 0.0)
            .unwrap();
        image
            .set_object_opacity(if i == 0 { 0.25 } else { 0.75 })
            .unwrap();
        images.push(image);
    }
    images[0]
        .add_effect(
            Blur::new(EffectUpdate::default().radius(Pixels(2.0))).unwrap(),
            "soften",
        )
        .unwrap();
    let mut rectangle = scene.rectangle(1.4, 0.8).unwrap();
    rectangle.disable_stroke().unwrap();
    rectangle.set_fill(0.0, 0.0, 1.0, 0.5).unwrap();
    rectangle.set_object_opacity(0.75).unwrap();
    let inner = scene
        .family(&[(&images[0]).into(), (&rectangle).into()])
        .unwrap();
    let outer = scene
        .family(&[(&inner).into(), (&images[1]).into()])
        .unwrap();
    let mut foreground = scene.rectangle(0.6, 0.4).unwrap();
    foreground.disable_stroke().unwrap();
    foreground.set_fill(1.0, 0.75, 0.125, 0.6).unwrap();
    foreground.set_z_index(1.0).unwrap();
    scene
        .add_many(&[(&outer).into(), (&foreground).into()])
        .unwrap();
    let mut added = scene.rectangle(0.4, 0.6).unwrap();
    added.disable_stroke().unwrap();
    added.set_fill(0.0, 1.0, 0.0, 0.4).unwrap();
    added.set_object_opacity(0.5).unwrap();
    added.shift(2.0, 0.0).unwrap();
    let root = scene.root();
    let mut session = scene.execution_session().unwrap();
    let mut surface = Surface::new(&device, &queue);
    let mut maximum = 0;
    for step in 0..13 {
        match step {
            1 => soften(&mut scene, &mut session, &inner),
            2 | 7 => soften(&mut scene, &mut session, &outer),
            3 => scene
                .live(&mut session)
                .set_effect(
                    &inner,
                    "soften",
                    EffectUpdate::default().radius(Pixels(0.0)),
                )
                .unwrap(),
            4 => scene
                .live(&mut session)
                .remove_effect(&outer, "soften")
                .unwrap(),
            5 => scene
                .live(&mut session)
                .remove_effect(&inner, "soften")
                .unwrap(),
            6 => scene
                .live(&mut session)
                .remove_effect(&images[0], "soften")
                .unwrap(),
            8 => {
                scene
                    .live(&mut session)
                    .add_family_members(&inner, &[(&added).into()])
                    .unwrap();
            }
            9 => {
                scene
                    .live(&mut session)
                    .remove_family_members(&inner, &[(&images[0]).into()])
                    .unwrap();
            }
            10 | 11 => {
                let mut tx = noon_core::SemanticMutationTransaction::new();
                if step == 10 {
                    tx.remove_member(root, outer.node_id());
                } else {
                    tx.add_member(root, outer.node_id());
                }
                scene.live(&mut session).apply(tx).unwrap();
            }
            _ => {}
        }
        let actual = surface.render_on(&device, &queue, &mut session, wgpu::Color::TRANSPARENT);
        let filtered = matches!(step, 2 | 3 | 7 | 8 | 9 | 11 | 12);
        let expected = reference(step, filtered);
        let error = actual
            .iter()
            .zip(&expected)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        maximum = maximum.max(error);
        assert!(
            error <= 2,
            "public composed step={step}: max RGBA byte error {error}"
        );
        if step == 2 {
            assert!(
                expected
                    .iter()
                    .zip(reference(step, false))
                    .map(|(a, b)| a.abs_diff(b))
                    .max()
                    .unwrap()
                    > 10,
                "oracle must distinguish isolated filtering from raw group drawing"
            );
        }
        if step == 6 {
            assert_eq!(surface.effect_stats.retained_texture_bytes, 0);
        }
        if step == 12 {
            assert_eq!(
                (
                    surface.effect_stats.sources_prepared,
                    surface.effect_stats.source_passes,
                    surface.effect_stats.bytes_uploaded
                ),
                (0, 0, 0)
            );
        }
        eprintln!("public composed step={step}: max byte error {error}");
    }
    eprintln!("public composed group: 13 mixed image/vector states, max byte error {maximum}");
}

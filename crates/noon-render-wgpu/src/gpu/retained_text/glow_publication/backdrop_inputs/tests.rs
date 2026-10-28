use super::*;
use noon_compile::{
    CompiledEffect, CompiledEffectStack, CompiledObject, CompiledScene, ExecutionPatch,
};
use noon_core::{
    BackdropBlur, Blur, Color, ComposedSource, EffectUpdate, GeometryRef, Pixels, SemanticNodeId,
    Style, Transform2D,
};
use noon_runtime::SceneInstance;
fn row(id: u64, x: f32) -> CompiledObject {
    CompiledObject::new(
        ObjectId::new(id),
        GeometryRef::rectangle(0.5, 0.5),
        Transform2D {
            translation: Vec2::new(x, 0.0),
            ..Transform2D::IDENTITY
        },
        Style {
            fill: Some(Color::WHITE),
            stroke: None,
            stroke_width: 0.0,
            ..Style::default()
        },
    )
}
fn stack(id: u32, backdrop: bool, radius: f64) -> Arc<CompiledEffectStack> {
    let update = EffectUpdate::default().radius(Pixels(radius));
    Arc::new(
        CompiledEffectStack::new([CompiledEffect {
            attachment: SemanticNodeId::new(id, 2),
            definition: if backdrop {
                BackdropBlur::new(update).unwrap().into()
            } else {
                Blur::new(update).unwrap().into()
            },
        }])
        .unwrap(),
    )
}
struct Cache {
    state: RetainedEffectPublication,
    inputs: BackdropInputs,
}
impl Cache {
    fn new() -> Self {
        Self {
            state: RetainedEffectPublication::default(),
            inputs: BackdropInputs::default(),
        }
    }
    fn refresh(&mut self, scene: &mut SceneInstance) -> QueryStats {
        let mut index = ExecutionSpatialIndex::default();
        // This test isolates renderer dependency work. Native session tests
        // separately qualify sparse updates of this borrowed runtime index.
        index.rebuild(
            scene.frame(),
            scene.geometry_resources(),
            scene
                .painter_order()
                .iter()
                .map(|&i| (ExecutionSlotId::new(i, 0), i as usize)),
        );
        let publication = scene.take_renderer_publication().with_spatial_index(&index);
        self.state.sync(&publication).unwrap();
        self.inputs.stats = QueryStats::default();
        self.inputs
            .refresh(
                &publication,
                &mut self.state,
                Camera2D::new(Vec2::ZERO, Vec2::new(8.0, 6.0)).unwrap(),
                [80, 60],
            )
            .unwrap();
        self.inputs.stats
    }
    fn presented(&mut self) {
        self.state.dirty.clear();
        self.state.capture_dirty.clear();
        self.state.invalidated = false;
    }
}
#[test]
fn local_prefix_work_is_bounded_with_twenty_thousand_rows_and_many_unrelated_targets() {
    let mut objects = (0..20_000)
        .map(|i| row(i, i as f32 * 3.0))
        .collect::<Vec<_>>();
    for i in 1..=100 {
        let mut panel = row(30_000 + i, i as f32 * 30.0);
        panel.effects = Some(stack(i as u32, true, 2.0));
        objects.push(panel);
    }
    let mut scene = SceneInstance::new(CompiledScene::compile_objects(objects, &[]).unwrap());
    let mut cache = Cache::new();
    let cold = cache.refresh(&mut scene);
    assert_eq!(cold.queries, 100);
    assert_eq!(cache.inputs.entries.len(), 100);
    cache.presented();
    scene.take_renderer_publication();
    assert_eq!(cache.refresh(&mut scene), QueryStats::default());
    cache.presented();
    let untouched = cache.inputs.entries[&20_099].source_epoch;
    let mut style = scene.frame().objects[10].style;
    style.fill = Some(Color::rgba(0.4, 0.2, 0.7, 1.0));
    scene
        .apply_execution_patch(&ExecutionPatch::SetStyle {
            object: ObjectId::new(10),
            style,
            fill_paint: None,
        })
        .unwrap();
    let local = cache.refresh(&mut scene);
    assert_eq!(local.queries, 1);
    assert_eq!(local.footprints, 0);
    assert!(local.candidates < 12, "{local:?}");
    assert_eq!(cache.state.dirty, [20_000].into_iter().collect());
    assert_eq!(cache.inputs.entries[&20_099].source_epoch, untouched);
    cache.presented();
    scene
        .apply_execution_patch(&ExecutionPatch::SetEffects {
            object: ObjectId::new(30_001),
            effects: stack(1, true, 1.99),
        })
        .unwrap();
    let epoch = cache.inputs.entries[&20_000].source_epoch;
    let parameter = cache.refresh(&mut scene);
    assert_eq!(parameter.queries, 0);
    assert_eq!(parameter.footprints, 1);
    assert_eq!(cache.inputs.entries[&20_000].source_epoch, epoch);
}
#[test]
fn complete_filtered_scope_halos_and_earlier_backdrop_changes_join_the_strict_prefix() {
    // Source geometry is outside the target window; its group blur halo enters.
    let mut source = row(10, -1.5);
    source.base_style.opacity = 0.5;
    let mut glass_a = row(20, 0.0);
    glass_a.effects = Some(stack(20, true, 1.0));
    let mut glass_b = row(30, 0.0);
    glass_b.effects = Some(stack(30, true, 1.0));
    let later = row(40, 0.0);
    let mut group = row(90, 0.0);
    group.content = noon_core::ObjectContentRef::Composed(Arc::new(
        ComposedSource::new([ObjectId::new(10)], None).unwrap(),
    ));
    group.draw_enabled = false;
    group.base_style.fill = None;
    group.effects = Some(stack(90, false, 3.0));
    let mut scene = SceneInstance::new(
        CompiledScene::compile_objects(vec![source, glass_a, glass_b, later, group], &[]).unwrap(),
    );
    let mut cache = Cache::new();
    cache.refresh(&mut scene);
    assert_eq!(&*cache.inputs.entries[&1].candidates, &[ObjectId::new(10)]);
    assert_eq!(
        &*cache.inputs.entries[&2].candidates,
        &[ObjectId::new(10), ObjectId::new(20)]
    );
    assert!(cache.inputs.required(4));
    cache.presented();
    scene.take_renderer_publication();
    let mut style = scene.frame().objects[0].style;
    style.opacity = 0.8;
    scene
        .apply_execution_patch(&ExecutionPatch::SetStyle {
            object: ObjectId::new(10),
            style,
            fill_paint: None,
        })
        .unwrap();
    let work = cache.refresh(&mut scene);
    assert_eq!(work.queries, 2);
    assert!(cache.state.dirty.contains(&1) && cache.state.dirty.contains(&2));
    cache.presented();
    scene
        .apply_execution_patch(&ExecutionPatch::SetTransform {
            object: ObjectId::new(40),
            transform: Transform2D {
                translation: Vec2::new(0.1, 0.0),
                ..Transform2D::IDENTITY
            },
        })
        .unwrap();
    assert_eq!(
        cache.refresh(&mut scene).queries,
        0,
        "later paint never enters earlier prefixes"
    );
    cache.presented();
    scene
        .apply_execution_patch(&ExecutionPatch::SetZIndex {
            object: ObjectId::new(40),
            value: -1.0,
        })
        .unwrap();
    assert_eq!(
        cache.refresh(&mut scene).queries,
        2,
        "a newly earlier source enters both existing windows"
    );
    assert_eq!(
        &*cache.inputs.entries[&1].candidates,
        &[ObjectId::new(40), ObjectId::new(10)]
    );
    cache.presented();
    scene
        .apply_execution_patch(&ExecutionPatch::RemoveObject(ObjectId::new(40)))
        .unwrap();
    assert_eq!(
        cache.refresh(&mut scene).queries,
        2,
        "removed input invalidates through old reverse links"
    );
    assert_eq!(&*cache.inputs.entries[&1].candidates, &[ObjectId::new(10)]);
}

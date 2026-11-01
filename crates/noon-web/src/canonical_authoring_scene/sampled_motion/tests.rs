use super::*;
use noon_core::SemanticAnimationCompositionKind;

#[derive(Clone)]
struct Batch {
    ids: Vec<ObjectId>,
    ends: Vec<u32>,
    phases: Vec<f64>,
    centers: Vec<f64>,
    angles: Vec<f64>,
    presence: Vec<u8>,
    options: Vec<f64>,
    rates: Vec<String>,
}

impl Batch {
    fn two() -> Self {
        Self {
            ids: vec![ObjectId::new(7), ObjectId::new(9)],
            ends: vec![2, 4],
            phases: vec![0.37, 1.0, 0.13, 1.0],
            centers: vec![
                2.0, 1.0, 0.0, 4.0, 3.0, 0.0, -1.0, 2.0, 0.0, -3.0, -2.0, 0.0,
            ],
            angles: vec![
                -2.0,
                -6.0 * std::f64::consts::PI,
                3.0,
                8.0 * std::f64::consts::PI,
            ],
            presence: vec![HAS_RUN_TIME; 2],
            options: vec![2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
            rates: vec!["linear".into(); 2],
        }
    }

    fn children(
        &self,
        context: &CanonicalAuthoringScene,
    ) -> Result<Vec<OrdinaryCompositionChild>, String> {
        self.with_play(context, None, "", None)
    }

    fn with_play(
        &self,
        context: &CanonicalAuthoringScene,
        duration: Option<f64>,
        rate: &str,
        lag: Option<f64>,
    ) -> Result<Vec<OrdinaryCompositionChild>, String> {
        children(
            context,
            &self.ids,
            &self.ends,
            &self.phases,
            &self.centers,
            &self.angles,
            &self.presence,
            &self.options,
            &self.rates,
            duration,
            rate,
            lag,
        )
    }
}

fn fixture() -> (CanonicalAuthoringScene, [noon::Mobject; 2]) {
    let mut context = CanonicalAuthoringScene::default();
    let first = context.scene.circle(0.4).unwrap();
    let second = context.scene.square(0.7).unwrap();
    context.bind_mobject(ObjectId::new(7), &first).unwrap();
    context.bind_mobject(ObjectId::new(9), &second).unwrap();
    (context, [first, second])
}

#[test]
fn bulk_records_preserve_nonuniform_keys_signed_turns_and_shared_option_precedence() {
    let (context, objects) = fixture();
    let revision = context.scene.revision();
    let mut batch = Batch::two();
    batch.phases.insert(3, 0.73);
    batch.centers.splice(9..9, [0.5, 0.25, 0.0]);
    batch.angles.insert(3, 4.0);
    batch.ends[1] = 5;
    batch.presence[0] |= HAS_REVERSE | REVERSE;
    batch.rates[0] = "smoothstep".into();
    let records = batch
        .with_play(&context, Some(3.4), "linear", None)
        .unwrap();
    assert_eq!(records.len(), 2);
    for (index, record) in records.iter().enumerate() {
        let OrdinaryCompositionChild::SampledMotion {
            target,
            motion,
            options,
        } = record
        else {
            panic!("bulk admission changed the ordinary leaf kind");
        };
        assert_eq!(target.node_id(), objects[index].node_id());
        let start = if index == 0 {
            0
        } else {
            batch.ends[index - 1] as usize
        };
        let end = batch.ends[index] as usize;
        assert_eq!(motion.phases(), &batch.phases[start..end]);
        assert_eq!(motion.rotation_offsets(), &batch.angles[start..end]);
        assert_eq!(motion.centers()[0].x, batch.centers[start * 3]);
        assert_eq!(options.run_time, Some(3.4));
        assert_eq!(options.rate_func, Some(RateFunction::Linear));
        assert_eq!(options.reverse_rate_function, Some(index == 0));
    }
    assert_eq!(context.scene.revision(), revision);
    assert!(context.player_ownership.is_unstarted());
}

#[test]
fn later_invalid_record_rejects_whole_batch_without_authored_or_runtime_changes() {
    let (context, _) = fixture();
    let revision = context.scene.revision();
    let bindings = context.bindings.clone();
    for mutate in [
        |batch: &mut Batch| batch.phases[3] = 0.99,
        |batch: &mut Batch| batch.ids[1] = ObjectId::new(99),
        |batch: &mut Batch| batch.ends[0] = 4,
        |batch: &mut Batch| batch.angles.pop().map(|_| ()).unwrap(),
        |batch: &mut Batch| batch.presence[1] = REVERSE,
        |batch: &mut Batch| batch.presence[1] = 128,
        |batch: &mut Batch| batch.options[3] = f64::NAN,
        |batch: &mut Batch| batch.rates[1] = "unknown".into(),
    ] {
        let mut batch = Batch::two();
        mutate(&mut batch);
        assert!(batch.children(&context).is_err());
        assert_eq!(context.scene.revision(), revision);
        assert_eq!(context.bindings, bindings);
        assert!(context.player_ownership.is_unstarted());
    }
    assert_eq!(Batch::two().children(&context).unwrap().len(), 2);
}

#[test]
fn optional_values_use_presence_and_shared_validation_without_nan_sentinels() {
    let (context, _) = fixture();
    let mut batch = Batch::two();
    batch.presence.fill(0);
    batch.options.fill(f64::NAN);
    assert!(batch.children(&context).is_ok());
    batch.presence[1] = HAS_RUN_TIME;
    assert!(batch.children(&context).is_err());
    for (mask, index, value) in [(HAS_LAG, 1, 0.1), (HAS_PATH_ARC, 2, 0.1)] {
        let mut batch = Batch::two();
        batch.presence[1] |= mask;
        batch.options[3 + index] = value;
        assert!(batch.children(&context).is_err());
    }
    assert!(Batch::two()
        .with_play(&context, Some(f64::NAN), "", None)
        .is_err());
    assert!(Batch::two()
        .with_play(&context, None, "", Some(0.1))
        .is_err());
}

#[test]
fn canonical_bulk_children_activate_one_ordinary_player_and_persist_completion() {
    let (mut context, objects) = fixture();
    let children = Batch::two().children(&context).unwrap();
    let end = context
        .begin_ordinary_mixed_composition(
            SemanticAnimationCompositionKind::Parallel,
            &children,
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .unwrap();
    assert_eq!(end, 2.0);
    let player = context.active_live_player().unwrap();
    assert!(!player.has_required_callbacks());
    let owner = player.ownership_identity();
    player.live_advance_segment_to(0.74).unwrap();
    let effective = player.live_effective(&objects[0]).unwrap().transform;
    assert!((effective.translation.x - 2.0).abs() < 1e-6);
    assert!((effective.translation.y - 1.0).abs() < 1e-6);
    assert!((effective.rotation + 2.0).abs() < 1e-6);
    player.live_advance_segment_to(end).unwrap();
    player.live_complete_segment().unwrap();
    assert_eq!(
        context.active_live_player().unwrap().ownership_identity(),
        owner
    );
    assert!((context.mobject_layout(&objects[0]).unwrap().0 - 4.0).abs() < 1e-6);
    assert!((context.mobject_layout(&objects[1]).unwrap().0 + 3.0).abs() < 1e-6);
}

#[test]
fn foreign_and_conflicting_children_fail_before_player_ownership_is_committed() {
    let (source, _) = fixture();
    let children = Batch::two().children(&source).unwrap();
    let (mut other, _) = fixture();
    let revision = other.scene.revision();
    assert!(other
        .begin_ordinary_mixed_composition(
            SemanticAnimationCompositionKind::Parallel,
            &children,
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .is_err());
    assert_eq!(other.scene.revision(), revision);
    assert!(other.player_ownership.is_unstarted());

    let mut duplicate = Batch::two();
    duplicate.ids[1] = duplicate.ids[0];
    let duplicate = duplicate.children(&other).unwrap();
    assert!(other
        .begin_ordinary_mixed_composition(
            SemanticAnimationCompositionKind::Parallel,
            &duplicate,
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .is_err());
    assert_eq!(other.scene.revision(), revision);
    assert!(other.player_ownership.is_unstarted());
    let valid = Batch::two().children(&other).unwrap();
    other
        .begin_ordinary_mixed_composition(
            SemanticAnimationCompositionKind::Parallel,
            &valid,
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .unwrap();
}

#[test]
fn planar_semantic_admission_rejects_nonzero_z_without_starting_a_player() {
    let (mut context, _) = fixture();
    let mut batch = Batch::two();
    batch.centers[11] = 0.1;
    // The generic immutable data admits Vec3; the planar semantic intent owns z policy.
    let children = batch.children(&context).unwrap();
    let revision = context.scene.revision();
    assert!(context
        .begin_ordinary_mixed_composition(
            SemanticAnimationCompositionKind::Parallel,
            &children,
            AnimationOptions::new(),
            AnimationOptions::new(),
        )
        .is_err());
    assert_eq!(context.scene.revision(), revision);
    assert!(context.player_ownership.is_unstarted());
}

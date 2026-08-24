from pathlib import Path

path = Path("crates/noon-runtime/src/execution_slots.rs")
text = path.read_text()
old = '''        assert_eq!(live.slot_for_object(second), Some(second_slot));
        assert_eq!(live.slot_for_object(third), Some(third_slot));
        assert_eq!(live.frame().objects[0].id, second);
        assert_eq!(
            live.last_execution_delta().slots(),
            &[ExecutionSlotId::new(0, 0)]
        );
'''
new = '''        assert_eq!(live.slot_for_object(second), Some(second_slot));
        assert_eq!(live.slot_for_object(third), Some(third_slot));
        assert!(!live.frame().objects[0].live);
        assert!(!live.frame().is_present(0));
        assert_eq!(
            live.frame().objects[second_slot.slot() as usize].id,
            second
        );
        assert!(live.frame().objects[second_slot.slot() as usize].live);
        assert_eq!(
            live.frame().objects[third_slot.slot() as usize].id,
            third
        );
        assert!(live.frame().objects[third_slot.slot() as usize].live);
        assert_eq!(
            live.last_execution_delta().slots(),
            &[ExecutionSlotId::new(0, 0)]
        );
'''
if text.count(old) != 1:
    raise SystemExit(f"stable slot test: expected one old dense assertion, found {text.count(old)}")
path.write_text(text.replace(old, new, 1))

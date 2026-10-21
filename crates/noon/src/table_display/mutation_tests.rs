//! Native counterpart of the pinned language-callback membership observations.
use super::alias_tests::{names, position_table};
use super::*;
use crate::integer_index::normalize_index;
use serde_json::{json, Value};

fn members(family: &MobjectFamily) -> Vec<TableEntry> {
    (0..family.member_count().unwrap())
        .map(|index| family.member_at(index).unwrap().unwrap())
        .collect()
}

#[test]
fn current_indexed_reads_match_pinned_callback_mutations() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/manim-v0.21/table-mutation-positions.json"
    ))
    .unwrap();
    assert_eq!(fixture["manimVersion"], "0.21.0");
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 576);
    for expected in fixture["cases"].as_array().unwrap() {
        let spec = &expected["spec"];
        let (mut scene, table, _, mut ids) = position_table(spec["labels"].as_bool().unwrap());
        let extra = scene.square(0.25).unwrap();
        ids.insert(extra.node_id(), "extra".to_owned());
        let body = spec["method"] == "get_entries_without_labels";
        let projection = if body {
            table.body_entries_family()
        } else {
            table.entries_family()
        }
        .unwrap();
        let group = if spec["access"] == "group" {
            let entries = members(&projection);
            scene
                .family(
                    &entries
                        .iter()
                        .map(TableEntry::as_target)
                        .collect::<Vec<_>>(),
                )
                .unwrap()
        } else {
            projection
        };
        let selection = if body {
            table.body_entries_indexing()
        } else {
            table.entries_indexing()
        }
        .unwrap();
        let row = spec["position"][0].as_i64().unwrap();
        let column = spec["position"][1].as_i64().unwrap();
        let index = crate::IntegerIndex::from(row - 1) * selection.row_stride() + column
            - selection.column_offset();
        assert_eq!(
            index,
            crate::IntegerIndex::from(expected["normalization"]["index"].as_i64().unwrap())
        );
        assert_eq!(names(members(&group), &ids), expected["before"], "{spec}");
        let before_count = group.member_count().unwrap();
        let before_nodes = scene.integration_store().borrow().len();
        let before_members = members(&group);
        match spec["action"].as_str().unwrap() {
            "remove_first" => {
                group.remove(before_members[0].as_target()).unwrap();
            }
            "clear" => {
                group
                    .remove_many(
                        &before_members
                            .iter()
                            .map(TableEntry::as_target)
                            .collect::<Vec<_>>(),
                    )
                    .unwrap();
            }
            "append_first" => {
                group.add(before_members[0].as_target()).unwrap();
            }
            "append_extra" => {
                group.add((&extra).into()).unwrap();
            }
            action => panic!("unknown pinned mutation {action}"),
        }
        // Arithmetic callbacks precede range(len); __index__ runs after its
        // length is captured. The lookup then observes current membership.
        let span = if spec["phase"] == "index" {
            before_count
        } else {
            group.member_count().unwrap()
        };
        assert_eq!(
            span,
            expected["normalization"]["length"].as_u64().unwrap() as usize,
            "{spec}"
        );
        if spec["access"] == "table" {
            assert_eq!(selection.len().unwrap(), group.member_count().unwrap());
        }
        let result = match normalize_index(index, span) {
            None => {
                json!({"exception":"IndexError", "message":TableAuthoringError::InvalidEntryIndex.to_string()})
            }
            Some(index) => {
                let entry = if spec["access"] == "table" {
                    selection.select_normalized_index(index).unwrap()
                } else {
                    group.member_at(index).unwrap()
                };
                entry
                    .map(|entry| names(vec![entry], &ids)[0].clone())
                    .unwrap_or_else(
                        || json!({"exception":"IndexError", "message":"list index out of range"}),
                    )
            }
        };
        assert_eq!(result, expected["result"], "{spec}");
        assert_eq!(names(members(&group), &ids), expected["after"], "{spec}");
        assert_eq!(
            names(table.get_entries().unwrap(), &ids),
            expected["displayed_after"],
            "{spec}"
        );
        assert_eq!(
            names(table.get_entries_without_labels().unwrap(), &ids),
            expected["body_after"],
            "{spec}"
        );
        assert_eq!(
            scene.integration_store().borrow().len(),
            before_nodes,
            "reads and membership edits allocate no semantic nodes"
        );
    }
}

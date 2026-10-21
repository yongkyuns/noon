//! Geometric table observations generated independently by pinned ManimCE.
use super::*;
use noon_core::{SemanticNodeId, SemanticObjectState, SemanticStore, StoredGeometry, Vec2};
use serde_json::{json, Value};
use std::collections::HashMap;

fn rect(store: &Rc<RefCell<SemanticStore>>, width: f64, height: f64, x: f64, y: f64) -> Mobject {
    let mut state = SemanticObjectState::new(StoredGeometry::Rectangle {
        size: Vec2::new(width as f32, height as f32),
    });
    state.transform.translation.x = x;
    state.transform.translation.y = y;
    Mobject::new(Rc::clone(store), state).unwrap()
}
fn key(entry: &TableEntry, names: &HashMap<SemanticNodeId, String>) -> Value {
    if let Some(name) = names.get(&entry.as_target().node_id()) {
        return json!(name);
    }
    let TableEntry::Mobject(object) = entry else {
        panic!("unknown fixture entry must be the retained empty corner");
    };
    assert!(object.layout_bounds().unwrap().is_none());
    json!({"type":"VMobject","points":0})
}
pub(super) fn names(entries: Vec<TableEntry>, ids: &HashMap<SemanticNodeId, String>) -> Value {
    Value::Array(entries.iter().map(|entry| key(entry, ids)).collect())
}
fn near(value: f64, expected: &Value, context: &str) {
    let expected = expected.as_f64().unwrap();
    assert!(
        (value - expected).abs() <= 1e-6,
        "{context}: {value} != {expected}"
    );
}
fn position(text: &str) -> (i128, i128) {
    let mut pair = text
        .trim_matches(['(', ')'])
        .split(',')
        .map(|part| part.trim().parse::<i128>().unwrap() - 1);
    (pair.next().unwrap(), pair.next().unwrap())
}

#[test]
fn geometric_table_queries_match_all_pinned_alias_and_label_cases() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/manim-v0.21/table-queries.json"
    ))
    .unwrap();
    assert_eq!(fixture["manimVersion"], "0.21.0");
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 29);
    for expected in fixture["cases"].as_array().unwrap() {
        let case = &expected["case"];
        let flag = |name: &str| case[name].as_bool().unwrap_or(false);
        let rows = case["body_rows"].as_u64().unwrap_or(2) as usize;
        let columns = case["body_cols"].as_u64().unwrap_or(2) as usize;
        let row_count = case["row_count"].as_u64().unwrap_or(rows as u64) as usize;
        let column_count = case["col_count"].as_u64().unwrap_or(columns as u64) as usize;
        let mut scene = Scene::new();
        let store = Rc::clone(scene.integration_store());
        let mut ids = HashMap::new();
        let mut cells: Vec<Vec<TableEntry>> = (0..rows)
            .map(|r| {
                (0..columns)
                    .map(|c| {
                        let side = 0.4 + 0.1 * (columns * r + c) as f64;
                        let object = rect(&store, side, side, 0.0, 0.0);
                        ids.insert(object.node_id(), format!("cell{r}{c}"));
                        TableEntry::Mobject(object)
                    })
                    .collect()
            })
            .collect();
        let row_labels: Vec<_> = (0..row_count)
            .map(|i| {
                let radius = 0.2 + 0.1 * i as f64;
                let object = rect(&store, 2.0 * radius, 2.0 * radius, 0.0, 0.0);
                ids.insert(object.node_id(), format!("row{i}"));
                object
            })
            .collect();
        // Rectangles reproduce each independently observed triangle's initial
        // dimensions and anchor box. Placement is tested without font/shape raster claims.
        let col_labels: Vec<_> = (0..column_count)
            .map(|i| {
                let b = &expected["initial_columns"][i];
                let object = rect(
                    &store,
                    b["width"].as_f64().unwrap(),
                    b["height"].as_f64().unwrap(),
                    b["center"][0].as_f64().unwrap(),
                    b["center"][1].as_f64().unwrap(),
                );
                ids.insert(object.node_id(), format!("col{i}"));
                object
            })
            .collect();
        let corner = rect(&store, 0.16, 0.16, 0.0, 0.0);
        ids.insert(corner.node_id(), "corner".into());
        if flag("alias") {
            cells[1][1] = cells[0][0].clone();
        }
        if flag("same_row_alias") {
            cells[0][1] = cells[0][0].clone();
        }
        if flag("same_column_alias") {
            cells[1][0] = cells[0][0].clone();
        }
        if flag("all_alias") {
            cells = vec![vec![cells[0][0].clone(); columns]; rows];
        }
        if flag("family_overlap") {
            let group = MobjectFamily::create(
                Rc::clone(&store),
                &[cells[0][0].as_target(), cells[0][1].as_target()],
            )
            .unwrap();
            ids.insert(group.node_id(), "family00".into());
            cells[0][0] = TableEntry::Family(group);
        }
        let result = MobjectTable::from_target_rows_with_optional_composite_labels(
            &mut scene,
            cells
                .iter()
                .map(|row| row.iter().map(TableEntry::as_target).collect::<Vec<_>>()),
            flag("rows").then(|| row_labels.iter().map(MobjectTarget::from).collect()),
            flag("cols").then(|| col_labels.iter().map(MobjectTarget::from).collect()),
            flag("corner").then(|| MobjectTarget::from(&corner)),
            TableOptions {
                include_inner_lines: !flag("omit_inner"),
                ..Default::default()
            },
        );
        if expected.get("constructor_exception").is_some() {
            let error = result.expect_err("pinned invalid labels must reject construction");
            assert_eq!(
                error.to_string(),
                expected["message"].as_str().unwrap(),
                "{case}"
            );
            continue;
        }
        let table = result
            .unwrap_or_else(|error| panic!("fixture {case}: {error}"))
            .into_table();
        let grouped = |groups: Vec<Vec<TableEntry>>| {
            Value::Array(
                groups
                    .into_iter()
                    .map(|entries| names(entries, &ids))
                    .collect(),
            )
        };
        assert_eq!(
            grouped(table.get_rows().unwrap()),
            expected["rows"],
            "rows {case}"
        );
        assert_eq!(
            grouped(table.get_columns().unwrap()),
            expected["columns"],
            "columns {case}"
        );
        assert_eq!(
            names(table.get_entries().unwrap(), &ids),
            expected["entries"],
            "entries {case}"
        );
        assert_eq!(
            names(table.get_entries_without_labels().unwrap(), &ids),
            expected["body_entries"],
            "body {case}"
        );
        assert_eq!(
            names(table.get_labels().unwrap(), &ids),
            expected["labels"],
            "labels {case}"
        );
        assert_eq!(
            json!([
                table.get_horizontal_lines().unwrap().len(),
                table.get_vertical_lines().unwrap().len()
            ]),
            expected["line_counts"],
            "lines {case}"
        );
        for (field, body) in [("entry_positions", false), ("body_positions", true)] {
            for (pos, wanted) in expected[field].as_object().unwrap() {
                let (r, c) = position(pos);
                let result = if body {
                    table.body_entry_at_position(r, c)
                } else {
                    table.entry_at_position(r, c)
                };
                if wanted.get("exception").is_some() {
                    assert_eq!(
                        result.expect_err("invalid entry selection").to_string(),
                        wanted["message"].as_str().unwrap(),
                        "{case}/{pos}"
                    );
                } else {
                    assert_eq!(key(&result.unwrap(), &ids), *wanted, "{case}/{pos}");
                }
            }
        }
        for (pos, wanted) in expected["cells"].as_object().unwrap() {
            let (r, c) = position(pos);
            let result = table.get_cell(r, c);
            if wanted.get("exception").is_some() {
                assert_eq!(
                    result.expect_err("invalid cell selection").to_string(),
                    wanted["message"].as_str().unwrap()
                );
            } else {
                let b = result.unwrap().layout_bounds().unwrap().unwrap();
                near((b.min_x + b.max_x) * 0.5, &wanted["center"][0], "cell x");
                near((b.min_y + b.max_y) * 0.5, &wanted["center"][1], "cell y");
                near(b.width(), &wanted["width"], "cell width");
                near(b.height(), &wanted["height"], "cell height");
            }
        }
        let painted = store
            .borrow()
            .semantic_family_members_checked(table.painted_entries.node_id())
            .unwrap();
        assert_eq!(
            painted,
            table
                .get_entries()
                .unwrap()
                .iter()
                .map(|entry| entry.as_target().node_id())
                .collect::<Vec<_>>()
        );
        let copied = table.family().copy_family().unwrap();
        let copied = Table::from_family(copied.root().clone()).unwrap();
        assert_eq!(
            copied.get_entries().unwrap().len(),
            table.get_entries().unwrap().len()
        );
        assert_eq!(
            copied.get_rows().unwrap().len(),
            table.get_rows().unwrap().len()
        );
    }
}

pub(super) fn position_table(
    labeled: bool,
) -> (Scene, Table, Vec<Mobject>, HashMap<SemanticNodeId, String>) {
    let mut scene = Scene::new();
    let mut ids = HashMap::new();
    let cells: Vec<_> = (0..4)
        .map(|index| {
            let object = scene.square(0.4 + 0.1 * index as f64).unwrap();
            ids.insert(object.node_id(), format!("cell{}{}", index / 2, index % 2));
            object
        })
        .collect();
    let rows: Vec<_> = (0..2)
        .map(|index| {
            let object = scene.circle(0.2 + 0.1 * index as f64).unwrap();
            ids.insert(object.node_id(), format!("row{index}"));
            object
        })
        .collect();
    let columns: Vec<_> = (0..2)
        .map(|index| {
            let object = scene.square(0.2 + 0.1 * index as f64).unwrap();
            ids.insert(object.node_id(), format!("col{index}"));
            object
        })
        .collect();
    let table = MobjectTable::from_target_rows_with_optional_composite_labels(
        &mut scene,
        cells
            .chunks(2)
            .map(|row| row.iter().map(MobjectTarget::from).collect::<Vec<_>>()),
        labeled.then(|| rows.iter().map(MobjectTarget::from).collect()),
        labeled.then(|| columns.iter().map(MobjectTarget::from).collect()),
        None,
        TableOptions::default(),
    )
    .unwrap()
    .into_table();
    (scene, table, cells, ids)
}
fn position_fixture() -> Value {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/manim-v0.21/table-positions.json"
    ))
    .unwrap();
    assert_eq!(fixture["manimVersion"], "0.21.0");
    fixture
}
fn cell_observation(cell: Mobject, wanted: &Value) {
    let bounds = cell.layout_bounds().unwrap().unwrap();
    near(
        (bounds.min_x + bounds.max_x) * 0.5,
        &wanted["center"][0],
        "cell x",
    );
    near(
        (bounds.min_y + bounds.max_y) * 0.5,
        &wanted["center"][1],
        "cell y",
    );
    near(bounds.width(), &wanted["width"], "cell width");
    near(bounds.height(), &wanted["height"], "cell height");
}

#[test]
fn integer_table_positions_match_pinned_signed_and_arbitrary_precision_queries() {
    let fixture = position_fixture();
    let mut observed = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let Some(position) = case["integer_position"].as_array() else {
            continue;
        };
        let (_scene, table, _, ids) = position_table(case["labels"].as_bool().unwrap());
        let row = position[0]
            .as_str()
            .unwrap()
            .parse::<crate::IntegerIndex>()
            .unwrap()
            - 1;
        let column = position[1]
            .as_str()
            .unwrap()
            .parse::<crate::IntegerIndex>()
            .unwrap()
            - 1;
        let result = match case["method"].as_str().unwrap() {
            "get_entries" => table.entry_at_position(row, column),
            "get_entries_without_labels" => table.body_entry_at_position(row, column),
            "get_cell" => table.get_cell(row, column).map(TableEntry::Mobject),
            "get_highlighted_cell" => table
                .get_highlighted_cell(row, column, Color::YELLOW, 0.75)
                .map(TableEntry::Mobject),
            _ => unreachable!(),
        };
        let wanted = &case["result"];
        if wanted.get("exception").is_some() {
            let error = result.expect_err("pinned integer position rejects selection");
            assert!(matches!(error, TableAuthoringError::InvalidEntryIndex));
            assert_eq!(error.to_string(), wanted["message"].as_str().unwrap());
        } else {
            let entry = result.unwrap();
            if case["method"].as_str().unwrap().contains("cell") {
                let TableEntry::Mobject(cell) = entry else {
                    panic!("cell is an object");
                };
                cell_observation(cell, wanted);
            } else {
                assert_eq!(key(&entry, &ids), *wanted, "{case}");
            }
        }
        observed += 1;
    }
    assert!(
        observed >= 80,
        "every integer case must execute: {observed}"
    );
}

#[test]
fn cell_argument_phases_observe_effective_geometry_after_both_callbacks() {
    let fixture = position_fixture();
    let cases = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|case| case["expression"] == "ShiftPosition()" && case["method"] == "get_cell");
    for case in cases {
        let (_scene, table, mut cells, _) = position_table(case["labels"].as_bool().unwrap());
        let rows = table.cell_rows().unwrap();
        cells[0].shift(0.25, 0.0).unwrap();
        let columns = rows.select_row(0).unwrap();
        cells[0].shift(0.0, 0.125).unwrap();
        cell_observation(columns.get_cell(0).unwrap(), &case["result"]);
    }
}

#[test]
fn retained_query_requests_reject_a_retired_table_owner() {
    let (scene, table, _, _) = position_table(false);
    let entries = table.entries_indexing().unwrap();
    let rows = table.cell_rows().unwrap();
    let columns = rows.select_row(0).unwrap();
    let mut tx = noon_core::SemanticMutationTransaction::new();
    tx.remove_node(table.family().node_id());
    tx.apply(&mut scene.integration_store().borrow_mut())
        .unwrap();
    assert!(entries.select_index(0).is_err());
    assert!(entries.select_normalized_index(0).is_err());
    assert!(rows.select_row(0).is_err());
    assert!(columns.get_cell(0).is_err());
    assert!(table.entries_family().is_err());
    assert!(table.body_entries_family().is_err());
    assert!(table.get_entries().is_err());
    assert!(table.get_entries_without_labels().is_err());
}

#[test]
fn canonical_entry_groups_match_pinned_mutation_order_queries_and_copies() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../fixtures/manim-v0.21/table-groups.json"
    ))
    .unwrap();
    assert_eq!(fixture["manimVersion"], "0.21.0");
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 2);
    for case in fixture["cases"].as_array().unwrap() {
        let (scene, table, cells, ids) = position_table(case["labels"].as_bool().unwrap());
        let store = scene.integration_store();
        let displayed = table.entries_family().unwrap();
        let body = table.body_entries_family().unwrap();
        let name = |entry: &TableEntry| {
            ids.get(&entry.as_target().node_id())
                .map(String::as_str)
                .unwrap_or("missing-corner")
                .to_owned()
        };
        let named = |entries: Vec<TableEntry>| entries.iter().map(&name).collect::<Vec<_>>();
        let snapshot = || {
            let selections: Vec<Vec<Value>> = [false, true]
                .into_iter()
                .map(|body| {
                    [(0, 0), (0, 1), (1, 1), (-1, -1)]
                        .into_iter()
                        .map(|(r, c)| {
                            let result = if body {
                                table.body_entry_at_position(r, c)
                            } else {
                                table.entry_at_position(r, c)
                            };
                            match result {
                                Ok(entry) => json!(name(&entry)),
                                Err(error) => {
                                    assert!(matches!(
                                        error,
                                        TableAuthoringError::InvalidEntryIndex
                                    ));
                                    json!({"exception":"IndexError", "message":error.to_string()})
                                }
                            }
                        })
                        .collect()
                })
                .collect();
            json!({
                "displayedStable": table.entries_family().unwrap().node_id() == displayed.node_id(),
                "bodyStable": table.body_entries_family().unwrap().node_id() == body.node_id(),
                "distinctGroups": displayed.node_id() != body.node_id(),
                "displayed": named(table.get_entries().unwrap()),
                "body": named(table.get_entries_without_labels().unwrap()),
                "rows": table.get_rows().unwrap().into_iter().map(&named).collect::<Vec<_>>(),
                "columns": table.get_columns().unwrap().into_iter().map(&named).collect::<Vec<_>>(),
                "selections": selections,
            })
        };
        let before = store.borrow().len();
        for (index, wanted) in case["states"].as_array().unwrap().iter().enumerate() {
            match index {
                0 => (),
                1 => {
                    displayed.remove((&cells[0]).into()).unwrap();
                }
                2 => {
                    body.remove((&cells[1]).into()).unwrap();
                }
                3 => {
                    displayed.add((&cells[0]).into()).unwrap();
                }
                4 => {
                    body.add((&cells[1]).into()).unwrap();
                }
                5 => {
                    displayed.add((&cells[1]).into()).unwrap();
                }
                _ => panic!("unhandled pinned mutation phase"),
            }
            assert_eq!(
                snapshot(),
                *wanted,
                "labels={}/phase={index}",
                case["labels"]
            );
            assert_eq!(
                store.borrow().len(),
                before,
                "queries and membership edits allocate no nodes"
            );
            assert_eq!(
                store
                    .borrow()
                    .semantic_family_members_checked(table.painted_entries.node_id())
                    .unwrap(),
                table
                    .get_entries()
                    .unwrap()
                    .iter()
                    .map(|entry| entry.as_target().node_id())
                    .collect::<Vec<_>>(),
                "the canonical displayed group is the actual painted group"
            );
        }
        let restored = Table::from_family(table.family().clone()).unwrap();
        assert_eq!(
            restored.entries_family().unwrap().node_id(),
            displayed.node_id()
        );
        assert_eq!(
            restored.body_entries_family().unwrap().node_id(),
            body.node_id()
        );
        let copy = table.family().copy_family().unwrap();
        let clone = Table::from_family(copy.root().clone()).unwrap();
        let copied_displayed = clone.entries_family().unwrap();
        let copied_body = clone.body_entries_family().unwrap();
        copied_displayed
            .remove(clone.get_entries().unwrap()[0].as_target())
            .unwrap();
        copied_body
            .remove(
                clone
                    .get_entries_without_labels()
                    .unwrap()
                    .last()
                    .unwrap()
                    .as_target(),
            )
            .unwrap();
        assert_eq!(
            json!({
                "displayedStable": copied_displayed.node_id() == clone.entries_family().unwrap().node_id(),
                "bodyStable": copied_body.node_id() == clone.body_entries_family().unwrap().node_id(),
                "independentGroups": copied_displayed.node_id() != displayed.node_id() && copied_body.node_id() != body.node_id(),
                "independentLeaves": clone.get_entries().unwrap().iter().all(|entry| !ids.contains_key(&entry.as_target().node_id())),
                "displayedCount": clone.get_entries().unwrap().len(),
                "bodyCount": clone.get_entries_without_labels().unwrap().len(),
                "originalUnchanged": snapshot() == *case["states"].as_array().unwrap().last().unwrap(),
            }),
            case["copy"]
        );
    }
}

fn optional_label_targets(present: bool, values: &[TableEntry]) -> Option<Vec<MobjectTarget<'_>>> {
    present.then(|| values.iter().map(TableEntry::as_target).collect())
}

#[test]
fn detached_labeled_admission_uses_the_existing_arena_without_an_extra_scene_root() {
    for labeled in [false, true] {
        let (mut scene, table, cells, _) = position_table(labeled);
        let store = Rc::clone(scene.integration_store());
        let row_labels = table.get_row_labels().unwrap();
        let column_labels = table.get_col_labels().unwrap();
        let rows = || {
            cells
                .chunks(2)
                .map(|row| row.iter().map(MobjectTarget::from).collect::<Vec<_>>())
                .collect::<Vec<_>>()
        };
        let before = store.borrow().len();
        let authored = MobjectTable::from_target_rows_with_optional_composite_labels(
            &mut scene,
            rows(),
            optional_label_targets(labeled, &row_labels),
            optional_label_targets(labeled, &column_labels),
            None,
            TableOptions::default(),
        )
        .unwrap();
        let scene_delta = store.borrow().len() - before;
        let before = store.borrow().len();
        let detached = MobjectTable::from_target_rows_with_optional_composite_labels_in_store(
            Rc::clone(&store),
            rows(),
            optional_label_targets(labeled, &row_labels),
            optional_label_targets(labeled, &column_labels),
            None,
            TableOptions::default(),
        )
        .unwrap();
        assert_eq!(store.borrow().len() - before, scene_delta);
        assert_eq!(
            detached.get_entries().unwrap().len(),
            authored.get_entries().unwrap().len()
        );
        assert_eq!(
            detached.get_rows().unwrap().len(),
            authored.get_rows().unwrap().len()
        );
    }
}

#[cfg(feature = "native-text")]
#[test]
fn detached_text_labels_share_the_native_publication_path() {
    let (mut scene, table, _, _) = position_table(true);
    let store = Rc::clone(scene.integration_store());
    let row_labels = table.get_row_labels().unwrap();
    let column_labels = table.get_col_labels().unwrap();
    let before = store.borrow().len();
    let authored = Table::from_rows_with_composite_labels(
        &mut scene,
        [["a", "b"], ["c", "d"]],
        optional_label_targets(true, &row_labels),
        optional_label_targets(true, &column_labels),
        None,
        TableOptions::default(),
    )
    .unwrap();
    let scene_delta = store.borrow().len() - before;
    let before = store.borrow().len();
    let detached = Table::from_rows_with_composite_labels_in_store(
        Rc::clone(&store),
        [["a", "b"], ["c", "d"]],
        optional_label_targets(true, &row_labels),
        optional_label_targets(true, &column_labels),
        None,
        TableOptions::default(),
    )
    .unwrap();
    assert_eq!(store.borrow().len() - before, scene_delta);
    assert_eq!(
        detached.get_entries().unwrap().len(),
        authored.get_entries().unwrap().len()
    );
    assert!(detached.get_cell(-1, -1).is_ok());
}

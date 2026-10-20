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
fn names(entries: Vec<TableEntry>, ids: &HashMap<SemanticNodeId, String>) -> Value {
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
            let result = table.get_cell(r as usize, c as usize);
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

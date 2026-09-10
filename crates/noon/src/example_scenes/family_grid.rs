//! Unequal row/column sizes and center-preserving live grid placement.
//! Paired with ordinary_family_grid.py.
use crate::{ExecutionSession, FamilyGridOptions, GridFlow, Scene};

pub fn session() -> Result<ExecutionSession, String> {
    let mut scene = Scene::new();
    let mut members = Vec::new();
    for (width, height) in [(2.0, 1.0), (1.0, 0.5), (0.5, 2.0), (1.0, 1.0)] {
        let mut object = scene
            .rectangle(width, height)
            .map_err(|error| error.to_string())?;
        object.shift(1.0, 0.0).map_err(|error| error.to_string())?;
        object
            .set_fill(0.2, 0.6, 1.0, 0.7)
            .map_err(|error| error.to_string())?;
        object.set_stroke_width(0.0).map_err(|e| e.to_string())?;
        members.push(object);
    }
    let family = scene
        .family(&members.iter().map(Into::into).collect::<Vec<_>>())
        .map_err(|error| error.to_string())?;
    family
        .arrange_in_grid(None, Some(2), 0.5, 0.25)
        .map_err(|error| error.to_string())?;
    assert_eq!(
        family.layout().map_err(|error| error.to_string())?.center(),
        (1.0, 0.0)
    );
    scene
        .add_many(&[(&family).into()])
        .map_err(|error| error.to_string())?;
    let mut session = scene.execution_session().map_err(|e| e.to_string())?;
    {
        let mut live = scene.live(&mut session);
        let wait = live.wait_segment(0.1).map_err(|e| e.to_string())?;
        live.advance_segment_to(wait, wait.end_time())
            .map_err(|e| e.to_string())?;
        live.complete_segment(wait).map_err(|e| e.to_string())?;
        live.arrange_family_in_grid_with_options(
            &family,
            &FamilyGridOptions {
                gap: (0.5, 0.25),
                row_alignments: Some("ud".into()),
                column_alignments: Some("lr".into()),
                row_heights: Some(vec![Some(3.0), None]),
                column_widths: Some(vec![None, Some(2.0)]),
                flow: GridFlow::DownRight,
                ..Default::default()
            },
        )
        .map_err(|e| e.to_string())?;
        let layout = live
            .effective_family_layout(&family)
            .map_err(|e| e.to_string())?;
        assert_eq!(layout.center, (1.0, 0.0));
        assert_eq!(layout.width, 4.5);
        assert_eq!(layout.height, 4.25);
        let wait = live.wait_segment(0.1).map_err(|e| e.to_string())?;
        live.advance_segment_to(wait, wait.end_time())
            .map_err(|e| e.to_string())?;
        live.complete_segment(wait).map_err(|e| e.to_string())?;
    }
    Ok(session)
}

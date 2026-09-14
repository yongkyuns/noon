from pathlib import Path

path = Path("crates/noon-render-wgpu/src/render_order.rs")
text = path.read_text()
text = text.replace(
    "visible.contains(&(**object_index as usize))",
    "visible.contains(&(*object_index as usize))",
)
old = '''    use noon_runtime::{DerivedDisplayObject, DerivedDisplayObjectState, SceneInstance};
'''
new = '''    use noon_runtime::{
        DerivedDisplayObject, DerivedDisplayObjectState, FrameObjectState, FrameState, SceneInstance,
    };
'''
if text.count(old) != 1:
    raise RuntimeError(f"expected derived-display runtime import once, found {text.count(old)}")
text = text.replace(old, new, 1)
anchor = '''    fn state(geometry: GeometryRef) -> DerivedDisplayObjectState {
        DerivedDisplayObjectState {
            z_index: 0.0,
            content: ObjectContentRef::Geometry(geometry),
            text_bounds: None,
            transform: Transform2D::IDENTITY,
            style: Style::default(),
            appearance: 1.0,
            presence: true,
            reveal: 1.0,
            morph: 0.0,
            render_geometry: None,
            render_transform: None,
        }
    }
'''
helper = anchor + '''
    fn frame(geometries: Vec<GeometryRef>) -> FrameState {
        let objects = geometries
            .into_iter()
            .enumerate()
            .map(|(index, geometry)| FrameObjectState {
                id: ObjectId::new(index as u64 + 1),
                z_index: 0.0,
                content: ObjectContentRef::Geometry(geometry),
                text_bounds: None,
                transform: Transform2D::IDENTITY,
                style: Style::default(),
                appearance: 1.0,
            })
            .collect::<Vec<_>>();
        let count = objects.len();
        FrameState {
            time: 0.0,
            objects,
            presences: vec![true; count],
            reveals: vec![1.0; count],
            morphs: vec![0.0; count],
            render_geometries: vec![None; count],
            render_transforms: vec![None; count],
            family_animations: vec![None; count],
            family_animation_plan_indices: vec![None; count],
        }
    }
'''
if text.count(anchor) != 1:
    raise RuntimeError(f"expected derived-display state helper once, found {text.count(anchor)}")
text = text.replace(anchor, helper, 1)
text = text.replace(
    '''frame(vec![
            object(0, GeometryRef::circle(1.0)),
            object(1, GeometryRef::rectangle(1.0, 1.0)),
        ])''',
    '''frame(vec![
            GeometryRef::circle(1.0),
            GeometryRef::rectangle(1.0, 1.0),
        ])''',
)
text = text.replace(
    "frame(vec![object(0, GeometryRef::circle(1.0))])",
    "frame(vec![GeometryRef::circle(1.0)])",
)
path.write_text(text)
print("LayerEnd renderer test support applied")

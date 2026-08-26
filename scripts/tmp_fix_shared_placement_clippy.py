from pathlib import Path

path = Path('crates/noon-web/src/authoring_mobject.rs')
text = path.read_text()

old = '''#[derive(Clone, Debug, PartialEq)]
pub struct FrontendMobjectHandle {
    snapshot: ObjectSnapshot,
    semantic_transform: SemanticTransform2_5D,
    semantic_style: SemanticStyle,
}
'''
new = '''#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ManimNextToArgs {
    direction: (f64, f64),
    buff: f64,
    aligned_edge: (f64, f64),
    mask: (f64, f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrontendMobjectHandle {
    snapshot: ObjectSnapshot,
    semantic_transform: SemanticTransform2_5D,
    semantic_style: SemanticStyle,
}
'''
if text.count(old) != 1:
    raise SystemExit('handle struct anchor mismatch')
text = text.replace(old, new, 1)

old = '''    pub fn manim_next_to_handle(
        &mut self,
        other: &Self,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        aligned_edge_x: f64,
        aligned_edge_y: f64,
        mask_x: f64,
        mask_y: f64,
    ) -> Result<(), String> {
        let direction = semantic_xy_f64(direction_x, direction_y)?;
        let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y)?;
        let mask = semantic_xy_f64(mask_x, mask_y)?;
        let buff = render_f64("buffer", buff)?;
'''
new = '''    pub fn manim_next_to_handle(
        &mut self,
        other: &Self,
        args: ManimNextToArgs,
    ) -> Result<(), String> {
        let direction = semantic_xy_f64(args.direction.0, args.direction.1)?;
        let edge = semantic_xy_f64(args.aligned_edge.0, args.aligned_edge.1)?;
        let mask = semantic_xy_f64(args.mask.0, args.mask.1)?;
        let buff = render_f64("buffer", args.buff)?;
'''
if text.count(old) != 1:
    raise SystemExit('handle next_to anchor mismatch')
text = text.replace(old, new, 1)

old = '''    pub fn manim_next_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        direction_x: f64,
        direction_y: f64,
        buff: f64,
        aligned_edge_x: f64,
        aligned_edge_y: f64,
        mask_x: f64,
        mask_y: f64,
    ) -> Result<(), String> {
        let point = semantic_xy_f64(point_x, point_y)?;
        let direction = semantic_xy_f64(direction_x, direction_y)?;
        let edge = semantic_xy_f64(aligned_edge_x, aligned_edge_y)?;
        let mask = semantic_xy_f64(mask_x, mask_y)?;
        let buff = render_f64("buffer", buff)?;
'''
new = '''    pub fn manim_next_to_point(
        &mut self,
        point_x: f64,
        point_y: f64,
        args: ManimNextToArgs,
    ) -> Result<(), String> {
        let point = semantic_xy_f64(point_x, point_y)?;
        let direction = semantic_xy_f64(args.direction.0, args.direction.1)?;
        let edge = semantic_xy_f64(args.aligned_edge.0, args.aligned_edge.1)?;
        let mask = semantic_xy_f64(args.mask.0, args.mask.1)?;
        let buff = render_f64("buffer", args.buff)?;
'''
if text.count(old) != 1:
    raise SystemExit('point next_to anchor mismatch')
text = text.replace(old, new, 1)

old = '''    use super::{FrontendFamilyTargetEditor, FrontendMobjectHandle, SemanticNodeId, SemanticStore};
'''
new = '''    use super::{
        FrontendFamilyTargetEditor, FrontendMobjectHandle, ManimNextToArgs, SemanticNodeId,
        SemanticStore,
    };
'''
if text.count(old) != 1:
    raise SystemExit('wasm import anchor mismatch')
text = text.replace(old, new, 1)

old = '''            self.0
                .manim_next_to_handle(
                    &other.0,
                    direction_x,
                    direction_y,
                    buff,
                    aligned_edge_x,
                    aligned_edge_y,
                    mask_x,
                    mask_y,
                )
                .map_err(js_error)
'''
new = '''            self.0
                .manim_next_to_handle(
                    &other.0,
                    ManimNextToArgs {
                        direction: (direction_x, direction_y),
                        buff,
                        aligned_edge: (aligned_edge_x, aligned_edge_y),
                        mask: (mask_x, mask_y),
                    },
                )
                .map_err(js_error)
'''
if text.count(old) != 1:
    raise SystemExit('wasm handle call anchor mismatch')
text = text.replace(old, new, 1)

old = '''            self.0
                .manim_next_to_point(
                    point_x,
                    point_y,
                    direction_x,
                    direction_y,
                    buff,
                    aligned_edge_x,
                    aligned_edge_y,
                    mask_x,
                    mask_y,
                )
                .map_err(js_error)
'''
new = '''            self.0
                .manim_next_to_point(
                    point_x,
                    point_y,
                    ManimNextToArgs {
                        direction: (direction_x, direction_y),
                        buff,
                        aligned_edge: (aligned_edge_x, aligned_edge_y),
                        mask: (mask_x, mask_y),
                    },
                )
                .map_err(js_error)
'''
if text.count(old) != 1:
    raise SystemExit('wasm point call anchor mismatch')
text = text.replace(old, new, 1)

old = '''        diagonal
            .manim_next_to_handle(&reference, 1.0, 1.0, 0.25, 0.0, 0.0, 1.0, 1.0)
            .unwrap();
'''
new = '''        diagonal
            .manim_next_to_handle(
                &reference,
                ManimNextToArgs {
                    direction: (1.0, 1.0),
                    buff: 0.25,
                    aligned_edge: (0.0, 0.0),
                    mask: (1.0, 1.0),
                },
            )
            .unwrap();
'''
if text.count(old) != 1:
    raise SystemExit('test call anchor mismatch')
text = text.replace(old, new, 1)

path.write_text(text)

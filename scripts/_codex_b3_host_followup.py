from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text()
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one match, found {count}")
    p.write_text(text.replace(old, new, 1))

# Reuse the already-qualified renderer/host preparation patch from the workflow.
workflow = Path('.github/workflows/_codex_b3_derived_host_patch.yml').read_text()
marker = '      - name: Wire viewport-filtered derived rows through retained host rendering\n'
start = workflow.index(marker) + len(marker)
run_marker = '        run: |\n'
start = workflow.index(run_marker, start) + len(run_marker)
end = workflow.index('      - name: Format\n', start)
lines = workflow[start:end].splitlines()
script = '\n'.join(line[10:] if line.startswith('          ') else line for line in lines)
exec(compile(script, '<derived-host-base>', 'exec'))

# Normalize both retained encode branches to JsValue in the direct web host.
path = 'crates/noon-web/src/execution_canvas.rs'
old = '''                let draw = if derived.slots.is_empty() {
                    self.renderer.encode_retained(
                        &mut encoder,
                        &view,
                        &prepared,
                        &self.direct_text_gpu,
                        self.clear_color,
                        query_set,
                    )
                } else {
                    self.renderer
                        .encode_retained_with_derived(
                            &mut encoder,
                            &view,
                            &prepared,
                            &derived,
                            self.clear_color,
                            query_set,
                        )
                        .map_err(js_error)
                };
                let draw = match draw {
                    Ok(draw) => draw,
                    Err(error) => {
                        if let Some(slot) = timestamp_slot {
                            profiler.expect("reserved profiler").cancel_slot(slot);
                        }
                        return Err(js_error(error));
                    }
                };
'''
new = '''                let draw: Result<_, JsValue> = if derived.slots.is_empty() {
                    self.renderer
                        .encode_retained(
                            &mut encoder,
                            &view,
                            &prepared,
                            &self.direct_text_gpu,
                            self.clear_color,
                            query_set,
                        )
                        .map_err(js_error)
                } else {
                    self.renderer
                        .encode_retained_with_derived(
                            &mut encoder,
                            &view,
                            &prepared,
                            &derived,
                            self.clear_color,
                            query_set,
                        )
                        .map_err(js_error)
                };
                let draw = match draw {
                    Ok(draw) => draw,
                    Err(error) => {
                        if let Some(slot) = timestamp_slot {
                            profiler.expect("reserved profiler").cancel_slot(slot);
                        }
                        return Err(error);
                    }
                };
'''
replace_once(path, old, new)

# Native host mirrors the same exact-publication, viewport-filtered derived lane.
path = 'crates/noon-native/src/lib.rs'
replace_once(
    path,
    'use noon_render_wgpu::{Camera2D, GpuRenderer, RetainedFramePreparer, RetainedTextGpuState};\n',
    'use noon_render_wgpu::{\n    prepare_derived_display_visible, Camera2D, GpuRenderer, RetainedFramePreparer,\n    RetainedTextGpuState,\n};\n',
)
old = '''        let metrics = gpu.text_metrics(camera)?;
        let prepared = gpu
            .preparer
            .prepare_planned_publication_visible(
                &gpu.device,
                &gpu.queue,
                &publication,
                visibility.object_indices(),
                metrics,
            )
            .map_err(|error| NativeHostError::Gpu(error.to_string()))?;
        gpu.renderer
            .upload_retained(&gpu.device, &gpu.queue, &prepared, &mut gpu.text_state);
'''
new = '''        let metrics = gpu.text_metrics(camera)?;
        let derived = prepare_derived_display_visible(&publication, visibility.object_indices())
            .map_err(|error| NativeHostError::Gpu(error.to_string()))?;
        let prepared = gpu
            .preparer
            .prepare_planned_publication_visible(
                &gpu.device,
                &gpu.queue,
                &publication,
                visibility.object_indices(),
                metrics,
            )
            .map_err(|error| NativeHostError::Gpu(error.to_string()))?;
        gpu.renderer
            .upload_retained(&gpu.device, &gpu.queue, &prepared, &mut gpu.text_state);
        if !derived.slots.is_empty() {
            gpu.renderer
                .upload_derived(&gpu.device, &gpu.queue, &derived);
        }
'''
replace_once(path, old, new)
old = '''        let _draw = gpu
            .renderer
            .encode_retained(
                &mut encoder,
                &view,
                &prepared,
                &gpu.text_state,
                CLEAR_COLOR,
                None,
            )
            .map_err(|error| NativeHostError::Gpu(error.to_string()))?;
'''
new = '''        let _draw = if derived.slots.is_empty() {
            gpu.renderer
                .encode_retained(
                    &mut encoder,
                    &view,
                    &prepared,
                    &gpu.text_state,
                    CLEAR_COLOR,
                    None,
                )
                .map_err(|error| NativeHostError::Gpu(error.to_string()))?
        } else {
            gpu.renderer
                .encode_retained_with_derived(
                    &mut encoder,
                    &view,
                    &prepared,
                    &derived,
                    CLEAR_COLOR,
                    None,
                )
                .map_err(|error| NativeHostError::Gpu(error.to_string()))?
        };
'''
replace_once(path, old, new)

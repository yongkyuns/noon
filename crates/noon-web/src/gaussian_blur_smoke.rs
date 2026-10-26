//! Explicit raster qualification boundary, excluded from production packages.
//! Borrows the normal browser host's device/queue; this is no Scene API or clock.
use noon_render_wgpu::{
    GaussianBlurCapture, GaussianBlurFilter, GaussianBlurParameters, GaussianBlurScope,
};
use std::{cell::Cell, rc::Rc};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

pub(crate) async fn qualify(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    sigma: f64,
) -> Result<JsValue, JsValue> {
    if width == 0
        || height == 0
        || width > 385
        || height > 385
        || pixels.len() != (width * height * 4) as usize
    {
        return Err(JsValue::from_str(
            "Gaussian raster fixture requires an exact RGBA image no larger than 385x385",
        ));
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("RGBA Gaussian browser fixture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    queue.write_texture(
        texture.as_image_copy(),
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        texture.size(),
    );
    let mut filter = GaussianBlurFilter::default();
    let mut scope = GaussianBlurScope::default();
    let capture = GaussianBlurCapture {
        source: &texture,
        revision: 1,
    };
    let mut results = Vec::new();
    for opacity in [1.0, 0.37] {
        let parameters = GaussianBlurParameters {
            sigma_pixels: sigma,
            scope_opacity: opacity,
            scratch_budget_bytes: 4 * 1024 * 1024,
        };
        let prepared = filter
            .prepare(device, queue, &mut scope, capture, parameters)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let mut encoder = device.create_command_encoder(&Default::default());
        let encoded = filter.encode(&mut encoder, &mut scope);
        queue.submit([encoder.finish()]);
        let output = scope.output().ok_or_else(|| {
            JsValue::from_str("positive raster probe unexpectedly became neutral")
        })?;
        let bytes = read(device, queue, output).await?;
        let clean_prepared = filter
            .prepare(device, queue, &mut scope, capture, parameters)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        let mut encoder = device.create_command_encoder(&Default::default());
        let clean_encoded = filter.encode(&mut encoder, &mut scope);
        queue.submit([encoder.finish()]);
        results.push(serde_json::json!({
            "opacity": opacity, "pixels": bytes,
            "pipelineCompiles": prepared.pipeline_compiles,
            "textureAllocations": prepared.texture_allocations,
            "bytesUploaded": prepared.bytes_uploaded,
            "blurPasses": encoded.blur_passes, "outputPasses": encoded.output_passes,
            "scratchBytes": prepared.scratch_bytes,
            "cleanBytesUploaded": clean_prepared.bytes_uploaded,
            "cleanTextureAllocations": clean_prepared.texture_allocations,
            "cleanBlurPasses": clean_encoded.blur_passes, "cleanOutputPasses": clean_encoded.output_passes,
        }));
    }
    serde_json::to_string(&results)
        .map(|value| JsValue::from_str(&value))
        .map_err(|error| JsValue::from_str(&error.to_string()))
}

async fn read(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Result<Vec<u8>, JsValue> {
    let width = texture.width();
    let stride = (width * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("RGBA Gaussian explicit test readback"),
        size: u64::from(stride * texture.height()),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(stride),
                rows_per_image: Some(texture.height()),
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let callback_buffer = buffer.clone();
    let completed = Rc::new(Cell::new(false));
    let promise = js_sys::Promise::new(&mut |resolve, reject| {
        let resolve = resolve.clone();
        let reject = reject.clone();
        let buffer = callback_buffer.clone();
        let completed = Rc::clone(&completed);
        callback_buffer.map_async(wgpu::MapMode::Read, .., move |result| {
            completed.set(true);
            match result {
                Ok(()) => {
                    let result = buffer
                        .get_mapped_range(..)
                        .map(|bytes| js_sys::Uint8Array::from(&*bytes));
                    buffer.unmap();
                    match result {
                        Ok(bytes) => {
                            let _ = resolve.call1(&JsValue::UNDEFINED, &bytes);
                        }
                        Err(error) => {
                            let _ = reject
                                .call1(&JsValue::UNDEFINED, &JsValue::from_str(&error.to_string()));
                        }
                    }
                }
                Err(error) => {
                    let _ =
                        reject.call1(&JsValue::UNDEFINED, &JsValue::from_str(&error.to_string()));
                }
            }
        });
    });
    // WebGL fences become observable after yielding to the browser. This
    // bounded pump belongs solely to explicit qualification readback.
    for _ in 0..6000 {
        device
            .poll(wgpu::PollType::Poll)
            .map_err(|error| JsValue::from_str(&error.to_string()))?;
        if completed.get() {
            break;
        }
        let yield_to_browser = js_sys::Promise::new(&mut |resolve, reject| {
            let result = web_sys::window()
                .ok_or_else(|| JsValue::from_str("raster qualification requires a browser window"))
                .and_then(|window| {
                    window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 5)
                });
            if let Err(error) = result {
                let _ = reject.call1(&JsValue::UNDEFINED, &error);
            }
        });
        JsFuture::from(yield_to_browser).await?;
    }
    if !completed.get() {
        return Err(JsValue::from_str(
            "Gaussian raster readback did not complete within the bounded poll window",
        ));
    }
    let bytes = JsFuture::from(promise)
        .await?
        .unchecked_into::<js_sys::Uint8Array>()
        .to_vec();
    Ok(bytes
        .chunks_exact(stride as usize)
        .flat_map(|row| row[..(width * 4) as usize].iter().copied())
        .collect())
}

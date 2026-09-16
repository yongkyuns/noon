// File access, NumPy coercion and asynchronous loading through the real worker.
import assert from 'node:assert/strict';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { PNG } from 'pngjs';

export const IMAGE_INPUT_CASES = Object.freeze([
  'nested-gray', 'nested-gray-channel', 'nested-rgb', 'nested-rgba',
  'numpy-gray', 'numpy-gray-channel', 'numpy-rgb', 'numpy-rgba', 'numpy-strided',
  'png-bytes', 'png-file-object', 'png-path', 'png-filename',
  'jpeg-bytes', 'jpeg-path', 'png-url', 'png-blob',
]);

export async function qualifyPythonImageInputs({context, baseUrl, root, oracleRoot, artifacts, backend, reports}) {
  const directory = path.join(root, 'browser-smoke-artifacts/image-input-assets');
  await mkdir(directory, {recursive: true});
  const png = await readFile(path.join(oracleRoot, 'input-fixture.png'));
  const jpeg = await readFile(path.join(oracleRoot, 'input-fixture.jpg'));
  await writeFile(path.join(directory, 'fixture.png'), png);
  const source = [
    `INPUT_CASES = ${JSON.stringify(IMAGE_INPUT_CASES)}`,
    `INPUT_PNG_HEX = ${JSON.stringify(png.toString('hex'))}`,
    `INPUT_JPEG_HEX = ${JSON.stringify(jpeg.toString('hex'))}`,
    `INPUT_URL = ${JSON.stringify(`${baseUrl}/browser-smoke-artifacts/image-input-assets/fixture.png`)}`,
    await readFile(path.join(root, 'scripts/image-input-smoke.py'), 'utf8'),
  ].join('\n');
  const expected = PNG.sync.read(await readFile(path.join(oracleRoot, 'input-raster.png')));
  const page = await context.newPage();
  page.on('pageerror', error => console.error('image inputs:', error));
  try {
    await page.goto(`${baseUrl}/web/manim-raster-host.html`);
    await page.waitForFunction(() => window.noonHostRaster);
    await page.evaluate(() => {
      const canvas = document.querySelector('#scene');
      canvas.width = canvas.height = 256;
      canvas.style.width = canvas.style.height = '256px';
    });
    const loaded = await page.evaluate(({source, duration}) => window.noonHostRaster.load(source, duration),
      {source, duration: IMAGE_INPUT_CASES.length + 1});
    assert.equal(loaded.rendererBackend, backend === 'webgpu' ? 'WebGPU' : 'WebGL2');
    const times = Array.from({length: IMAGE_INPUT_CASES.length + 1}, (_, i) => i + 0.5);
    for (const [index, input] of IMAGE_INPUT_CASES.entries()) {
      const frame = await page.evaluate(({index, times}) => window.noonHostRaster.renderThrough(index, times), {index, times});
      assert.equal(frame.time, times[index]);
      assert.equal(frame.objectCount, 2, `${input}: background and one admitted image`);
      const bytes = await page.locator('#scene').screenshot();
      await writeFile(path.join(artifacts, `${backend}-input-${input}.png`), bytes);
      const actual = PNG.sync.read(bytes);
      assert.deepEqual([actual.width, actual.height], [expected.width, expected.height]);
      let maximum = 0;
      for (let i = 0; i < actual.data.length; ++i) {
        maximum = Math.max(maximum, Math.abs(actual.data[i] - expected.data[i]));
      }
      const observation = {kind: 'python-image-input', backend, input, maximum, time: frame.time};
      reports.push(observation);
      // JPEG is lossy; both PNG and array paths must match the reference exactly
      // apart from a single quantization level on the actual browser surface.
      assert.ok(maximum <= (input.startsWith('jpeg-') ? 2 : 1), JSON.stringify(observation));
    }
    const final = await page.evaluate(({index, times}) => window.noonHostRaster.renderThrough(index, times),
      {index: IMAGE_INPUT_CASES.length, times});
    assert.equal(final.objectCount, 1, 'every image must be removed after its input case');
    assert.equal(final.time, times.at(-1));
    reports.push({kind: 'python-image-input-completion', backend, count: IMAGE_INPUT_CASES.length, time: final.time});
    console.log(`[PASS] ${backend}: ${IMAGE_INPUT_CASES.length} real Python image inputs and rejected-input recovery`);
  } finally {
    await page.evaluate(() => window.noonHostRaster?.close()).catch(() => {});
    await page.close();
  }
}

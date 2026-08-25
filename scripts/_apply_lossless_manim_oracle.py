from pathlib import Path

path = Path('scripts/manim-raster-differential.mjs')
text = path.read_text()

start = text.index('async function findSingleFile(')
end = text.index('function sampleFrames(', start)
text = text[:start] + '''async function findPngFrames(root, scene) {
  const escapedScene = scene.replace(/[.*+?^${}()|[\\]\\\\]/g, "\\\\$&");
  const pattern = new RegExp(`^${escapedScene}(\\\\d+)\\\\.png$`);
  const frames = (await walkFiles(root))
    .map((file) => ({ file, match: path.basename(file).match(pattern) }))
    .filter(({ match }) => match)
    .sort((left, right) => Number(left.match[1]) - Number(right.match[1]))
    .map(({ file }) => file);
  assert.ok(frames.length > 0, `${scene}: expected Manim PNG frames under ${root}`);
  return frames;
}

''' + text[end:]

start = text.index('function extractFrame(')
end = text.index('async function renderManimReferences()', start)
text = text[:start] + text[end:]

text = text.replace('"--format=mp4",', '"--format=png",')

old = '''    const videoPath = await findSingleFile(mediaDir, ".mp4", fixture.scene);
    const video = probeVideo(videoPath);
    assert.equal(video.width, reference.pixel_width, `${fixture.id}: Manim reference width`);
    assert.equal(video.height, reference.pixel_height, `${fixture.id}: Manim reference height`);
    assert.ok(
      Math.abs(video.frameRate - reference.frame_rate) < 1e-9,
      `${fixture.id}: Manim reference FPS ${video.frameRate}`,
    );

    const samples = sampleFrames(video.frameCount);
    for (const sample of samples) {
      const outputPath = path.join(frameDir, `${sample.label}.png`);
      extractFrame(videoPath, sample.frameIndex, outputPath);
      sample.referencePath = outputPath;
    }

    results.set(fixture.id, { fixture, video, samples });
'''
new = '''    const frameFiles = await findPngFrames(mediaDir, fixture.scene);
    const firstFrame = PNG.sync.read(await readFile(frameFiles[0]));
    const frames = {
      frameCount: frameFiles.length,
      frameRate: reference.frame_rate,
      duration: frameFiles.length / reference.frame_rate,
      width: firstFrame.width,
      height: firstFrame.height,
      format: "png-sequence",
    };
    assert.equal(frames.width, reference.pixel_width, `${fixture.id}: Manim reference width`);
    assert.equal(frames.height, reference.pixel_height, `${fixture.id}: Manim reference height`);

    const samples = sampleFrames(frames.frameCount);
    for (const sample of samples) {
      const outputPath = path.join(frameDir, `${sample.label}.png`);
      await writeFile(outputPath, await readFile(frameFiles[sample.frameIndex]));
      sample.referencePath = outputPath;
    }

    results.set(fixture.id, { fixture, frames, samples });
'''
if old not in text:
    raise SystemExit('reference video block not found')
text = text.replace(old, new)

text = text.replace('referenceResult.video.duration', 'referenceResult.frames.duration')
text = text.replace('manimVideoDuration: referenceResult.video.duration', 'manimDuration: referenceResult.frames.duration')
text = text.replace('manim: referenceResult.video,', 'manim: referenceResult.frames,')

path.write_text(text)

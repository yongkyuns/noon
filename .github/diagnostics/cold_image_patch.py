"""One-shot #2211 diagnostic. Never updates repository refs or pull requests."""
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import urllib.request

BASE = "172806b576cd7a363ff13e7a42cfb54dbabf8c03"
EXPECTED = {
    "crates/noon-render-wgpu/src/gpu/retained_text.rs": "cc42443802b160ff5f8e3debab2defdcda5e79e7",
    "crates/noon-render-wgpu/src/gpu/raster_image_prepare.rs": "bf59aace6a37a9a6b40340fa541912d577906a16",
    "crates/noon/tests/raster_support/mod.rs": "019f18917440862629601f7d5df6a04cd82ddbf8",
    "crates/noon/tests/prepared_prefix_lifecycle.rs": "d4a91b40b2ed6dfa107caec0d9bd44ad594df790",
    "crates/noon/tests/prepared_prefix_identity/terminal.rs": "07cd58780e82e000ea4400a69f6eef6966a603d5",
}


def blob_hash(data):
    return hashlib.sha1(f"blob {len(data)}\0".encode() + data).hexdigest()


def replace_once(path, old, new):
    p = Path(path)
    text = p.read_text()
    if text.count(old) != 1:
        raise RuntimeError(f"expected exactly one source match in {path}")
    p.write_text(text.replace(old, new, 1))


def patch():
    for path, expected in EXPECTED.items():
        if blob_hash(Path(path).read_bytes()) != expected:
            raise RuntimeError(f"baseline mismatch: {path}")
    replace_once(
        "crates/noon-render-wgpu/src/gpu/raster_image_prepare.rs",
        "pub(super) struct ImagePreparation {\n    reset: bool,",
        "pub(super) struct ImagePreparation {\n    pub(super) reset: bool,",
    )
    replace_once(
        "crates/noon-render-wgpu/src/gpu/retained_text.rs",
        """            .map_err(RetainedPrepareError::Image)?;
        Ok(PreparedImagePublication {
            rows,
            eligibility: self.stage_image_draw_eligibility(frame, changes),
            object_count: frame.objects.len(),
        })""",
        """            .map_err(RetainedPrepareError::Image)?;
        // Cold or failed image preparation rebuilds residency even when the
        // runtime delta is empty or sparse. Rebuild draw eligibility from that
        // same decision; consumed producer dirtiness is not a renderer baseline.
        // Successful warm preparation keeps the ordinary sparse path.
        let reset_changes = rows.reset.then(FrameChanges::all);
        let eligibility_changes = reset_changes.as_ref().unwrap_or(changes);
        Ok(PreparedImagePublication {
            rows,
            eligibility: self.stage_image_draw_eligibility(frame, eligibility_changes),
            object_count: frame.objects.len(),
        })""",
    )
    support = "crates/noon/tests/raster_support/mod.rs"
    replace_once(support, "self.capture_frame(publication, false, None)",
                 "self.capture_frame(publication, false, None, None)")
    replace_once(support, "self.capture_frame(publication, true, overlay)",
                 "self.capture_frame(publication, true, overlay, None)")
    replace_once(support, "    fn capture_frame(\n", """    #[allow(dead_code)]
    pub fn capture_visible(
        &mut self,
        publication: &RendererPublication<'_>,
        visible_indices: &[usize],
    ) -> Vec<u8> {
        self.capture_frame(publication, false, None, Some(visible_indices))
    }

    fn capture_frame(
""")
    replace_once(support, """        interactive: bool,
        overlay: Option<noon_render_wgpu::AnalyticOverlay>,
    ) -> Vec<u8> {""", """        interactive: bool,
        overlay: Option<noon_render_wgpu::AnalyticOverlay>,
        visible_indices: Option<&[usize]>,
    ) -> Vec<u8> {""")
    replace_once(support, """        let prepared = self
            .preparer
            .prepare_publication(
                &self.device,
                publication,
                TextDeviceMetrics::uniform(self.layout.height() as f32 / WORLD_SIZE).unwrap(),
            )
            .unwrap();""", """        let metrics =
            TextDeviceMetrics::uniform(self.layout.height() as f32 / WORLD_SIZE).unwrap();
        let prepared = match visible_indices {
            Some(indices) => self.preparer.prepare_publication_visible(
                &self.device,
                publication,
                indices,
                metrics,
            ),
            None => self
                .preparer
                .prepare_publication(&self.device, publication, metrics),
        }
        .unwrap();""")
    lifecycle = "crates/noon/tests/prepared_prefix_lifecycle.rs"
    replace_once(lifecycle, """        let mut fresh = raster_support::Raster::new().await;
        assert_eq!(fresh.capture(&reader.take_renderer_publication()), endpoint);
        eprintln!(\"PREFIX_TERMINAL_RASTER: exact endpoint image, unchanged append frame, repeated rewind, producer destruction, fresh renderer\");""", """        // No seek or manufactured full delta before renderer recreation.
        let publication = reader.take_renderer_publication();
        assert!(publication.changes().is_empty());
        let context = publication.context();
        assert_cold_image_recovery(&publication, &endpoint).await;
        drop(publication);
        assert_eq!(reader.take_renderer_publication().context(), context);
        assert_eq!(reader.frame().time, 1.0);
        assert_eq!(reader.end_time(), Some(1.0));

        // A new renderer must also reconstruct an unchanged image when its
        // first publication dirties only an unrelated analytic object.
        let object = reader
            .frame()
            .objects
            .iter()
            .find(|row| matches!(row.content.geometry(), Some(noon_core::GeometryRef::Circle { .. })))
            .unwrap()
            .id;
        let binding = noon_core::SemanticClickIndicate::new(1.25, noon_core::Color::YELLOW, 0.5);
        let prepared = reader.prepare_click_indicate(object, binding).unwrap().unwrap();
        reader.start_transient_animation(prepared).unwrap();
        reader.advance_interactions(10.0).unwrap();
        reader.advance_interactions(10.25).unwrap();
        let publication = reader.take_renderer_publication();
        let image_index = publication
            .frame()
            .objects
            .iter()
            .position(|row| row.content.image().is_some())
            .unwrap();
        assert!(!publication.changes().is_empty());
        assert!(!publication.changes().is_all());
        assert!(!publication.changes().object_indices().contains(&image_index));
        let context = publication.context();
        let expected = raster.capture(&publication);
        assert_cold_image_recovery(&publication, &expected).await;
        drop(publication);
        assert_eq!(reader.take_renderer_publication().context(), context);
        assert!(reader.interactions_active());
        assert_eq!(reader.frame().time, 1.0);
        reader.advance_interactions(10.5).unwrap();
        assert_eq!(raster.capture(&reader.take_renderer_publication()), endpoint);
        eprintln!(\"PREFIX_TERMINAL_RASTER: exact endpoint, unchanged append, rewind, source destruction, clean/sparse cold recovery, visible projection, zero-upload warm reuse\");""")
    p = Path(lifecycle)
    p.write_text(p.read_text() + """

async fn assert_cold_image_recovery(
    publication: &noon_runtime::RendererPublication<'_>,
    expected: &[u8],
) {
    let visible: Vec<_> = publication.painter_order().iter().map(|&index| index as usize).collect();
    let mut fresh = raster_support::Raster::new().await;
    assert_eq!(fresh.capture(publication).as_slice(), expected);
    if publication.changes().is_empty() {
        assert_eq!(fresh.capture(publication).as_slice(), expected);
        assert_eq!(fresh.last_scene_upload_bytes, 0);
        assert_eq!(fresh.last_scene_repacked, 0);
    }

    let mut viewport = raster_support::Raster::new().await;
    assert_eq!(viewport.capture_visible(publication, &visible).as_slice(), expected);
    if publication.changes().is_empty() {
        assert_eq!(viewport.capture_visible(publication, &visible).as_slice(), expected);
        assert_eq!(viewport.last_scene_upload_bytes, 0);
        assert_eq!(viewport.last_scene_repacked, 0);
    }
}
""")


def publish():
    if os.environ["GITHUB_REPOSITORY"] != "yongkyuns/noon":
        raise RuntimeError("wrong repository")
    if os.environ["GITHUB_REF"] != "refs/heads/diagnostic/noon-cold-image-20261022":
        raise RuntimeError("wrong diagnostic branch")
    changed = subprocess.check_output(["git", "diff", "--name-only"], text=True).splitlines()
    if set(changed) != set(EXPECTED):
        raise RuntimeError(f"unexpected tested changes: {changed}")
    result = {"base": BASE, "diagnostic_commit": os.environ["GITHUB_SHA"], "blobs": {}}
    for path in changed:
        data = Path(path).read_bytes()
        request = urllib.request.Request(
            "https://api.github.com/repos/yongkyuns/noon/git/blobs",
            data=json.dumps({"content": base64.b64encode(data).decode(), "encoding": "base64"}).encode(),
            headers={"Authorization": "Bearer " + os.environ["GH_TOKEN"], "Accept": "application/vnd.github+json", "Content-Type": "application/json"},
            method="POST",
        )
        with urllib.request.urlopen(request, timeout=60) as response:
            sha = json.load(response)["sha"]
        if sha != blob_hash(data):
            raise RuntimeError(f"uploaded blob mismatch: {path}")
        result["blobs"][path] = sha
    Path("cold-image-proof/blobs.json").write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    if sys.argv[1:] == ["publish"]:
        publish()
    elif not sys.argv[1:]:
        patch()
    else:
        raise SystemExit("usage: cold_image_patch.py [publish]")

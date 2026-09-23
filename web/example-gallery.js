// Public routing only. Exact-source reference contracts remain unchanged.
export * from "./example-gallery-reference.js";
import {
  loadGalleryManifest as loadReferenceGallery,
  parityLabel as referenceParityLabel,
} from "./example-gallery-reference.js";
import { isShowcaseRequest, loadShowcaseGallery } from "./showcase-gallery.js";
import { installShowcasePresentation } from "./showcase-presentation.js";

export async function loadGalleryManifest(
  url,
  fetchImpl = globalThis.fetch,
  locationLike = globalThis.location,
) {
  const showcase = url === undefined && isShowcaseRequest(locationLike);
  const gallery = showcase
    ? await loadShowcaseGallery(fetchImpl)
    : await loadReferenceGallery(url, fetchImpl);
  if (typeof document !== "undefined") {
    installShowcasePresentation(document, gallery.examples, showcase);
  }
  return gallery;
}

export function parityLabel(status) {
  return status === "noon-showcase" ? "Noon showcase preview" : referenceParityLabel(status);
}

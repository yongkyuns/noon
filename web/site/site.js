import { installTheme } from "./theme.js";
// Existing shared links used the playground at the project root. A query means
// "open the playground"; section anchors without a query remain on the website.
export function playgroundRedirect(locationLike) {
  if (!locationLike.search || locationLike.search === "?") return null;
  const target = new URL("./playground.html", locationLike.href);
  target.search = locationLike.search;
  target.hash = locationLike.hash;
  return target.href;
}

if (typeof window !== "undefined") {
  // The legacy query redirect applies only to the project homepage.
  // Documentation pages must retain their own search parameters.
  if (/\/(?:index\.html)?$/.test(window.location.pathname)) {
    const target = playgroundRedirect(window.location);
    if (target) window.location.replace(target);
  }
  // No autoplay: clips start only after an explicit gesture, including when the
  // visitor prefers reduced motion. Native controls work without JavaScript.
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) document.querySelectorAll("video").forEach(video => video.pause());
  });
}

if (typeof document !== "undefined") installTheme();

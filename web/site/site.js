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
  const target = playgroundRedirect(window.location);
  if (target) window.location.replace(target);
  // No autoplay: clips start only after an explicit gesture, including when the
  // visitor prefers reduced motion. Native controls work without JavaScript.
  document.addEventListener("visibilitychange", () => {
    if (document.hidden) document.querySelectorAll("video").forEach(video => video.pause());
  });
}

function setupTheme() {
  const root = document.documentElement;
  let saved = null;
  try { saved = localStorage.getItem("noon-site-theme"); } catch {}
  if (saved === "light" || saved === "dark") root.dataset.theme = saved;
  const currentTheme = () => root.dataset.theme || (matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
  const updateLabels = () => document.querySelectorAll("[data-theme-toggle]").forEach(button => {
    button.textContent = currentTheme() === "dark" ? "Light" : "Dark";
    button.setAttribute("aria-label", "Switch to " + (currentTheme() === "dark" ? "light" : "dark") + " theme");
  });
  updateLabels();
  for (const button of document.querySelectorAll("[data-theme-toggle]")) button.addEventListener("click", () => {
    const next = currentTheme() === "dark" ? "light" : "dark";
    root.dataset.theme = next;
    try { localStorage.setItem("noon-site-theme", next); } catch {}
    updateLabels();
  });
  matchMedia("(prefers-color-scheme: dark)").addEventListener?.("change", updateLabels);
}
if (typeof document !== "undefined") {
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", setupTheme, {once:true});
  else setupTheme();
}

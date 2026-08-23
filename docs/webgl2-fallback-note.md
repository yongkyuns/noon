# Browser GPU fallback

The Noon playground should prefer browser WebGPU when a usable adapter is available and fall back automatically to WebGL2 otherwise. This specifically covers Chrome/Linux configurations where `navigator.gpu` exists but `requestAdapter()` returns no adapter, including common X11 setups.

The final implementation and CI validation are applied by the branch's one-shot PR workflow and this note can remain as a portability rationale.

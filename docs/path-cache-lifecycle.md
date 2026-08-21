# Path mesh cache lifecycle

Noon's renderer caches tessellated `VectorPath` meshes so transform, color, opacity, reveal, and morph-progress changes can reuse geometry instead of tessellating every frame.

Long-lived interactive authoring changes the cache requirement: a session can create many distinct path/style combinations over time, so retaining every mesh forever would make renderer memory grow monotonically even after those paths disappear from the scene.

## Policy

`FramePreparer` now treats the path mesh cache as a bounded retained working set.

- The default retention target is 256 path/style meshes.
- Every cache hit and insertion advances a deterministic recency counter.
- Eviction only runs when entering a full frame rebuild. Incremental frame updates never compact or reindex the cache.
- Before applying the retention target, meshes already required by the incoming frame are pinned.
- Remaining stale entries are retained by least-recently-used policy up to the configured target.
- If the incoming frame itself needs more meshes than the target, all of those active meshes remain cached. The target bounds stale retention; it never makes an active frame incomplete or forces already-cached active geometry to be retessellated.
- Cache compaction rebuilds the hash lookup from the retained entries, so lookup indices and the compacted mesh vector stay consistent.

This keeps the frame-critical incremental path unchanged while bounding memory retained from historical authoring states.

## Configuration

`FramePreparer::set_path_mesh_cache_limit(limit)` changes the retention target. The setting takes effect at a subsequent full rebuild boundary.

`FramePreparer::path_mesh_cache_limit()` reports the configured target, and `cached_path_mesh_count()` reports the current number of cached meshes. The current count can legitimately exceed the target when the incoming active frame itself needs more unique path/style meshes.

A limit of zero means no stale mesh retention: meshes used by the incoming frame are still pinned and may remain in the cache.

## Correctness invariants

The implementation is tested for these properties:

1. Recently reused meshes survive stale-cache pruning.
2. A least-recently-used stale mesh is tessellated again after eviction.
3. Incoming-frame meshes are pinned before eviction, even when the active set exceeds the configured target.
4. Full-rebuild compaction does not leave stale hash lookup indices.
5. Incremental animation updates continue to reuse existing path geometry and do not trigger cache pruning.

The cache policy is deliberately CPU-side and renderer-local. It does not change semantic scene identity, timeline evaluation, serialized IR, or the WebGPU instance format.

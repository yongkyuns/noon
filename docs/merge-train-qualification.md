# Initial merge-train qualification evidence (2026-11-16)

This is an audit record of the first successful **Mergify-owned** merge for the opt-in `master` queue, tracked by [#2495](https://github.com/yongkyuns/noon/issues/2495). It is not evidence of a multi-PR cumulative batch or a failing-candidate rejection.

- Original candidate: [#2580](https://github.com/yongkyuns/noon/pull/2580), documentation only, head `6318fa97c951b50493f8ed868d9743f94ad5dee7`.
- Pre-merge `master`: `e2df403a2416e816a148775fc1522f1f68164b82`.
- Mergify's [integration draft #2590](https://github.com/yongkyuns/noon/pull/2590) tested commit `2507de15375406fe6aff706fc400834b9b5cb0b1`. Its tree was `73978f7553c4cc898a4ff9cd1cbf295707b6417c`.
- All six required integration jobs succeeded, plus the website check. The final selected website check completed at **16:35:29 UTC**.
- [Mergify merged #2580](https://github.com/yongkyuns/noon/pull/2580#issuecomment-6922594082) at **16:36:19 UTC** as `1069e88e252ee107bdad976ea292b84bb625907f`. The merge's tree exactly matches the integration draft; only `docs/merge-train.md` changed (+8 lines).

For later qualification, retain the same evidence standard: original PR SHA and checks, the actual Mergify-generated combined commit and tree, *every* applicable check and completion time, then the merge commit and tree. A successful single-candidate draft does not prove cumulative merging. Branch-protection settings require repository-owner verification because the connected GitHub app cannot read the protection API (HTTP 403).

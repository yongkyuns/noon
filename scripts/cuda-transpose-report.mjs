// External test-evidence codec only. Runtime handles, frames and metrics remain
// unchanged in memory. Large integer diagnostics use exact decimal strings;
// coercing them to Number could silently corrupt 64-bit identities/counters.
export function stringifyEvidence(value) {
  return JSON.stringify(value, (_key, item) =>
    typeof item === 'bigint' ? item.toString() : item, 2);
}

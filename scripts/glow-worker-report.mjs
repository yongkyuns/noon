/**
 * Preserve worker GPU/presentation counters that cross the JS boundary as BigInt.
 * JSON cannot encode BigInt natively; decimal strings retain the full u64 value
 * without rounding it through Number. Ordinary JSON fields are unchanged.
 */
export function encodeGlowWorkerReport(report) {
  return JSON.stringify(report, (_key, value) => (
    typeof value === "bigint" ? value.toString() : value
  ), 2) + "\n";
}

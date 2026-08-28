export function readCompletePlayerFrameMetrics(player) {
  const metrics = {
    cpuFrameMs: player.lastCpuFrameMs(),
    runtimeMs: player.lastRuntimeEvaluationMs(),
    prepareMs: player.lastFramePrepareMs(),
    uploadMs: player.lastUploadMs(),
    encodeSubmitMs: player.lastEncodeSubmitMs(),
  };

  return Object.values(metrics).every(Number.isFinite) ? metrics : null;
}

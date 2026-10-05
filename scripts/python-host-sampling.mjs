// Source completion is a semantic barrier, not a renderer receipt. Only source
// failure may interrupt a sample request; success still waits for presentation.
export async function sampleOrSourceFailure(sample, terminal) {
  const failed = terminal.then(result => result.ok
    ? new Promise(() => {}) : { terminal: result });
  const sampled = sample.then(value => ({ sample: value }), async error => {
    const result = await terminal;
    if (result.ok) throw error;
    return { terminal: result, sampleError: String(error) };
  });
  return Promise.race([sampled, failed]);
}

// Failure-only Chromium diagnostics. Attach through CDP after the real runtime
// assertion has failed; no debugger or probe participates in a passing run.
const RENDER_WORKER = /(?:^|\/)execution-render-worker\.js(?:[?#]|$)/;

function bounded(promise, milliseconds) {
  let timer;
  return Promise.race([
    Promise.resolve(promise),
    new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('CDP probe timeout')), milliseconds); }),
  ]).finally(() => clearTimeout(timer));
}

function frameSummary(frames, scripts) {
  return frames.slice(0, 16).map(frame => ({
    functionName: frame.functionName || '(anonymous)',
    url: frame.url || scripts.get(frame.location?.scriptId) || null,
    line: Number.isInteger(frame.location?.lineNumber) ? frame.location.lineNumber + 1 : null,
    column: Number.isInteger(frame.location?.columnNumber) ? frame.location.columnNumber + 1 : null,
  }));
}

export async function captureStalledRenderStacks(page, { timeoutMs = 2000 } = {}) {
  let cdp;
  try {
    cdp = await bounded(page.context().newCDPSession(page), timeoutMs);
    const { targetInfos = [] } = await bounded(cdp.send('Target.getTargets'), timeoutMs);
    const renderers = targetInfos.filter(target => target.type === 'worker' &&
      RENDER_WORKER.test(target.url || '')).slice(0, 8);
    const stacks = [];
    for (const target of renderers) {
      let sessionId = null;
      let paused = false;
      const scripts = new Map();
      let resolvePause;
      const pauseObserved = new Promise(resolve => { resolvePause = resolve; });
      const onTargetMessage = event => {
        if (event.sessionId !== sessionId) return;
        let message;
        try { message = JSON.parse(event.message); } catch { return; }
        if (message.method === 'Debugger.scriptParsed') {
          scripts.set(message.params.scriptId, message.params.url);
        } else if (message.method === 'Debugger.paused') {
          paused = true;
          resolvePause(frameSummary(message.params.callFrames ?? [], scripts));
        }
      };
      cdp.on('Target.receivedMessageFromTarget', onTargetMessage);
      const sendWorker = (id, method) => bounded(cdp.send('Target.sendMessageToTarget', {
        sessionId, message: JSON.stringify({ id, method }),
      }), timeoutMs);
      try {
        ({ sessionId } = await bounded(cdp.send('Target.attachToTarget', {
          targetId: target.targetId, flatten: false,
        }), timeoutMs));
        // V8's inspector can interrupt a busy JS loop, even when worker.evaluate
        // and normal worker messages cannot run. An uninterruptible native GPU
        // call simply times out and remains an explicit unknown, not a stack.
        await sendWorker(1, 'Debugger.enable');
        await sendWorker(2, 'Debugger.pause');
        const frames = await bounded(pauseObserved, timeoutMs);
        stacks.push({ url: target.url, frames });
      } catch (error) {
        stacks.push({ url: target.url, error: String(error) });
      } finally {
        if (paused) await sendWorker(3, 'Debugger.resume').catch(() => {});
        if (sessionId !== null) {
          await bounded(cdp.send('Target.detachFromTarget', { sessionId }), timeoutMs).catch(() => {});
        }
        cdp.off('Target.receivedMessageFromTarget', onTargetMessage);
      }
    }
    return stacks;
  } catch (error) {
    return [{ error: String(error) }];
  } finally {
    if (cdp) await bounded(cdp.detach(), timeoutMs).catch(() => {});
  }
}

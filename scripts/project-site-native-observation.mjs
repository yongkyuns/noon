// Diagnostic replay only. Keep receiver, arguments, return/Promise identity and
// thrown errors unchanged while recording which native call fails to return.
export function observeRendererCalls(Renderer, emit, isTransition = () => true) {
  const restorers = [];
  for (const [owner, name] of [[Renderer, 'create'], ...[
    'free', 'flushGpuDiagnostics', 'resize', 'replaceResourceBundle',
    'applyDeltaTransportJson', 'render',
    'waitForSubmittedWork',
  ].map(name => [Renderer.prototype, name])]) {
    const original = owner[name];
    if (typeof original !== 'function') continue;
    function wrapped(...args) {
      const observed = ['create', 'free', 'flushGpuDiagnostics', 'resize',
        'replaceResourceBundle'].includes(name) || isTransition();
      const record = phase => { if (observed) { try { emit({name, phase}); } catch {} } };
      record('enter');
      try {
        const result = Reflect.apply(original, this, args);
        record('return');
        if (observed && result instanceof Promise) {
          void result.then(() => record('resolved'), () => record('rejected'));
        }
        return result;
      } catch (error) { record('threw'); throw error; }
    }
    owner[name] = wrapped;
    restorers.push(() => { if (owner[name] === wrapped) owner[name] = original; });
  }
  return () => restorers.reverse().forEach(restore => restore());
}

// A separate failure-only replay, NEVER the merge qualification. Preserve the
// original assertion/captures and run the same real test in a fresh browser.
import { chromium } from 'playwright';
import { appendFileSync, existsSync } from 'node:fs';
import { cp, mkdir, rm, rename } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { observeRendererCalls } from './project-site-native-observation.mjs';

const output = fileURLToPath(new URL('../browser-smoke-artifacts/project-site/', import.meta.url));
const original = `${output}/embedded`, backup = `${output}/original-runtime-failure`;
await mkdir(output, {recursive:true});
if (existsSync(original)) await cp(original, backup, {recursive:true});
const journal = `${output}/native-replay.ndjson`;
const prefix = 'NOON_NATIVE_CALL ';
let nextWorker = 0;
const launch = chromium.launch;
chromium.launch = async function (...args) {
  const browser = await Reflect.apply(launch, this, args);
  const newContext = browser.newContext;
  browser.newContext = async function (...contextArgs) {
    const context = await Reflect.apply(newContext, this, contextArgs);
    context.on('page', page => {
      page.on('console', message => {
        const text = message.text();
        if (text.startsWith(prefix)) appendFileSync(journal, `${text.slice(prefix.length)}\n`);
      });
      page.on('worker', worker => {
        if (!worker.url().endsWith('/execution-render-worker.js')) return;
        const id = ++nextWorker;
        void worker.evaluate(async ({id, source, prefix}) => {
          const { RetainedExecutionCanvasRenderer: Renderer } = await import(new URL('./pkg/noon_web.js', self.location.href).href);
          let requestId = null;
          self.addEventListener('message', event => {
            if (['rebuild_engine','switch_engine'].includes(event.data?.type)) requestId = event.data.requestId;
          });
          const post = self.postMessage;
          self.postMessage = function (...args) {
            if (['renderer_rebuilt','mode_switched','error'].includes(args[0]?.type)) requestId = null;
            return Reflect.apply(post, this, args);
          };
          // Source is our checked-in pure observer, never user-authored code.
          const observe = (0, eval)(`(${source})`);
          observe(Renderer, event => console.debug(prefix + JSON.stringify({
            worker:id,requestId,atMs:performance.now(),...event,
          })), () => requestId !== null);
        }, {id,source:observeRendererCalls.toString(),prefix}).catch(error => {
          appendFileSync(journal, `${JSON.stringify({worker:id,installError:String(error)})}\n`);
        });
      });
    });
    return context;
  };
  return browser;
};
try {
  await import('./project-site-embed-smoke.mjs');
} finally {
  chromium.launch = launch;
  if (existsSync(original)) await rename(original, `${output}/native-replay-captures`);
  if (existsSync(backup)) {
    await cp(backup, original, {recursive:true});
    await rm(backup, {recursive:true});
  }
}

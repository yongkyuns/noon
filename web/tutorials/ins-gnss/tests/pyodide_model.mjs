// Run the actual numerical implementation inside Noon's pinned WASM Python.
import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
const [runtime, source] = process.argv.slice(2);
if (!runtime || !source) throw new Error('Usage: node pyodide_model.mjs /path/to/pyodide.mjs /path/to/model.py');
const { loadPyodide } = await import(pathToFileURL(runtime));
const pyodide = await loadPyodide();
await pyodide.runPythonAsync(await readFile(source,'utf8'));
console.log(await pyodide.runPythonAsync(`
import json, sys
from bisect import bisect_right
assert bisect_right([0., 1.], 0.5) == 1
c = Experiment()
rows = simulate(c)
assert len(rows) == 12001
assert abs(rows[-1].bias-c.bias) < .005
assert all(s.accepted is None for s in rows if c.outage_start <= s.time < c.outage_end)
json.dumps(dict(python=sys.version, pyodide_verified=True, metrics=metrics(rows,c)))
`));

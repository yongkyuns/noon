// Execute the same numerical test suite in Noon's pinned WASM Python.
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const [runtime, source] = process.argv.slice(2);
if (!runtime || !source) throw new Error('Usage: node pyodide_model.mjs /path/to/pyodide.mjs /path/to/model.py');
const { loadPyodide } = await import(pathToFileURL(runtime));
const pyodide = await loadPyodide();
const testSource = path.join(path.dirname(source), '../tests/test_model.py');
pyodide.FS.mkdirTree('/tutorial/src');
pyodide.FS.mkdirTree('/tutorial/tests');
pyodide.FS.writeFile('/tutorial/src/model.py', await readFile(source, 'utf8'));
pyodide.FS.writeFile('/tutorial/src/uncertainty.py',
  await readFile(path.join(path.dirname(source), 'uncertainty.py'), 'utf8'));
pyodide.FS.writeFile('/tutorial/tests/test_model.py', await readFile(testSource, 'utf8'));

for (const [kind, name] of [['src', 'prediction.py'], ['tests', 'test_prediction.py'],
                            ['src', 'measurement.py'], ['tests', 'test_measurement.py']]) {
  pyodide.FS.writeFile(`/tutorial/${kind}/${name}`,
    await readFile(path.join(path.dirname(source), '..', kind, name), 'utf8'));
}

const report = JSON.parse(await pyodide.runPythonAsync(`
import io, json, sys, unittest
from bisect import bisect_right
sys.path[:0] = ['/tutorial/src', '/tutorial/tests']
from model import Experiment, simulate, metrics
import test_model
assert bisect_right([0., 1.], 0.5) == 1
stream = io.StringIO()
suite = unittest.defaultTestLoader.loadTestsFromModule(test_model)
result = unittest.TextTestRunner(stream=stream, verbosity=2).run(suite)
config = Experiment()
json.dumps(dict(python=sys.version, pyodide_verified=result.wasSuccessful(),
                tests_run=result.testsRun, test_log=stream.getvalue(),
                metrics=metrics(simulate(config),config)))
`));
console.log(JSON.stringify(report, null, 2));
if (!report.pyodide_verified) throw new Error('Numerical tests failed in Pyodide');

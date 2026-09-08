// Fixed inputs for the existing scene codec, worker transport and recovery tests.
// Captured from the former fixture producer at c7d708c7; semantic authoring is
// qualified by paired live programs. #959 deletes these with the old codec.
const fixtures = fetch(new URL("../web/fixtures/execution-transport.json", import.meta.url))
  .then((response) => {
    if (!response.ok) throw new Error(`transport fixtures: HTTP ${response.status}`);
    return response.json();
  });

export async function loadExecutionTransportFixture(name) {
  const corpus = await fixtures;
  if (!Object.hasOwn(corpus, name)) throw new Error(`unknown transport fixture: ${name}`);
  return JSON.stringify(corpus[name]);
}

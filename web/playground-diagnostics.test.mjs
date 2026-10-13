import assert from "node:assert/strict";
import test from "node:test";
import { fallbackFailureMessage } from "./playground-diagnostics.js";

test("missing diagnostic metadata still shows the original Python or startup failure", () => {
  assert.equal(fallbackFailureMessage(new Error("NameError: MissingShape is not defined\nPython traceback follows")),
    "NameError: MissingShape is not defined");
  assert.equal(fallbackFailureMessage(new Error("Unable to initialize Pyodide: network request failed")),
    "Unable to initialize Pyodide: network request failed");
});

test("fallback preserves literal text; it never guesses a Noon exception category", () => {
  assert.equal(fallbackFailureMessage(new Error("<script>evil()</script>")), "<script>evil()</script>");
  assert.equal(fallbackFailureMessage("unsupported_operation is only user text"),
    "unsupported_operation is only user text");
});

test("fallback bounds lengthy Unicode errors without splitting surrogate pairs", () => {
  const summary = fallbackFailureMessage(new Error("😀".repeat(500)));
  assert.ok([...summary].length <= 400);
  assert.doesNotThrow(() => encodeURI(summary));
});

test("empty or inaccessible messages retain the truthful generic fallback", () => {
  assert.equal(fallbackFailureMessage(new Error(" \n\n ")), "Noon could not complete this operation.");
  assert.equal(fallbackFailureMessage({ get message() { throw new Error("inaccessible"); } }),
    "Noon could not complete this operation.");
});

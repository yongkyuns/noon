import assert from "node:assert/strict";
import test from "node:test";
import { fallbackFailureMessage, failureTechnicalDetails } from "./playground-diagnostics.js";

test("technical details retain the Python traceback when Safari's stack omits the message", () => {
  const message = 'Python failed: Traceback (most recent call last):\n  File "noon-s1-r3.py", line 85\nValueError: Unreachable code should not be executed';
  const stack = "#handleMessage@https://example.test/authoring-client.js:245:32";
  assert.equal(failureTechnicalDetails({ message, stack }), `${message}\n\n${stack}`);
});

test("technical details do not duplicate messages already present in a stack", () => {
  const error = new Error("Python failed: ValueError\nOriginal Python traceback");
  assert.equal(failureTechnicalDetails(error), error.stack);
});

test("technical details preserve plain failures and tolerate inaccessible properties", () => {
  assert.equal(failureTechnicalDetails("Original failure"), "Original failure");
  const error = new Error("A failure without a stack");
  delete error.stack;
  assert.equal(failureTechnicalDetails(error), String(error));
  assert.equal(failureTechnicalDetails({ get stack() { throw new Error("inaccessible"); } }),
    "Technical details are unavailable for this failure.");
});

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

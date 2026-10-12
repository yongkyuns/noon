import assert from "node:assert/strict";
import test from "node:test";
import {
  browserArgs,
  isIdentifiedGpuAdapter,
  isSoftwareGpuAdapter,
} from "../../scripts/manim-raster-support.mjs";

test("hardware WebGPU mode never forces SwiftShader", () => {
  const args = browserArgs("webgpu", { gpuMode: "hardware" });
  const joined = args.join(" ").toLowerCase();
  assert.ok(args.includes("--enable-unsafe-webgpu"));
  assert.ok(args.includes("--use-webgpu-power-preference=default-high-performance"));
  assert.ok(!joined.includes("swiftshader"));
  assert.ok(!joined.includes("--use-webgpu-adapter="));
});

test("software WebGPU mode remains deterministic", () => {
  const args = browserArgs("webgpu");
  assert.ok(args.includes("--use-webgpu-adapter=swiftshader"));
});

test("physical adapter classification rejects software implementations", () => {
  const apple = {
    vendor: "Apple",
    architecture: "apple-m4",
    device: "",
    description: "Apple M4",
    isFallbackAdapter: false,
  };
  assert.ok(isIdentifiedGpuAdapter(apple));
  assert.equal(isSoftwareGpuAdapter(apple), false);
  assert.equal(isSoftwareGpuAdapter({ description: "Google SwiftShader" }), true);
  assert.equal(isSoftwareGpuAdapter({ description: "llvmpipe" }), true);
  assert.equal(isSoftwareGpuAdapter({ isFallbackAdapter: true }), true);
  assert.equal(isIdentifiedGpuAdapter({}), false);
});

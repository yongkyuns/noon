import { Buffer } from "node:buffer";

import { RENDERING_TOOL_NAMES, renderingToolContracts } from "./rendering-contract.mjs";

function requireService(service) {
  const methods = ["open", "sampleFrames", "inspect", "getArtifact", "close"];
  if (!service || methods.some((name) => typeof service[name] !== "function")) {
    throw new TypeError("MCP rendering adapter requires the shared preview service");
  }
  return service;
}

function boundedText(value, fallback, limit = 1200) {
  const text = typeof value === "string" && value.trim() ? value : fallback;
  return text.slice(0, limit);
}

function errorResult(error) {
  const detail = Object.freeze({
    name: boundedText(error?.name, "Error", 128),
    code: typeof error?.code === "string" ? boundedText(error.code, "ERROR", 128) : null,
    message: boundedText(error?.message ?? String(error), "rendering operation failed"),
  });
  const structuredContent = Object.freeze({ error: detail });
  return Object.freeze({
    isError: true,
    content: Object.freeze([{ type: "text", text: JSON.stringify(structuredContent) }]),
    structuredContent,
  });
}

function successResult(structuredContent, images = []) {
  const content = [{ type: "text", text: JSON.stringify(structuredContent) }];
  for (const image of images) {
    content.push(Object.freeze({
      type: "image",
      data: image.png.toString("base64"),
      mimeType: "image/png",
    }));
  }
  return Object.freeze({ content: Object.freeze(content), structuredContent });
}

function retainedImage(service, scope, sessionId, descriptor) {
  if (!descriptor || typeof descriptor !== "object" || typeof descriptor.id !== "string") {
    throw new Error("preview service returned no retained artifact descriptor");
  }
  const retained = service.getArtifact(scope, sessionId, descriptor.id);
  if (!retained || retained.descriptor?.id !== descriptor.id ||
      retained.descriptor?.mimeType !== "image/png" || !Buffer.isBuffer(retained.png)) {
    throw new Error("preview service returned no readable retained PNG artifact");
  }
  return retained;
}

async function retireAfterDeliveryFailure(service, scope, sessionId, reason, primaryError) {
  try {
    await service.close(scope, sessionId, reason);
  } catch (cleanupError) {
    if (primaryError && typeof primaryError === "object") {
      primaryError.cleanupError = boundedText(cleanupError?.message ?? String(cleanupError), "preview cleanup failed");
    }
  }
}

function toolHandler(operation) {
  return async (input, context) => {
    try {
      return await operation(input, { signal: context?.mcpReq?.signal });
    } catch (error) {
      return errorResult(error);
    }
  };
}

/**
 * Thin MCP mapping over one transport-owned AgentPreviewService scope.
 * This module owns no session/runtime/renderer state and opens no transport.
 * The caller is responsible for closing the scope/service on disconnect.
 */
export function registerRenderingTools(server, { service, scope } = {}) {
  if (!server || typeof server.registerTool !== "function") {
    throw new TypeError("MCP rendering adapter requires an existing server");
  }
  requireService(service);
  if (!scope || typeof scope !== "object") {
    throw new TypeError("MCP rendering adapter requires a transport-owned preview scope");
  }

  server.registerTool("noon_open_scene", renderingToolContracts.noon_open_scene, toolHandler(async (input, { signal }) => {
    const opened = await service.open(scope, input.source, {
      ...(input.loopDurationSeconds === undefined ? {} : { loopDurationSeconds: input.loopDurationSeconds }),
      signal,
    });
    try {
      const retained = retainedImage(service, scope, opened.sessionId, opened.artifact);
      const structuredContent = Object.freeze({
        session: opened.sessionId,
        snapshot: opened.snapshot,
        artifact: retained.descriptor,
      });
      return successResult(structuredContent, [retained]);
    } catch (error) {
      await retireAfterDeliveryFailure(service, scope, opened.sessionId, "MCP open_scene image delivery failed", error);
      throw error;
    }
  }));

  server.registerTool("noon_sample_frames", renderingToolContracts.noon_sample_frames, toolHandler(async (input, { signal }) => {
    const sampled = await service.sampleFrames(scope, input.session, input.times, { signal });
    try {
      const retained = sampled.map((frame) => retainedImage(service, scope, input.session, frame.artifact));
      const structuredContent = Object.freeze({
        session: input.session,
        frames: Object.freeze(sampled.map((frame, index) => Object.freeze({
          snapshot: frame.snapshot,
          artifact: retained[index].descriptor,
        }))),
      });
      return successResult(structuredContent, retained);
    } catch (error) {
      await retireAfterDeliveryFailure(service, scope, input.session, "MCP sample_frames image delivery failed", error);
      throw error;
    }
  }));

  server.registerTool("noon_inspect", renderingToolContracts.noon_inspect, toolHandler(async (input) => {
    const inspected = service.inspect(scope, input.session);
    return successResult(Object.freeze({ session: input.session, ...inspected }));
  }));

  server.registerTool("noon_close_scene", renderingToolContracts.noon_close_scene, toolHandler(async (input) => {
    const closed = await service.close(scope, input.session, "MCP close_scene");
    return successResult(Object.freeze({ session: input.session, closed }));
  }));

  return RENDERING_TOOL_NAMES;
}

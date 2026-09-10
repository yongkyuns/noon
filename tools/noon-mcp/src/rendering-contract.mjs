import { Buffer } from "node:buffer";
import * as z from "zod/v4";

// Contract preparation only. These schemas are intentionally not imported by
// server.mjs until #1197 qualifies the shared runner and its isolation/cleanup
// boundary. Keeping the contract inert prevents a schema-only change from
// accidentally exposing source execution through MCP.
export const RENDERING_CONTRACT_VERSION = 1;
export const MAX_RENDER_SOURCE_BYTES = 1_000_000;
export const MAX_RENDER_TIME_SECONDS = 600;
// AgentPreviewService retains 32 frames per session by default and open_scene
// publishes the initial frame. A single sample_frames request therefore admits
// at most 31 additional frames; the service separately preflights remaining
// capacity across repeated calls before any forward-only advancement.
export const MAX_RENDER_SAMPLES_PER_CALL = 31;

const sessionHandle = z.string()
  .min(16)
  .max(128)
  .regex(/^[A-Za-z0-9._~-]+$/);

const source = z.string()
  .refine((value) => value.trim().length > 0, "source must be non-empty")
  .refine((value) => !value.includes("\0"), "source must not contain NUL")
  .refine((value) => Buffer.byteLength(value, "utf8") <= MAX_RENDER_SOURCE_BYTES,
    "source exceeds byte limit");

const renderTime = z.number().finite().nonnegative().max(MAX_RENDER_TIME_SECONDS);
const sampleSchedule = z.array(renderTime).min(1).max(MAX_RENDER_SAMPLES_PER_CALL)
  .superRefine((values, context) => {
    for (let index = 1; index < values.length; index += 1) {
      if (values[index] < values[index - 1]) {
        context.addIssue({
          code: "custom",
          message: "sample times must be nondecreasing for forward-only preview",
          path: [index],
        });
      }
    }
  });

const sessionOnly = z.strictObject({ session: sessionHandle });

export const renderingToolContracts = Object.freeze({
  noon_open_scene: Object.freeze({
    description: "Open one isolated Noon preview session and return only after its first coherent frame is ready.",
    inputSchema: z.strictObject({
      source,
      loopDurationSeconds: z.number().finite().positive().max(MAX_RENDER_TIME_SECONDS).optional(),
    }),
    annotations: Object.freeze({
      readOnlyHint: false,
      destructiveHint: false,
      idempotentHint: false,
      openWorldHint: false,
    }),
  }),
  noon_sample_frames: Object.freeze({
    description: "Advance one owned Noon preview session through a forward-only sampling schedule.",
    inputSchema: z.strictObject({ session: sessionHandle, times: sampleSchedule }),
    annotations: Object.freeze({
      readOnlyHint: false,
      destructiveHint: false,
      idempotentHint: false,
      openWorldHint: false,
    }),
  }),
  noon_inspect: Object.freeze({
    description: "Read the last coherent observations supported by one owned Noon preview session.",
    inputSchema: sessionOnly,
    annotations: Object.freeze({
      readOnlyHint: true,
      destructiveHint: false,
      idempotentHint: true,
      openWorldHint: false,
    }),
  }),
  noon_close_scene: Object.freeze({
    description: "Cancel and close one owned Noon preview session and invalidate its handle.",
    inputSchema: sessionOnly,
    annotations: Object.freeze({
      readOnlyHint: false,
      destructiveHint: true,
      idempotentHint: false,
      openWorldHint: false,
    }),
  }),
});

export const RENDERING_TOOL_NAMES = Object.freeze(Object.keys(renderingToolContracts));

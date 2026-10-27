#!/usr/bin/env bash
# Qualify the unchanged Python audio fixture on the existing hosted browser jobs.
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"
: "${NOON_PYTHON_AUDIO_BROWSER:?select the existing matrix browser}"
: "${NOON_PYTHON_AUDIO_BACKEND:?select the actual audio renderer backend}"
: "${NOON_PYTHON_AUDIO_ARTIFACTS:?provide the retained evidence directory}"
mkdir -p "$NOON_PYTHON_AUDIO_ARTIFACTS"
NOON_PYTHON_AUDIO_ARTIFACTS="$(cd "$NOON_PYTHON_AUDIO_ARTIFACTS" && pwd)"
export NOON_PYTHON_AUDIO_ARTIFACTS

record_provenance() {
  NOON_AUDIO_PROVENANCE_STAGE="$1" NOON_AUDIO_EXIT_STATUS="${2:-}" node --input-type=module <<'NODE'
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import path from "node:path";
import { createRuntimeBuildIdentity, runtimeBuildIdentityCore } from "./scripts/build-runtime-identity.mjs";
const git = (...args) => execFileSync("git", args, { encoding: "utf8" }).trim();
const require = createRequire(import.meta.url);
const declared = JSON.parse(await readFile("web/runtime-build-identity.json", "utf8"));
const observed = await createRuntimeBuildIdentity();
const untracked = git("ls-files", "--others", "--exclude-standard", "-z").split("\0").filter(Boolean);
const expectedOutput = name => name.startsWith("node_modules/") ||
  name.startsWith("browser-smoke-artifacts/") || name === "package-lock.json";
let report = null;
let reportReadError = null;
if (process.env.NOON_AUDIO_PROVENANCE_STAGE === "after") {
  try {
    report = JSON.parse(await readFile(path.join(process.env.NOON_PYTHON_AUDIO_ARTIFACTS,
      "report.json"), "utf8"));
  } catch (error) {
    reportReadError = { code: error.code ?? null, message: error.message };
  }
}
const receipt = {
  revision: git("rev-parse", "HEAD"), tree: git("rev-parse", "HEAD^{tree}"),
  sourceStatus: git("status", "--porcelain", "--untracked-files=all"),
  trackedSourceStatus: git("status", "--porcelain", "--untracked-files=no"),
  generatedOutputPaths: untracked.filter(expectedOutput),
  unexpectedUntrackedPaths: untracked.filter(name => !expectedOutput(name)),
  smokeSha256: createHash("sha256").update(await readFile("scripts/python-background-audio-smoke.mjs")).digest("hex"),
  browser: process.env.NOON_PYTHON_AUDIO_BROWSER, backend: process.env.NOON_PYTHON_AUDIO_BACKEND,
  requestedRenderHost: process.env.NOON_PLAYGROUND_RENDER_HOST ?? "auto",
  ...(process.env.NOON_AUDIO_PROVENANCE_STAGE === "after" ? {
    actualRenderHost: report?.renderHost ?? null,
    reportStatus: report?.status ?? null, reportReadError,
  } : {}),
  viewport: { width: 1280, height: 900, deviceScaleFactor: 1 },
  playwrightVersion: require("playwright/package.json").version,
  browserRevisions: JSON.parse(await readFile(path.join(path.dirname(
    require.resolve("playwright-core/package.json")), "browsers.json"), "utf8")).browsers,
  nodeVersion: process.version, platform: process.platform, architecture: process.arch,
  runnerImage: process.env.ImageOS, runnerImageVersion: process.env.ImageVersion,
  runId: process.env.GITHUB_RUN_ID, runAttempt: process.env.GITHUB_RUN_ATTEMPT,
  job: process.env.GITHUB_JOB, githubSha: process.env.GITHUB_SHA,
  declaredRuntime: declared, observedRuntime: observed,
};
await writeFile(path.join(process.env.NOON_PYTHON_AUDIO_ARTIFACTS,
  `provenance-${process.env.NOON_AUDIO_PROVENANCE_STAGE}.json`), `${JSON.stringify(receipt, null, 2)}\n`);
assert.ok(["auto", "worker", "main-thread"].includes(receipt.requestedRenderHost),
  "unsupported requested render host");
assert.equal(receipt.trackedSourceStatus, "", "tracked source and index must match the tested commit");
assert.deepEqual(receipt.unexpectedUntrackedPaths, [], "unexpected untracked source cannot qualify");
assert.equal(declared.sourceRevision, receipt.revision, "downloaded runtime must attest this exact checkout");
assert.deepEqual(observed.files, declared.files, "downloaded runtime file hashes must match the build identity");
// npm and prior smoke evidence may make observed.sourceRevision null. Preserve
// that observation; independently verify the producer's identity against HEAD.
const core = runtimeBuildIdentityCore({ sourceRevision: receipt.revision,
  hashes: Object.fromEntries(Object.entries(observed.files).map(([key, file]) => [key, file.sha256])) });
assert.deepEqual({ ...core, buildId: createHash("sha256").update(JSON.stringify(core)).digest("hex") },
  declared, "producer build identity must bind these exact assets to the tested commit");
if (process.env.NOON_AUDIO_PROVENANCE_STAGE === "after") {
  const before = JSON.parse(await readFile(path.join(process.env.NOON_PYTHON_AUDIO_ARTIFACTS,
    "provenance-before.json"), "utf8"));
  assert.equal(receipt.revision, before.revision, "source revision changed during qualification");
  assert.equal(receipt.tree, before.tree, "source tree changed during qualification");
  assert.equal(receipt.smokeSha256, before.smokeSha256, "original smoke changed during qualification");
  assert.equal(receipt.requestedRenderHost, before.requestedRenderHost,
    "requested render host changed during qualification");
  assert.deepEqual(declared, before.declaredRuntime, "runtime identity changed during qualification");
  if (process.env.NOON_AUDIO_EXIT_STATUS === "0") {
    assert.equal(reportReadError, null, "successful qualification requires a readable JSON report");
    assert.equal(report?.status, "passed", "successful qualification requires a passed report");
    if (receipt.requestedRenderHost !== "auto") {
      assert.equal(receipt.actualRenderHost, receipt.requestedRenderHost,
        "actual render host must match the explicit qualification profile");
    }
  }
}
NODE
}

noon_audio_dir="$(mktemp -d "${TMPDIR:-/tmp}/noon-python-audio.XXXXXX")"
noon_pulse_pid=
cleanup() {
  noon_exit_status=$?
  trap - EXIT
  if record_provenance after "$noon_exit_status"; then
    :
  elif (( noon_exit_status == 0 )); then
    noon_exit_status=1
  fi
  if [[ -n "$noon_pulse_pid" ]]; then
    kill -KILL "$noon_pulse_pid" 2>/dev/null || true
    wait "$noon_pulse_pid" 2>/dev/null || true
  fi
  rm -rf "$noon_audio_dir"
  printf '%s\n' "$noon_exit_status" > "$NOON_PYTHON_AUDIO_ARTIFACTS/exit-status.txt"
  exit "$noon_exit_status"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
record_provenance before
uname -a > "$NOON_PYTHON_AUDIO_ARTIFACTS/host.txt"

case "$(uname -s)" in
  Linux)
    for executable in pulseaudio pactl; do
      command -v "$executable" >/dev/null || {
        echo "Python audio qualification requires $executable" >&2
        exit 2
      }
    done
    pulseaudio --version > "$NOON_PYTHON_AUDIO_ARTIFACTS/audio-device-version.txt"
    mkdir -m 700 "$noon_audio_dir/runtime" "$noon_audio_dir/state" "$noon_audio_dir/config"
    export PULSE_RUNTIME_PATH="$noon_audio_dir/runtime"
    export PULSE_STATE_PATH="$noon_audio_dir/state"
    export PULSE_CONFIG_PATH="$noon_audio_dir/config"
    export PULSE_SERVER="unix:$noon_audio_dir/pulse.sock"
    export ALSA_CONFIG_PATH="$noon_audio_dir/alsa.conf"
    # Same private, wall-clocked native sink as scripts/native-audio-smoke.sh.
    # Browser Web Audio still processes through its real platform output path.
    cat > "$noon_audio_dir/server.pa" <<EOF
load-module module-native-protocol-unix socket=$noon_audio_dir/pulse.sock auth-anonymous=1
load-module module-null-sink sink_name=noon_test rate=48000 channels=2 format=float32le norewinds=true
set-default-sink noon_test
set-default-source noon_test.monitor
EOF
    cat > "$ALSA_CONFIG_PATH" <<EOF
</usr/share/alsa/alsa.conf>
pcm.!default {
    type pulse
    server "$PULSE_SERVER"
    device "noon_test"
}
ctl.!default {
    type pulse
    server "$PULSE_SERVER"
}
EOF
    cp "$noon_audio_dir/server.pa" "$NOON_PYTHON_AUDIO_ARTIFACTS/pulseaudio-config.txt"
    cp "$ALSA_CONFIG_PATH" "$NOON_PYTHON_AUDIO_ARTIFACTS/alsa-config.txt"
    pulseaudio -n --daemonize=no --exit-idle-time=-1 --use-pid-file=no \
      --disable-shm=yes --realtime=no --high-priority=no --log-target=stderr \
      --log-level=debug --log-time=yes --file="$noon_audio_dir/server.pa" \
      > "$NOON_PYTHON_AUDIO_ARTIFACTS/pulseaudio.log" 2>&1 &
    noon_pulse_pid=$!
    deadline=$((SECONDS + 10))
    until pactl info > "$NOON_PYTHON_AUDIO_ARTIFACTS/audio-device.txt" 2>/dev/null; do
      if ! kill -0 "$noon_pulse_pid" 2>/dev/null || (( SECONDS >= deadline )); then
        cat "$NOON_PYTHON_AUDIO_ARTIFACTS/pulseaudio.log" >&2
        echo "Private PulseAudio server did not become ready" >&2
        exit 1
      fi
      sleep 0.05
    done
    pactl list short sinks > "$NOON_PYTHON_AUDIO_ARTIFACTS/audio-sinks.txt"
    ;;
  Darwin)
    # Query the existing default Core Audio output. Do not install a virtual
    # device or substitute an AudioContext clock when the runner has no output.
    cat > "$noon_audio_dir/default-output.c" <<'C'
#include <CoreAudio/CoreAudio.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>

static void require_status(OSStatus status, const char *operation) {
  if (status != noErr) {
    fprintf(stderr, "Core Audio %s failed: %d\n", operation, (int)status);
    exit(1);
  }
}
int main(void) {
  AudioObjectPropertyAddress address = { kAudioHardwarePropertyDefaultOutputDevice,
    kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyElementMain };
  AudioDeviceID device = kAudioObjectUnknown;
  UInt32 size = sizeof(device);
  require_status(AudioObjectGetPropertyData(kAudioObjectSystemObject, &address,
    0, NULL, &size, &device), "default output lookup");
  if (device == kAudioObjectUnknown) {
    fputs("No default Core Audio output device; WebKit audio remains unqualified.\n", stderr);
    return 1;
  }
  UInt32 alive = 0;
  size = sizeof(alive);
  address.mSelector = kAudioDevicePropertyDeviceIsAlive;
  require_status(AudioObjectGetPropertyData(device, &address, 0, NULL, &size, &alive), "device liveness");
  Float64 rate = 0;
  size = sizeof(rate);
  address.mSelector = kAudioDevicePropertyNominalSampleRate;
  require_status(AudioObjectGetPropertyData(device, &address, 0, NULL, &size, &rate), "sample rate");
  address.mSelector = kAudioDevicePropertyStreamConfiguration;
  address.mScope = kAudioDevicePropertyScopeOutput;
  require_status(AudioObjectGetPropertyDataSize(device, &address, 0, NULL, &size), "output size");
  AudioBufferList *buffers = malloc(size);
  if (!buffers) return 1;
  require_status(AudioObjectGetPropertyData(device, &address, 0, NULL, &size, buffers), "output channels");
  UInt32 channels = 0;
  for (UInt32 index = 0; index < buffers->mNumberBuffers; ++index)
    channels += buffers->mBuffers[index].mNumberChannels;
  free(buffers);
  if (!alive || !isfinite(rate) || rate <= 0 || channels == 0) {
    fputs("Default Core Audio output is unavailable; WebKit audio remains unqualified.\n", stderr);
    return 1;
  }
  printf("{\"deviceId\":%u,\"alive\":%u,\"sampleRate\":%.9g,\"outputChannels\":%u}\n",
    (unsigned)device, (unsigned)alive, rate, (unsigned)channels);
  return 0;
}
C
    xcrun clang -Wall -Wextra -Werror -framework CoreAudio \
      "$noon_audio_dir/default-output.c" -o "$noon_audio_dir/default-output"
    if ! "$noon_audio_dir/default-output" > "$NOON_PYTHON_AUDIO_ARTIFACTS/audio-device.json" \
        2> "$NOON_PYTHON_AUDIO_ARTIFACTS/audio-device-error.txt"; then
      cat "$NOON_PYTHON_AUDIO_ARTIFACTS/audio-device-error.txt" >&2
      exit 1
    fi
    ;;
  *)
    echo "No supported native audio setup for $(uname -s)" >&2
    exit 2
    ;;
esac

# Keep the existing 240-second process deadline on both supported hosts. The
# process group also contains Xvfb/browser children; a timeout is never retried.
python3 - "$@" <<'PY' 2>&1 | tee "$NOON_PYTHON_AUDIO_ARTIFACTS/smoke.log"
import os
import signal
import subprocess
import sys
import time

class Cancelled(Exception):
    pass

def stop_group(process, first_signal):
    def signal_group(signum):
        try:
            os.killpg(process.pid, signum)
            return True
        except ProcessLookupError:
            return False
    if signal_group(first_signal):
        # Cleanup grace is not qualification time: the result is already fixed.
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline and signal_group(0):
            process.poll()
            time.sleep(0.05)
        signal_group(signal.SIGKILL)
    process.wait(timeout=1)

def run_qualification(command, deadline_seconds):
    process = None
    cancelled_signal = None
    def cancel(signum, _frame):
        nonlocal cancelled_signal
        cancelled_signal = cancelled_signal or signum
        if process is not None:
            raise Cancelled()
    previous = {signum: signal.signal(signum, cancel)
                for signum in (signal.SIGINT, signal.SIGTERM)}
    try:
        process = subprocess.Popen(command, start_new_session=True)
        if cancelled_signal is not None:
            raise Cancelled()
        status = process.wait(timeout=deadline_seconds)
        return status if status >= 0 else 128 - status
    except subprocess.TimeoutExpired:
        print(f"Python audio smoke exceeded its original {deadline_seconds}-second deadline", flush=True)
        return 124
    except Cancelled:
        return 128 + cancelled_signal
    finally:
        # Do not interrupt owned process-group teardown with a second signal.
        for signum in previous:
            signal.signal(signum, signal.SIG_IGN)
        try:
            if process is not None:
                stop_group(process, cancelled_signal or signal.SIGTERM)
        finally:
            for signum, handler in previous.items():
                signal.signal(signum, handler)

if __name__ == "__main__":
    command = [*sys.argv[1:], "node", "scripts/python-background-audio-smoke.mjs"]
    sys.exit(run_qualification(command, 240))
PY

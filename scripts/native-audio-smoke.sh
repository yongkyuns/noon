#!/usr/bin/env bash
# Exercise the real ALSA output path through a private, wall-clocked PulseAudio sink.
set -euo pipefail

repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repository_root"
for executable in cargo pulseaudio pactl pacat xvfb-run python3 timeout; do
  command -v "$executable" >/dev/null || {
    echo "Native audio smoke requires $executable" >&2
    exit 2
  }
done

mkdir -p "${NOON_NATIVE_AUDIO_PROOF_DIR:-native-audio-proof}"
export NOON_NATIVE_AUDIO_PROOF_DIR
NOON_NATIVE_AUDIO_PROOF_DIR="$(cd "${NOON_NATIVE_AUDIO_PROOF_DIR:-native-audio-proof}" && pwd)"

# Compile before recording: cold compilation is not an audio timing observation.
cargo test -p noon-native --features audio --lib native_audio_device_smoke --no-run \
  2>&1 | tee "$NOON_NATIVE_AUDIO_PROOF_DIR/build.log"

noon_pulse_dir="$(mktemp -d /tmp/noon-native-audio.XXXXXX)"
pulse_pid=
record_pid=
loss_pid=
cleanup() {
  if [[ -n "$loss_pid" ]]; then
    kill "$loss_pid" 2>/dev/null || true
    wait "$loss_pid" 2>/dev/null || true
  fi
  if [[ -n "$record_pid" ]]; then
    kill "$record_pid" 2>/dev/null || true
    wait "$record_pid" 2>/dev/null || true
  fi
  if [[ -n "$pulse_pid" ]]; then
    kill -KILL "$pulse_pid" 2>/dev/null || true
    wait "$pulse_pid" 2>/dev/null || true
  fi
  rm -rf "$noon_pulse_dir"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -m 700 "$noon_pulse_dir/runtime" "$noon_pulse_dir/state" "$noon_pulse_dir/config"
export PULSE_RUNTIME_PATH="$noon_pulse_dir/runtime"
export PULSE_STATE_PATH="$noon_pulse_dir/state"
export PULSE_CONFIG_PATH="$noon_pulse_dir/config"
export PULSE_SERVER="unix:$noon_pulse_dir/pulse.sock"
export ALSA_CONFIG_PATH="$noon_pulse_dir/alsa.conf"

# Disable the null sink's default two-second rewind buffer so its monitor has
# deterministic paced output from startup. PulseAudio 16.1 documents the option:
# https://github.com/pulseaudio/pulseaudio/blob/v16.1/src/modules/module-null-sink.c
cat > "$noon_pulse_dir/server.pa" <<EOF
load-module module-native-protocol-unix socket=$noon_pulse_dir/pulse.sock auth-anonymous=1
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

pulseaudio -n --daemonize=no --exit-idle-time=-1 --use-pid-file=no \
  --disable-shm=yes --realtime=no --high-priority=no --log-target=stderr \
  --file="$noon_pulse_dir/server.pa" \
  > "$NOON_NATIVE_AUDIO_PROOF_DIR/pulseaudio.log" 2>&1 &
pulse_pid=$!
deadline=$((SECONDS + 10))
until pactl info > "$NOON_NATIVE_AUDIO_PROOF_DIR/server.txt" 2>/dev/null; do
  if ! kill -0 "$pulse_pid" 2>/dev/null || (( SECONDS >= deadline )); then
    cat "$NOON_NATIVE_AUDIO_PROOF_DIR/pulseaudio.log" >&2
    echo "Private PulseAudio server did not become ready" >&2
    exit 1
  fi
  sleep 0.05
done
pactl list short sinks > "$NOON_NATIVE_AUDIO_PROOF_DIR/sinks.txt"
pactl list short sources > "$NOON_NATIVE_AUDIO_PROOF_DIR/sources.txt"

pacat --record --raw --device=noon_test.monitor --rate=48000 --channels=2 \
  --format=float32le --latency-msec=10 --client-name=noon-native-audio-proof \
  > "$NOON_NATIVE_AUDIO_PROOF_DIR/capture.f32" \
  2> "$NOON_NATIVE_AUDIO_PROOF_DIR/recorder.log" &
record_pid=$!
deadline=$((SECONDS + 10))
until [[ -n "$(pactl list short source-outputs)" ]]; do
  if ! kill -0 "$record_pid" 2>/dev/null || (( SECONDS >= deadline )); then
    cat "$NOON_NATIVE_AUDIO_PROOF_DIR/recorder.log" >&2
    echo "Private PulseAudio monitor did not become ready" >&2
    exit 1
  fi
  sleep 0.05
done

# A registered source-output may still be buffering. Require 0.2 seconds of
# actual stereo float PCM before the fixture can exercise silent preparation.
deadline=$((SECONDS + 10))
while (( $(wc -c < "$NOON_NATIVE_AUDIO_PROOF_DIR/capture.f32") < 76800 )); do
  if ! kill -0 "$record_pid" 2>/dev/null || ! kill -0 "$pulse_pid" 2>/dev/null \
      || (( SECONDS >= deadline )); then
    cat "$NOON_NATIVE_AUDIO_PROOF_DIR/recorder.log" >&2
    echo "Private PulseAudio monitor did not deliver paced PCM" >&2
    exit 1
  fi
  sleep 0.05
done
wc -c < "$NOON_NATIVE_AUDIO_PROOF_DIR/capture.f32" \
  > "$NOON_NATIVE_AUDIO_PROOF_DIR/recorder-ready-bytes.txt"

set +e
xvfb-run -a -s "-screen 0 1280x720x24" \
  cargo test -p noon-native --features audio --lib native_audio_device_smoke \
    -- --ignored --nocapture --test-threads=1 \
    2>&1 | tee "$NOON_NATIVE_AUDIO_PROOF_DIR/device-test.log"
test_status=${PIPESTATUS[0]}
set -e
# Retain the paced monitor's trailing silence after the native stream retires.
sleep 0.2
kill "$record_pid"
wait "$record_pid" 2>/dev/null || true
record_pid=
if (( test_status != 0 )); then
  exit "$test_status"
fi
python3 scripts/native_audio_capture.py "$NOON_NATIVE_AUDIO_PROOF_DIR"

# A fresh stream must report real platform loss and retire without a window.
# Clear only this phase's owned handshake/output; prior evidence cannot admit it.
rm -f "$NOON_NATIVE_AUDIO_PROOF_DIR/device-loss-ready.txt" \
  "$NOON_NATIVE_AUDIO_PROOF_DIR/device-loss-report.json"
timeout --signal=TERM --kill-after=3s 25s \
  cargo test -p noon-native --features audio --lib native_audio_device_loss_smoke \
    -- --ignored --nocapture --test-threads=1 \
    > "$NOON_NATIVE_AUDIO_PROOF_DIR/device-loss-test.log" 2>&1 &
loss_pid=$!
deadline=$((SECONDS + 10))
until [[ -s "$NOON_NATIVE_AUDIO_PROOF_DIR/device-loss-ready.txt" ]]; do
  if ! kill -0 "$loss_pid" 2>/dev/null || (( SECONDS >= deadline )); then
    cat "$NOON_NATIVE_AUDIO_PROOF_DIR/device-loss-test.log" >&2
    echo "Native audio loss fixture did not admit a healthy stream" >&2
    exit 1
  fi
  sleep 0.05
done

# This is the daemon created above, never an existing user's audio service.
kill "$pulse_pid"
deadline=$((SECONDS + 15))
while kill -0 "$loss_pid" 2>/dev/null; do
  if (( SECONDS >= deadline )); then
    cat "$NOON_NATIVE_AUDIO_PROOF_DIR/device-loss-test.log" >&2
    echo "Native audio loss fixture did not retire within 15 seconds" >&2
    exit 1
  fi
  sleep 0.05
done
set +e
wait "$loss_pid"
loss_status=$?
set -e
loss_pid=
cat "$NOON_NATIVE_AUDIO_PROOF_DIR/device-loss-test.log"
if (( loss_status != 0 )); then
  exit "$loss_status"
fi
if kill -0 "$pulse_pid" 2>/dev/null; then
  kill -KILL "$pulse_pid" 2>/dev/null || true
fi
wait "$pulse_pid" 2>/dev/null || true
pulse_pid=
python3 - "$NOON_NATIVE_AUDIO_PROOF_DIR/device-loss-report.json" <<'PY'
import json
import math
from pathlib import Path
import sys

report = json.loads(Path(sys.argv[1]).read_text())
assert report["error_observed"] is True, "Device loss did not produce an output error"
assert report["stream_dropped"] is True, "Failed output stream was not retired"
assert report["late_host_wake_preserved_anchor"] is True, "Late host wake lost the shared audio anchor"
assert abs(report["late_host_wake_end_time"] - 0.2) < 1.0e-9, "Late host wake missed the exact endpoint"
assert type(report["fault_code"]) is int, "Missing device fault observation"
assert isinstance(report["error"], str) and report["error"], "Missing device error message"
seconds = report["detection_seconds"]
assert math.isfinite(seconds) and 0 <= seconds <= 10, "Device loss exceeded the detection deadline"
print(json.dumps(report, indent=2))
PY

#!/usr/bin/env bash
# Build the upstream PulseAudio ALSA timing fix only for the private audio proof.
# #2263 owns this CI pin; remove it when the hosted Ubuntu plugin includes the fix.
set -euo pipefail

if (( $# != 1 )); then
  echo "Usage: bash scripts/ci-build-pulse-plugin.sh OUTPUT_DIRECTORY" >&2
  exit 2
fi
for executable in autoreconf cc curl make pkg-config sha256sum tar timeout; do
  command -v "$executable" >/dev/null || {
    echo "Private PulseAudio plugin build requires $executable" >&2
    exit 2
  }
done
pkg-config --exists 'alsa >= 1.1.6' 'libpulse >= 0.9.11'

# Ubuntu 24.04's plugin turns temporarily unavailable timing into EIO by
# comparing PA_ERR_NODATA with the wrong sign. Use the unmodified upstream fix:
# https://github.com/alsa-project/alsa-plugins/commit/811c2ce69d1265ace42a05ef045ca1b4ed72e01c
noon_pulse_commit=811c2ce69d1265ace42a05ef045ca1b4ed72e01c
noon_pulse_source_sha256=165c78a5a1df76dae88d8a6571f984abb01dbf21f54cf21009d19ae6431c585b
noon_pulse_source_url="https://codeload.github.com/alsa-project/alsa-plugins/tar.gz/$noon_pulse_commit"
mkdir -p "$1"
noon_pulse_output="$(cd "$1" && pwd)"
noon_pulse_build="$(mktemp -d /tmp/noon-pulse-plugin.XXXXXX)"
trap 'rm -rf "$noon_pulse_build"' EXIT
noon_pulse_archive="$noon_pulse_output/alsa-plugins-$noon_pulse_commit.tar.gz"

curl --fail --location --silent --show-error --proto '=https' \
  --connect-timeout 20 --max-time 120 --retry 2 --retry-max-time 180 \
  "$noon_pulse_source_url" --output "$noon_pulse_archive"
printf '%s  %s\n' "$noon_pulse_source_sha256" "$noon_pulse_archive" | sha256sum --check --strict
tar --extract --gzip --file "$noon_pulse_archive" --directory "$noon_pulse_build"

(
  cd "$noon_pulse_build/alsa-plugins-$noon_pulse_commit"
  timeout --signal=TERM --kill-after=10s 120s autoreconf --install --force
  timeout --signal=TERM --kill-after=10s 120s ./configure \
    --prefix="$noon_pulse_output" --with-plugindir="$noon_pulse_output" \
    --disable-static --enable-pulseaudio --disable-oss --disable-mix \
    --disable-usbstream --disable-arcamav --disable-jack --disable-samplerate \
    --disable-maemo-plugin --disable-maemo-resource-manager --disable-libav \
    --disable-a52 --disable-lavrate --disable-speexdsp --with-speex=no --disable-aaf
  timeout --signal=TERM --kill-after=10s 120s \
    make -C pulse -j2 libasound_module_pcm_pulse.la
) 2>&1 | tee "$noon_pulse_output/build.log"

# No global install or ALSA configuration changes. The harness selects this
# exact library using pcm_type.pulse.lib inside its owned ALSA_CONFIG_PATH.
noon_pulse_plugin="$noon_pulse_output/libasound_module_pcm_pulse.so"
install -m 755 \
  "$noon_pulse_build/alsa-plugins-$noon_pulse_commit/pulse/.libs/libasound_module_pcm_pulse.so" \
  "$noon_pulse_plugin"
cp "$noon_pulse_build/alsa-plugins-$noon_pulse_commit/COPYING" "$noon_pulse_output/COPYING"
{
  printf 'upstream_commit=%s\nsource_url=%s\nsource_sha256=%s\n' \
    "$noon_pulse_commit" "$noon_pulse_source_url" "$noon_pulse_source_sha256"
  printf 'alsa_version=%s\npulse_version=%s\n' \
    "$(pkg-config --modversion alsa)" "$(pkg-config --modversion libpulse)"
  sha256sum "$noon_pulse_plugin"
} > "$noon_pulse_output/provenance.txt"
printf 'NOON_NATIVE_AUDIO_PULSE_PLUGIN=%s\n' "$noon_pulse_plugin"

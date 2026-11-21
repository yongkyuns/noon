// MP4 input and WebCodecs output. Noon supplies the requested frame index and
// owns playback time. This module owns only compressed samples and decoded pixels.

const MAX_FILE_BYTES = 128 * 1024 * 1024;
const MAX_SECONDS = 60;
const MAX_FRAMES = 7200;
const READ_BYTES = 1024 * 1024;
const CACHE_BYTES = 32 * 1024 * 1024;
const DECODE_QUEUE = 4;
// WebCodecs can output already-submitted frames when its input queue empties.
// Keep those surfaces until the cache window reaches them.
const REORDER_RESERVE = 16;

const mediaError = (detail) => new Error(`Cannot import MP4: ${detail}`);
const seconds = (ticks, scale) => ticks / scale;
const microseconds = (time) => Math.round(time * 1_000_000);

function checkSignal(signal) {
  signal?.throwIfAborted();
}

/// Decode independently of any HTML media element and its clock.
export async function openMp4(blob, mp4, signal) {
  if (!(blob instanceof Blob) || blob.size === 0 || blob.size > MAX_FILE_BYTES) {
    throw mediaError("use a nonempty local MP4 up to 128 MiB");
  }
  const file = mp4.createFile(false);
  let info;
  file.onReady = (movie) => { info = movie; };
  file.onError = (_kind, error) => { throw mediaError(error); };
  // Follow the parser past mdat. Only MP4 metadata is needed to build the index,
  // including for files that place moov after the compressed media data.
  for (let offset = 0; offset < blob.size && info === undefined;) {
    checkSignal(signal);
    const chunk = await blob.slice(offset, offset + READ_BYTES).arrayBuffer();
    const next = file.appendBuffer(mp4.MP4BoxBuffer.fromArrayBuffer(chunk, offset));
    offset = Math.max(next ?? 0, offset + chunk.byteLength);
  }
  checkSignal(signal);
  if (!info || info.isFragmented || info.videoTracks.length !== 1 ||
      info.audioTracks.length > 1 ||
      info.tracks.length !== 1 + info.audioTracks.length) {
    throw mediaError("expected one H.264 video track and optional AAC in an ordinary MP4");
  }
  const [track] = info.videoTracks;
  const width = track.video.width, height = track.video.height;
  if (!track.codec.startsWith("avc1.") || !validDimensions(width, height)) {
    throw mediaError("expected H.264 video within a 1920 × 1080 pixel budget");
  }
  const packets = trackPackets(file, track, blob.size);
  const description = packets[0].description.avcC;
  if (!description || !packets[0].key) throw mediaError("missing AVC configuration or first keyframe");
  const configStream = new mp4.DataStream();
  description.write(configStream);
  const config = {
    codec: track.codec, codedWidth: width, codedHeight: height,
    description: configStream.buffer.slice(8),
  };
  if (typeof VideoDecoder === "undefined" || !(await VideoDecoder.isConfigSupported(config)).supported) {
    throw mediaError("the browser cannot decode this H.264 profile");
  }
  const edit = trackEdit(track);
  const clipEnd = edit.mediaStart + edit.duration;
  const ordered = packets.filter((packet) => packet.time >= edit.mediaStart && packet.time < clipEnd)
    .sort((a, b) => a.timestamp - b.timestamp);
  const origin = ordered[0]?.time;
  const duration = clipEnd - origin;
  if (!(duration > 0) || duration > MAX_SECONDS ||
      ordered.some((packet, i) => i > 0 && packet.timestamp <= ordered[i - 1].timestamp)) {
    throw mediaError("invalid video duration or duplicate presentation timestamps");
  }
  const timestamps = new Float64Array(ordered.length);
  for (const [index, packet] of ordered.entries()) {
    packet.frame = index;
    timestamps[index] = packet.time - origin;
  }
  const [audioTrack] = info.audioTracks;
  const audio = audioTrack && audioDescription(
    file, audioTrack, blob.size, edit.movieStart + origin - edit.mediaStart,
  );
  checkSignal(signal);
  return { blob, config, width, height, packets, ordered, timestamps, duration, audio };
}

export async function loadMp4(blob, signal) {
  return openMp4(blob, await import("./pkg/mp4box.js"), signal);
}

function validDimensions(width, height) {
  return Number.isInteger(width) && Number.isInteger(height) && width > 0 && height > 0 &&
    Math.max(width, height) <= 1920 && width * height <= 1920 * 1080;
}

function trackPackets(file, track, fileSize) {
  const samples = file.getTrackSamplesInfo(track.id);
  if (!samples?.length || samples.length > MAX_FRAMES ||
      !Number.isSafeInteger(track.timescale) || track.timescale <= 0) {
    throw mediaError("invalid track index or too many samples");
  }
  let keyIndex = 0;
  return samples.map((sample, index) => {
    const { cts, duration, offset, size, description, description_index: format } = sample;
    if (![cts, duration, offset, size].every(Number.isSafeInteger) || duration < 0 ||
        offset < 0 || size <= 0 || offset + size > fileSize ||
        format !== samples[0].description_index) {
      throw mediaError("invalid or changing sample description");
    }
    const time = seconds(cts, track.timescale);
    if (sample.is_sync) keyIndex = index;
    return {
      index, offset, size, time, description, keyIndex,
      timestamp: microseconds(time), duration: microseconds(seconds(duration, track.timescale)),
      key: sample.is_sync === true, frame: null,
    };
  });
}

function trackEdit(track) {
  if (!track.edits?.length) {
    return { movieStart: 0, mediaStart: 0, duration: seconds(track.duration, track.timescale) };
  }
  let movieStart = 0;
  let media;
  for (const entry of track.edits) {
    const duration = seconds(entry.segment_duration, track.movie_timescale);
    if (media || !(duration >= 0) || entry.media_rate_integer !== 1 ||
        entry.media_rate_fraction !== 0 || entry.media_time < -1) {
      throw mediaError("unsupported MP4 edit list");
    }
    if (entry.media_time === -1) {
      movieStart += duration;
    } else {
      media = { movieStart, mediaStart: seconds(entry.media_time, track.timescale), duration };
    }
  }
  if (!media) throw mediaError("track has no playable interval");
  return media;
}

function audioDescription(file, track, fileSize, videoStart) {
  const edit = trackEdit(track);
  const sampleRate = track.audio.sample_rate, numberOfChannels = track.audio.channel_count;
  if (track.codec !== "mp4a.40.2" || !(sampleRate >= 8000 && sampleRate <= 192000) ||
      ![1, 2].includes(numberOfChannels) || !(edit.duration > 0 && edit.duration <= MAX_SECONDS)) {
    throw mediaError("expected mono or stereo AAC-LC within 60 seconds");
  }
  const packets = trackPackets(file, track, fileSize);
  const esd = packets[0].description.esds?.esd;
  const description = esd?.descs.find((entry) => entry.tag === 4)
    ?.descs.find((entry) => entry.tag === 5)?.data;
  if (!description?.length) throw mediaError("missing AAC configuration");
  return {
    edit, packets, offset: edit.movieStart - videoStart,
    config: { codec: track.codec, sampleRate, numberOfChannels, description },
  };
}

async function encodedSample(blob, packet) {
  return await blob.slice(packet.offset, packet.offset + packet.size).arrayBuffer();
}

export async function decodeSoundtrack(asset, signal) {
  if (!asset.audio) return null;
  const { edit, packets, config, offset } = asset.audio;
  if (typeof AudioDecoder === "undefined" || !(await AudioDecoder.isConfigSupported(config)).supported) {
    throw mediaError("the browser cannot decode this AAC track");
  }
  const { sampleRate, numberOfChannels: channels } = config;
  const frames = Math.ceil(edit.duration * sampleRate);
  // Bound retained audio on browsers with high-rate input.
  if (frames * channels * 4 > 64 * 1024 * 1024) throw mediaError("decoded soundtrack exceeds 64 MiB");
  const pcm = new Float32Array(frames * channels);
  let failure;
  let wake;
  const decoder = new AudioDecoder({
    output(data) {
      try {
        if (data.sampleRate !== sampleRate || data.numberOfChannels !== channels) {
          throw mediaError("the decoded audio format changed");
        }
        const first = Math.round((seconds(data.timestamp, 1_000_000) - edit.mediaStart) * sampleRate);
        const from = Math.max(0, -first);
        const end = Math.min(data.numberOfFrames, frames - first);
        if (from >= end) return;
        const plane = new Float32Array(data.numberOfFrames);
        for (let channel = 0; channel < channels; channel++) {
          data.copyTo(plane, {planeIndex: channel, format: "f32-planar"});
          for (let i = from; i < end; i++) pcm[(first + i) * channels + channel] = plane[i];
        }
      } catch (error) {
        failure = error;
        wake?.();
      } finally {
        data.close();
      }
    },
    error(error) { failure = error; wake?.(); },
  });
  const onAbort = () => {
    if (decoder.state !== "closed") decoder.close();
    wake?.();
  };
  signal?.addEventListener("abort", onAbort, {once: true});
  decoder.ondequeue = () => wake?.();
  try {
    checkSignal(signal);
    decoder.configure(config);
    for (const packet of packets) {
      if (packet.time >= edit.mediaStart + edit.duration) break;
      while (!failure && !signal?.aborted && decoder.decodeQueueSize >= 16) {
        await new Promise((resolve) => { wake = resolve; });
        wake = undefined;
      }
      checkSignal(signal);
      if (failure) throw failure;
      const data = await encodedSample(asset.blob, packet);
      checkSignal(signal);
      if (failure) throw failure;
      decoder.decode(new EncodedAudioChunk({type: "key", timestamp: packet.timestamp, duration: packet.duration, data}));
    }
    await decoder.flush();
    if (failure) throw failure;
    return {pcm, offset, sampleRate, channels};
  } finally {
    signal?.removeEventListener("abort", onAbort);
    if (decoder.state !== "closed") decoder.close();
  }
}

/// Decode-order cursor plus a bounded cache of presentation frames. One moving
/// request feeds the existing decoder. A disjoint seek invalidates its callbacks
/// and starts from the nearest preceding keyframe instead.
export class VideoDecodeWindow {
  #asset;
  #notify;
  #frames = new Map();
  #wanted = null;
  #run = null;
  #closed = false;
  #failure = null;
  #ahead;
  #behind;
  #converting = 0;

  constructor(asset, notify) {
    this.#asset = asset;
    this.#notify = notify;
    const capacity = Math.min(12, Math.floor(CACHE_BYTES / (asset.width * asset.height * 4)));
    this.#behind = Math.floor(capacity / 3);
    this.#ahead = capacity - this.#behind - 1;
  }

  // Borrow a ready frame until the next request or close. Call from the render
  // host; the caller must copy it into its texture before yielding.
  request(index) {
    if (this.#closed) throw new Error("video decode window is closed");
    if (this.#failure) throw this.#failure;
    if (!Number.isInteger(index) || !this.#asset.ordered[index]) {
      throw new RangeError("video frame index is outside the asset");
    }
    if (index !== this.#wanted) {
      const previous = this.#wanted;
      const ready = this.#frames.has(index);
      this.#wanted = index;
      const run = this.#run;
      if (run === null || (!ready && (
        index < previous || index > previous + this.#ahead + 1 ||
        index >= run.firstDiscarded
      ))) this.#restart(index);
      this.#trim();
      this.#promote(this.#run);
      this.#pump(this.#run);
    }
    return this.#frames.get(index) ?? null;
  }

  readySpan() {
    if (this.#failure) throw this.#failure;
    let start = this.#wanted, end = start;
    if (!this.#frames.has(start)) return [start, start];
    while (this.#frames.has(start - 1)) start--;
    while (this.#frames.has(end)) end++;
    return [start, end];
  }

  close() {
    if (this.#closed) return;
    this.#closed = true;
    this.#retireRun();
    this.#trim(true);
  }

  #end() {
    return Math.min(this.#asset.ordered.length - 1, this.#wanted + this.#ahead);
  }

  #keep(index) {
    return index !== undefined && index >= this.#wanted - this.#behind && index <= this.#end();
  }

  #trim(all = false) {
    for (const [index, bitmap] of this.#frames) {
      if (all || !this.#keep(index)) {
        bitmap.close();
        this.#frames.delete(index);
      }
    }
  }

  #retireRun() {
    const run = this.#run;
    this.#run = null;
    if (run) {
      if (run.decoder.state !== "closed") run.decoder.close();
      for (const frame of run.pending.values()) frame.close();
      run.pending.clear();
    }
  }

  #restart(index) {
    this.#retireRun();
    const packet = this.#asset.ordered[index];
    const byTimestamp = new Map();
    const run = {
      cursor: packet.keyIndex, byTimestamp, pending: new Map(), pumping: false,
      flushed: false, decoder: null, firstDiscarded: Infinity,
    };
    run.decoder = new VideoDecoder({
      output: (frame) => this.#receive(run, frame),
      error: (error) => this.#fail(run, error),
    });
    run.decoder.ondequeue = () => this.#pump(run);
    this.#run = run;
    run.decoder.configure(this.#asset.config);
  }

  #fail(run, error) {
    if (this.#run !== run) return;
    this.#failure = error;
    this.#retireRun();
    this.#notify();
  }

  #receive(run, frame) {
    const index = run.byTimestamp.get(frame.timestamp);
    run.byTimestamp.delete(frame.timestamp);
    if (this.#run !== run || index === undefined || index < this.#wanted - this.#behind) {
      frame.close();
      return;
    }
    if (run.pending.size >= REORDER_RESERVE) {
      frame.close();
      run.firstDiscarded = Math.min(index, run.firstDiscarded);
      return;
    }
    run.pending.set(index, frame);
    this.#promote(run);
  }

  #promote(run) {
    for (const [index, frame] of run.pending) {
      if (index < this.#wanted - this.#behind) {
        frame.close();
        run.pending.delete(index);
        continue;
      }
      if (this.#converting >= DECODE_QUEUE || !this.#keep(index)) continue;
      run.pending.delete(index);
      this.#converting++;
      void this.#convert(run, index, frame);
    }
  }

  async #convert(run, index, frame) {
    try {
      // WebKit and Chrome can disagree about RGBA copyTo layout. Both can
      // convert a decoded frame to an uploadable ImageBitmap with correct colors.
      const bitmap = await createImageBitmap(frame);
      if (this.#run === run && this.#keep(index)) {
        this.#frames.get(index)?.close();
        this.#frames.set(index, bitmap);
        this.#notify();
      } else {
        if (this.#run === run && index > this.#end()) {
          run.firstDiscarded = Math.min(index, run.firstDiscarded);
        }
        bitmap.close();
      }
    } catch (error) {
      this.#fail(run, error);
    } finally {
      frame.close();
      this.#converting--;
      if (this.#run) {
        this.#promote(this.#run);
        this.#pump(this.#run);
      }
    }
  }

  #windowReady() {
    const lookahead = Math.min(this.#end(), this.#wanted + Math.max(1, this.#ahead - 2));
    for (let index = this.#wanted; index <= lookahead; index++) {
      if (!this.#frames.has(index)) return false;
    }
    return true;
  }

  #pump(run) {
    if (!run || run !== this.#run || run.pumping || run.flushed) return;
    run.pumping = true;
    void this.#feed(run).catch((error) => this.#fail(run, error)).finally(() => {
      run.pumping = false;
    });
  }

  async #feed(run) {
    const packets = this.#asset.packets;
    const { decoder } = run;
    while (this.#run === run && !this.#windowReady() &&
           decoder.decodeQueueSize + this.#converting < DECODE_QUEUE &&
           run.byTimestamp.size + run.pending.size < REORDER_RESERVE) {
      const packet = packets[run.cursor];
      if (!packet) {
        run.flushed = true;
        await decoder.flush();
        if (this.#run === run && this.#converting === 0 &&
            !run.pending.has(this.#wanted) && !this.#frames.has(this.#wanted)) {
          throw mediaError("the requested presentation frame did not decode");
        }
        return;
      }
      const data = await encodedSample(this.#asset.blob, packet);
      if (this.#run !== run) return;
      if (packet.frame !== null) run.byTimestamp.set(packet.timestamp, packet.frame);
      decoder.decode(new EncodedVideoChunk({
        type: packet.key ? "key" : "delta", timestamp: packet.timestamp, duration: packet.duration, data,
      }));
      run.cursor++;
    }
  }
}

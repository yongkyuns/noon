// The render worker owns decoder windows and lends ready pixels to Rust. Frame
// demands and window updates use the existing execution/render message port.
import { loadMp4, VideoDecodeWindow } from "./video-source.js";

export class VideoFrameHost {
  #assets = new Map();
  #clips = new Map();
  #aborted = new AbortController();
  #wake;
  #publish;
  #fail;
  #waiting = false;
  #resolve = null;
  #readback = null;

  constructor(wake, publish, fail) {
    this.#wake = wake;
    this.#publish = publish;
    this.#fail = fail;
  }

  register(assets) {
    for (const { id, blob } of assets) {
      if (!Number.isSafeInteger(id) || id <= 0 || id > 0xffffffff ||
          !(blob instanceof Blob) || this.#assets.has(id)) {
        throw new Error("invalid or duplicate browser video asset");
      }
      this.#assets.set(id, {blob, prepared: null});
    }
  }

  get waiting() { return this.#waiting; }
  beginPresent() { this.#waiting = false; }
  wait() { return new Promise(resolve => { this.#resolve = resolve; }); }

  request(assetId, key, index, cpuPixels = false) {
    let clip = this.#clips.get(key);
    if (!clip) {
      const asset = this.#assets.get(assetId);
      if (!asset) throw new Error("video asset is missing from this render session");
      clip = {assetId, index, window: null, announced: null};
      this.#clips.set(key, clip);
      asset.prepared ??= loadMp4(asset.blob, this.#aborted.signal);
      void asset.prepared.then(media => {
        if (this.#aborted.signal.aborted) return;
        clip.window = new VideoDecodeWindow(media, () => this.#changed(key, clip));
        this.#request(key, clip);
      }).catch(error => this.#report(error));
    } else if (clip.assetId !== assetId) {
      throw new Error("video clip changed its media identity");
    }
    clip.index = index;
    const bitmap = this.#request(key, clip);
    this.#waiting ||= bitmap === null;
    return bitmap !== null && cpuPixels ? this.#rgba(clip, bitmap) : bitmap;
  }

  #rgba(clip, bitmap) {
    if (clip.rgba?.index !== clip.index) {
      this.#readback ??= new OffscreenCanvas(1, 1).getContext("2d", {willReadFrequently: true});
      const context = this.#readback;
      if (!context) throw new Error("software WebGPU requires a canvas for decoded video pixels");
      const {width, height} = bitmap;
      if (context.canvas.width !== width || context.canvas.height !== height) {
        context.canvas.width = width;
        context.canvas.height = height;
      }
      context.drawImage(bitmap, 0, 0);
      const {buffer, byteOffset, byteLength} = context.getImageData(0, 0, width, height).data;
      clip.rgba = {index: clip.index, bytes: new Uint8Array(buffer, byteOffset, byteLength)};
    }
    return clip.rgba.bytes;
  }

  prepare({assetId, clip, index}) {
    // Preparation may need to refill a held target before a delta exists.
    const waiting = this.#waiting;
    try { this.request(assetId, clip, index); }
    finally { this.#waiting = waiting; }
  }

  close() {
    this.#aborted.abort();
    for (const clip of this.#clips.values()) clip.window?.close();
    this.#clips.clear();
    this.#assets.clear();
    this.#readback = null;
    this.#resolve?.();
    this.#resolve = null;
  }

  #request(key, clip) {
    if (!clip.window) return null;
    const bitmap = clip.window.request(clip.index);
    this.#announce(key, clip);
    return bitmap;
  }

  #announce(key, clip) {
    const [start, end] = clip.window.readySpan();
    if (clip.announced?.[0] === start && clip.announced[1] === end) return;
    clip.announced = [start, end];
    this.#publish({type: "video_window", clip: key, start, end});
  }

  #changed(key, clip) {
    try { this.#announce(key, clip); }
    catch (error) { this.#report(error); return; }
    this.#waiting = false;
    this.#resolve?.();
    this.#resolve = null;
    this.#wake();
  }

  #report(error) {
    if (this.#aborted.signal.aborted) return;
    this.#resolve?.();
    this.#resolve = null;
    this.#fail(error);
  }
}

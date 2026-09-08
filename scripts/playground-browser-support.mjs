// Shared software graphics configuration for the playground browser gates.
export function playgroundLaunchOptions(browserName) {
  if (browserName === "chromium") {
    return {
      headless: true,
      args: [
        "--disable-features=WebGPU",
        "--enable-unsafe-swiftshader",
        "--ignore-gpu-blocklist",
        "--use-gl=angle",
        "--use-angle=swiftshader",
        "--disable-gpu-sandbox",
        "--disable-dev-shm-usage",
      ],
    };
  }
  if (browserName === "firefox") {
    return {
      // Linux headless Firefox does not expose a usable WebGL surface on the
      // CI runner. Xvfb supplies a display while Mesa provides software rendering.
      headless: !(process.platform === "linux" && process.env.DISPLAY),
      firefoxUserPrefs: {
        "webgl.disabled": false,
        "webgl.force-enabled": true,
      },
    };
  }
  return { headless: true };
}


# Browser image is pinned to the Playwright 1.62.1 multi-architecture OCI digest.
FROM mcr.microsoft.com/playwright@sha256:dcc5531e97840b9b5e794f2814476b21571c5124a3fca2267d73041f56e7580e

ENV PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
RUN npm install --global --ignore-scripts --no-audit --no-fund \
      playwright@1.62.1 \
      pyodide@314.0.5 \
    && mkdir -p /opt/noon-runner \
    && ln -s "$(npm root -g)/playwright" /opt/noon-runner/playwright \
    && ln -s "$(npm root -g)/pyodide" /opt/noon-runner/pyodide \
    && test "$(node -p \"require('/opt/noon-runner/pyodide/package.json').version\")" = "314.0.5" \
    && chmod -R a+rX /opt/noon-runner

USER pwuser
WORKDIR /work

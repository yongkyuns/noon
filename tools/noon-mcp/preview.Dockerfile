# Browser image is pinned to the Playwright 1.62.1 multi-architecture OCI digest.
FROM mcr.microsoft.com/playwright@sha256:dcc5531e97840b9b5e794f2814476b21571c5124a3fca2267d73041f56e7580e

ENV PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD=1
ARG PYODIDE_VERSION=314.0.5
ARG PYODIDE_CORE_SHA256=f528dccea95fa8ec54295fd65bf86dd61183d11f0e52563dc8eadda45e0f78d6
RUN npm install --global --ignore-scripts --no-audit --no-fund playwright@1.62.1 \
    && mkdir -p /opt/noon-runner/pyodide /tmp/pyodide-core \
    && ln -s "$(npm root -g)/playwright" /opt/noon-runner/playwright \
    && curl --fail --location --retry 4 --retry-all-errors --connect-timeout 15 \
      "https://github.com/pyodide/pyodide/releases/download/${PYODIDE_VERSION}/pyodide-core-${PYODIDE_VERSION}.tar.bz2" \
      --output /tmp/pyodide-core.tar.bz2 \
    && echo "${PYODIDE_CORE_SHA256}  /tmp/pyodide-core.tar.bz2" | sha256sum --check --strict \
    && tar -xjf /tmp/pyodide-core.tar.bz2 -C /opt/noon-runner/pyodide --strip-components=1 \
    && test -r /opt/noon-runner/pyodide/pyodide.mjs \
    && test -r /opt/noon-runner/pyodide/pyodide.asm.mjs \
    && test -r /opt/noon-runner/pyodide/pyodide.asm.wasm \
    && test -r /opt/noon-runner/pyodide/python_stdlib.zip \
    && test -r /opt/noon-runner/pyodide/package.json \
    && test "$(node -p \"require('/opt/noon-runner/pyodide/package.json').version\")" = "${PYODIDE_VERSION}" \
    && rm -rf /tmp/pyodide-core /tmp/pyodide-core.tar.bz2 \
    && chmod -R a+rX /opt/noon-runner

USER pwuser
WORKDIR /work

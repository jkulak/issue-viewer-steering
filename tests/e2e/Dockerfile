# The browser and smoke tests: Microsoft's Playwright image, with the Python packages pinned in
# uv.lock (the e2e group). It never imports the app; it talks to the e2e-app service over HTTP.
FROM ghcr.io/astral-sh/uv:0.12.18 AS uv

FROM mcr.microsoft.com/playwright/python:v1.63.0-noble
COPY --from=uv /uv /usr/local/bin/uv
COPY pyproject.toml uv.lock /tmp/lock/
RUN uv export --project /tmp/lock --frozen --only-group e2e --no-hashes --no-emit-project -o /tmp/requirements.txt \
    && uv pip install --system --break-system-packages -r /tmp/requirements.txt \
    && rm -rf /tmp/lock /tmp/requirements.txt
WORKDIR /work

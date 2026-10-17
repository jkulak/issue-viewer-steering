# The smoke and browser tests: Microsoft's Playwright image with the Python packages pinned
# in requirements.txt. It never runs the viewer; it talks to the binary over HTTP.
FROM ghcr.io/astral-sh/uv:0.12.18 AS uv

FROM mcr.microsoft.com/playwright/python:v1.63.0-noble
COPY --from=uv /uv /usr/local/bin/uv
COPY requirements.txt /tmp/requirements.txt
RUN uv pip install --system --break-system-packages -r /tmp/requirements.txt \
    && rm /tmp/requirements.txt
WORKDIR /work

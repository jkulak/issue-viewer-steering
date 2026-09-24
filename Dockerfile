# syntax=docker/dockerfile:1
FROM ghcr.io/astral-sh/uv:0.12.18 AS uv

# Python and uv, nothing of the project yet: `make lock` runs uv from here.
FROM python:3.13-slim AS tools
COPY --from=uv /uv /usr/local/bin/uv
ENV PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    UV_COMPILE_BYTECODE=1 \
    UV_LINK_MODE=copy \
    UV_PYTHON_DOWNLOADS=never \
    UV_PROJECT_ENVIRONMENT=/opt/venv \
    PATH="/opt/venv/bin:$PATH"
WORKDIR /app

FROM tools AS deps
COPY pyproject.toml uv.lock ./
RUN uv sync --frozen --no-dev --no-install-project

# Tests and lint. compose mounts the working tree over /app, the venv stays in /opt/venv.
FROM deps AS dev
RUN uv sync --frozen --group dev --no-install-project
COPY . .
RUN uv sync --frozen --group dev

FROM deps AS runtime
COPY README.md ./
COPY src ./src
RUN uv sync --frozen --no-dev --no-editable \
    && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin app
USER app
EXPOSE 8080
HEALTHCHECK --interval=5s --timeout=3s --start-period=5s --retries=5 \
    CMD ["python", "-c", "import sys, urllib.request; sys.exit(urllib.request.urlopen('http://127.0.0.1:8080/healthz', timeout=2).status != 200)"]
CMD ["python", "-m", "issue_viewer"]

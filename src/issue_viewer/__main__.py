"""Run the viewer: `python -m issue_viewer`, configured from the environment."""

from __future__ import annotations

import logging
import sys

import uvicorn

from .settings import ConfigError, load_settings


def main() -> int:
    try:
        settings = load_settings()
    except ConfigError as error:
        print(f"Configuration error: {error}", file=sys.stderr)
        return 2
    logging.basicConfig(
        level=settings.log_level,
        format="%(asctime)s %(levelname)s %(name)s: %(message)s",
        stream=sys.stdout,
    )
    for project in settings.projects:
        mode = "read-only" if project.read_only else "read-write"
        logging.getLogger("issue_viewer").info(
            "project %s: %s (%s)", project.name, project.path, mode
        )
    # Inside the container the app listens on every interface; compose publishes the port on
    # 127.0.0.1 only, so nothing outside the machine can reach it.
    uvicorn.run(
        "issue_viewer.app:create_app",
        factory=True,
        host="0.0.0.0",
        port=settings.listen_port,
        log_level=settings.log_level.lower(),
        server_header=False,
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

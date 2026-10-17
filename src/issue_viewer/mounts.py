"""Print the compose override that mounts the configured project directories.

compose cannot turn a list in .env into volumes, so `make mounts` runs this module and writes its
output to compose.override.yaml. Every directory is mounted at the path it has on the host, so
PROJECTS and OPEN_ROOT mean the same inside the container as outside, and a `:ro` entry is mounted
read-only so a bug in the viewer could not write to it either.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path

from .settings import ConfigError, OpenRoot, ProjectConfig, parse_open_root, parse_projects


def _bind(path: Path, read_only: bool) -> dict:
    return {"type": "bind", "source": str(path), "target": str(path), "read_only": read_only}


def compose_override(
    projects: tuple[ProjectConfig, ...], open_root: OpenRoot | None = None
) -> dict:
    # Docker mounts a directory before the ones inside it, so a project inside the open root
    # is mounted over it and keeps its own mode.
    volumes = [_bind(open_root.path, open_root.read_only)] if open_root else []
    volumes += [_bind(project.path, project.read_only) for project in projects]
    return {"services": {"app": {"volumes": volumes}}}


def main() -> int:
    try:
        projects = parse_projects(os.environ.get("PROJECTS", ""))
        open_root = parse_open_root(os.environ.get("OPEN_ROOT", ""))
    except ConfigError as error:
        print(f".env: {error}", file=sys.stderr)
        return 1
    if not projects and not open_root:
        print("PROJECTS in .env is empty: the viewer will have nothing to show", file=sys.stderr)
    # JSON is valid YAML, and the standard library writes it without a dependency.
    json.dump(compose_override(projects, open_root), sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

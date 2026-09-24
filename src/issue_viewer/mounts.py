"""Print the compose override that mounts the configured project directories.

compose cannot turn a list in .env into volumes, so `make mounts` runs this module and writes its
output to compose.override.yaml. Every directory is mounted at the path it has on the host, so
PROJECTS means the same inside the container as outside, and a `:ro` entry is mounted read-only
so a bug in the viewer could not write to it either.
"""

from __future__ import annotations

import json
import os
import sys

from .settings import ConfigError, ProjectConfig, parse_projects


def compose_override(projects: tuple[ProjectConfig, ...]) -> dict:
    volumes = [
        {
            "type": "bind",
            "source": str(project.path),
            "target": str(project.path),
            "read_only": project.read_only,
        }
        for project in projects
    ]
    return {"services": {"app": {"volumes": volumes}}}


def main() -> int:
    try:
        projects = parse_projects(os.environ.get("PROJECTS", ""))
    except ConfigError as error:
        print(f"PROJECTS in .env: {error}", file=sys.stderr)
        return 1
    if not projects:
        print("PROJECTS in .env is empty: the viewer will have nothing to show", file=sys.stderr)
    # JSON is valid YAML, and the standard library writes it without a dependency.
    json.dump(compose_override(projects), sys.stdout, indent=2)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

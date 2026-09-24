"""Configuration, read from the environment and nowhere else."""

from __future__ import annotations

import os
import re
from collections.abc import Mapping
from dataclasses import dataclass
from datetime import UTC, tzinfo
from pathlib import Path
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError

PROJECT_NAME = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")
# A project with one of these names would shadow a route of the app itself.
RESERVED_NAMES = frozenset({"static", "healthz", "openapi.json", "favicon.ico", "-"})
# Loopback names cannot be used for DNS rebinding, so they are always accepted.
LOOPBACK_HOSTS = ("localhost", "127.0.0.1")


class ConfigError(ValueError):
    """The environment does not describe a configuration the app can run with."""


@dataclass(frozen=True)
class ProjectConfig:
    name: str
    path: Path
    read_only: bool = False


@dataclass(frozen=True)
class Settings:
    projects: tuple[ProjectConfig, ...] = ()
    allowed_hosts: tuple[str, ...] = LOOPBACK_HOSTS
    timezone: tzinfo = UTC
    refresh_seconds: float = 3.0
    log_level: str = "INFO"
    listen_port: int = 8080


def parse_projects(value: str) -> tuple[ProjectConfig, ...]:
    """Parse `name=/absolute/path[:ro],…` into project configurations."""
    projects: list[ProjectConfig] = []
    for raw in value.split(","):
        entry = raw.strip()
        if not entry:
            continue
        name, separator, location = (part.strip() for part in entry.partition("="))
        if not separator or not location:
            raise ConfigError(f"{entry!r} is not name=/absolute/path")
        if not PROJECT_NAME.match(name) or name in RESERVED_NAMES:
            raise ConfigError(
                f"{name!r} cannot be a project name: use letters, digits, '.', '_' or '-', "
                f"and none of {', '.join(sorted(RESERVED_NAMES))}"
            )
        if any(project.name == name for project in projects):
            raise ConfigError(f"project {name!r} is listed twice")
        read_only = location.endswith(":ro")
        if read_only or location.endswith(":rw"):
            location = location[:-3]
        if not location.startswith("/"):
            raise ConfigError(f"the path of {name!r} must be absolute, got {location!r}")
        projects.append(ProjectConfig(name, Path(os.path.normpath(location)), read_only))
    return tuple(projects)


def parse_hosts(value: str) -> tuple[str, ...]:
    hosts = [host.strip().lower() for host in value.split(",") if host.strip()]
    return tuple(dict.fromkeys([*LOOPBACK_HOSTS, *hosts]))


def parse_timezone(value: str) -> ZoneInfo:
    name = value.strip().lstrip(":") or "UTC"
    try:
        return ZoneInfo(name)
    except (ZoneInfoNotFoundError, ValueError) as error:
        raise ConfigError(f"TZ={value!r} is not a time zone the system knows") from error


def load_settings(environ: Mapping[str, str] | None = None) -> Settings:
    env = os.environ if environ is None else environ
    try:
        refresh = float(env.get("REFRESH_SECONDS") or 3)
        port = int(env.get("LISTEN_PORT") or 8080)
    except ValueError as error:
        raise ConfigError(f"REFRESH_SECONDS and LISTEN_PORT must be numbers: {error}") from error
    if refresh < 0:
        raise ConfigError("REFRESH_SECONDS cannot be negative")
    return Settings(
        projects=parse_projects(env.get("PROJECTS", "")),
        allowed_hosts=parse_hosts(env.get("ALLOWED_HOSTS", "")),
        timezone=parse_timezone(env.get("TZ", "")),
        refresh_seconds=refresh,
        log_level=(env.get("LOG_LEVEL") or "INFO").upper(),
        listen_port=port,
    )

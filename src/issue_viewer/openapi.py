"""Print the published OpenAPI schema; `make openapi` writes it to openapi.json."""

from __future__ import annotations

import json
import sys

from .app import create_app
from .settings import Settings


def schema() -> dict:
    return create_app(Settings()).openapi()


def main() -> int:
    json.dump(schema(), sys.stdout, indent=2, ensure_ascii=False)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

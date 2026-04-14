#!/usr/bin/env python3
from __future__ import annotations

from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
SCAN_DIRS = [ROOT / "rfc", ROOT / "implementation-plan"]

HEADER_RE = re.compile(
    r"^##\s+(Pending|Open Questions|Open Items|TBD|Pending Decisions|Pending Workflows)\b",
    re.IGNORECASE,
)


def main() -> int:
    failures: list[str] = []
    for base in SCAN_DIRS:
        for path in sorted(base.rglob("*.md")):
            rel = path.relative_to(ROOT)
            try:
                lines = path.read_text(encoding="utf-8").splitlines()
            except Exception as exc:  # pragma: no cover
                failures.append(f"{rel}: read failed ({exc})")
                continue

            for idx, line in enumerate(lines, start=1):
                if "[ ]" in line:
                    failures.append(f"{rel}:{idx}: unchecked checklist item")
                if HEADER_RE.search(line):
                    failures.append(f"{rel}:{idx}: stale pending/open heading")

    if failures:
        print("Stage doc hygiene check failed:")
        for item in failures:
            print(f"- {item}")
        return 1

    print("Stage doc hygiene check passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())

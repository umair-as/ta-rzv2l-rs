#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""PostToolUse: run rustfmt on any .rs file the agent just edited.

Skips third_party/ (vendored code stays byte-identical to upstream). If
rustfmt cannot parse the file, exit 2 so the parse error is fed back to the
agent; the edit itself has already happened either way.
"""

import json
import subprocess
import sys


def main() -> int:
    try:
        event = json.load(sys.stdin)
    except (json.JSONDecodeError, ValueError):
        return 0

    path = str((event.get("tool_input") or {}).get("file_path") or "")
    norm = path.replace("\\", "/")
    if not norm.endswith(".rs") or "third_party/" in norm:
        return 0

    res = subprocess.run(
        ["rustfmt", "--edition", "2021", path],
        capture_output=True,
        text=True,
    )
    if res.returncode != 0:
        sys.stderr.write(f"rustfmt failed on {path}:\n{res.stderr}")
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())

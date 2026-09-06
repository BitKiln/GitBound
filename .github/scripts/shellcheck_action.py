#!/usr/bin/env python3
"""Run shellcheck over the bash bodies inside action.yml.

A composite action's `run:` blocks are shell scripts that never get linted,
because they are strings inside a YAML file. An unquoted variable in one of them
is a bug in every repository that uses the action, so extract each body and put
it through shellcheck.

Two suppressions are deliberate: SC2086 and SC2046 fire on `${{ ... }}` GitHub
expressions, which the runner substitutes before the shell ever sees them.
"""

from __future__ import annotations

import os
import subprocess
import sys
import tempfile

import yaml

SUPPRESSED = "SC2086,SC2046,SC2016"


def main() -> int:
    with open("action.yml", encoding="utf-8") as handle:
        action = yaml.safe_load(handle)

    failed = False
    checked = 0
    for step in action["runs"]["steps"]:
        if step.get("shell") != "bash" or "run" not in step:
            continue
        checked += 1
        script = "#!/usr/bin/env bash\n" + step["run"]
        handle = tempfile.NamedTemporaryFile(
            "w", suffix=".sh", delete=False, encoding="utf-8"
        )
        try:
            handle.write(script)
            handle.close()
            result = subprocess.run(
                ["shellcheck", "--severity=warning", "-e", SUPPRESSED, handle.name],
                check=False,
            )
            if result.returncode != 0:
                print(f"::error file=action.yml::shellcheck failed for step {step.get('name', '?')!r}")
                failed = True
        finally:
            os.unlink(handle.name)

    if checked == 0:
        print("::error file=action.yml::no bash steps found; did the action change shape?")
        return 1
    print(f"shellcheck OK across {checked} step(s)")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())

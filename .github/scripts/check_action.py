#!/usr/bin/env python3
"""Structural checks on action.yml.

The action is executable content other repositories run, so it gets linted like
the rest of the code. This does not test behaviour -- it catches the mistakes
that would otherwise only surface in somebody else's pipeline: an output that
names a step that no longer exists, an input the documentation promises but the
file does not declare, or a switch to a Docker action, which would change the
performance and trust characteristics of every consumer without anyone noticing.
"""

from __future__ import annotations

import sys

import yaml

REQUIRED_INPUTS = {
    "version",
    "command",
    "range",
    "format",
    "policy",
    "no-policy",
    "enforce-signing",
    "sarif-file",
    "fail-on",
    "working-directory",
}


def main() -> int:
    with open("action.yml", encoding="utf-8") as handle:
        action = yaml.safe_load(handle)

    problems: list[str] = []

    if action["runs"]["using"] != "composite":
        problems.append(
            "action must stay composite; a Docker action costs every consumer a "
            "container pull"
        )

    declared = set(action.get("inputs", {}))
    for missing in sorted(REQUIRED_INPUTS - declared):
        problems.append(f"action.yml does not declare the documented input {missing!r}")

    step_ids = {step["id"] for step in action["runs"]["steps"] if "id" in step}
    for name, spec in action.get("outputs", {}).items():
        value = spec.get("value", "")
        if "steps." not in value:
            problems.append(f"output {name!r} is not wired to a step")
            continue
        referenced = value.split("steps.", 1)[1].split(".", 1)[0]
        if referenced not in step_ids:
            problems.append(
                f"output {name!r} reads steps.{referenced}, which is not a step id"
            )

    for problem in problems:
        print(f"::error file=action.yml::{problem}")
    if problems:
        return 1

    print(f"action.yml OK: {len(declared)} inputs, {len(step_ids)} identified steps")
    return 0


if __name__ == "__main__":
    sys.exit(main())

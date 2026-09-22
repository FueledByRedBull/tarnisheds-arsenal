"""Select conservative PR checks from the actual merge tree; pushes always run all."""
from __future__ import annotations

import os
from pathlib import Path, PurePosixPath
import subprocess


FRONTEND_FILES = {
    "apps/desktop/index.html",
    "apps/desktop/vite.config.ts",
    "apps/desktop/vitest.config.ts",
    "apps/desktop/playwright.config.ts",
    "apps/desktop/tsconfig.json",
    "apps/desktop/scripts/run-e2e.mjs",
}


def select_jobs(event: str, paths: list[str]) -> tuple[bool, bool]:
    if event != "pull_request" or not paths:
        return True, True
    frontend = False
    for path in paths:
        if path in {"README.md", "LICENSE"} or (
            path.startswith("docs/")
            and PurePosixPath(path).suffix in {".md", ".png", ".jpg", ".svg", ".webp"}
        ):
            continue
        if path.startswith(("apps/desktop/src/", "apps/desktop/tests/")) or path in FRONTEND_FILES:
            frontend = True
        else:
            return True, True
    return False, frontend


def changed_paths(root: Path) -> list[str]:
    parents = subprocess.check_output(
        ["git", "show", "-s", "--format=%P", "HEAD"], cwd=root
    ).split()
    if len(parents) != 2:
        raise RuntimeError("PR routing requires a merge commit with both parents available")
    # No rename detection: moving source into docs must still include the deleted source path.
    names = subprocess.check_output(
        ["git", "diff", "--no-renames", "--name-only", "-z", "HEAD^1", "HEAD", "--"],
        cwd=root,
    )
    return names.decode("utf-8", errors="surrogateescape").rstrip("\0").split("\0") if names else []


def main() -> None:
    event = os.environ["GITHUB_EVENT_NAME"]
    root = Path(__file__).resolve().parents[2]
    rust, frontend = select_jobs(event, changed_paths(root) if event == "pull_request" else [])
    outputs = f"rust={str(rust).lower()}\nfrontend={str(frontend).lower()}\n"
    with Path(os.environ["GITHUB_OUTPUT"]).open("a", encoding="utf-8") as output:
        output.write(outputs)
    print(outputs, end="")


if __name__ == "__main__":
    main()

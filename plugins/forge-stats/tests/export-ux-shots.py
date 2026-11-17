#!/usr/bin/env python3
"""Run the sandbox harness and export its receipts outside the mod runtime."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def main():
    root = Path(__file__).resolve().parents[3]
    command = ["claude", "plugin", "test", "plugins/forge-stats"]
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    shots = []
    sample = None
    for line in result.stdout.splitlines():
        if line.startswith("UX_SHOT "):
            shots.append(json.loads(line.removeprefix("UX_SHOT ")))
        elif line.startswith("UX_BRIEF "):
            sample = json.loads(line.removeprefix("UX_BRIEF "))
        else:
            print(line)
    if result.returncode:
        return result.returncode
    fixture = root / "plugins/forge-stats/tests/fixtures/brief-sample.json"
    if sample != json.loads(fixture.read_text()):
        print("brief fixture mirror drift", file=sys.stderr)
        return 1
    expected = {f"{name}-{cols}x{rows}-{theme}.txt" for name, cols, rows in [("S1", 160, 40), ("S2", 120, 30), ("S3", 80, 24), ("S4", 160, 40), ("S5-missing", 160, 40), ("S5-stale", 160, 40), ("S6", 160, 40)] for theme in ["dark", "light"]}
    if {s["name"] for s in shots} != expected or len(shots) != len(expected):
        print("missing or duplicate UX shots", file=sys.stderr)
        return 1
    target = os.environ.get("UX_SHOTS_DIR")
    if target:
        directory = Path(target)
        directory.mkdir(parents=True, exist_ok=True)
        for shot in shots:
            (directory / shot["name"]).write_text("\n".join(shot["plain"]) + "\n\n語意色\n" + "\n".join(shot["semantic"]) + "\n")
        manifest = {"command": command, "exit_code": result.returncode, "fixture_sha256": hashlib.sha256(fixture.read_bytes()).hexdigest(), "shots": [{"path": s["name"], "rows": len(s["plain"]), "sha256": hashlib.sha256((directory / s["name"]).read_bytes()).hexdigest()} for s in shots]}
        (directory / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
        print(f"已匯出 {len(shots)} 份畫面：{directory}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

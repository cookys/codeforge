#!/usr/bin/env python3
"""Run the sandbox harness and export its receipts outside the mod runtime."""
import hashlib
import json
import os
import mmap
from pathlib import Path
import re
import shutil
import subprocess
import sys


def installed_themes():
    """Extract only factual palette values from the installed native host's default resolver."""
    with Path(shutil.which("claude")).resolve().open("rb") as binary:
        with mmap.mmap(binary.fileno(), 0, access=mmap.ACCESS_READ) as data:
            anchor = data.find(b'inactive:"rgb(102,102,102)"')
            if anchor < 0:
                raise ValueError("installed host palette format changed; update the pinned theme fixture")
            start = data.rfind(b"var ", 0, anchor)
            chunk = data[start:data.find(b"\0", anchor)]
    light = re.search(rb'case"light":return ([A-Za-z_$][\w$]*)', chunk)[1]
    dark = re.search(rb'default:return ([A-Za-z_$][\w$]*)', chunk)[1]
    themes = {}
    for name, key in [("light", light), ("dark", dark)]:
        table = re.search(rb"(?:var |,)" + re.escape(key) + rb"=\{(.*?)\}(?:,|;)", chunk)[1]
        roles = {role: re.search(rb"\b" + role.encode() + rb':"([^"]+)"', table)[1].decode() for role in ["inactive", "subtle", "merged", "warning", "error", "success"]}
        background = re.search(rb'\binverseText:"([^"]+)"', table)[1].decode()
        themes[name] = {"background": background, "foreground": roles}
    return themes, {"source": "Claude Code 2.1.326 bundled default theme tables and G8 switch resolver", "chunk_sha256": hashlib.sha256(chunk).hexdigest(), "byte_offset": start, "reference_background": "inverseText: white for light, black for dark; custom/ANSI palettes and physical terminal are not covered", "themes": themes}


def main():
    root = Path(__file__).resolve().parents[3]
    command = ["claude", "plugin", "test", "plugins/forge-stats"]
    result = subprocess.run(command, cwd=root, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    shots = []
    sample = None
    palettes = None
    for line in result.stdout.splitlines():
        if line.startswith("UX_SHOT "):
            shots.append(json.loads(line.removeprefix("UX_SHOT ")))
        elif line.startswith("UX_BRIEF "):
            sample = json.loads(line.removeprefix("UX_BRIEF "))
        elif line.startswith("UX_THEMES "):
            palettes = json.loads(line.removeprefix("UX_THEMES "))
        else:
            print(line)
    if result.returncode:
        return result.returncode
    fixture = root / "plugins/forge-stats/tests/fixtures/brief-sample.json"
    if sample != json.loads(fixture.read_text()):
        print("brief fixture mirror drift", file=sys.stderr)
        return 1
    actual_palettes, provenance = installed_themes()
    if palettes != actual_palettes:
        print("installed host theme palette drift", file=sys.stderr)
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
            resolved = "\n".join(json.dumps(line, ensure_ascii=False) for line in shot["resolved"])
            (directory / shot["name"]).write_text("\n".join(shot["plain"]) + "\n\n語意色\n" + "\n".join(shot["semantic"]) + "\n\n色彩解析（宿主預設色盤、黑白參考底色）\n" + resolved + "\n")
        (directory / "host-theme-provenance.json").write_text(json.dumps(provenance, ensure_ascii=False, indent=2) + "\n")
        manifest = {"command": command, "exit_code": result.returncode, "fixture_sha256": hashlib.sha256(fixture.read_bytes()).hexdigest(), "shots": [{"path": s["name"], "rows": len(s["plain"]), "sha256": hashlib.sha256((directory / s["name"]).read_bytes()).hexdigest()} for s in shots]}
        (directory / "manifest.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
        print(f"已匯出 {len(shots)} 份畫面：{directory}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

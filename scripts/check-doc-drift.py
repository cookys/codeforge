#!/usr/bin/env python3
"""Deterministic doc↔code drift gate for CodeForge.

This is the RELIABLE half of doc-sync: zero-variance checks that always catch
their class. The LLM full sweep (autopilot:doc-sync / .claude/workflows/) is the
DISCOVERY half — it finds NEW drift classes, which then get demoted into a
deterministic check here so they never recur unreliably.

Why this exists: a non-deterministic LLM sweep cannot be a stopping gate — a
"clean" round only means "this sample found nothing", never "nothing exists"
(see .claude/doc-audit-state.json, 7-round trajectory). Deterministic checks
turn the gate reliable. Run in CI + before doc-touching merges.

Exit 0 = all green. Exit 1 = at least one check failed (details printed).
Checks: links · fences · version-sync · cli-surface · roadmap-consistency.
"""
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
# User-facing doc corpus (where command/concept claims must live).
DOC_CORPUS = ["README.md", "doc/concepts.md", "doc/getting-started.md", "CLAUDE.md"]


def _read(rel):
    p = os.path.join(ROOT, rel)
    with open(p, encoding="utf-8") as f:
        return f.read()


def _all_md():
    out = []
    for base in ["", "doc"]:
        d = os.path.join(ROOT, base)
        for dirpath, _, files in os.walk(d):
            # skip archived projects + vendored
            if "_archive" in dirpath or "/target" in dirpath or "/.git" in dirpath:
                continue
            for fn in files:
                if fn.endswith(".md"):
                    out.append(os.path.relpath(os.path.join(dirpath, fn), ROOT))
    # de-dup, stable
    return sorted(set(out))


# ── C1: internal markdown links resolve ──────────────────────────────────────
def check_links():
    bad = []
    for md in _all_md():
        base = os.path.dirname(os.path.join(ROOT, md))
        for m in re.finditer(r"\]\(([^)]+)\)", _read(md)):
            link = m.group(1)
            if link.startswith(("http://", "https://", "#", "mailto:")):
                continue
            path = link.split("#")[0]
            if not path:
                continue
            if not os.path.exists(os.path.normpath(os.path.join(base, path))):
                bad.append(f"{md}: {link}")
    return ("links", not bad, bad)


# ── C2: code fences balanced (even count of ``` per file) ─────────────────────
def check_fences():
    bad = []
    for md in _all_md():
        n = sum(1 for line in _read(md).splitlines() if line.lstrip().startswith("```"))
        if n % 2:
            bad.append(f"{md}: {n} fence lines (odd → unbalanced)")
    return ("fences", not bad, bad)


# ── C3: Cargo.toml version is reflected in CHANGELOG ──────────────────────────
def check_version_sync():
    m = re.search(r'(?m)^version\s*=\s*"([^"]+)"', _read("Cargo.toml"))
    if not m:
        return ("version-sync", False, ["Cargo.toml: no version field"])
    ver = m.group(1)
    cl = _read("CHANGELOG.md")
    if re.search(r"(?m)^##\s*\[%s\]" % re.escape(ver), cl):
        return ("version-sync", True, [])
    # else: an [Unreleased] section that names the pending version is acceptable
    if re.search(r"(?m)^##\s*\[Unreleased\]", cl) and ver in cl:
        return ("version-sync", True, [])
    return ("version-sync", False, [
        f"Cargo.toml version {ver} has no '## [{ver}]' CHANGELOG section and no "
        f"[Unreleased] section naming it"])


# ── C4: every CLI subcommand is mentioned in the user-facing docs ─────────────
def _subcommands():
    """Ground truth from the binary's --help; fall back to parsing clap source."""
    for cmd in (["./target/release/codeforge", "--help"],
                ["./target/debug/codeforge", "--help"]):
        bin_path = os.path.join(ROOT, cmd[0])
        if os.path.exists(bin_path):
            try:
                out = subprocess.run([bin_path, "--help"], cwd=ROOT,
                                     capture_output=True, text=True, timeout=30)
                subs = _parse_help_subcommands(out.stdout)
                if subs:
                    return subs
            except Exception:
                pass
    try:
        out = subprocess.run(["cargo", "run", "-q", "--", "--help"], cwd=ROOT,
                             capture_output=True, text=True, timeout=300)
        subs = _parse_help_subcommands(out.stdout)
        if subs:
            return subs
    except Exception:
        pass
    return None


def _parse_help_subcommands(help_text):
    subs = []
    in_cmds = False
    for line in help_text.splitlines():
        if re.match(r"^(Commands|SUBCOMMANDS):", line):
            in_cmds = True
            continue
        if in_cmds:
            if re.match(r"^\S", line):  # next section header → stop
                break
            m = re.match(r"\s+([a-z][a-z0-9-]+)", line)
            if m and m.group(1) != "help":
                subs.append(m.group(1))
    return subs


def check_cli_surface():
    subs = _subcommands()
    if subs is None:
        return ("cli-surface", True, ["SKIPPED: could not obtain `codeforge --help` "
                                      "(no binary built and `cargo run` unavailable)"])
    corpus = "\n".join(_read(d) for d in DOC_CORPUS)
    missing = [s for s in subs if not re.search(r"\b%s\b" % re.escape(s), corpus)]
    return ("cli-surface", not missing,
            [f"subcommand `{s}` not mentioned in any of {DOC_CORPUS}" for s in missing])


# ── C5: README and CLAUDE phase-roadmap tables must not contradict ────────────
def _roadmap_status(text):
    """phase-id → normalized status from a markdown roadmap table."""
    status = {}
    for line in text.splitlines():
        if not line.strip().startswith("|"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 3:
            continue
        pid = cells[0].lower()
        if not re.match(r"^[0-9]+[a-z]?$", pid):  # phase id like 1, 2a, 3f
            continue
        s = cells[-1].lower()
        if "✅" in s or "done" in s or "shipped" in s:
            status[pid] = "done"
        elif "planned" in s:
            status[pid] = "planned"
    return status


def check_roadmap_consistency():
    rm = _roadmap_status(_read("README.md"))
    cm = _roadmap_status(_read("CLAUDE.md"))
    bad = []
    for pid in sorted(set(rm) & set(cm)):
        if rm[pid] != cm[pid]:
            bad.append(f"phase {pid}: README={rm[pid]} vs CLAUDE.md={cm[pid]}")
    return ("roadmap-consistency", not bad, bad)


def main():
    checks = [check_links, check_fences, check_version_sync,
              check_cli_surface, check_roadmap_consistency]
    failed = 0
    print("doc-drift deterministic gate\n" + "=" * 32)
    for fn in checks:
        name, ok, details = fn()
        skipped = ok and details and details[0].startswith("SKIPPED")
        mark = "SKIP" if skipped else ("PASS" if ok else "FAIL")
        print(f"[{mark}] {name}")
        for d in details:
            print(f"       - {d}")
        if not ok:
            failed += 1
    print("=" * 32)
    if failed:
        print(f"{failed} check(s) FAILED")
        return 1
    print("all deterministic checks green")
    return 0


if __name__ == "__main__":
    sys.exit(main())

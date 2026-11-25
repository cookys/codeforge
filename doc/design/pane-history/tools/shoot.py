#!/usr/bin/env python3
"""Capture one archived autopilot version in a private tmux and copied sandbox HOME."""

from __future__ import annotations

import argparse
import atexit
import importlib.util
import json
import os
import pathlib
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import uuid

HERE = pathlib.Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]
HISTORY = HERE.parent

AP_REPO = str(pathlib.Path.home() / "projects" / "autopilot-dev")
HARNESS_DIR = str(REPO_ROOT / 'scripts/pane-gate/v74-keys')
FALLBACK_FIX_SHA = "6db7e47e"

COLS = 120
ROWS = 40
WAIT_AFTER_CMD_S = 8.0
ROW_TIMEOUT_S = 240

UNKNOWN_CMD_RE = re.compile(
    r"(?:unknown (?:slash )?command|unknown skill|unrecognized command|is not a valid command)",
    re.IGNORECASE,
)
UI_MARKER_RE = re.compile(
    r"(?:[╭╰┌└│]|現況|待決|派工|審查|花費|殘留|艦隊|產線|指揮|團隊|額度|主機|本席|runs|decide|dispatch|usage|fleet)"
)


class RowTimeoutError(Exception):
    """Raised when capturing a single SHA exceeds ROW_TIMEOUT_S."""


def public_text(value: str) -> str:
    """Anonymize only the sandbox HOME header, preserving terminal cell count."""
    home = str(pathlib.Path.home())
    return value.replace(home, "/home/" + "x" * len(pathlib.Path.home().name))


def append_note(out_dir: pathlib.Path, sha_label: str, status: str, detail: str) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    clean_detail = public_text(" ".join(detail.splitlines()).strip())
    line = f"{sha_label} {status} {clean_detail}\n"
    with (out_dir / "notes.log").open("a", encoding="utf-8") as f:
        f.write(line)


def resolve_ap_sha(sha: str) -> str:
    res = subprocess.run(
        ["git", "-C", AP_REPO, "rev-parse", "--verify", f"{sha}^{{commit}}"],
        capture_output=True,
        text=True,
    )
    if res.returncode == 0:
        return res.stdout.strip()
    raise RuntimeError(f"autopilot-dev 找不到 commit {sha}")


def prepare_worktree(sha_arg: str, full_sha: str) -> pathlib.Path:
    """Read objects only: never create a worktree or mutate the autopilot repository."""
    import tarfile
    import io
    dst = pathlib.Path(tempfile.mkdtemp(prefix="pane-history-checkout-"))
    try:
        raw = subprocess.check_output(["git", "-C", AP_REPO, "archive", full_sha])
        with tarfile.open(fileobj=io.BytesIO(raw)) as archive:
            archive.extractall(dst, filter="data")
        return dst
    except BaseException:
        shutil.rmtree(dst, ignore_errors=True)
        raise


def cleanup_worktree(wt_path):
    if wt_path is not None:
        shutil.rmtree(wt_path, ignore_errors=True)


def sandbox_setup(ap, out, scenario, theme):
    work = tempfile.mkdtemp(prefix="pane-history-home-")
    data = os.path.join(work, "data")
    home = os.path.join(data, "home")
    cwd = os.path.join(work, "demo")
    for directory in (home, cwd, os.path.join(home, ".claude"), os.path.join(home, ".local/state/fleet")):
        os.makedirs(directory, exist_ok=True)
    subprocess.run(["git", "init", "-q", "-b", "main", cwd], check=True)
    pathlib.Path(cwd, ".fleet-no-register").touch()
    real = pathlib.Path.home()
    config = real / ".claude.json"
    target = pathlib.Path(home, ".claude.json")
    if config.is_file():
        shutil.copyfile(config, target)
        target.chmod(0o600)
        cfg = json.loads(target.read_text())
    else:
        cfg = {}
    cfg.update(theme=theme, hasCompletedOnboarding=True)
    target.write_text(json.dumps(cfg))
    target.chmod(0o600)
    shutil.copyfile(target, pathlib.Path(home, ".claude/.claude.json"))
    pathlib.Path(home, ".claude/.claude.json").chmod(0o600)
    # Copy only authentication, never hooks, plugins, MCP registrations or transcripts.
    credentials = real / ".claude/.credentials.json"
    if credentials.is_file():
        copy = pathlib.Path(home, ".claude/.credentials.json")
        shutil.copyfile(credentials, copy)
        copy.chmod(0o600)
    pathlib.Path(home, ".claude/settings.json").write_text('{"permissions":{"disableBypassPermissionsMode":"disable"},"skipDangerousModePermissionPrompt":true}')
    env = dict(os.environ, V74_FIX=os.path.join(ap, "mods/live/fixtures"), V74_KEYS_LOG=os.path.join(out, "fleet-calls-shot.jsonl"))
    return work, data, home, cwd, env


def build_shim_here(meta_dir: pathlib.Path) -> pathlib.Path:
    """建立沙盒專用假 fleet（舊版 checkout 缺少 mods/live/fixtures 時自動退回假資料預設值）。"""
    fallback_dir = meta_dir / "fallback-fix"
    fallback_dir.mkdir(parents=True, exist_ok=True)
    archive_proc = subprocess.Popen(
        ["git", "-C", AP_REPO, "archive", FALLBACK_FIX_SHA, "mods/live/fixtures"],
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
    )
    if archive_proc.stdout is not None:
        subprocess.run(
            ["tar", "-x", "-C", str(fallback_dir)],
            stdin=archive_proc.stdout,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
    archive_proc.wait()
    fb_fix = fallback_dir / "mods" / "live" / "fixtures"

    shim_here = meta_dir / "here"
    bin_dir = shim_here / "bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    fleet_script = bin_dir / "fleet"
    fleet_script.write_text(
        f"""#!/usr/bin/env python3
import json, os, sys, time

a = sys.argv[1:]
F = os.environ.get("V74_FIX", "")
FB = {str(fb_fix)!r}
log_path = os.environ.get("V74_KEYS_LOG")
if log_path:
    os.makedirs(os.path.dirname(log_path), exist_ok=True)
    with open(log_path, "a", encoding="utf-8") as log:
        log.write(json.dumps(a, ensure_ascii=False) + "\\n")

def load_fixture(rel, default):
    for base in (F, FB):
        if base:
            p = os.path.join(base, rel)
            if os.path.isfile(p):
                with open(p, encoding="utf-8") as f:
                    return json.load(f)
    return default

def shifted(d, at_key):
    if not isinstance(d, dict) or at_key not in d:
        return d
    shift = int(time.time() * 1000) - d[at_key]
    def mv(o):
        if isinstance(o, dict):
            return {{k: (v + shift if k.endswith("_ms") and isinstance(v, int) and v > 10**12 else mv(v)) for k, v in o.items()}}
        if isinstance(o, list):
            return [mv(x) for x in o]
        return o
    return mv(d)

now_ms = int(time.time() * 1000)
if a[:1] == ["--help"]:
    print("usage:\\n  fleet decision claim <card> [--option <label>] [--free-text <text>]\\n  fleet pm status\\n  fleet console")
elif a[:2] == ["decision", "claim"]:
    sys.stderr.write("fake fleet: claim blocked in read-only capture mode\\n")
    sys.exit(1)
elif a[:2] == ["pm", "status"]:
    sys.stdout.write(json.dumps(load_fixture("v74/pm-status.json", {{"open": [], "asks": []}}), ensure_ascii=False))
elif a[:2] == ["decision", "list"]:
    rows = load_fixture("v74/decide.json", [])
    if isinstance(rows, dict):
        rows = rows.get("cards", [])
    cards = []
    for r in rows:
        opts = r.get("options", [])
        cards.append({{"card": {{
            "card_id": r.get("id", "d-fake"),
            "kind": r.get("title", ""),
            "question": r.get("question", ""),
            "background": r.get("background"),
            "options": [{{"label": o.get("label", ""), "description": o.get("detail", "")}} for o in opts],
            "recommendation": next((o.get("label") for o in opts if o.get("recommended")), None),
        }}}})
    print(json.dumps({{"cards": cards}}, ensure_ascii=False))
elif a[:1] == ["console"]:
    default_console = {{"schema": "fleet.console/1", "generated_at_ms": now_ms, "hosts": [], "seats": [], "pools": []}}
    print(json.dumps(shifted(load_fixture("console/c0/console-full.json", default_console), "generated_at_ms"), ensure_ascii=False))
elif a[:1] == ["tasks"]:
    print("[]")
else:
    print("{{}}")
""",
        encoding="utf-8",
    )
    fleet_script.chmod(0o755)
    return shim_here


def load_harness_module():
    spec = importlib.util.spec_from_file_location("gaterun", os.path.join(HARNESS_DIR, "run.py"))
    if spec is None or spec.loader is None:
        raise RuntimeError(f"無法載入 harness: {HARNESS_DIR}/run.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def screen_opened(before_text: str, after_sc, cmd: str) -> bool:
    after_text = after_sc.text
    if not after_text.strip():
        return False
    if cmd in (after_sc.prompt() or ""):
        return False
    before_lines = set(before_text.splitlines())
    added_lines = [ln for ln in after_text.splitlines() if ln not in before_lines]
    added_joined = "\n".join(added_lines)
    if UNKNOWN_CMD_RE.search(added_joined):
        return False
    meaningful_added = [
        ln for ln in added_lines
        if ln.strip() and cmd not in ln
    ]
    if meaningful_added and (UI_MARKER_RE.search("\n".join(meaningful_added)) or len(meaningful_added) >= 2):
        return True
    # 若啟動時框體已自動開啟且指令無錯誤地完成
    has_frame = ("╭" in after_text and "╰" in after_text) or ("┌" in after_text and "└" in after_text) or ("╶[" in after_text)
    if has_frame and not UNKNOWN_CMD_RE.search(after_text):
        return True
    if cmd == "/live" and not UNKNOWN_CMD_RE.search(after_text) and UI_MARKER_RE.search(after_text):
        non_prompt = [
            ln for ln in after_text.splitlines()
            if ln.strip() and not ln.strip().startswith(("❯", ">"))
        ]
        if len(non_prompt) >= 4:
            return True
    return False


def try_open_command(g, cmd: str):
    cur_prompt = g.snap().prompt()
    if cur_prompt:
        g.keys("C-u")
        time.sleep(0.3)
    before = g.snap().text
    g.keys(cmd, literal=True)
    time.sleep(0.3)
    typed = g.snap().prompt() or ""
    if cmd not in typed:
        # 焦點可能不在輸入框，按一次 Escape 回到輸入框重試
        g.keys("Escape")
        time.sleep(0.4)
        g.keys("C-u")
        time.sleep(0.3)
        g.keys(cmd, literal=True)
        time.sleep(0.3)
        typed = g.snap().prompt() or ""
    if not typed.strip().startswith("/live"):
        return None
    g.keys("Enter")
    time.sleep(WAIT_AFTER_CMD_S)
    g.tmux("resize-window", "-t", g.sess, "-x", str(COLS), "-y", str(ROWS), check=False)
    time.sleep(0.5)
    sc = g.snap()
    if screen_opened(before, sc, cmd):
        return sc
    return None


def shoot_single(sha_arg: str, out_dir: pathlib.Path, theme: str) -> int:
    out_dir.mkdir(parents=True, exist_ok=True)
    meta_dir = pathlib.Path(tempfile.mkdtemp(prefix="shoot-row-"))
    wt_path: pathlib.Path | None = None
    work: str | None = None
    regen: subprocess.Popen | None = None
    g = None
    sha_label = sha_arg[:9] + "/" + theme

    def _kill_tmux() -> None:
        if g is not None:
            g.tmux("kill-server", check=False)

    atexit.register(_kill_tmux)

    def _on_alarm(_signum, _frame):
        raise RowTimeoutError(f"超過單列時限 {ROW_TIMEOUT_S}s")

    def _on_term(_signum, _frame):
        _kill_tmux()
        sys.exit(1)

    old_alrm = signal.signal(signal.SIGALRM, _on_alarm)
    signal.signal(signal.SIGTERM, _on_term)
    signal.signal(signal.SIGHUP, _on_term)
    signal.alarm(ROW_TIMEOUT_S)

    try:
        full_sha = resolve_ap_sha(sha_arg)
        sha9 = full_sha[:9]
        sha_label = sha9 + "/" + theme
        wt_path = prepare_worktree(sha_arg, full_sha)

        R = load_harness_module()
        R.HERE = str(build_shim_here(meta_dir))

        gate_out = meta_dir / "gate-out"
        gate_out.mkdir(parents=True, exist_ok=True)
        scenario = "shot"

        work, data, home, cwd, env = sandbox_setup(str(wt_path), str(gate_out), scenario, theme)
        sid = str(uuid.uuid4())
        gen_cmd = [sys.executable, "-I", os.path.join(HARNESS_DIR, "gen-data.py"), data, sid, cwd]
        subprocess.run(gen_cmd, check=True)
        regen = subprocess.Popen(["bash", "-c", 'while sleep 25; do "$@"; done', "regen", *gen_cmd])

        g = R.Gate(str(wt_path), str(gate_out), scenario)
        g.work = work
        eta_path = os.path.join(work, "pool-eta.json")
        R.write_eta(eta_path, 0)

        alias = None
        R.launch(
            g,
            str(wt_path),
            str(gate_out),
            scenario,
            data,
            home,
            cwd,
            env,
            sid,
            alias,
            cols=COLS,
            rows=ROWS,
            extra_env=(f"AUTOPILOT_POOL_ETA={eta_path}", f"CLAUDE_CONFIG_DIR={home}/.claude", f"XDG_CONFIG_HOME={home}/.config", f"XDG_STATE_HOME={home}/.local/state", "AUTOPILOT_LIVE_V75=0", "CLAUDE_CODE_DISABLE_AUTO_MEMORY=1"),
        )
        if g.snap().prompt() is None:
            append_note(out_dir, sha_label, "FAIL", "claude 啟動逾時或未進入輸入框")
            return 1

        g.tmux("resize-window", "-t", g.sess, "-x", str(COLS), "-y", str(ROWS), check=False)
        time.sleep(3.0)

        chosen_cmd = None
        captured = None
        for candidate in ("/live-pane", "/live"):
            sc = try_open_command(g, candidate)
            if sc is not None:
                chosen_cmd = candidate
                captured = sc
                break

        if captured is None or chosen_cmd is None:
            last_sc = g.snap()
            if not last_sc.text.strip():
                reason = "畫面空白"
            else:
                reason = "/live-pane 與 /live 皆未能開啟面板: " + " ".join(last_sc.text.splitlines())[-1600:]
            append_note(out_dir, sha_label, "FAIL", reason)
            return 1

        dest = out_dir / sha9
        dest.mkdir(parents=True, exist_ok=True)
        txt_path = dest / f"{theme}.txt"
        ans_path = dest / f"{theme}.ans"
        txt_path.write_text(public_text(captured.text), encoding="utf-8")
        ans_path.write_text(public_text("\n".join(captured.ansi or [])), encoding="utf-8")
        append_note(out_dir, sha_label, "OK", chosen_cmd)
        return 0

    except RowTimeoutError as exc:
        append_note(out_dir, sha_label, "FAIL", str(exc))
        return 1
    except Exception as exc:
        append_note(out_dir, sha_label, "FAIL", f"{type(exc).__name__}: {exc}")
        return 1
    finally:
        signal.alarm(0)
        signal.signal(signal.SIGALRM, old_alrm)
        if regen is not None:
            regen.kill()
            try:
                regen.wait(timeout=3)
            except subprocess.TimeoutExpired:
                pass
        if g is not None:
            try:
                g.results.close()
            except Exception:
                pass
            g.tmux("kill-server", check=False)
        if work is not None:
            shutil.rmtree(work, ignore_errors=True)
        shutil.rmtree(meta_dir, ignore_errors=True)
        cleanup_worktree(wt_path)



def main():
    parser = argparse.ArgumentParser(description="Capture a historical pane at 120x40 in an isolated HOME (240s limit).")
    parser.add_argument("sha")
    parser.add_argument("--theme", choices=("dark", "light"), required=True)
    parser.add_argument("--out", type=pathlib.Path, default=HISTORY / "shots")
    args = parser.parse_args()
    return shoot_single(args.sha, args.out.resolve(), args.theme)


if __name__ == "__main__":
    sys.exit(main())

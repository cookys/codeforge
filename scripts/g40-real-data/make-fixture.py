#!/usr/bin/env python3
"""Write a synthetic autopilot live dir for scripts/g40-real-data/check.sh.

Usage: make-fixture.py <live_dir> [--negative]

Times are relative to now, so the fixture never ages out of the mirror's
windows. --negative renames one account to an `acct-` alias, which the check
must refuse (the negative control).
"""
import json
import os
import sys
import time


def view(provider, alias, windows, ns):
    record = {
        "schema": "fleet.quota/1", "provider": provider, "alias": alias, "visibility": "L3",
        "windows": windows, "refusal": None, "token_state": "unknown", "token_state_at": None,
        "host_role": "agent", "token_issued_at": None, "token_expires_at": None,
    }
    row = {"writer_courier": "local", "host_role": "agent", "observed_at_hub": None, "skewed": False, "record": record}
    return {"schema": "fleet.quota-view/1", "namespace": ns, "hub_offset": 0, "rows": [row]}


def win(kind, util, now, resets_in):
    return {"kind": kind, "utilization": util, "used": None, "limit": None, "unit": None,
            "resets_at": now + resets_in, "source": "passive", "observed_at": now - 60}


def write(path, obj):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        json.dump(obj, f, ensure_ascii=False)


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: make-fixture.py <live_dir> [--negative]")
    live = sys.argv[1]
    negative = "--negative" in sys.argv[2:]
    now = int(time.time())
    ns = "fixture"
    work = "acct-neg0" if negative else "work"
    write(f"{live}/quota/namespace.json", {"schema": "fleet.quota-ns/1", "namespace": ns})
    write(f"{live}/quota/claude/{work}.json", view("claude", work, [
        win("rolling_5h", 0.62, now, 7530), win("weekly", 0.31, now, 3 * 86400 + 4 * 3600)], ns))
    write(f"{live}/quota/claude/home.json", view("claude", "home", [
        win("rolling_5h", 0.12, now, 4 * 3600 + 2400), win("weekly", 0.91, now, 86400 + 7200)], ns))
    write(f"{live}/quota/kimi/main.json", view("kimi", "main", [win("rolling_5h", 0.4, now, 3000)], ns))
    day = time.strftime("%Y-%m-%d", time.gmtime(now))
    rows = []
    for model, inp, out, cost in [("claude-opus-5-5", 1000, 500, 1.5), ("claude-sonnet-5-5", 4000, 900, 0.4),
                                  ("gpt-6-astra", 2000, 300, None)]:
        rows.append({"day": day, "source": "claude", "account": work, "model": model, "speed": "standard",
                     "input": inp, "output": out, "cache_write_5m": 0, "cache_write_1h": 0, "cache_read": 800,
                     "requests": 3, "cost_usd": cost})
    write(f"{live}/codeforge/usage/summary.json", {
        "schema": "codeforge.usage-summary/1", "generated_at_ms": now * 1000 - 60_000, "host": "host-0f1e2d3c",
        "prices_as_of": "2026-10-06",
        "accounts": [{"alias": work, "written_at_ms": now * 1000,
                      "five_hour": {"pct": 0.62, "resets_at": now + 7530}, "seven_day": None}],
        "days": rows,
    })


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Autopilot-format BACKLOG.md as a view of a hippo store (reverse restore of `hippo import --from autopilot-backlog`).

  backlog-restore.py render   <repo> --header FILE [--order asc|desc]        print the file
  backlog-restore.py check    <repo> --header FILE [--order ..] [--file F]   exit 1 when the file differs from the store
  backlog-restore.py diff     <repo> <source.md> --header FILE [--order ..]  per-row report, classes: missed / extra / format
  backlog-restore.py backfill <repo> <source.md> --header FILE [--order ..] [--apply]
                                                                             put what the store lacks into the store, never delete

The importer drops fields it does not keep (Done-Check, Artifact, Needs, Closed, the Status text, a `blocked`
Trigger-Type ...). A row is rendered from the record: Id from the provenance value, Status / Origin / Reporter /
Trigger-Type / Effort from status and tags, every other field from the `- **Name**: value` lines of the body (the part
before a `---` sidecar line). A body line always wins over the derived value, so `backfill` writes the lines the
derivation cannot reproduce. Row order is canonical (by id); field order is CANON, then body order.
{next-id} in the header file becomes BL-<max id + 1>. Needs python3 and the hippo binary (HIPPO_BIN, else PATH)."""
import argparse, json, os, re, subprocess, sys, tempfile

CANON = ["Id", "Status", "Since", "Origin", "Reporter", "Report", "Reply", "Problem", "Impact", "Trigger",
         "Trigger-Type", "Effort", "Source", "Pointer", "Context"]
KV = re.compile(r"^\s*- \*\*([^*]+?):?\*\*:?\s*(.*?)\s*$")
STATUS_WORD = {"open": "open", "active": "open", "done": "done", "cancelled": "dropped", "superseded": "superseded"}


def hippo(repo, *args, check=True):
    exe = os.environ.get("HIPPO_BIN") or "hippo"
    p = subprocess.run([exe, "--json", "-C", repo, *args], capture_output=True, text=True)
    if check and p.returncode != 0:
        sys.exit(f"backlog-restore: hippo {' '.join(args)} failed: {(p.stderr or p.stdout).strip()[:300]}")
    return p


def load(repo):
    """All records that came from an autopilot BACKLOG.md row: (id, record) sorted by provenance value."""
    ids, off = [], 0
    while True:
        # --history: the default list hides superseded records, which are rows of the file too
        d = json.loads(hippo(repo, "list", "--history", "--offset", str(off)).stdout)["data"]
        ids += [r["id"] for r in d["records"] if r["kind"] == "issue"]
        if d.get("next_offset") is None:
            break
        off = d["next_offset"]
    out = []
    for i in ids:
        r = json.loads(hippo(repo, "show", i).stdout)["data"]["record"]
        pv = r.get("provenance")
        if pv and pv.get("kind") == "autopilot-backlog":
            out.append(r)
    out.sort(key=lambda r: idkey(r["provenance"]["value"]))
    return out


def idkey(v):
    m = re.search(r"(\d+)$", v)
    return (v[: m.start()] if m else v, int(m.group(1)) if m else 0, v)


def split_body(text):
    """(header lines, sidecar text or None): the sidecar starts at the first line that is exactly `---`."""
    lines = (text or "").split("\n")
    if "---" in lines:
        k = lines.index("---")
        return lines[:k], "\n".join(lines[k:])
    return lines, None


def body_fields(text):
    out = []  # (Name, value) in body order, first occurrence of a name wins
    seen = set()
    for l in split_body(text)[0]:
        m = KV.match(l)
        if m and m.group(1).lower() not in seen:
            seen.add(m.group(1).lower())
            out.append((m.group(1), m.group(2)))
    return out


def tag_value(tags, prefix):
    for t in tags:
        if t.startswith(prefix):
            return t[len(prefix):]
    return None


def derived(r):
    d = {"Id": r["provenance"]["value"], "Status": STATUS_WORD.get(r["status"], r["status"])}
    o = tag_value(r["tags"], "origin:")
    if o:
        d["Origin"] = o
    rp = tag_value(r["tags"], "reporter:")
    if rp:
        d["Reporter"] = rp
    tt = tag_value(r["tags"], "trigger:")
    if tt:
        d["Trigger-Type"] = re.sub(r"^date-", "date:", tt)
    e = tag_value(r["tags"], "effort:")
    if e:
        d["Effort"] = e.upper() + ("!" if "urgent" in r["tags"] else "")
    return d


def fields_of(r):
    """Ordered [(Name, value)] of the rendered row."""
    d = {k.lower(): (k, v) for k, v in derived(r).items()}
    b = body_fields(r["body"])
    order = []
    for k, v in b:
        d[k.lower()] = (k, v)
    for c in CANON:
        if c.lower() in d:
            order.append(d[c.lower()])
    canon = {c.lower() for c in CANON}
    order += [(k, v) for k, v in b if k.lower() not in canon]
    return order


def render_rows(recs, order):
    recs = sorted(recs, key=lambda r: idkey(r["provenance"]["value"]), reverse=(order == "desc"))
    out = []
    for r in recs:
        out.append("")
        out.append(f"### {r['title']}")
        out += [f"- **{k}**: {v}" if v != "" else f"- **{k}**:" for k, v in fields_of(r)]
    return out


def render(recs, header, order):
    mx = max([idkey(r["provenance"]["value"])[1] for r in recs] or [0])
    pre = re.match(r"(\D*)", recs[0]["provenance"]["value"]).group(1) if recs else "BL-"
    # zero-pad like the ids already there: BL-0004 -> 4 digits, B52 -> none
    top = max(recs, key=lambda r: idkey(r["provenance"]["value"])[1]) if recs else None
    run = re.search(r"(\d+)$", top["provenance"]["value"]) if top else None
    width = len(run.group(1)) if run and run.group(1).startswith("0") else (4 if not run else 0)
    h = open(header, encoding="utf-8").read().replace("{next-id}", f"{pre}{mx + 1:0{width}d}").rstrip("\n")
    return h + "\n" + "\n".join(render_rows(recs, order)) + "\n"


def parse_source(path):
    """Rows of a BACKLOG.md like the importer sees them: [{heading, fields[(Name,value)], extra[non-kv lines], line}]."""
    rows, cur, fence = [], None, False
    for n, l in enumerate(open(path, encoding="utf-8").read().split("\n"), 1):
        if l.strip().startswith("```"):
            fence = not fence
        if not fence and (l.startswith("### ") or l.rstrip() == "###"):
            cur = {"heading": l[4:].strip(), "fields": [], "extra": [], "line": n}
            rows.append(cur)
            continue
        if not fence and (l.startswith("## ") or l.startswith("# ")):
            cur = None
            continue
        if cur is None:
            continue
        m = KV.match(l)
        if m and m.group(1).lower() not in {k.lower() for k, _ in cur["fields"]}:
            cur["fields"].append((m.group(1), m.group(2)))
        elif l.strip() and not m:
            cur["extra"].append(l)
    for r in rows:
        r["id"] = next((v.split()[0] for k, v in r["fields"] if k.lower() == "id" and v.split()), None)
    return rows


def diff(recs, src_rows, order, header, src_path):
    by_id = {r["provenance"]["value"]: r for r in recs}
    src_ids = [r["id"] for r in src_rows]
    report = []  # (class, id, detail)
    for s in src_rows:
        if s["id"] not in by_id:
            report.append(("missed", s["id"], "row is not in the store"))
            continue
        rec = by_id[s["id"]]
        mine = {k.lower(): v for k, v in fields_of(rec)}
        if rec["title"] != s["heading"].strip()[:200].strip():
            report.append(("format", s["id"], f"heading differs: store {rec['title']!r}"))
        for k, v in s["fields"]:
            if k.lower() not in mine:
                report.append(("missed", s["id"], f"field {k} not in the store"))
            elif mine[k.lower()] != v:
                ahead = k.lower() == "status" and rec["status"] not in ("open", "active") and v.split()[:1] == ["open"]
                report.append(("extra" if ahead else "missed", s["id"], f"field {k}: store {mine[k.lower()]!r} source {v!r}" + (" (store is ahead: closed in the store)" if ahead else "")))
        names = {k.lower() for k, _ in s["fields"]}
        for k in mine:
            if k not in names:
                report.append(("extra", s["id"], f"field {k} only in the store"))
        for e in s["extra"]:
            report.append(("format", s["id"], f"line outside the field list is not in the store: {e[:60]!r}"))
        got = [k.lower() for k, _ in fields_of(rec) if k.lower() in names]
        if got != [k.lower() for k, _ in s["fields"] if k.lower() in mine]:
            report.append(("format", s["id"], "field order differs (canonical order used)"))
    for r in recs:
        if r["provenance"]["value"] not in src_ids:
            report.append(("extra", r["provenance"]["value"], f"record {r['short']} ({r['status']}) is not in the source"))
    want = sorted([i for i in src_ids if i in by_id], key=idkey, reverse=(order == "desc"))
    if [i for i in src_ids if i in by_id] != want:
        report.append(("format", "-", f"row order differs (canonical: id {order})"))
    return report


def closing(s):
    """The terminal status a source row's Status line says, or None while it is open."""
    st = next((v for k, v in s["fields"] if k.lower() == "status"), "").split()
    w = st[0].lower() if st else ""
    return {"done": "done", "shipped": "done", "dropped": "cancelled", "superseded": "superseded"}.get(w)


def backfill(repo, recs, src_rows, src_path, header, order, apply):
    by_id = {r["provenance"]["value"]: r for r in recs}
    acts = []
    for s in src_rows:
        if s["id"] not in by_id:
            acts.append(("import", s["id"], None))
            continue
        rec = by_id[s["id"]]
        mine = {k.lower(): v for k, v in fields_of(rec)}
        # a row the store already closed while the source still says open: the store is ahead, keep it
        ahead = rec["status"] not in ("open", "active") and (closing(s) is None)
        add = [(k, v) for k, v in s["fields"]
               if (k.lower() not in mine or mine[k.lower()] != v) and not (ahead and k.lower() == "status")]
        if add:
            acts.append(("body", s["id"], (rec, add)))
        want = closing(s)
        if want and want != "superseded" and rec["status"] in ("open", "active"):
            acts.append(("close", s["id"], (rec, want)))
    for kind, i, arg in acts:
        what = {"import": "row import", "close": f"close as {arg[1] if arg else ''}"}.get(kind) or "body lines: " + ", ".join(k for k, _ in arg[1])
        print(f"{'APPLY' if apply else 'PLAN '} {i}: {what}")
    if not apply:
        return len(acts)
    for kind, i, arg in acts:
        if kind == "close":
            hippo(repo, "close", arg[0]["id"], arg[1])
            continue
        if kind != "body":
            continue
        rec, add = arg
        head, side = split_body(rec["body"])
        names = {k.lower() for k, _ in add}
        keep = []
        for l in head:
            m = KV.match(l)
            if not (m and m.group(1).lower() in names):
                keep.append(l)
        while keep and keep[-1] == "":
            keep.pop()
        keep += [f"- **{k}**: {v}" if v != "" else f"- **{k}**:" for k, v in add]
        text = "\n".join(keep) + ("\n" + side if side is not None else "")
        with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False, encoding="utf-8") as f:
            f.write(text + "\n")
        try:
            hippo(repo, "edit", rec["id"], "--body-file", f.name)
        finally:
            os.unlink(f.name)
    ids = [i for kind, i, _ in acts if kind == "import"]
    if ids:
        import_rows(repo, src_path, ids)
    return len(acts)


def mint_open(repo, recs):
    """Give an open record that has no BL id (written straight into the store) the next id; done ones stay unrendered."""
    ids, off = [], 0
    while True:
        d = json.loads(hippo(repo, "list", "--offset", str(off)).stdout)["data"]
        ids += [r for r in d["records"] if r["kind"] == "issue"]
        if d.get("next_offset") is None:
            break
        off = d["next_offset"]
    mx = max([idkey(r["provenance"]["value"])[1] for r in recs] or [0])
    pre = re.match(r"(\D*)", recs[0]["provenance"]["value"]).group(1) if recs else "BL-"
    n = 0
    for r in sorted(ids, key=lambda r: r["created"]):
        if r["provenance"] is None and r["status"] in ("open", "active"):
            mx += 1
            v = f"{pre}{mx:04d}"
            hippo(repo, "set", r["id"], "provenance", "--kind", "autopilot-backlog", "--value", v, "--locator", f"{BACKLOG}#{v}")
            print(f"MINT {v} -> {r['short']} ({r['title'][:50]})")
            n += 1
    return n


def import_rows(repo, src_path, ids):
    """Import the rows missing from the store. A `done` row is read as `shipped`, as onboard-repo.sh does it."""
    t = open(src_path, encoding="utf-8").read()
    out = []
    for b in re.split(r"(?m)^(?=### )", t):
        m = re.search(r"(?m)^- \*\*Status\*\*: done\b(.*)$", b)
        if m:
            c = re.search(r"(?m)^- \*\*Closed\*\*: (.*)$", b)
            rest = m.group(1).strip() or (c.group(1).strip() if c else "(no Closed line)")
            b = b[: m.start()] + "- **Status**: shipped " + rest + b[m.end():]
        out.append(b)
    keep = open(src_path, "rb").read()
    try:
        open(src_path, "w", encoding="utf-8").write("".join(out))
        p = hippo(repo, "import", "--from", "autopilot-backlog", src_path, "--only", ",".join(ids))
        print(p.stdout.strip()[-400:])
    finally:
        open(src_path, "wb").write(keep)


BACKLOG = "docs/BACKLOG.md"  # repo-relative path of the file (--backlog)


def main():
    global BACKLOG
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("cmd", choices=["render", "check", "diff", "backfill"])
    ap.add_argument("repo")
    ap.add_argument("source", nargs="?")
    ap.add_argument("--header", required=True)
    ap.add_argument("--order", choices=["asc", "desc"], default="desc")
    ap.add_argument("--backlog", default=BACKLOG, help="repo-relative path of the BACKLOG.md (default docs/BACKLOG.md)")
    ap.add_argument("--file", default=None, help="check: the file to compare (default <repo>/<--backlog>)")
    ap.add_argument("--apply", action="store_true")
    ap.add_argument("--mint-open", action="store_true", help="backfill --apply: give open records without a BL id the next id")
    a = ap.parse_args()
    BACKLOG = a.backlog
    recs = load(a.repo)
    if a.cmd == "render":
        sys.stdout.write(render(recs, a.header, a.order))
    elif a.cmd == "check":
        f = a.file or os.path.join(a.repo, a.backlog)
        if open(f, encoding="utf-8").read() != render(recs, a.header, a.order):
            sys.exit(f"backlog-restore: {a.backlog} differs from the store; run scripts/backlog-view.sh (never edit the file by hand)")
    else:
        src = a.source or sys.exit("backlog-restore: this command needs <source.md>")
        rows = parse_source(src)
        if a.cmd == "diff":
            rep = diff(recs, rows, a.order, a.header, src)
            for c, i, d in rep:
                print(f"{c}\t{i}\t{d}")
            n = {c: sum(1 for x in rep if x[0] == c) for c in ("missed", "extra", "format")}
            print(f"SUMMARY rows_source={len(rows)} rows_store={len(recs)} missed={n['missed']} extra={n['extra']} format={n['format']}")
        else:
            n = backfill(a.repo, recs, rows, src, a.header, a.order, a.apply)
            if a.mint_open and a.apply:
                n += mint_open(a.repo, recs)
            # a row imported in this pass gets its other fields in a second pass
            for _ in range(2 if a.apply and n else 0):
                if not backfill(a.repo, load(a.repo), rows, src, a.header, a.order, True):
                    break
            print(f"RESULT actions={n} applied={'yes' if a.apply else 'no'}")


if __name__ == "__main__":
    main()

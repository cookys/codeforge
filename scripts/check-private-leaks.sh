#!/usr/bin/env bash
# Privacy guard for the public tree: no private host / person / company names, no real emails.
#
# Why hashes: the denylist must not itself disclose the names, so it holds sha256 of lowercase
# tokens (scripts/private-denylist.sha256). Add one with:  printf %s token | sha256sum
#
# Token rule. Every tracked text file (git ls-files, binaries skipped) is read line by line and
# split on [^A-Za-z0-9.-]+. A token is hashed whole and also as each piece split on '-' and '.'
# (so a host name inside a fleet address or a dashed name is caught). A line matches when any
# hash is in the denylist. Entry forms in the denylist (one per line, '#' comments):
#   <hex>        matches a whole token or any '-'/'.' piece of it
#   exact:<hex>  matches only a whole token (use for domains whose pieces are common words)
#   home:<hex>   matches only the user part of /home/<user>/ or -home-<user>- (a login name that
#                is public elsewhere, e.g. a GitHub owner, is therefore fine in URLs)
# Emails: any address whose domain is not allowed below is flagged (class "non-allowlisted email").
# Known-red allowance: scripts/private-leaks.allow, lines "<path> <max matching lines>"; a listed
# file may hit up to its count, any other file may not hit at all.
# Output: path:line plus the class, never the matched value. Exit 1 on any violation.
#
# Env (for the test): CODEFORGE_LEAKS_ROOT, CODEFORGE_LEAKS_DENYLIST, CODEFORGE_LEAKS_ALLOW.
set -euo pipefail
root=${CODEFORGE_LEAKS_ROOT:-$(cd "$(dirname "$0")/.." && pwd)}
here=$(cd "$(dirname "$0")" && pwd)
export CODEFORGE_LEAKS_ROOT="$root"
export CODEFORGE_LEAKS_DENYLIST=${CODEFORGE_LEAKS_DENYLIST:-$here/private-denylist.sha256}
export CODEFORGE_LEAKS_ALLOW=${CODEFORGE_LEAKS_ALLOW:-$here/private-leaks.allow}
exec python3 -I - <<'PY'
import hashlib, os, re, subprocess, sys

root = os.environ["CODEFORGE_LEAKS_ROOT"]
def read_list(p):
    try:
        with open(p, encoding="utf-8") as f:
            return [l.strip() for l in f if l.strip() and not l.lstrip().startswith("#")]
    except FileNotFoundError:
        return []
any_h, exact_h, home_h = set(), set(), set()
for e in read_list(os.environ["CODEFORGE_LEAKS_DENYLIST"]):
    if e.startswith("exact:"): exact_h.add(e[6:])
    elif e.startswith("home:"): home_h.add(e[5:])
    else: any_h.add(e)
allow = {}
for e in read_list(os.environ["CODEFORGE_LEAKS_ALLOW"]):
    p, n = e.rsplit(None, 1)
    allow[p] = int(n)

OK_DOMAINS = ("example.com", "example.org", "example.net")
OK_SUFFIX = (".invalid", ".test", ".example", ".local")
OK_DOMAIN_EXACT = {"x.com", "a.com", "b.com", "x.rs"}
OK_ADDR = {"noreply@anthropic.com", "git@github.com"}
EMAIL = re.compile(r"[A-Za-z0-9._%+-]+@([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,})")
HOME = re.compile(r"(?:/home/|-home-)([A-Za-z0-9_]+)")
SPLIT = re.compile(r"[^A-Za-z0-9.-]+")
PIECE = re.compile(r"[-.]+")
h = lambda s: hashlib.sha256(s.encode()).hexdigest()

def email_ok(addr, dom):
    d = dom.lower()
    return (addr.lower() in OK_ADDR or d in OK_DOMAIN_EXACT or d in OK_DOMAINS
            or any(d.endswith("." + x) or d.startswith(x + ".") for x in OK_DOMAINS) or d.endswith(OK_SUFFIX))

def classes(line):
    out = set()
    for tok in SPLIT.split(line):
        t = tok.strip(".-").lower()
        if not t: continue
        if h(t) in exact_h: out.add("denylisted token")
        for piece in {t, *PIECE.split(t)}:
            if piece and h(piece) in any_h: out.add("denylisted token")
    for m in HOME.finditer(line):
        if h(m.group(1).lower()) in home_h: out.add("denylisted home user")
    for m in EMAIL.finditer(line):
        if not email_ok(m.group(0), m.group(1)): out.add("non-allowlisted email")
    return out

files = subprocess.run(["git", "-C", root, "ls-files", "-z"], capture_output=True, check=True).stdout.split(b"\0")
hits = {}
for fb in files:
    if not fb: continue
    f = fb.decode()
    p = os.path.join(root, f)
    if not os.path.isfile(p) or os.path.islink(p): continue
    data = open(p, "rb").read()
    if b"\0" in data[:8192]: continue
    for i, line in enumerate(data.decode("utf-8", "replace").splitlines(), 1):
        c = classes(line)
        if c: hits.setdefault(f, []).append((i, sorted(c)))
bad = 0
for f, hl in sorted(hits.items()):
    if f in allow and len(hl) <= allow[f]:
        continue
    why = f" (allowance {allow[f]} exceeded)" if f in allow else ""
    for i, c in hl:
        print(f"{f}:{i}: {', '.join(c)}{why}")
        bad += 1
if bad:
    print(f"check-private-leaks: FAIL, {bad} violating line(s)", file=sys.stderr)
    sys.exit(1)
print("check-private-leaks: ok")
PY

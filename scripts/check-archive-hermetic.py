#!/usr/bin/env python3
"""Fail when a release tarball carries anything from the build machine or the build moment (B43).

    python3 scripts/check-archive-hermetic.py <archive.tar.gz> --epoch <n> [--forbid <string>]...

Reads the raw bytes, not `tar -tv` output (which hides pax headers and owner names). Each of
these is a reason to exit 1:
  gzip  more than one member; any optional header field (FTEXT/FHCRC/FEXTRA/FNAME/FCOMMENT);
        a nonzero MTIME (use `gzip -n`, one stream);
  tar   a pax global or per-entry extended header (atime/ctime/...); a GNU header atime/ctime;
        an entry whose mtime is not <epoch>, whose uid/gid is not 0, or which stores an
        owner/group name; entries not in byte order (directory-listing order leaked); non-zero
        bytes after the last entry;
  data  a --forbid string (checkout path, $HOME, cargo home) in any entry name, link target or
        file content (the binary included), or anywhere else in the decompressed stream.
Exit 0 = hermetic, 2 = usage error.
"""
import argparse
import io
import sys
import tarfile
import zlib


def gzip_problems(raw):
    bad = []
    if len(raw) < 18 or raw[0:2] != b"\x1f\x8b" or raw[2] != 8:
        return ["not a gzip (deflate) file"], None
    flg = raw[3]
    if flg:
        names = [n for b, n in [(1, "FTEXT"), (2, "FHCRC"), (4, "FEXTRA"), (8, "FNAME"), (16, "FCOMMENT")] if flg & b]
        bad.append("gzip header has optional fields %s (use gzip -n, no comment/extra)" % "/".join(names or ["reserved"]))
    if int.from_bytes(raw[4:8], "little"):
        bad.append("gzip header stores a timestamp (use gzip -n)")
    d = zlib.decompressobj(16 + zlib.MAX_WBITS)
    try:
        data = d.decompress(raw) + d.flush()
    except zlib.error as e:
        return bad + ["gzip stream is corrupt: %s" % e], None
    if not d.eof:
        bad.append("gzip stream is truncated")
    if d.unused_data:
        bad.append("gzip file has more than one member (%d trailing bytes); pack one stream" % len(d.unused_data))
    return bad, data


def tar_problems(data, epoch, forbid):
    bad = []
    try:
        tf = tarfile.open(fileobj=io.BytesIO(data), mode="r:")
    except tarfile.TarError as e:
        return ["not a tar archive: %s" % e]
    names = []
    try:
        members = list(tf)
    except tarfile.TarError as e:
        return ["tar stream is unreadable: %s" % e]
    for m in members:
        names.append(m.name)
        if m.pax_headers:
            bad.append("%s: pax extended header %s" % (m.name, sorted(m.pax_headers)))
        if m.mtime != epoch:
            bad.append("%s: mtime %s, want %d" % (m.name, m.mtime, epoch))
        if m.uid or m.gid:
            bad.append("%s: uid/gid %d/%d, want 0/0" % (m.name, m.uid, m.gid))
        if m.uname or m.gname:
            bad.append("%s: stores owner names %r/%r (use --numeric-owner)" % (m.name, m.uname, m.gname))
        hay = [m.name.encode(), (m.linkname or "").encode()]
        if m.isfile():
            try:
                hay.append(tf.extractfile(m).read())
            except tarfile.TarError as e:
                bad.append("%s: content unreadable: %s" % (m.name, e))
        for s in forbid:
            if any(s.encode() in h for h in hay):
                bad.append("%s: contains the build-machine string '%s'" % (m.name, s))
    if tf.pax_headers:
        bad.append("pax global header %s" % sorted(tf.pax_headers))
    if names != sorted(names, key=lambda n: n.encode()):
        bad.append("entries are not in byte order (directory-listing order leaked; use tar --sort=name)")
    if not names:
        bad.append("archive has no entries")
    bad += raw_tar_problems(data, members, forbid)
    return bad


GNU_MAGIC = b"ustar  \x00"  # GNU/oldgnu header magic (offset 257); its atime/ctime fields follow


def _octal(field):
    if field[:1] and field[0] & 0x80:  # GNU base-256
        return int.from_bytes(field[1:], "big")
    v = field.split(b"\x00", 1)[0].strip()
    return int(v, 8) if v else 0


def raw_tar_problems(data, members, forbid):
    """What TarInfo does not expose. Walks every physical 512-byte header (GNU long-name/link
    and pax extension headers included) by its declared payload size: a GNU header's
    atime/ctime must be zero, only zero bytes may follow the end of the archive, and a
    forbidden string may appear nowhere in the decompressed stream."""
    bad = []
    off = 0
    while off + 512 <= len(data):
        hdr = data[off:off + 512]
        if not hdr.strip(b"\x00"):
            break  # end-of-archive marker
        try:
            stored = _octal(hdr[148:156])
            size = _octal(hdr[124:136])
        except ValueError:
            bad.append("unparseable tar header at byte %d" % off)
            return bad
        if stored != sum(hdr[:148]) + 8 * 32 + sum(hdr[156:]):
            bad.append("bad tar header checksum at byte %d" % off)
            return bad
        name = hdr[0:100].split(b"\x00", 1)[0].decode("utf-8", "replace")
        if hdr[156:157] == b"S":
            bad.append("%s: GNU sparse entry (not produced by pack_tarball)" % name)
        if hdr[257:265] == GNU_MAGIC:
            for field, at in (("atime", 345), ("ctime", 357)):
                if hdr[at:at + 12].strip(b"\x00 0"):
                    bad.append("%s (header at byte %d): GNU header stores %s %r" % (name, off, field, hdr[at:at + 12]))
        payload = size if hdr[156:157] not in (b"1", b"2", b"3", b"4", b"5", b"6") else 0
        off += 512 + ((payload + 511) // 512) * 512
    rest = data[off:].strip(b"\x00")
    if rest:
        bad.append("%d non-zero bytes after the end of the archive (data hidden past the end)" % len(rest))
    for s in forbid:
        if s.encode() in data:
            bad.append("the tar stream contains the build-machine string '%s'" % s)
    return bad


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("archive")
    ap.add_argument("--epoch", type=int, required=True)
    ap.add_argument("--forbid", action="append", default=[])
    a = ap.parse_args()
    try:
        raw = open(a.archive, "rb").read()
    except OSError as e:
        print("check-archive-hermetic: %s" % e, file=sys.stderr)
        return 2
    bad, data = gzip_problems(raw)
    if data is not None:
        bad += tar_problems(data, a.epoch, [s for s in a.forbid if s])
    if bad:
        print("check-archive-hermetic: %s is not hermetic:" % a.archive, file=sys.stderr)
        for b in bad:
            print("  - %s" % b, file=sys.stderr)
        return 1
    print("check-archive-hermetic: ok (%s)" % a.archive, file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())

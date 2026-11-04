# shellcheck shell=bash
# Reproducible release tarball (B43): the same staged files and epoch always give the same bytes.
#
#   pack_tarball <stage-dir> <name> <archive-path> <epoch>
#
# Packs <stage-dir>/<name> into <archive-path> (.tar.gz) with nothing taken from the build
# machine or the moment: entries sorted by name, every mtime = <epoch>, owner/group 0/0 with no
# names, modes normalized (no umask leak), GNU format (no pax atime/ctime headers), and gzip -n
# (no file name or timestamp in the gzip header). Needs GNU tar.
pack_tarball() {
  local stage="$1" name="$2" archive="$3" epoch="$4"
  tar --format=gnu --sort=name --mtime="@$epoch" \
      --owner=0 --group=0 --numeric-owner --mode='u+rwX,go+rX,go-w' \
      -cf - -C "$stage" "$name" | gzip -n -9 >"$archive"
}

# The epoch every release archive uses: SOURCE_DATE_EPOCH when set (reproducible-builds.org
# convention), else the commit time of HEAD — never the wall clock.
release_epoch() {
  if [ -n "${SOURCE_DATE_EPOCH:-}" ]; then
    printf '%s\n' "$SOURCE_DATE_EPOCH"
  else
    git log -1 --format=%ct
  fi
}

#!/usr/bin/env bash
# Measure L0 metadata coverage and digest durability across 5 fixture sessions
# before vs after P1 item 2.
set -euo pipefail

cd "$(dirname "$0")/.."

cargo test --quiet dream::ingest_digests::tests::measure_l0_metadata_and_digest_safety_5_sessions_report -- --nocapture

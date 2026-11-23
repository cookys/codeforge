#!/usr/bin/env bash
# Measure LLM egress before vs after summarize_for_llm across 20 fixtures.
# Prints a table of longest common contiguous substring length (in Unicode chars)
# between the outbound prompt and original L0 signal.content before and after the fix.
set -euo pipefail

cd "$(dirname "$0")/.."

cargo test --quiet dream::compile::tests::measure_llm_egress_20_fixtures_report -- --nocapture

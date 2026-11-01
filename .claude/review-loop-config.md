# Review Loop — CodeForge Config

> owner 2026-11-02：主審換 gpt-6-astra（low）；另一席換 grok-4.7（high）。
> 未列出的欄位走 autopilot 預設（project-config-template/review-loop-config.md）。

## Settings

- reviewer_engine: gpt-6-astra
- reviewer_effort: low
- reviewer_runner: codex
- qc_panel: gpt-6-astra, claude-opus-5-5, grok-4.7
- qc_panel_runners: codex, claude-native, grok
- qc_panel_efforts: low, high, high
- qc_panel_endpoints: @none, @none, @none
- qc_panel_aggregation: union-on-verified-critical
- min_panel_size: 3
- plan_review: auto
- hetero_review: auto
- plan_reviewer_engine: gpt-6-astra
- plan_reviewer_effort: low
- plan_reviewer_runner: codex
- plan_deep_reviewer_engine: grok-4.7
- plan_deep_reviewer_effort: high
- plan_deep_reviewer_runner: grok

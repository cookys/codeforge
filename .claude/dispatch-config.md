# Dispatch — CodeForge Config
# Declares which dispatcher autopilot's orchestrator skills prefer. autopilot
# picks the FIRST AVAILABLE entry per chain. See autopilot
# project-config-template/dispatch-config.md for the full schema.

## Doc Drift Audit
# How autopilot:doc-sync runs. CodeForge ships Claude-Code Workflow scripts as
# the fast path; native is the portable fallback (used on non-CC platforms or
# when the Workflow tool is unavailable).
- workflow:.claude/workflows/doc-drift-scoped.js
- workflow:.claude/workflows/doc-code-drift-audit.js
- native

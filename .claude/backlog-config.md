# Backlog — Project Config
#
# Schema: autopilot `references/backlog-entry.md`.

## Style
- style: heading

## Pointer Roots
- doc/specs/
- doc/plans/
- doc/design/
- doc/backlog/
- doc/projects/

## Id Pattern
- id_pattern: ^B[0-9]+$

## Mode
- mode: block
- required_fields: Id, Since, Origin, Problem, Impact, Trigger-Type

## Done Retention Days
# BACKLOG is a queue: a shipped/dropped row is deleted, history stays in git.
- done_retention_days: 0

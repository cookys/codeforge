# CodeForge: Memory Search

Use this skill to search your knowledge base.

## Instructions

To search: `codeforge memory search "your query"`

To check status: `codeforge memory status`

To add knowledge: `codeforge learn "what you learned"`

To import web chat exports: `codeforge ingest path/to/claude-export.json`

## How memory works

CodeForge maintains a two-layer memory system:
- **L0** (signals): Raw entries in `.codeforge/signals/*.jsonl` — append-only log
- **L1** (knowledge): Compiled Markdown in `.codeforge/store/{concepts,connections,qa}/`

Run `codeforge dream` to compile L0 signals into searchable L1 knowledge entries.

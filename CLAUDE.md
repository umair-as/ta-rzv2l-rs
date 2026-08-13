# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

@AGENTS.md

## Claude Code specifics

- A `PostToolUse` hook (`.claude/settings.json` → `.claude/hooks/rustfmt-on-edit.py`) runs
  `rustfmt` on every `.rs` file you edit (skipping `third_party/`). Don't hand-format;
  `make lint` is the gate that must pass before a change is done.
- The board is driven over ssh (`local.mk` holds the address); serial console access exists
  via the `mcp-serial-rs` MCP server when ssh is down.
- `scratch/` holds session handoffs — start there when it exists.

---
name: glide-overhaul
description: "Whole-project refactor, optimisation and bug-fix pass over the Glide workspace, crate by crate, with subagents. Use when the user asks to read the whole project and refactor / optimise / fix as many problems as possible, or names crates for such a pass (e.g. /glide-overhaul glide-ui glidectl)."
argument-hint: "[crate ...] (default: next unprocessed crates, smallest first)"
---

# Glide overhaul

A staged pass that reads the code, finds real problems, fixes them and proves
the fixes, one small batch of crates at a time. Reply in Ukrainian.

## Scope per stage

- Arguments name the crates. Without them, take the next 2–3 crates not yet
  listed in `docs/overhaul-log.md`, smallest first (`git ls-files` line count).
  `glide-settings` and `glide-compositor` are split by module directory
  instead of taken whole.
- One stage = one Workflow run over that batch, then verification and one
  commit per crate.

## Workflow shape (one pipeline item per crate, no barrier)

1. **Find** — one agent reads every `.rs` file of the crate in full and returns
   findings: `bug`, `perf`, `duplication`, `dead-code`, `simplify`,
   `agents-md-trap` (§7 of AGENTS.md), `comment` (§6), each with file, line,
   evidence and the proposed change. It must quote the code it relies on.
   No style nits, no renames for taste, no speculative "might be slow".
2. **Fix** — one agent re-checks each finding against the code (skips the ones
   it cannot confirm, with the reason), applies the rest with `Edit`, then runs
   `scripts/ai-cargo.sh clippy -p <crate> --all-targets -- -D warnings` and
   `scripts/ai-cargo.sh test -p <crate>` until both pass. Never changes the
   signature of a `pub` item used by another crate unless it updates every
   caller and the brief allows cross-crate edits.
3. **Review** — one agent reads `git diff -- crates/<crate>` cold and tries to
   refute each hunk: behaviour change, lost edge case, a test weakened to pass,
   a new comment, a new `#[allow]`. Returns hunks to revert or fix.

Agents never commit, never touch files outside their crate (unless allowed),
never run plain `cargo` (only `cargo fmt` directly), never write Python.

## After the workflow (main thread)

1. Apply or revert what the reviewer flagged.
2. `cargo fmt --all`, then `scripts/ai-cargo.sh clippy` and `test` for the
   touched crates and their direct dependents.
3. One commit per crate: subject names the crate, body lists the fixes by kind.
   Leave the user's own uncommitted files (e.g. `scripts/ai-cargo.sh`,
   `design/`, `user_plan.md`) out.
4. Append the crate, date, commit and net line delta to
   `docs/overhaul-log.md`.
5. Report per crate: what was fixed, what was skipped and why, net lines
   (`git diff --shortstat`), and the running total. Name what is unverified
   (rendering, hardware).
6. Ask before starting the next stage.

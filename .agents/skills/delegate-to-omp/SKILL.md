---
name: delegate-to-omp
description: Delegate bounded, repetitive, objectively verifiable Jellypilot implementation or verification tasks from Codex to an OMP RPC worker launched through Herdr. Use when Codex has already made the major decisions, can define an exact write scope and verification commands, and needs OMP to execute the work and return a structured report. Do not use for unresolved product or architecture decisions, ambiguous diagnosis, or critical user coordination.
---

# Delegate to OMP

Keep Codex responsible for intent, direction, decomposition, verification design, report review, and the final user response. Give OMP only closed execution packets.

## Eligibility gate

Delegate only when all of these are true:

- Resolve the intended behavior and major implementation direction first.
- Bound every repository-visible edit to exact files or directory prefixes.
- State concrete requirements and acceptance criteria.
- Provide at least one exact verification command and its expected evidence.
- Ensure the work is reversible and does not overlap another editing OMP task.

Keep the task in Codex when it needs a user choice, architecture or public-interface design, ambiguous diagnosis, security-sensitive judgment, irreversible external action, or an unbounded refactor. OMP may choose only local syntax or mechanics that do not change the decided behavior or interface.

## Create the task packet

Inspect `git status` and the relevant diff before writing the packet. Preserve existing user changes. Create a JSON file outside the repository, normally under `/tmp`, with this shape:

```json
{
  "task_id": "bounded-task-slug",
  "title": "Short task title",
  "mode": "edit",
  "objective": "One resolved outcome",
  "context_paths": ["src/relevant-file.ts"],
  "write_scope": ["src/relevant-file.ts", "tests/feature/"],
  "requirements": ["Exact implementation requirement"],
  "acceptance_criteria": ["Observable completion condition"],
  "verification": [
    {
      "command": "bun run test -- tests/relevant.test.ts",
      "expected": "Exit 0 with the focused scenarios passing"
    }
  ],
  "skills": ["solidjs"],
  "extra_tools": [],
  "timeout_seconds": 1800
}
```

Use `mode: "verify"` with an empty `write_scope` for read-only verification. A write-scope entry names either one exact repository-relative path or, when it ends in `/`, every descendant of that directory. Select only the skills OMP needs; an empty `skills` list disables skill loading. Optional extra tools are limited to `browser`, `inspect_image`, and `python`.

Do not put unresolved choices in `requirements`. Resolve them in Codex or ask the user first.

## Run through Herdr

Confirm `HERDR_ENV=1`. If it is not set, stop and explain that this workflow must run inside a Herdr-managed pane.

Run:

```bash
bun .agents/skills/delegate-to-omp/scripts/run-omp-task.mjs \
  --packet /tmp/<task-id>.json
```

Add `--keep-pane` only when the successful bridge lifecycle needs inspection. The runner:

- starts a fresh named agent through the Herdr socket API and drives `omp --mode rpc` over a private Unix socket;
- auto-approves the closed task while excluding OMP's `task` and `ask` tools;
- serializes edit tasks with a repository-specific lock;
- fingerprints HEAD, index state, and tracked or untracked non-ignored files;
- requires a structured OMP result whose verification commands exactly match the packet;
- reads the final assistant report from OMP RPC rather than scraping terminal output;
- closes a successful pane, but keeps blocked or invalid runs available for inspection.

Treat a nonzero runner exit as a failed delegation. Never recover or revert files automatically. Read the reported pane, inspect the current diff, and decide whether to issue a corrected packet or take the task back into Codex.

## Review the result

After a successful run:

1. Read the relayed OMP summary and verification evidence.
2. Inspect the resulting diff, including any pre-existing dirty files in the write scope.
3. Confirm the reported changed files match the intended slice and that no major decision was made by OMP.
4. Accept the result, issue a narrower correction packet, or take over. Rerun verification in Codex only when evidence is incomplete, contradictory, or high risk.
5. Report the consolidated outcome to the user. OMP never owns user-facing completion.

Parallelize verification packets only when their evidence is independent of active edits. Run at most one edit packet per repository.

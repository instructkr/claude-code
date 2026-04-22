# Review Dashboard — claw-code

**Last updated:** 2026-04-23 03:34 Seoul
**Queue state:** 14 review-ready branches
**Main HEAD:** `f18f45c` (ROADMAP #161 filed)

This is an integration support artifact (per cycle #64 doctrine). Its purpose: let reviewers see all queued branches, cluster membership, and merge priorities without re-deriving from git log.

---

## At-A-Glance

| Priority | Cluster | Branches | Complexity | Status |
|---|---|---|---|---|
| P0 | Typed-error threading | #248, #249, #251 | S–M | Merge-ready |
| P1 | Diagnostic-strictness | #122, #122b | S | Merge-ready |
| P1 | Help-parity | #130b-#130e | S each | Merge-ready (batch) |
| P2 | Suffix-guard | #152-init, #152-bootstrap-plan | XS each | Merge-ready (batch) |
| P2 | Verb-classification | #160 | S | Merge-ready (just shipped) |
| P3 | Doc truthfulness | docs/parity-update | XS | Merge-ready |

**Suggested merge order:** P0 → P1 → P2 → P3. Within P0, start with #249 (smallest diff).

---

## Detailed Branch Inventory

### P0: Typed-Error Threading (3 branches)

#### `feat/jobdori-249-resumed-slash-kind` — **SMALLEST. START HERE.**
- **Commit:** `eb4b1eb`
- **Diff:** 61 lines in `rust/crates/rusty-claude-cli/src/main.rs`
- **Scope:** Two Err arms in `resume_session()` at lines 2745, 2782 now emit `kind` + `hint`
- **Cluster:** Completes #247 parent's typed-error family
- **Tests:** 181 binary tests pass (no regressions)
- **Reviewer checklist:** see `/tmp/pr-summary-249.md`
- **Expected merge time:** ~5 minutes

#### `feat/jobdori-248-unknown-verb-option-classify`
- **Commit:** `6c09172`
- **Scope:** Unknown verb + option classifier family
- **Cluster:** #247 parent's typed-error family (sibling of #249)

#### `feat/jobdori-251-session-dispatch`
- **Commit:** `dc274a0`
- **Scope:** Intercepts session-management verbs (`list-sessions`, `load-session`, `delete-session`, `flush-transcript`) at top-level parser
- **Cluster:** #247 parent's typed-error family
- **Note:** Larger change than #248/#249 — prefer merging those first

### P1: Diagnostic-Strictness (2 branches)

#### `feat/jobdori-122-doctor-stale-base`
- **Commit:** `5bb9eba`
- **Scope:** `claw doctor` now warns on stale-base (same check as prompt preflight)
- **Cluster:** Diagnostic surfaces reflect runtime reality (cycle #57 principle)

#### `feat/jobdori-122b-doctor-broad-cwd`
- **Commit:** `0aa0d3f`
- **Scope:** `claw doctor` now warns when cwd is broad path (home/root)
- **Cluster:** Same as #122 (direct sibling)
- **Batch suggestion:** Review together with #122

### P1: Help-Parity (4 branches, batch-reviewable)

All four implement uniform `--help` flag handling. Related by fix locus (help-topic routing).

#### `feat/jobdori-130b-filesystem-context`
- **Commit:** `d49a75c`
- **Scope:** Filesystem I/O errors enriched with operation + path context

#### `feat/jobdori-130c-diff-help`
- **Commit:** `83f744a`
- **Scope:** `claw diff --help` routes to help topic

#### `feat/jobdori-130d-config-help`
- **Commit:** `19638a0`
- **Scope:** `claw config --help` routes to help topic

#### `feat/jobdori-130e-dispatch-help` + `feat/jobdori-130e-surface-help`
- **Commits:** `0ca0344`, `9dd7e79`
- **Scope:** Category A (dispatch-order) + Category B (surface) help-anomaly fixes from systematic sweep
- **Batch suggestion:** Review #130c, #130d, #130e-dispatch, #130e-surface as one unit — all use same pattern (add help flag guard before action)

### P2: Suffix-Guard (2 branches, batch-reviewable)

#### `feat/jobdori-152-init-suffix-guard`
- **Commit:** `860f285`
- **Scope:** `claw init` rejects trailing args
- **Cluster:** Uniform no-arg verb suffix guards

#### `feat/jobdori-152-bootstrap-plan-suffix-guard`
- **Commit:** `3a533ce`
- **Scope:** `claw bootstrap-plan` rejects trailing args
- **Cluster:** Same as above (direct sibling)
- **Batch suggestion:** Review together

### P2: Verb-Classification (1 branch, just shipped cycle #63)

#### `feat/jobdori-160-verb-classification`
- **Commit:** `5538934`
- **Scope:** Reserved-semantic verbs (resume, compact, memory, commit, pr, issue, bughunter) with positional args now emit slash-command guidance
- **Cluster:** Sibling of #251 (dispatch leak family), applied to promptable/reserved split
- **Design closure note:** Investigation in cycle #61 revealed verb-classification was the actual need; cycle #63 implemented the class table

### P3: Doc Truthfulness (1 branch, just shipped cycle #64)

#### `docs/parity-update-2026-04-23`
- **Commit:** `92a79b5`
- **Scope:** PARITY.md stats refreshed (Rust LOC +66%, Test LOC +76%, Commits +235% since 2026-04-03)
- **Risk:** Near-zero (4-line diff, doc-only)
- **Merge time:** ~1 minute

---

## Batch Review Patterns

For reviewer efficiency, these groups share the same fix-locus or pattern:

| Batch | Branches | Shared pattern |
|---|---|---|
| Help-parity bundle | #130c, #130d, #130e-dispatch, #130e-surface | All add help-flag guard before action in dispatch |
| Suffix-guard bundle | #152-init, #152-bootstrap-plan | Both add `rest.len() > 1` check to no-arg verbs |
| Diagnostic-strictness bundle | #122, #122b | Both extend `check_workspace_health()` with new preflights |
| Typed-error bundle | #248, #249, #251 | All thread `classify_error_kind` + `split_error_hint` into specific Err arms |

If reviewer has limited time, batch review saves context switches.

---

## Review Friction Map

**Lowest friction (safe start):**
- docs/parity-update (4 lines, doc-only)
- #249 (61 lines, 2 Err arms, 181 tests pass)
- #160 (23 lines, new helper + pre-check)

**Medium friction:**
- #122, #122b (each ~100 lines, diagnostic extensions)
- #248 (classifier family)
- #152-* branches (XS each)

**Highest friction:**
- #251 (broader parser changes, multi-verb coverage)
- #130e bundle (help-parity systematic sweep)

---

## Open Pinpoints Awaiting Implementation

| # | Title | Priority | Est. diff | Notes |
|---|---|---|---|---|
| #157 | Auth remediation registry | S-M | 50-80 lines | Cycle #59 audit pre-fill |
| #158 | Hook validation at worker boot | S | 30-50 lines | Cycle #59 audit pre-fill |
| #159 | Plugin manifest validation at worker boot | S | 30-50 lines | Cycle #59 audit pre-fill |
| #161 | Stale Git SHA in worktree builds | S | ~15 lines in build.rs | Cycle #65 just filed |

None of these should be implemented while current queue is 14. Prioritize merging queue first.

---

## Merge Throughput Notes

**Target throughput:** 2-3 branches per review session. At current cycle velocity (cycles #39–#65 = 27 cycles in ~3 hours), 2-3 merges unblock:
- 3+ cluster closures (typed-error, diagnostic-strictness, help-parity)
- 1 doctrine loop closure (verb-classification → #160)
- 1 doc freshness (PARITY.md)

**Post-merge expected state:** ~10 branches remaining, queue shifts from saturated (14) to manageable (10), velocity cycles can resume in safe zone.

---

## For The Reviewer

**Reviewing checklist (per-branch):**
- [ ] Diff matches pinpoint description
- [ ] Tests pass (cite count: should be 181+ for branches that touched main.rs)
- [ ] Backward compatibility verified (check-list in commit message)
- [ ] No related cluster branches yet to land (check cluster column above)

**Reviewer shortcut for #249** (recommended first-merge):
```bash
cd /tmp/jobdori-249
git log --oneline -1  # eb4b1eb
git diff main..HEAD -- rust/crates/rusty-claude-cli/src/main.rs | head -50
```

Or skip straight to: `/tmp/pr-summary-249.md` (pre-prepared PR-ready artifact).

---

**Dashboard source:** Cycle #66 (2026-04-23 03:34 Seoul). Updates should be re-run when branches merge or new pinpoints land.

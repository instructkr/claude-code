# OPT_OUT Demand Log

**Purpose:** Record real demand signals for promoting OPT_OUT surfaces to CLAWABLE. Without this log, the audit criteria in `OPT_OUT_AUDIT.md` have no evidentiary base.

**Status:** Active survey window (post-#178/#179, cycles #21+)

## How to file a demand signal

When any external claw, operator, or downstream consumer actually needs JSON output from one of the 12 OPT_OUT surfaces, add an entry below. **Speculation, "could be useful someday," and internal hypotheticals do NOT count.**

A valid signal requires:
- **Source:** Who/what asked (human, automation, agent session, external tool)
- **Surface:** Which OPT_OUT command (from the 12)
- **Use case:** The concrete orchestration problem they're trying to solve
- **Would-parse-Markdown alternative checked?** Why the existing OPT_OUT output is insufficient
- **Date:** When the signal was received

## Promotion thresholds

Per `OPT_OUT_AUDIT.md` criteria:
- **2+ independent signals** for the same surface within a survey window → file promotion pinpoint
- **1 signal + existing stable schema** → file pinpoint for discussion
- **0 signals** → surface stays OPT_OUT (documented rationale in audit file)

The threshold is intentionally high. Single-use hacks can be served via one-off Markdown parsing; schema promotion is expensive (docs, tests, maintenance).

---

## Demand Signals Received

### Group A: Rich-Markdown Reports

#### `summary`
**Signals received: 0**

Notes: No demand recorded. Markdown output is intentional and useful for human review.

#### `manifest`
**Signals received: 0**

Notes: No demand recorded.

#### `parity-audit`
**Signals received: 0**

Notes: No demand recorded. Report consumers are humans reviewing porting progress, not automation.

#### `setup-report`
**Signals received: 0**

Notes: No demand recorded.

---

### Group B: List Commands with Query Filters

#### `subsystems`
**Signals received: 0**

Notes: `--limit` already provides filtering. No claws requesting JSON.

#### `commands`
**Signals received: 0**

Notes: `--query`, `--limit`, `--no-plugin-commands`, `--no-skill-commands` already allow filtering. No demand recorded.

#### `tools`
**Signals received: 0**

Notes: `--query`, `--limit`, `--simple-mode` provide filtering. No demand recorded.

---

### Group C: Simulation / Debug Surfaces

#### `remote-mode`
**Signals received: 0**

Notes: Simulation-only. No production orchestration need.

#### `ssh-mode`
**Signals received: 0**

Notes: Simulation-only.

#### `teleport-mode`
**Signals received: 0**

Notes: Simulation-only.

#### `direct-connect-mode`
**Signals received: 0**

Notes: Simulation-only.

#### `deep-link-mode`
**Signals received: 0**

Notes: Simulation-only.

---

## Survey Window Status

| Cycle | Date | New Signals | Running Total | Action |
|---|---|---|---|---|
| #21 | 2026-04-22 | 0 | 0 | Survey opened; log established |

**Current assessment:** Zero demand for any OPT_OUT surface promotion. This is consistent with `OPT_OUT_AUDIT.md` prediction that all 12 likely stay OPT_OUT long-term.

---

## Signal Entry Template

```
### <surface-name>
**Signal received: [N]**

Entry N (YYYY-MM-DD):
- Source: <who/what>
- Use case: <concrete orchestration problem>
- Markdown-alternative-checked: <yes/no + why insufficient>
- Follow-up: <filed pinpoint / discussion thread / closed>
```

---

## Decision Framework

At cycle #22 (or whenever survey window closes):

### If 0 signals total (likely):
- Move all 12 surfaces to `PERMANENTLY_OPT_OUT` or similar
- Remove `OPT_OUT_SURFACES` from `test_cli_parity_audit.py` (everything is explicitly non-goal)
- Update `CLAUDE.md` to reflect maintainership mode
- Close `OPT_OUT_AUDIT.md` with "audit complete, no promotions"

### If 1–2 signals on isolated surfaces:
- File individual promotion pinpoints per surface with demand evidence
- Each goes through standard #171/#172/#173 loop (parity audit, SCHEMAS.md, consistency test)

### If high demand (3+ signals):
- Reopen audit: is the OPT_OUT classification actually correct?
- Review whether protocol expansion is warranted

---

## Related Files

- **`OPT_OUT_AUDIT.md`** — Audit criteria, decision table, rationale by group
- **`SCHEMAS.md`** — JSON contract for the 14 CLAWABLE surfaces
- **`tests/test_cli_parity_audit.py`** — Machine enforcement of CLAWABLE/OPT_OUT classification
- **`CLAUDE.md`** — Development posture (maintainership mode)

---

## Philosophy

**Prevent speculative expansion.** The discipline of requiring real signals before promotion protects the protocol from schema bloat. Every new CLAWABLE surface adds:
- A SCHEMAS.md section (maintenance burden)
- Test coverage (test suite tax)
- Documentation (cognitive load for new developers)
- Version compatibility (schema_version bump risk)

If a claw can't articulate *why* it needs JSON for `summary` beyond "it would be nice," then JSON for `summary` is not needed. The Markdown output is a feature, not a gap.

The audit log closes the loop on "governed non-goals": OPT_OUT surfaces are intentionally not clawable until proven otherwise by evidence.

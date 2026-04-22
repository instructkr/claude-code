# OPT_OUT Surface Audit Roadmap

**Status:** Pre-audit (decision table ready, survey pending)

This document governs the audit and potential promotion of 12 OPT_OUT surfaces (commands that currently do **not** support `--output-format json`).

## OPT_OUT Classification Rationale

A surface is classified as OPT_OUT when:
1. **Human-first by nature:** Rich Markdown prose / diagrams / structured text where JSON would be information loss
2. **Query-filtered alternative exists:** Commands with internal `--query` / `--limit` don't need JSON (users already have escape hatch)
3. **Simulation/debug only:** Not meant for production orchestration (e.g., mode simulators)
4. **Future JSON work is planned:** Documented in ROADMAP with clear upgrade path

---

## OPT_OUT Surfaces (12 Total)

### Group A: Rich-Markdown Reports (4 commands)

**Rationale:** These emit structured narrative prose. JSON would require lossy serialization.

| Command | Output | Current use | JSON case |
|---|---|---|---|
| `summary` | Multi-section workspace summary (Markdown) | Human readability | Not applicable; Markdown is the output |
| `manifest` | Workspace manifest with project tree (Markdown) | Human readability | Not applicable; Markdown is the output |
| `parity-audit` | TypeScript/Python port comparison report (Markdown) | Human readability | Not applicable; Markdown is the output |
| `setup-report` | Preflight + startup diagnostics (Markdown) | Human readability | Not applicable; Markdown is the output |

**Audit decision:** These likely remain OPT_OUT long-term (Markdown-as-output is intentional). If JSON version needed in future, would be a separate `--output-format json` path generating structured data (project summary object, manifest array, audit deltas, setup checklist) — but that's a **new contract**, not an addition to existing Markdown surfaces.

**Pinpoint:** #175 (deferred) — audit whether `summary`/`manifest` should emit JSON structured versions *in parallel* with Markdown, or if Markdown-only is the right UX.

---

### Group B: List Commands with Query Filters (3 commands)

**Rationale:** These already support `--query` and `--limit` for filtering. JSON output would be redundant; users can pipe to `jq`.

| Command | Filtering | Current output | JSON case |
|---|---|---|---|
| `subsystems` | `--limit` | Human-readable list | Use `--query` to filter, users can parse if needed |
| `commands` | `--query`, `--limit`, `--no-plugin-commands`, `--no-skill-commands` | Human-readable list | Use `--query` to filter, users can parse if needed |
| `tools` | `--query`, `--limit`, `--simple-mode` | Human-readable list | Use `--query` to filter, users can parse if needed |

**Audit decision:** `--query` / `--limit` are already the machine-friendly escape hatch. These commands are **intentionally** list-filter-based (not orchestration-primary). Promoting to CLAWABLE would require:
1. Formalizing what the structured output *is* (command array? tool array?)
2. Versioning the schema per command
3. Updating tests to validate per-command schemas

**Cost-benefit:** Low. Users who need structured data can already use `--query` to narrow results, then parse. Effort to promote > value.

**Pinpoint:** #176 (backlog) — audit `--query` UX; consider if a `--query-json` escape hatch (output JSON of matching items) is worth the schema tax.

---

### Group C: Simulation / Debug Surfaces (5 commands)

**Rationale:** These are intentionally **not production-orchestrated**. They simulate behavior, test modes, or debug scenarios. JSON output doesn't add value.

| Command | Purpose | Output | Use case |
|---|---|---|---|
| `remote-mode` | Simulate remote execution | Text (mock session) | Testing harness behavior under remote constraints |
| `ssh-mode` | Simulate SSH execution | Text (mock SSH session) | Testing harness behavior over SSH-like transport |
| `teleport-mode` | Simulate teleport hop | Text (mock hop session) | Testing harness behavior with teleport bouncing |
| `direct-connect-mode` | Simulate direct network | Text (mock session) | Testing harness behavior with direct connectivity |
| `deep-link-mode` | Simulate deep-link invocation | Text (mock deep-link) | Testing harness behavior from URL/deeplink |

**Audit decision:** These are **intentionally simulation-only**. Promoting to CLAWABLE means:
1. "This simulated mode is now a valid orchestration surface"
2. Need to define what JSON output *means* (mock session state? simulation log?)
3. Need versioning + test coverage

**Cost-benefit:** Very low. These are debugging tools, not orchestration endpoints. Effort to promote >> value.

**Pinpoint:** #177 (backlog) — decide if mode simulators should ever be CLAWABLE (probably no).

---

## Audit Workflow (Future Cycles)

### For each surface:
1. **Survey:** Check if any external claw actually uses --output-format with this surface
2. **Cost estimate:** How much schema work + testing?
3. **Value estimate:** How much demand for JSON version?
4. **Decision:** CLAWABLE, remain OPT_OUT, or new pinpoint?

### Promotion criteria (if promoting to CLAWABLE):

A surface moves from OPT_OUT → CLAWABLE **only if**:
- ✅ Clear use case for JSON (not just "hypothetically could be JSON")
- ✅ Schema is simple and stable (not 20+ fields)
- ✅ At least one external claw has requested it
- ✅ Tests can be added without major refactor
- ✅ Maintainability burden is worth the value

### Demote criteria (if staying OPT_OUT):

A surface stays OPT_OUT **if**:
- ✅ JSON would be information loss (Markdown reports)
- ✅ Equivalent filtering already exists (`--query` / `--limit`)
- ✅ Use case is simulation/debug, not production
- ✅ Promotion effort > value to users

---

## Post-Audit Outcomes

### Likely scenario (high confidence)

**Group A (Markdown reports):** Remain OPT_OUT
- `summary`, `manifest`, `parity-audit`, `setup-report` are **intentionally** human-first
- If JSON-like structure is needed in future, would be separate `*-json` commands or distinct `--output-format`, not added to Markdown surfaces

**Group B (List filters):** Remain OPT_OUT
- `subsystems`, `commands`, `tools` have `--query` / `--limit` as query layer
- Users who need structured data already have escape hatch

**Group C (Mode simulators):** Remain OPT_OUT
- `remote-mode`, `ssh-mode`, etc. are debug tools, not orchestration endpoints
- No demand for JSON version; promotion would be forced, not driven

**Result:** OPT_OUT audit concludes that 12/12 surfaces should **remain OPT_OUT** (no promotions).

### If demand emerges

If external claws report needing JSON from any OPT_OUT surface:
1. File pinpoint with use case + rationale
2. Estimate cost + value
3. If value > cost, promote to CLAWABLE with full test coverage
4. Update SCHEMAS.md
5. Update CLAUDE.md

---

## Timeline

- **Post-#174 (now):** OPT_OUT audit documented (this file)
- **Cycles #19–#21 (deferred):** Survey period — collect data on external demand
- **Cycle #22 (deferred):** Final audit decision + any promotions
- **Post-audit:** Move to protocol maintenance mode (new commands/fields/surfaces)

---

## Related

- **OPT_OUT_DEMAND_LOG.md** — Active survey recording real demand signals (evidentiary base for any promotion decision)
- **SCHEMAS.md** — Clawable surface contracts
- **CLAUDE.md** — Development guidance
- **test_cli_parity_audit.py** — Parametrized tests for CLAWABLE_SURFACES enforcement
- **ROADMAP.md** — Macro phases (this audit is Phase 3 before Phase 2 closure)

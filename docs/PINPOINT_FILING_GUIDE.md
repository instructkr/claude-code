# Pinpoint Filing Guide

This guide walks through the workflow for filing a new claw-code pinpoint, from initial friction to merged ROADMAP entry. For format details, see [CONTRIBUTING.md](../CONTRIBUTING.md). For issue template, see [.github/ISSUE_TEMPLATE/pinpoint.md](../.github/ISSUE_TEMPLATE/pinpoint.md).

## What is a Pinpoint?

A pinpoint is a precise, distinct claw-code clawability gap captured in ROADMAP.md format. Pinpoints differ from generic issues by:
- **Specificity:** Exact file paths, function names, line numbers when available
- **Distinctness:** Verified not already covered by existing pinpoints
- **Live evidence:** Real friction event, not hypothetical
- **Fix shape:** Concrete delta proposal, not vague "should improve X"

## Workflow

### Step 1: Identify friction

Use claw-code in real work. When you hit friction (slow startup, broken behavior, opaque error, missing feature, test brittleness, etc.), STOP and capture:
- What you were trying to do
- What you expected to happen
- What actually happened
- Exact error message / log output (verbatim)

### Step 2: Identify distinct axis

Open ROADMAP.md and search for related existing pinpoints (use the [Cluster Index](../ROADMAP.md#pinpoint-cluster-index)).

For each candidate match:
- Does the existing pinpoint cover this exact symptom?
- Does it cover this exact axis (e.g., timing vs envelope vs config)?
- Is your case a SUBSET, a SUPERSET, or an ORTHOGONAL axis?

If your case is orthogonal, file new. If subset, add live-evidence as additional context to existing pinpoint. If superset, file new + cross-reference existing.

### Step 3: Verify with code

Before filing, look at the relevant source code:
- `rust/crates/api/src/sse.rs` — provider routing
- `rust/crates/runtime/src/conversation.rs` — auto-compaction logic
- `rust/crates/rusty-claude-cli/src/main.rs` — CLI entry
- Search with grep / ripgrep to find the relevant module

If the code clearly does NOT have the feature you expected, file a pinpoint. If the code DOES have the feature but it's broken, file a bug.

### Step 4: Write the entry

Follow the canonical 5-section format (see [CONTRIBUTING.md](../CONTRIBUTING.md)):
1. **Exact pinpoint** — One precise sentence
2. **Live evidence** — Real friction event with timestamps
3. **Why distinct** — Explicit comparison to nearest existing pinpoints
4. **Concrete delta** — What you're filing (e.g., "ROADMAP.md appended")
5. **Fix shape recorded** — Bullet list of suggested implementation steps

### Step 5: Submit

Append to ROADMAP.md and commit:

```
git add ROADMAP.md
git commit -m "roadmap: #<NNN> filed (<short title>)"
git push origin <branch>
git push fork <branch>
```

Verify three-way parity (local == origin == fork) before posting any update.

## Worked Example: #290 (stream-init failure envelope)

This shows how #290 was filed in real-time on 2026-04-26.

### Step 1: Friction identified

gaebal-gajae's session hit `500 empty_stream: upstream stream closed before first payload` repeatedly (4x in 30 min). Bare-string error surfaced; no diagnostics, no retry guidance.

### Step 2: Distinct axis identified

- #266 (typed-error-kind taxonomy) covers single-failure categorization, NOT stream-init specifically
- #287 (auto-compaction reactive) covers session-size failures, NOT transport
- #288 (JSON envelope failure) covers context-window envelope, NOT stream-init

→ Orthogonal: filed new #290 covering typed-stream-init-failure-envelope

### Step 3: Code verified

Inspected `rust/crates/api/src/sse.rs` — confirmed no `failure_class=upstream_stream_init` discriminant, no retry recommendation in JSON envelope.

### Step 4: Entry written

Used canonical 5-section format. Listed 4 live evidence timestamps. Cross-referenced #266, #287, #288 in "Why distinct."

### Step 5: Submitted

Commit `0f38975`, pushed to both origin and fork, parity verified, Discord post under 1500 chars.

**Total time: ~2 minutes from friction identification to merged ROADMAP entry.**

## Tips

- **File while it's fresh.** Wait too long and you'll forget exact symptoms.
- **Check Cluster Index FIRST** — saves time vs scanning full ROADMAP.
- **Write Fix Shape even if you don't implement.** Helps future contributors.
- **Live evidence with timestamps > theoretical examples.** Real-world friction always wins.

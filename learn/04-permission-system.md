# 4. Permission System

> How Claude Code prevents an AI from doing dangerous things — a multi-layered defense.

---

## Why Permissions Matter

Claude Code can run **arbitrary bash commands**, **write to any file**, and **make network requests**. Without a permission system, a single misguided model response could `rm -rf /` your entire system.

The permission system is a chain of checks — if any link denies, the tool doesn't run.

---

## The Permission Flow

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#dc3545', 'primaryBorderColor': '#dc3545'}}}%%
flowchart TD
    ENTRY["Tool call arrives"]:::start

    DR{"Deny rules?<br/>blanket deny, pattern match"}
    AR{"Allow rules?<br/>always-allow from settings"}
    TSP{"tool.checkPermissions?<br/>tool-specific logic"}
    HOOK{"PreToolUse hooks?<br/>user-defined scripts"}
    CLASS{"Auto-mode classifier?<br/>transcript safety analysis"}
    DIALOG{"User permission dialog<br/>Y / n / always-allow"}

    ALLOW["ALLOW<br/>execute tool"]:::allow
    DENY["DENY<br/>return error to model"]:::deny

    ENTRY --> DR

    DR -->|"matched deny rule"| DENY
    DR -->|"no match"| AR

    AR -->|"matched allow rule"| ALLOW
    AR -->|"no match"| TSP

    TSP -->|"tool says allow"| HOOK
    TSP -->|"tool says deny"| DENY

    HOOK -->|"hook approves"| ALLOW
    HOOK -->|"hook denies"| DENY
    HOOK -->|"no decision"| CLASS

    CLASS -->|"classified safe"| ALLOW
    CLASS -->|"classified unsafe"| DIALOG
    CLASS -->|"not in auto-mode"| DIALOG

    DIALOG -->|"user accepts"| ALLOW
    DIALOG -->|"user rejects"| DENY
    DIALOG -->|"always allow"| AR

    classDef start fill:#1a2d4a,stroke:#4a9eff,color:#e0e0e0,stroke-width:2px
    classDef allow fill:#1b3a1b,stroke:#28a745,color:#e0e0e0,stroke-width:2px
    classDef deny fill:#4a1a1a,stroke:#dc3545,color:#e0e0e0,stroke-width:2px
```

---

## Layer 1: Deny Rules

**First check. Highest priority. Cannot be overridden.**

Deny rules are pattern-matched against tool name and input. If a deny rule matches, the tool is **immediately rejected** — no further checks run.

Sources of deny rules:
- `settings.json` — User-configured
- CLAUDE.md — Project-level rules
- Organization policy — Enterprise MDM settings

Example deny rules:
```json
{
  "alwaysDenyRules": {
    "settings": [
      { "tool": "Bash", "pattern": "rm -rf" },
      { "tool": "FileWrite", "pattern": "/etc/*" }
    ]
  }
}
```

### Permission Matching

Tools can implement `preparePermissionMatcher()` for custom pattern matching:

```typescript
// Bash tool: "git *" matches any git command
preparePermissionMatcher(input) {
  return async (pattern) => minimatch(input.command, pattern)
}
```

---

## Layer 2: Allow Rules

If no deny rule matched, check if an **allow rule** grants automatic approval.

Allow rules come from:
- User clicking "always allow" in the permission dialog
- `settings.json` configuration
- Slash command grants (e.g., `/plan` exit grants specific operations)

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#28a745', 'primaryBorderColor': '#28a745'}}}%%
flowchart LR
    subgraph Sources["Allow Rule Sources"]
        S1["settings.json<br/>user config"]
        S2["CLAUDE.md<br/>project rules"]
        S3["User dialog<br/>always-allow choice"]
        S4["Command grants<br/>plan mode exit"]
    end

    MERGE["ToolPermissionContext<br/>alwaysAllowRules"]:::merge

    CHECK{"Pattern match<br/>against tool + input"}:::check

    ALLOW["Auto-approved"]:::allow
    NEXT["Continue to<br/>next layer"]:::next

    S1 --> MERGE
    S2 --> MERGE
    S3 --> MERGE
    S4 --> MERGE

    MERGE --> CHECK
    CHECK -->|"match"| ALLOW
    CHECK -->|"no match"| NEXT

    classDef merge fill:#1a2d4a,stroke:#4a9eff,color:#e0e0e0,stroke-width:2px
    classDef check fill:#2d2d0d,stroke:#ffc107,color:#e0e0e0,stroke-width:2px
    classDef allow fill:#1b3a1b,stroke:#28a745,color:#e0e0e0,stroke-width:2px
    classDef next fill:#333,stroke:#888,color:#e0e0e0,stroke-width:1px
```

---

## Layer 3: Tool-Specific Permissions

Each tool implements `checkPermissions(input, context)`:

```typescript
// Example: FileRead defaults to allow (it's read-only)
checkPermissions: () => Promise.resolve({ behavior: 'allow' })

// Example: Bash checks if the command is read-only
checkPermissions: (input) => {
  if (isReadOnlyCommand(input.command)) {
    return { behavior: 'allow' }
  }
  return { behavior: 'askUser', message: `Run: ${input.command}` }
}
```

The result can be:
- `{ behavior: 'allow' }` — Approved
- `{ behavior: 'deny', message }` — Rejected with reason
- `{ behavior: 'askUser', message }` — Escalate to user prompt

---

## Layer 4: PreToolUse Hooks

User-defined scripts that run before tool execution. Configured in `settings.json` or CLAUDE.md:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "command": "/path/to/safety-check.sh"
      }
    ]
  }
}
```

Hook scripts receive the tool name and input as JSON on stdin. They can:
- **Approve** (exit 0, no output)
- **Deny** (exit non-zero, stderr has reason)
- **Modify input** (exit 0, stdout has modified JSON)

---

## Layer 5: Auto-Mode Classifier

In `--auto` mode, a **classifier** examines the conversation transcript to determine if a tool call is safe:

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#17a2b8', 'primaryBorderColor': '#17a2b8'}}}%%
flowchart TD
    TC["Tool call in auto-mode"]:::start

    BUILD["Build classifier input<br/>tool.toAutoClassifierInput(input)"]:::step
    TRANSCRIPT["Append recent transcript<br/>for context"]:::step
    CLASSIFY["Run safety classifier<br/>is this operation safe?"]:::step

    SAFE{"Classified as?"}:::check

    ALLOW["Auto-approved<br/>no user prompt"]:::allow
    PROMPT["Escalate to<br/>user dialog"]:::deny

    TC --> BUILD --> TRANSCRIPT --> CLASSIFY --> SAFE
    SAFE -->|"safe"| ALLOW
    SAFE -->|"unsafe"| PROMPT

    classDef start fill:#1a2d4a,stroke:#4a9eff,color:#e0e0e0,stroke-width:2px
    classDef step fill:#0d4f4f,stroke:#17a2b8,color:#e0e0e0,stroke-width:2px
    classDef check fill:#2d2d0d,stroke:#ffc107,color:#e0e0e0,stroke-width:2px
    classDef allow fill:#1b3a1b,stroke:#28a745,color:#e0e0e0,stroke-width:2px
    classDef deny fill:#4a1a1a,stroke:#dc3545,color:#e0e0e0,stroke-width:2px
```

Each tool provides `toAutoClassifierInput()` which returns a compact representation for the classifier. Security-irrelevant tools return `''` to skip classification.

---

## Layer 6: User Permission Dialog

The last resort — ask the human:

```
╭────────────────────────────────────────╮
│  Claude wants to run:                  │
│                                        │
│  $ npm install lodash                  │
│                                        │
│  (Y)es  ·  (n)o  ·  (a)lways allow    │
╰────────────────────────────────────────╯
```

Choosing "always allow" adds a permanent allow rule.

---

## Permission Modes

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#fd7e14', 'primaryBorderColor': '#fd7e14'}}}%%
flowchart TB
    START(["Session Start"]):::neutral --> DEFAULT

    DEFAULT["DEFAULT MODE<br/>Prompt user on every write tool"]:::mode1
    PLAN["PLAN MODE<br/>Read tools auto-approved<br/>Write tools require approval"]:::mode2
    AUTO["AUTO MODE<br/>Classifier decides safety<br/>Safe = allow, Unsafe = prompt"]:::mode3
    BYPASS["BYPASS MODE<br/>Everything auto-approved<br/>No permission checks"]:::mode4

    DEFAULT -->|"/plan command<br/>or model enters plan"| PLAN
    PLAN -->|"model exits<br/>plan mode"| DEFAULT
    DEFAULT -->|"--auto flag<br/>user opts in"| AUTO
    AUTO -->|"denial limit<br/>exceeded"| DEFAULT
    DEFAULT -->|"--dangerously-<br/>skip-permissions"| BYPASS

    classDef neutral fill:#333,stroke:#888,color:#e0e0e0,stroke-width:1px
    classDef mode1 fill:#1a2d4a,stroke:#4a9eff,color:#e0e0e0,stroke-width:2px
    classDef mode2 fill:#2d2d0d,stroke:#ffc107,color:#e0e0e0,stroke-width:2px
    classDef mode3 fill:#0d4f4f,stroke:#17a2b8,color:#e0e0e0,stroke-width:2px
    classDef mode4 fill:#4a1a1a,stroke:#dc3545,color:#e0e0e0,stroke-width:2px
```

### Default Mode
- Every write operation prompts the user
- Read operations (FileRead, Glob, Grep) auto-approved
- Most secure, most friction

### Plan Mode
- Entered via `/plan` command or model's `EnterPlanMode` tool
- All read tools auto-approved
- All write tools require explicit user approval
- Model can plan freely, execute cautiously

### Auto Mode
- Enabled via `--auto` flag
- Safety classifier decides per-tool
- Falls back to prompting if classifier says "unsafe"
- Has a **denial limit** — too many denials drops back to Default

### Bypass Mode
- Enabled via `--dangerously-skip-permissions`
- **Everything auto-approved** — no checks at all
- Named to be scary because it IS scary
- No permission system protection whatsoever

---

## The `ToolPermissionContext` Type

All permission state lives in `AppState.toolPermissionContext`:

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#dc3545', 'primaryBorderColor': '#dc3545'}}}%%
graph TB
    subgraph TPC["ToolPermissionContext — Immutable"]
        MODE["mode<br/>default / plan / auto / bypass"]
        AWD["additionalWorkingDirectories<br/>extra safe paths"]
        ALLOW["alwaysAllowRules<br/>by source: settings, command, etc."]
        DENY["alwaysDenyRules<br/>by source"]
        ASK["alwaysAskRules<br/>force prompt even if allowed"]
        BPM["isBypassPermissionsModeAvailable<br/>can user enable bypass?"]
        AUTO_A["isAutoModeAvailable<br/>can user enable auto?"]
        AVOID["shouldAvoidPermissionPrompts<br/>background agents that cannot show UI"]
        AWAIT["awaitAutomatedChecksBeforeDialog<br/>coordinator workers"]
        PRE["prePlanMode<br/>mode to restore after plan exits"]
    end
```

This is wrapped in `DeepImmutable<T>` — TypeScript enforces that nobody mutates this in place. Updates go through `setAppState(prev => ({ ...prev, toolPermissionContext: { ... } }))`.

---

## Denial Tracking

Auto mode tracks denials to prevent runaway unsafe operations:

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#dc3545', 'primaryBorderColor': '#dc3545'}}}%%
flowchart LR
    START["Auto mode active"]:::start
    D1["Denial 1"]:::deny
    D2["Denial 2"]:::deny
    DN["Denial N<br/>limit exceeded"]:::deny
    FALLBACK["Fall back to<br/>Default mode"]:::result

    START --> D1 --> D2 -->|"..."| DN --> FALLBACK

    classDef start fill:#0d4f4f,stroke:#17a2b8,color:#e0e0e0,stroke-width:2px
    classDef deny fill:#3d2b00,stroke:#fd7e14,color:#e0e0e0,stroke-width:2px
    classDef result fill:#1a2d4a,stroke:#4a9eff,color:#e0e0e0,stroke-width:2px
```

This is stored in `DenialTrackingState` — for async subagents that can't show UI, a local tracking copy is used since their `setAppState` is a no-op.

---

**Previous:** [← Tool System](./03-tool-system.md) · **Next:** [Context Management →](./05-context-management.md)

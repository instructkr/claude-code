# 3. Tool System

> How 42 built-in tools are defined, validated, orchestrated, and rendered.

---

## Overview

Every capability Claude Code has — reading files, running bash, editing code, searching the web — is a **Tool**. Tools are the bridge between the model's intentions and the real world.

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'primaryBorderColor': '#28a745', 'lineColor': '#28a745', 'secondaryColor': '#16213e', 'tertiaryColor': '#0f3460'}}}%%
graph TB
    subgraph Interface["Tool Interface — Tool.ts"]
        direction LR
        IS["inputSchema<br/>Zod validation"]
        CP["checkPermissions"]
        CALL["call — execute"]
        PROMPT["prompt — model instructions"]
        RENDER["render — terminal UI"]
    end

    IS --> CP --> CALL --> PROMPT --> RENDER

    subgraph FileOps["File Operations"]
        FR["FileRead"]
        FW["FileWrite"]
        FE["FileEdit"]
        GL["Glob"]
        GR["Grep"]
        NE["NotebookEdit"]
    end

    subgraph Exec["Execution"]
        BA["Bash"]
        PS["PowerShell"]
    end

    subgraph Web["Web"]
        WF["WebFetch"]
        WS["WebSearch"]
    end

    subgraph AgentTools["Agent and Task"]
        AG["Agent — spawn sub-agent"]
        TC["TaskCreate"]
        TG["TaskGet"]
        TU["TaskUpdate"]
        TL["TaskList"]
        TS["TaskStop"]
        SM["SendMessage"]
    end

    subgraph Meta["Meta Tools"]
        AQ["AskUserQuestion"]
        SK["SkillTool"]
        TW["TodoWrite"]
        EP["EnterPlanMode"]
        XP["ExitPlanMode"]
        TSR["ToolSearch"]
    end

    subgraph Dynamic["Dynamic — Runtime Loaded"]
        MCP_T["MCP Tools<br/>from external servers"]
        LSP_T["LSP Tool<br/>language server queries"]
    end

    subgraph Orchestration["Orchestration Layer"]
        RUN["toolOrchestration.ts<br/>runTools — parallel dispatch"]
        STE["StreamingToolExecutor<br/>execute as blocks stream in"]
        TEX["toolExecution.ts — 60KB<br/>single tool lifecycle"]
        THK["toolHooks.ts<br/>Pre/Post hook dispatch"]
    end

    Interface --> FileOps
    Interface --> Exec
    Interface --> Web
    Interface --> AgentTools
    Interface --> Meta
    Interface --> Dynamic

    FileOps --> Orchestration
    Exec --> Orchestration
    Web --> Orchestration
    AgentTools --> Orchestration
    Meta --> Orchestration
    Dynamic --> Orchestration

    RUN --> STE
    RUN --> TEX
    TEX --> THK
```

---

## The Tool Interface — `Tool.ts` (793 lines)

Every tool implements the `Tool<Input, Output, Progress>` type. Here are the key methods:

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#28a745', 'primaryBorderColor': '#28a745'}}}%%
flowchart LR
    subgraph Definition["Tool Definition"]
        NAME["name: string"]
        SCHEMA["inputSchema: Zod"]
        ALIASES["aliases?: string array"]
        HINT["searchHint?: string"]
    end

    subgraph Lifecycle["Lifecycle Methods"]
        VAL["validateInput<br/>pre-execution check"]
        PERM["checkPermissions<br/>allow / deny / prompt"]
        CALL["call<br/>execute the tool"]
        DESC["description<br/>model-facing summary"]
    end

    subgraph Rendering["Rendering Methods"]
        RUM["renderToolUseMessage<br/>show input in terminal"]
        RRM["renderToolResultMessage<br/>show output in terminal"]
        RPM["renderToolUseProgressMessage<br/>spinner / progress bar"]
        GRP["renderGroupedToolUse<br/>parallel display"]
    end

    subgraph Metadata["Metadata Methods"]
        RO["isReadOnly<br/>does it write?"]
        CS["isConcurrencySafe<br/>parallel safe?"]
        EN["isEnabled<br/>available now?"]
        DS["isDestructive<br/>irreversible?"]
        AC["toAutoClassifierInput<br/>safety classifier text"]
    end

    Definition --> Lifecycle --> Rendering
    Definition --> Metadata
```

### The `buildTool` Factory

All tools go through `buildTool()` which provides safe defaults:

```typescript
const TOOL_DEFAULTS = {
  isEnabled: () => true,
  isConcurrencySafe: () => false,    // Assume not safe
  isReadOnly: () => false,            // Assume writes
  isDestructive: () => false,
  checkPermissions: (input) =>        // Defer to general system
    Promise.resolve({ behavior: 'allow', updatedInput: input }),
  toAutoClassifierInput: () => '',    // Skip classifier
  userFacingName: () => '',
}

export function buildTool(def) {
  return { ...TOOL_DEFAULTS, userFacingName: () => def.name, ...def }
}
```

This "fail-closed" design means a tool that forgets to implement `isConcurrencySafe` defaults to `false` (not safe for parallel execution).

---

## The 42 Built-in Tools

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#17a2b8', 'primaryBorderColor': '#17a2b8'}}}%%
graph TB
    subgraph FileOps["File Operations — Read + Write + Search"]
        FR["FileRead<br/>Read files, images,<br/>PDFs, notebooks"]
        FW["FileWrite<br/>Create or overwrite<br/>entire files"]
        FE["FileEdit<br/>Partial string<br/>replacement edits"]
        GL["Glob<br/>File pattern<br/>matching search"]
        GR["Grep<br/>ripgrep content<br/>search"]
        NE["NotebookEdit<br/>Jupyter notebook<br/>cell editing"]
    end

    subgraph Exec["Execution — Run Commands"]
        BA["Bash<br/>Shell command<br/>execution"]
        PS["PowerShell<br/>Windows shell<br/>execution"]
        REPL["REPL<br/>Persistent JS/TS<br/>runtime context"]
    end

    subgraph Web["Web — Fetch and Search"]
        WF["WebFetch<br/>HTTP GET to URLs<br/>HTML to markdown"]
        WS["WebSearch<br/>Web search via<br/>Brave or similar"]
    end

    subgraph AgentTask["Agent and Task Management"]
        AG["Agent<br/>Spawn sub-agent<br/>with forked context"]
        TC["TaskCreate<br/>Background task"]
        TG["TaskGet<br/>Check task status"]
        TU["TaskUpdate<br/>Update task state"]
        TL["TaskList<br/>List all tasks"]
        TS["TaskStop<br/>Terminate task"]
        SM["SendMessage<br/>Inter-agent<br/>messaging"]
        TmC["TeamCreate<br/>Create agent team"]
        TmD["TeamDelete<br/>Remove agent team"]
    end

    subgraph Meta["Meta Tools — Control Claude's Behavior"]
        AQ["AskUserQuestion<br/>Interactive prompt"]
        SK["SkillTool<br/>Execute skills"]
        TW["TodoWrite<br/>Manage task lists"]
        EP["EnterPlanMode<br/>Switch to read-only"]
        XP["ExitPlanMode<br/>Resume full access"]
        TSR["ToolSearch<br/>Find deferred tools"]
        BF["Brief<br/>Toggle concise mode"]
        SL["Sleep<br/>Idle wait for<br/>proactive mode"]
        SO["SyntheticOutput<br/>Structured JSON<br/>output"]
    end

    subgraph Dynamic["Dynamic — Loaded at Runtime"]
        MCP["MCP Tools<br/>From external<br/>MCP servers"]
        LSP["LSP Tool<br/>Language server<br/>queries"]
    end

    subgraph Special["Special Purpose"]
        EW["EnterWorktree<br/>Git worktree<br/>isolation"]
        XW["ExitWorktree<br/>Leave worktree"]
        RT["RemoteTrigger<br/>Remote execution"]
        SC["ScheduleCron<br/>Timed triggers"]
        CF["Config<br/>Settings management"]
    end
```

---

## Tool Orchestration — Parallel Execution

When the model returns multiple `tool_use` blocks, Claude Code can execute them **in parallel**:

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#4a9eff', 'primaryBorderColor': '#4a9eff'}}}%%
sequenceDiagram
    participant Q as query.ts
    participant O as toolOrchestration.ts
    participant STE as StreamingToolExecutor
    participant T1 as Tool 1 — FileRead
    participant T2 as Tool 2 — Grep
    participant T3 as Tool 3 — Bash
    participant P as Permission System

    Q->>O: runTools(3 tool_use blocks)
    activate O

    Note over O: Check concurrency safety

    O->>STE: FileRead — isConcurrencySafe = true
    O->>STE: Grep — isConcurrencySafe = true
    O->>STE: Bash — isConcurrencySafe = false

    par Parallel Execution
        STE->>P: checkPermissions(FileRead)
        P-->>STE: allow
        STE->>T1: call(input)
        T1-->>STE: result

        STE->>P: checkPermissions(Grep)
        P-->>STE: allow
        STE->>T2: call(input)
        T2-->>STE: result
    end

    Note over STE: Wait for parallel tools

    STE->>P: checkPermissions(Bash)
    P-->>STE: prompt user
    STE->>T3: call(input)
    T3-->>STE: result

    O-->>Q: yield all tool_result messages
    deactivate O
```

Key files in the orchestration layer:

- **`toolOrchestration.ts`** — `runTools()`: dispatches tools, handles parallel vs. sequential
- **`StreamingToolExecutor`** — Starts permission checks while model is still streaming
- **`toolExecution.ts`** (60KB) — Single tool lifecycle: validate → permissions → execute → hooks
- **`toolHooks.ts`** — Dispatches PreToolUse and PostToolUse hooks

---

## Single Tool Lifecycle

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#28a745', 'primaryBorderColor': '#28a745'}}}%%
flowchart TD
    BLOCK["tool_use block arrives<br/>from model stream"]:::start

    PARSE["Parse + validate input<br/>via Zod inputSchema"]:::step
    VAL{"validateInput?"}:::check

    DENY_VAL["Return error to model<br/>with validation message"]:::deny

    PRE_HOOK["Run PreToolUse hooks<br/>user-defined scripts"]:::hook
    HOOK_R{"Hook result?"}:::check

    PERM["Check permissions<br/>deny → allow → tool → hooks → classifier → dialog"]:::step
    PERM_R{"Permission?"}:::check

    EXEC["tool.call(input, context)<br/>execute the operation"]:::step
    RESULT["Map output to tool_result<br/>via mapToolResultToToolResultBlockParam"]:::step

    SIZE{"Result exceeds<br/>maxResultSizeChars?"}:::check
    PERSIST["Persist to disk<br/>return file path + preview"]:::step

    POST_HOOK["Run PostToolUse hooks"]:::hook
    RENDER["Render in terminal<br/>renderToolResultMessage"]:::step

    YIELD["Yield tool_result<br/>to query loop"]:::done

    DENY_PERM["Return permission_denied<br/>error to model"]:::deny

    BLOCK --> PARSE --> VAL
    VAL -->|"pass"| PRE_HOOK
    VAL -->|"fail"| DENY_VAL

    PRE_HOOK --> HOOK_R
    HOOK_R -->|"approve"| PERM
    HOOK_R -->|"deny"| DENY_PERM
    HOOK_R -->|"modify input"| PERM

    PERM --> PERM_R
    PERM_R -->|"allow"| EXEC
    PERM_R -->|"deny"| DENY_PERM

    EXEC --> RESULT --> SIZE
    SIZE -->|"within limit"| POST_HOOK
    SIZE -->|"exceeds limit"| PERSIST --> POST_HOOK

    POST_HOOK --> RENDER --> YIELD

    classDef start fill:#1a2d4a,stroke:#4a9eff,color:#e0e0e0,stroke-width:2px
    classDef step fill:#1b3a1b,stroke:#28a745,color:#e0e0e0,stroke-width:2px
    classDef check fill:#2d2d0d,stroke:#ffc107,color:#e0e0e0,stroke-width:2px
    classDef hook fill:#3d2b00,stroke:#fd7e14,color:#e0e0e0,stroke-width:2px
    classDef deny fill:#4a1a1a,stroke:#dc3545,color:#e0e0e0,stroke-width:2px
    classDef done fill:#0d4f4f,stroke:#17a2b8,color:#e0e0e0,stroke-width:2px
```

---

## ToolSearch — Deferred Tool Loading

With 42+ tools, sending all schemas to the model wastes tokens. **ToolSearch** defers tools that aren't immediately needed:

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#ffc107', 'primaryBorderColor': '#ffc107'}}}%%
flowchart LR
    ALL["42+ Tools"]:::input

    SPLIT{"shouldDefer?"}:::check

    EAGER["~15 Eager Tools<br/>Always in prompt<br/>FileRead, Bash, Grep..."]:::eager
    DEFER["~27 Deferred Tools<br/>Schema not sent initially<br/>TaskCreate, WebSearch..."]:::defer
    ALWAYS["alwaysLoad Tools<br/>Forced eager by MCP meta"]:::eager

    SEARCH["ToolSearch Tool<br/>Model searches by keyword<br/>using searchHint"]:::tool

    FOUND["Tool schema injected<br/>into next request"]:::result

    ALL --> SPLIT
    SPLIT -->|"no"| EAGER
    SPLIT -->|"yes"| DEFER
    SPLIT -->|"alwaysLoad"| ALWAYS

    DEFER --> SEARCH
    SEARCH --> FOUND

    classDef input fill:#1a2d4a,stroke:#4a9eff,color:#e0e0e0,stroke-width:2px
    classDef check fill:#2d2d0d,stroke:#ffc107,color:#e0e0e0,stroke-width:2px
    classDef eager fill:#1b3a1b,stroke:#28a745,color:#e0e0e0,stroke-width:2px
    classDef defer fill:#3d2b00,stroke:#fd7e14,color:#e0e0e0,stroke-width:2px
    classDef tool fill:#2d1b4e,stroke:#6f42c1,color:#e0e0e0,stroke-width:2px
    classDef result fill:#0d4f4f,stroke:#17a2b8,color:#e0e0e0,stroke-width:2px
```

---

## Dynamic Tools — MCP and LSP

Beyond built-in tools, Claude Code loads tools dynamically at runtime:

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#17a2b8', 'primaryBorderColor': '#17a2b8'}}}%%
flowchart TD
    subgraph MCP["MCP Tools — Model Context Protocol"]
        SRV["External MCP Servers<br/>configured in settings"]
        CONN["MCPConnectionManager<br/>stdio / SSE transport"]
        DISC["Discover tools<br/>via tools/list"]
        WRAP["Wrap as Tool objects<br/>name: mcp__server__tool"]
    end

    subgraph LSP["LSP Tool — Language Server Protocol"]
        LS["Language Server<br/>runtime type info"]
        QUERY_LSP["Query definitions,<br/>references, diagnostics"]
    end

    SRV --> CONN --> DISC --> WRAP
    LS --> QUERY_LSP

    MERGE["Merged into tool pool<br/>via useMergedTools hook"]:::merge

    WRAP --> MERGE
    QUERY_LSP --> MERGE

    classDef merge fill:#1b3a1b,stroke:#28a745,color:#e0e0e0,stroke-width:2px
```

MCP tools are prefixed with `mcp__<server>__<tool>` unless running in SDK no-prefix mode. They go through the same permission system as built-in tools.

---

## Key Design Decisions

### 1. Self-Contained Modules

Each tool directory (`src/tools/<ToolName>/`) contains everything:
- `index.ts` — Tool definition via `buildTool()`
- `prompt.ts` — Model-facing instructions
- `*.test.ts` — Tests
- Additional helpers as needed

### 2. Fail-Closed Defaults

`buildTool()` defaults are conservative:
- `isConcurrencySafe = false` — Won't run in parallel unless explicitly safe
- `isReadOnly = false` — Assumed to write unless stated otherwise
- `checkPermissions` defaults to `allow` — But the general permission system still applies

### 3. Result Size Budgets

Each tool has `maxResultSizeChars`. Oversized results are persisted to disk and the model gets a truncated preview + file path. This prevents single tool results from consuming the entire context window.

### 4. Observable Input Backfilling

`backfillObservableInput()` adds derived fields to tool inputs for SDK consumers and transcripts, without mutating the API-bound input (which would break prompt caching):

```typescript
// The API sees: { file_path: "src/foo.ts" }
// SDK/transcript sees: { file_path: "src/foo.ts", resolved_path: "/abs/path/src/foo.ts" }
```

---

**Previous:** [← The Agentic Loop](./02-agentic-loop.md) · **Next:** [Permission System →](./04-permission-system.md)

# 6. State Management

> A single immutable store with 50+ fields — how Claude Code manages application state.

---

## Architecture

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#4a9eff', 'primaryBorderColor': '#4a9eff'}}}%%
graph TB
    subgraph Store["AppState — Single Immutable Store"]
        direction TB
        subgraph Core_S["Core Session State"]
            Model["mainLoopModel"]
            Think["thinkingEnabled"]
            Fast["fastMode"]
            Effort["effortValue"]
            Settings["settings: SettingsJson"]
        end

        subgraph Perm_S["Permission State"]
            TPC["toolPermissionContext"]
            Mode["mode: default / plan / auto / bypass"]
            Allow["alwaysAllowRules"]
            Deny["alwaysDenyRules"]
        end

        subgraph MCP_S["MCP State"]
            MCPCli["clients: MCPServerConnection array"]
            MCPTool["tools: Tool array"]
            MCPCmd["commands: Command array"]
        end

        subgraph Task_S["Background Tasks"]
            Tasks["tasks: taskId to TaskState map"]
            AgentReg["agentNameRegistry"]
            FG["foregroundedTaskId"]
        end

        subgraph UI_S["UI State"]
            Spec["speculation: predictive execution"]
            Suggest["promptSuggestion: autocomplete"]
            Notif["notifications queue"]
            Bridge["replBridge: remote control state"]
        end

        subgraph History_S["History and Tracking"]
            FH["fileHistory: snapshots for rewind"]
            Attr["attribution: commit metadata"]
            Todos["todos: per-agent lists"]
        end
    end

    REPL_C["REPL.tsx<br/>reads + subscribes"]:::consumer
    QE_C["QueryEngine<br/>reads via getAppState"]:::consumer
    Tools_C["Tools<br/>reads via ToolUseContext"]:::consumer

    SET["setAppState<br/>functional update"]:::mutator
    ONCHANGE["onChangeAppState<br/>side effect reactions"]:::mutator

    Store --> REPL_C
    Store --> QE_C
    Store --> Tools_C

    SET --> Store
    ONCHANGE --> Store

    classDef consumer fill:#1b3a1b,stroke:#28a745,color:#e0e0e0,stroke-width:2px
    classDef mutator fill:#2d1b4e,stroke:#e83e8c,color:#e0e0e0,stroke-width:2px
```

---

## Core Concepts

### Immutability via `DeepImmutable<T>`

The `AppState` type is wrapped in `DeepImmutable<T>` — TypeScript enforces that no consumer can mutate state in place:

```typescript
export type AppState = DeepImmutable<{
  settings: SettingsJson
  mainLoopModel: ModelSetting
  toolPermissionContext: ToolPermissionContext
  // ... 50+ fields
}>
```

### Functional Updates

State is updated via `setAppState(prev => newState)`:

```typescript
setAppState(prev => ({
  ...prev,
  toolPermissionContext: {
    ...prev.toolPermissionContext,
    mode: 'plan',
  },
}))
```

### Side Effects via `onChangeAppState`

After state changes, `onChangeAppState.ts` fires reactive side effects — persisting settings, updating UI, notifying remote sessions, etc.

---

## Key State Groups

### Session State
`mainLoopModel`, `thinkingEnabled`, `fastMode`, `effortValue` — control how the model behaves each turn.

### Permission State
`toolPermissionContext` — contains mode, allow/deny rules, and bypass availability. See [Guide 4](./04-permission-system.md).

### MCP State
`mcp.clients`, `mcp.tools`, `mcp.commands` — dynamically connected MCP servers and their exposed tools/commands.

### Background Tasks
`tasks` — a map of `taskId → TaskState` for background agent tasks. `foregroundedTaskId` controls which task's messages appear in the main view.

### UI State
`speculation` — predictive execution state for pre-computing responses. `promptSuggestion` — autocomplete suggestions. `notifications` — queued UI notifications.

### History
`fileHistory` — snapshots for `/rewind`. `attribution` — commit metadata for git attribution. `todos` — per-agent task lists.

---

## Key Files

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#4a9eff', 'primaryBorderColor': '#4a9eff'}}}%%
graph LR
    subgraph StateDir["src/state/"]
        AS["AppState.tsx<br/>React context + hooks"]
        ASS["AppStateStore.ts<br/>Type definition + defaults"]
        OC["onChangeAppState.ts<br/>Side effect reactions"]
        SEL["selectors.ts<br/>Derived state"]
        ST["store.ts<br/>Store type"]
    end
```

---

**Previous:** [← Context Management](./05-context-management.md) · **Next:** [Extension Model →](./07-extension-model.md)

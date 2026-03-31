# 8. API Client — `claude.ts`

> Streaming, retries, caching, and model fallback — how Claude Code talks to the Anthropic API.

---

## Request Lifecycle

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#28a745', 'actorTextColor': '#e0e0e0', 'actorBorder': '#28a745', 'signalColor': '#28a745', 'noteBkgColor': '#16213e', 'noteTextColor': '#e0e0e0', 'activationBkgColor': '#1b3a1b', 'activationBorderColor': '#28a745'}}}%%
sequenceDiagram
    participant Q as query.ts
    participant C as claude.ts
    participant R as withRetry
    participant K as AnthropicClient
    participant A as Anthropic API

    Q->>C: queryModel(messages, tools, options)
    activate C

    C->>C: resolve model — runtime override, plan-mode swap
    C->>C: normalize messages — strip internal fields
    C->>C: build tool schemas — filter by deny, defer via ToolSearch
    C->>C: configure betas, cache_control, effort, task_budget
    C->>C: add prompt cache breakpoints
    C->>C: compute metadata — user_id, session_id, device_id

    C->>R: withRetry(clientFactory, requestFn)
    activate R

    loop Retry on 429, 529, timeouts
        R->>K: getAnthropicClient(apiKey, model)
        K->>A: beta.messages.stream(params)
        activate A

        alt 200 OK
            A-->>R: SSE event stream
        else 429 Rate Limited
            R->>R: exponential backoff
        else 529 Overloaded
            R->>R: backoff + optional model fallback
        else 401 Auth Error
            R-->>C: CannotRetryError — abort
        end
        deactivate A
    end

    deactivate R

    C->>C: parse stream into AssistantMessage
    C->>C: update usage tracking and cost
    C->>C: detect prompt cache breaks
    C-->>Q: yield AssistantMessage + StreamEvents
    deactivate C
```

---

## Request Building

Before each API call, `claude.ts` builds the request through several steps:

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#28a745', 'primaryBorderColor': '#28a745'}}}%%
flowchart TD
    subgraph ModelRes["1. Model Resolution"]
        RUNTIME["Runtime override<br/>from AppState"]
        PLAN_SWAP["Plan-mode model<br/>swap for 200K+ contexts"]
        FALLBACK_M["Fallback model<br/>on 529 overload"]
    end

    subgraph MsgNorm["2. Message Normalization"]
        STRIP["Strip internal fields<br/>uuid, timestamp, etc."]
        THINKING["Preserve thinking blocks<br/>within trajectory boundaries"]
        SIGNS["Strip signature blocks"]
    end

    subgraph ToolBuild["3. Tool Schema Building"]
        FILTER_DENY["Filter denied tools"]
        DEFER["Defer tools via<br/>ToolSearch deferred loading"]
        EAGER["Eager tools always<br/>in prompt"]
    end

    subgraph Config["4. Request Configuration"]
        BETAS["Beta features<br/>prompt caching, token counting"]
        CACHE_CTL["cache_control breakpoints<br/>system prompt caching"]
        EFFORT_V["effort parameter<br/>controls thinking depth"]
        TASK_BUD["task_budget<br/>agentic turn spend limit"]
        METADATA["metadata<br/>user_id, session_id"]
    end

    ModelRes --> MsgNorm --> ToolBuild --> Config

    API_REQ["POST /v1/messages<br/>SSE stream"]:::api
    Config --> API_REQ

    classDef api fill:#1b3a1b,stroke:#28a745,color:#e0e0e0,stroke-width:2px
```

---

## Retry Logic — `withRetry`

The retry wrapper handles transient API failures:

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#dc3545', 'primaryBorderColor': '#dc3545'}}}%%
flowchart TD
    REQUEST["API Request"]:::start

    RESPONSE{"Response<br/>status?"}:::check

    OK["200 OK<br/>Stream response"]:::success
    RATE["429 Rate Limited"]:::error
    OVER["529 Overloaded"]:::error
    AUTH["401 Auth Error"]:::fatal
    TIMEOUT["Timeout"]:::error

    BACKOFF["Exponential backoff<br/>wait and retry"]:::retry
    FALLBACK_SWITCH["Switch to fallback model<br/>if configured"]:::retry
    ABORT["CannotRetryError<br/>abort immediately"]:::fatal

    REQUEST --> RESPONSE
    RESPONSE -->|"200"| OK
    RESPONSE -->|"429"| RATE --> BACKOFF --> REQUEST
    RESPONSE -->|"529"| OVER --> FALLBACK_SWITCH --> REQUEST
    RESPONSE -->|"401"| AUTH --> ABORT
    RESPONSE -->|"timeout"| TIMEOUT --> BACKOFF

    classDef start fill:#1a2d4a,stroke:#4a9eff,color:#e0e0e0,stroke-width:2px
    classDef check fill:#2d2d0d,stroke:#ffc107,color:#e0e0e0,stroke-width:2px
    classDef success fill:#1b3a1b,stroke:#28a745,color:#e0e0e0,stroke-width:2px
    classDef error fill:#3d2b00,stroke:#fd7e14,color:#e0e0e0,stroke-width:2px
    classDef fatal fill:#4a1a1a,stroke:#dc3545,color:#e0e0e0,stroke-width:2px
    classDef retry fill:#2d1b4e,stroke:#6f42c1,color:#e0e0e0,stroke-width:2px
```

### Streaming Fallback

A unique feature: if the model is overloaded mid-stream (529), Claude Code can:
1. **Tombstone** the partial assistant messages
2. Switch to a fallback model
3. Restart the stream from scratch
4. The user sees no interruption — orphaned messages are removed from UI

---

## Prompt Caching

Claude Code uses Anthropic's prompt cache to avoid re-processing unchanged context:

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#4a9eff', 'primaryBorderColor': '#4a9eff'}}}%%
flowchart LR
    SYS["System prompt<br/>cache_control breakpoint"]:::cached
    TOOLS["Tool schemas<br/>cache_control breakpoint"]:::cached
    HISTORY["Conversation history<br/>bytes must match exactly"]:::uncached

    API["API Request"]:::api

    HIT["Cache HIT<br/>~90% cheaper<br/>~10x faster"]:::hit
    MISS["Cache MISS<br/>full processing<br/>new cache created"]:::miss

    SYS --> API
    TOOLS --> API
    HISTORY --> API

    API --> HIT
    API --> MISS

    classDef cached fill:#1b3a1b,stroke:#28a745,color:#e0e0e0,stroke-width:2px
    classDef uncached fill:#333,stroke:#888,color:#e0e0e0,stroke-width:1px
    classDef api fill:#1a2d4a,stroke:#4a9eff,color:#e0e0e0,stroke-width:2px
    classDef hit fill:#0d4f4f,stroke:#17a2b8,color:#e0e0e0,stroke-width:2px
    classDef miss fill:#3d2b00,stroke:#fd7e14,color:#e0e0e0,stroke-width:2px
```

Cache breaks are detected and logged. The `backfillObservableInput()` pattern exists specifically to avoid breaking the cache — the original API-bound input is never mutated.

---

## SSE Stream Events

The API returns Server-Sent Events in this order:

```mermaid
%%{init: {'theme': 'dark', 'themeVariables': { 'primaryColor': '#1a1a2e', 'primaryTextColor': '#e0e0e0', 'lineColor': '#4a9eff', 'primaryBorderColor': '#4a9eff'}}}%%
sequenceDiagram
    participant API as Anthropic API
    participant C as claude.ts

    API->>C: message_start — model, usage, id
    
    loop For each content block
        API->>C: content_block_start — type, index
        loop Delta events
            API->>C: content_block_delta — text / thinking / tool_use JSON
        end
        API->>C: content_block_stop
    end

    API->>C: message_delta — stop_reason, final usage
    API->>C: message_stop

    Note over C: Parse into AssistantMessage<br/>Track usage + cost<br/>Yield to query.ts
```

---

## Cost Tracking

Every API call's usage is tracked in `cost-tracker.ts`:
- Input tokens (including cache reads/writes)
- Output tokens
- Per-model pricing
- Session totals exposed via `/cost` command

---

**Previous:** [← Extension Model](./07-extension-model.md) · **Next:** [UI Architecture →](./09-ui-architecture.md)

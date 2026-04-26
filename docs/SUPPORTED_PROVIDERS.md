# Supported Providers

claw-code currently supports the following LLM providers. This is a snapshot of the current code state and may change. The canonical source of truth is `MODEL_REGISTRY` and provider routing logic in `rust/crates/api/src/providers/mod.rs`.

> **Note:** A declarative `providers` / `models` / `websearch` config in `settings.json` is tracked as pinpoint #285 and is not yet implemented. Until then, provider/model selection is determined by:
> 1. The model name prefix (e.g., `claude-`, `grok-`, `openai/`, `qwen/`, `kimi-`)
> 2. Environment variables (e.g., `ANTHROPIC_API_KEY`, `XAI_API_KEY`, `DASHSCOPE_API_KEY`, `OPENAI_API_KEY`)
> 3. Hard-coded heuristics in `MODEL_REGISTRY` and `detect_provider_kind()`

## Anthropic

- **Status:** Primary supported provider
- **Models:**
  - `claude-opus-4-6` (alias: `opus`) — 200K context, 32K max output
  - `claude-sonnet-4-6` (alias: `sonnet`) — 200K context, 64K max output
  - `claude-haiku-4-5-20251213` (alias: `haiku`) — 200K context, 64K max output
- **Auth:** `ANTHROPIC_API_KEY` env var, or OAuth bearer via `claw login` (`ANTHROPIC_AUTH_TOKEN`)
- **Base URL:** `https://api.anthropic.com` (override: `ANTHROPIC_BASE_URL`)
- **Known issues:** Subject to upstream stream-init failures (see #290, #291)

## xAI (Grok)

- **Status:** Supported via OpenAI-compatible client
- **Models:**
  - `grok-3` (aliases: `grok`, `grok-3`) — 131K context, 64K max output
  - `grok-3-mini` (aliases: `grok-mini`, `grok-3-mini`) — 131K context, 64K max output
  - `grok-2` — context/output limits not yet registered in token metadata
- **Auth:** `XAI_API_KEY`
- **Base URL:** `https://api.x.ai/v1` (override: `XAI_BASE_URL`)
- **Known issues:** None currently tracked

## Alibaba DashScope (Qwen / Kimi)

- **Status:** Supported via OpenAI-compatible client pointed at DashScope compatible-mode endpoint
- **Models:**
  - `qwen/*` and `qwen-*` prefix — routes to DashScope (e.g., `qwen-plus`, `qwen-max`, `qwen-turbo`, `qwen/qwen3-coder`)
  - `kimi-k2.5` (alias: `kimi`) — 256K context, 16K max output
  - `kimi-k1.5` — 256K context, 16K max output
  - `kimi/*` and `kimi-*` prefix — routes to DashScope
- **Auth:** `DASHSCOPE_API_KEY`
- **Base URL:** `https://dashscope.aliyuncs.com/compatible-mode/v1` (override: `DASHSCOPE_BASE_URL`)
- **Known issues:** None currently tracked

## OpenAI / OpenAI-Compatible Endpoints

- **Status:** Supported via OpenAI-compatible client; also covers local providers (Ollama, LM Studio, vLLM, OpenRouter)
- **Models:** `openai/` prefix (e.g., `openai/gpt-4.1-mini`) or bare `gpt-*` prefix
- **Auth:** `OPENAI_API_KEY`
- **Base URL:** `https://api.openai.com/v1` (override: `OPENAI_BASE_URL` — also used for local providers)
- **Local provider routing:** When `OPENAI_BASE_URL` is set and `OPENAI_API_KEY` is present, unknown model names (e.g., `qwen2.5-coder:7b`) also route here
- **Known issues:** Declarative per-model config tracked in #285

## Web Search

- **Status:** Hard-coded heuristics; declarative `websearch` config tracked in #285

## Provider Selection Order

When the model name has no recognized prefix, `detect_provider_kind()` falls through in this order:

1. Model prefix match (`claude-` → Anthropic, `grok-` → xAI, `openai/` or `gpt-` → OpenAI, `qwen/` or `qwen-` → DashScope, `kimi/` or `kimi-` → DashScope)
2. `OPENAI_BASE_URL` + `OPENAI_API_KEY` set → OpenAI-compat
3. Anthropic credentials found → Anthropic
4. `OPENAI_API_KEY` found → OpenAI
5. `XAI_API_KEY` found → xAI
6. `OPENAI_BASE_URL` set (no key) → OpenAI-compat (for keyless local providers)
7. Default fallback → Anthropic

## Reporting Provider Issues

For provider-specific bugs (e.g., `500 empty_stream` from upstream), see [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for mitigation steps.

For pinpointing a missing provider feature, file via [ISSUE_TEMPLATE/pinpoint.md](../.github/ISSUE_TEMPLATE/pinpoint.md).

## Related Pinpoints

- #245 — Provider declarative config
- #246 — Backend swap
- #285 — Provider/model/websearch source of truth
- #290 — Stream-init failure envelope
- #291 — Repeat-failure circuit-breaker

# MCP/LSP 프로토콜 연동 분석

> **레벨**: 3 — 내부 구현 심층 분석
> **대상 독자**: Claude Code 내부 아키텍처를 이해하고자 하는 엔지니어, 프로토콜 통합 방식을 연구하는 개발자
> **전제 지식**: TypeScript, JSON-RPC 2.0, MCP 사양(modelcontextprotocol.io), LSP 사양(microsoft.github.io/language-server-protocol)

---

## 1. 개요

Claude Code는 두 가지 독립적인 확장 프로토콜을 병렬로 운영한다.

| 프로토콜 | 목적 | 방향성 | 주요 표준 |
|----------|------|--------|-----------|
| **MCP** (Model Context Protocol) | AI 모델에 외부 도구·리소스·프롬프트 제공 | 양방향 (클라이언트 ↔ 서버) | Anthropic MCP Spec 2025-03-26 |
| **LSP** (Language Server Protocol) | 코드 인텔리전스 (정의, 참조, 진단 등) | 클라이언트 → 서버 (단방향 요청 + 서버 알림) | Microsoft LSP 3.17 |

MCP는 Claude 모델이 사용할 수 있는 **도구(Tool)** 를 외부 서버로부터 동적으로 수급하는 메커니즘이며, LSP는 파일 편집·분석 작업 시 언어 서버로부터 **코드 인텔리전스**를 얻기 위한 메커니즘이다. 두 프로토콜은 서로 독립적으로 초기화되며, 각각 별도의 관리자(Manager) 싱글턴을 통해 생명주기가 제어된다.

```
┌─────────────────────────────────────────┐
│              Claude Code                │
│                                         │
│  ┌───────────────┐  ┌────────────────┐  │
│  │  MCP 클라이언트  │  │  LSP 관리자    │  │
│  │  (mcpClient)  │  │  (manager.ts)  │  │
│  └──────┬────────┘  └───────┬────────┘  │
│         │                   │           │
│  ┌──────▼────────┐  ┌───────▼────────┐  │
│  │   MCPTool     │  │   LSPTool      │  │
│  │ (Tool 매핑)   │  │ (Tool 래퍼)    │  │
│  └───────────────┘  └────────────────┘  │
└─────────────────────────────────────────┘
         │                   │
  MCP 서버들           LSP 서버들 (플러그인 제공)
  (stdio/SSE/HTTP/WS)  (stdio, vscode-jsonrpc)
```

---

## 2. MCP (Model Context Protocol) 통합

### 2.1 서버 설정 및 스코프 계층

MCP 서버 설정은 `src/services/mcp/types.ts`에 Zod 스키마로 엄밀하게 정의된다. 설정은 **스코프(scope)** 개념을 통해 계층화된다.

```typescript
// ConfigScopeSchema — 우선순위 낮은 순서
type ConfigScope =
  | 'enterprise'   // 관리형 정책 파일 (최고 우선순위)
  | 'user'         // 사용자 전역 설정
  | 'project'      // .mcp.json (프로젝트별)
  | 'local'        // 로컬 오버라이드
  | 'dynamic'      // 런타임 동적 추가
  | 'claudeai'     // claude.ai 클라우드 서버
  | 'managed'      // 플러그인 제공 서버
```

각 서버 항목은 `ScopedMcpServerConfig` 타입으로 래핑되어 원본 설정과 스코프가 함께 보존된다. `config.ts`의 `getAllMcpConfigs()`가 모든 소스를 병합하여 최종 서버 목록을 반환한다.

프로젝트 수준 설정은 `.mcp.json` 파일에 저장된다. `writeMcpjsonFile()` 함수는 원자적 파일 쓰기를 구현한다 — 임시 파일에 기록(`O_WRONLY`), `datasync()`로 디스크 플러시, `rename()`으로 원자적 교체. 이는 설정 파일 손상을 방지하기 위한 의도적인 설계다.

### 2.2 전송 계층 (Transport Layer)

MCP는 여섯 가지 전송 유형을 지원하며, 각각 독립적인 연결 전략을 사용한다.

```
TransportType:
  stdio        — 서브프로세스 stdin/stdout (로컬 서버, 가장 일반적)
  sse          — Server-Sent Events over HTTP (원격, OAuth 지원)
  sse-ide      — IDE 확장용 SSE (인증 없음)
  http         — Streamable HTTP (MCP Spec 2025-03-26)
  ws           — WebSocket
  ws-ide       — IDE 확장용 WebSocket (authToken 지원)
  sdk          — 인프로세스 (InProcessTransport)
  claudeai-proxy — claude.ai 프록시 서버
```

`connectToServer()` 함수(`client.ts:595`)는 `memoize`로 래핑되어 동일한 서버에 대한 중복 연결을 방지한다. 캐시 키는 서버 이름과 설정 JSON의 조합(`getServerCacheKey()`)으로 생성된다.

**연결 타임아웃**: 기본 30초(`MCP_TIMEOUT` 환경변수로 조정 가능). `Promise.race()`를 통해 연결과 타임아웃 프로미스를 경쟁시키며, 타임아웃 발생 시 전송 계층을 강제 종료한다.

**배치 연결**: 로컬 서버(stdio/sdk)는 동시 3개, 원격 서버(SSE/HTTP/WS)는 동시 20개까지 병렬 연결한다(`pMap` 활용). 환경변수 `MCP_SERVER_CONNECTION_BATCH_SIZE`와 `MCP_REMOTE_SERVER_CONNECTION_BATCH_SIZE`로 조정 가능.

#### HTTP 전송의 특수 처리

Streamable HTTP(`type: 'http'`)는 MCP 사양 2025-03-26을 따른다. POST 요청에는 반드시 `Accept: application/json, text/event-stream` 헤더가 포함되어야 한다. `wrapFetchWithTimeout()` 함수가 이 헤더를 보장하며, 동시에 60초 타임아웃을 적용한다.

```typescript
// GET 요청(SSE 스트림)은 타임아웃에서 제외 — 장시간 유지되는 연결이기 때문
if (method === 'GET') {
  return baseFetch(url, init)
}
```

`AbortSignal.timeout()` 대신 `setTimeout` + `clearTimeout` 패턴을 사용한다. 이는 Bun 런타임에서 `AbortSignal.timeout()`이 GC 전까지 요청당 ~2.4KB의 네이티브 메모리를 누수하는 버그를 회피하기 위함이다.

#### 인프로세스 서버

Chrome과 Computer Use 서버는 서브프로세스 대신 **인프로세스**로 실행된다. `createLinkedTransportPair()`로 연결된 `InProcessTransport` 쌍을 통해 통신하며, ~325MB의 서브프로세스 오버헤드를 제거한다.

### 2.3 리소스 및 프롬프트

MCP 서버가 제공하는 **리소스(Resource)** 와 **프롬프트(Prompt)** 는 각각 `ListMcpResourcesTool`과 `ReadMcpResourceTool`을 통해 Claude에 노출된다.

```typescript
type ServerResource = Resource & { server: string }
// server 필드를 추가하여 리소스 출처 서버를 식별
```

`MCPCliState` 인터페이스는 연결된 클라이언트, 설정, 도구, 리소스, 정규화된 이름 매핑을 하나의 직렬화 가능한 상태로 묶는다. CLI 세션 간 상태 전달을 위한 구조다.

### 2.4 Tool 매핑

MCP 서버가 제공하는 도구는 `MCPTool` 정적 템플릿을 기반으로 **동적으로 생성**된다. `MCPTool.ts`의 정적 정의는 모두 기본값(빈 이름, 빈 설명)이며, `mcpClient.ts`에서 실제 서버의 도구 메타데이터로 오버라이드된다.

**도구 이름 정규화**: MCP 도구는 `mcp__{서버명}__{도구명}` 형식의 이름을 갖는다. `normalizeNameForMCP()` 함수가 API 패턴 `^[a-zA-Z0-9_-]{1,64}$`에 맞지 않는 문자를 언더스코어로 치환한다.

```typescript
// normalization.ts
export function normalizeNameForMCP(name: string): string {
  let normalized = name.replace(/[^a-zA-Z0-9_-]/g, '_')
  if (name.startsWith('claude.ai ')) {
    // claude.ai 서버: 연속 언더스코어 축소, 선두/말미 언더스코어 제거
    // __ 구분자와의 충돌 방지
    normalized = normalized.replace(/_+/g, '_').replace(/^_|_$/g, '')
  }
  return normalized
}
```

`MCPCliState.normalizedNames`는 정규화된 이름과 원본 이름 간의 역방향 매핑을 보존한다.

**도구 설명 길이 제한**: OpenAPI 기반 MCP 서버가 15~60KB의 설명을 주입하는 사례가 관찰됨에 따라, `MAX_MCP_DESCRIPTION_LENGTH = 2048` 상수로 잘라낸다.

**IDE 도구 필터링**: `mcp__ide__` 접두사를 가진 도구는 화이트리스트(`mcp__ide__executeCode`, `mcp__ide__getDiagnostics`)에 있는 것만 포함된다.

**도구 호출 타임아웃**: 기본값 `100_000_000ms` (~27.8시간, 사실상 무한대). `MCP_TOOL_TIMEOUT` 환경변수로 조정 가능.

### 2.5 OAuth 인증 통합

MCP OAuth 흐름은 `src/services/mcp/auth.ts`의 `ClaudeAuthProvider`가 담당한다. `@modelcontextprotocol/sdk/client/auth.js`의 `OAuthClientProvider` 인터페이스를 구현한다.

```
OAuth 인증 흐름:
1. 서버 연결 시도 → UnauthorizedError 발생
2. handleRemoteAuthFailure() → needs-auth 상태로 전환
3. 인증 캐시 확인 (isMcpAuthCached, TTL 15분)
4. ClaudeAuthProvider.auth() → 브라우저에서 인증 페이지 열기
5. PKCE 코드 챌린지 생성 (SHA-256)
6. 콜백 서버 시작 (임의 포트) → 인가 코드 수신
7. 토큰 교환 → Keychain 저장
8. 재연결 시도
```

**민감한 OAuth 파라미터 로그 보호**: `SENSITIVE_OAUTH_PARAMS`(`state`, `nonce`, `code_challenge`, `code_verifier`, `code`)는 로그 출력 전 `[REDACTED]`로 치환된다.

**Slack 비표준 에러 정규화**: Slack의 `invalid_refresh_token`, `expired_refresh_token`, `token_expired`는 RFC 6749 표준 `invalid_grant`로 정규화된다.

**XAA (Cross-App Access, SEP-990)**: 서버별 `xaa: true` 플래그를 통해 IdP 연동 토큰 교환을 수행하는 기업용 인증 확장. `xaaIdpLogin.ts`에서 OIDC 디스커버리와 IdP ID 토큰 획득을 처리한다.

**인증 캐시**: `~/.claude/mcp-needs-auth-cache.json`에 서버별 인증 필요 상태를 캐싱한다. 동시 쓰기 경쟁(`writeChain` 프로미스 체인)을 직렬화하여 read-modify-write 레이스 조건을 방지한다.

**세션 만료 처리**: HTTP 404 + JSON-RPC 에러 코드 `-32001`의 조합을 세션 만료로 판별한다. 이 조합이 아닌 일반 404(잘못된 URL, 서버 다운)와 구분하기 위해 두 가지 신호를 모두 확인한다.

---

## 3. LSP (Language Server Protocol) 클라이언트

### 3.1 아키텍처 개요

LSP 통합은 세 계층으로 구성된다.

```
manager.ts          — 싱글턴 생명주기 관리
  └─ LSPServerManager.ts  — 다중 서버 라우팅
       └─ LSPServerInstance.ts — 단일 서버 인스턴스
            └─ LSPClient.ts    — vscode-jsonrpc 래퍼
```

LSP 서버는 **플러그인만을 통해** 제공된다. `config.ts`의 `getAllLspServers()`는 활성화된 플러그인들로부터 서버 설정을 로드한다. 사용자 설정이나 프로젝트 설정으로는 LSP 서버를 직접 등록할 수 없다.

### 3.2 싱글턴 초기화 (`manager.ts`)

LSP 관리자는 모듈 스코프 싱글턴 패턴으로 구현된다. 초기화 상태는 `'not-started' | 'pending' | 'success' | 'failed'` 네 가지다.

```typescript
// 비동기 초기화 — 시작 시간을 차단하지 않음
export function initializeLspServerManager(): void {
  if (isBareMode()) return  // --bare 모드에서는 LSP 비활성화
  lspManagerInstance = createLSPServerManager()
  initializationState = 'pending'
  initializationPromise = lspManagerInstance.initialize()
    .then(() => { initializationState = 'success' })
    .catch((error) => {
      initializationState = 'failed'
      lspManagerInstance = undefined
    })
}
```

**세대 카운터(Generation Counter)**: `initializationGeneration` 값을 증가시켜 진행 중인 초기화 프로미스를 무효화한다. 플러그인 새로고침(`reinitializeLspServerManager()`) 시 이전 초기화 결과가 새 상태를 덮어쓰는 것을 방지한다.

**플러그인 재초기화 문제**: `loadAllPlugins()`가 메모이즈되어 있어, 마켓플레이스 조정 전에 빈 플러그인 목록으로 캐싱될 수 있다. `reinitializeLspServerManager()`는 플러그인 캐시 갱신 후 LSP를 재초기화하여 이 문제를 해결한다.

`isLspConnected()`는 `LSPTool.isEnabled()`의 게이트로, 실행 중인 서버가 하나 이상이고 `error` 상태가 아닌 경우에만 `true`를 반환한다.

### 3.3 다중 서버 라우팅 (`LSPServerManager.ts`)

`LSPServerManager`는 파일 확장자 기반 라우팅을 구현한다.

```typescript
// 확장자 → 서버명 매핑
const extensionMap: Map<string, string[]> = new Map()

function getServerForFile(filePath: string): LSPServerInstance | undefined {
  const ext = path.extname(filePath).toLowerCase()
  const serverNames = extensionMap.get(ext)
  return serverNames ? servers.get(serverNames[0]!) : undefined
}
```

동일 확장자를 처리하는 서버가 여럿일 경우 첫 번째 등록된 서버를 사용한다(우선순위 로직 추가 예정).

**열린 파일 추적**: `openedFiles: Map<string, string>` (URI → 서버명)를 통해 동일 파일의 중복 `didOpen` 알림을 방지한다.

**`workspace/configuration` 처리**: TypeScript 언어 서버 등은 `workspace/configuration` 요청을 보내지만, Claude Code는 이를 지원하지 않는다. 모든 항목에 `null`을 반환하여 프로토콜 요구사항을 충족시킨다.

**셧다운 전략**: `running` 또는 `error` 상태의 서버만 명시적으로 중지한다. `Promise.allSettled()`를 사용하여 일부 서버 중지 실패가 나머지 서버 정리를 방해하지 않도록 한다.

### 3.4 단일 서버 인스턴스 (`LSPServerInstance.ts`)

서버 인스턴스의 상태 머신:

```
stopped → starting → running
running → stopping → stopped
any     → error  (크래시/실패)
error   → starting (재시도, maxRestarts 상한 있음)
```

**크래시 복구**: `createLSPClient()`의 `onCrash` 콜백으로 크래시를 감지하여 `state = 'error'`로 전환한다. 다음 요청 시 `ensureServerStarted()`가 자동으로 재시작을 시도한다. `config.maxRestarts`(기본값 3)를 초과하면 `Error`를 던지고 재시도를 포기한다.

**일시적 에러 재시도**: LSP 에러 코드 `-32801`("Content Modified")은 서버가 아직 인덱싱 중일 때 발생하는 일시적 에러다. 최대 3회, 500ms/1000ms/2000ms 지수 백오프로 재시도한다.

**지연 로딩(Lazy Loading)**: `vscode-jsonrpc`(~129KB)는 LSP 서버가 실제로 인스턴스화될 때까지 로드하지 않는다. `require('./LSPClient.js')`를 런타임에 호출하여 정적 임포트 체인에서 제외한다.

### 3.5 JSON-RPC 연결 (`LSPClient.ts`)

`createLSPClient()`는 `child_process.spawn()`으로 LSP 서버 프로세스를 시작하고, `vscode-jsonrpc`의 `createMessageConnection()`으로 stdio 기반 JSON-RPC 연결을 수립한다.

**스폰 경쟁 조건 처리**: `spawn()` 반환 직후 스트림을 사용하면 `ENOENT`(명령어 없음) 에러가 비동기적으로 발생하여 처리되지 않은 프로미스 거부가 생길 수 있다. 이를 방지하기 위해 `'spawn'` 이벤트를 기다린 후에 스트림을 사용한다.

```typescript
await new Promise<void>((resolve, reject) => {
  spawnedProcess.once('spawn', () => { cleanup(); resolve() })
  spawnedProcess.once('error', (error) => { cleanup(); reject(error) })
})
```

**에러 핸들러 등록 순서**: `connection.onError()`와 `connection.onClose()`를 `connection.listen()` **이전에** 등록한다. 서버가 즉시 크래시하는 경우 모든 에러를 캡처하기 위함이다.

**stdin 에러 격리**: LSP 서버 프로세스가 종료된 후 stdin에 쓰기 시도가 발생하면 처리되지 않은 프로미스 거부가 발생할 수 있다. `process.stdin.on('error', ...)` 핸들러로 이를 격리한다.

### 3.6 주요 LSP 작업

`LSPTool`이 지원하는 작업과 각각의 LSP 메서드:

| 작업 (`operation`) | LSP 메서드 | 설명 |
|-------------------|-----------|------|
| `goToDefinition` | `textDocument/definition` | 심볼 정의 위치로 이동 |
| `findReferences` | `textDocument/references` | 심볼 참조 위치 목록 |
| `hover` | `textDocument/hover` | 커서 위치 심볼 정보 |
| `documentSymbol` | `textDocument/documentSymbol` | 파일 내 심볼 트리 |
| `workspaceSymbol` | `workspace/symbol` | 워크스페이스 전체 심볼 검색 |
| `goToImplementation` | `textDocument/implementation` | 인터페이스 구현체로 이동 |
| `prepareCallHierarchy` | `textDocument/prepareCallHierarchy` | 호출 계층 준비 |
| `incomingCalls` | `callHierarchy/incomingCalls` | 호출자 목록 |
| `outgoingCalls` | `callHierarchy/outgoingCalls` | 피호출자 목록 |

입력 좌표는 **1-based** (에디터 표시 기준)이며, LSP로 전달할 때 0-based로 변환한다.

**파일 크기 제한**: 10MB(`MAX_LSP_FILE_SIZE_BYTES`)를 초과하는 파일은 처리를 거부한다.

**보안**: UNC 경로(`\\` 또는 `//`로 시작)는 NTLM 자격증명 누수를 방지하기 위해 파일시스템 작업 없이 검증을 통과시킨다.

### 3.7 진단 레지스트리 (`LSPDiagnosticRegistry.ts`)

LSP 서버는 `textDocument/publishDiagnostics` 알림을 비동기적으로 전송한다. `LSPDiagnosticRegistry`는 이를 수신하여 다음 대화 턴에 첨부파일(Attachment)로 전달한다.

```
publishDiagnostics 수신
  → registerPendingLSPDiagnostic() (UUID 키로 저장)
  → checkForLSPDiagnostics() (다음 쿼리 시 조회)
  → getLSPDiagnosticAttachments() → Attachment[]
  → getAttachments() → 대화에 자동 주입
```

**볼륨 제한**:
- 파일당 최대 10개 진단
- 전체 최대 30개 진단
- 심각도 순(Error > Warning > Info > Hint)으로 정렬하여 상위 항목 우선 전달

**교차 턴 중복 제거**: `deliveredDiagnostics` LRU 캐시(최대 500개 파일)에 이미 전달한 진단의 해시(메시지 + 심각도 + 범위 + 소스 + 코드)를 저장한다. 동일한 진단이 여러 턴에 걸쳐 반복 전달되지 않도록 방지한다.

---

## 4. MCPTool / LSPTool 구현 분석

### 4.1 MCPTool

`MCPTool`(`src/tools/MCPTool/MCPTool.ts`)은 모든 MCP 도구의 **정적 템플릿**이다. 실제 MCP 서버 도구들은 이 템플릿을 기반으로 `mcpClient.ts`에서 동적으로 생성된다.

```typescript
export const MCPTool = buildTool({
  isMcp: true,
  name: 'mcp',           // mcpClient.ts에서 실제 이름으로 오버라이드
  maxResultSizeChars: 100_000,
  inputSchema: z.object({}).passthrough(), // MCP 서버가 스키마 정의
  async call() { return { data: '' } },   // mcpClient.ts에서 오버라이드
  async checkPermissions() {
    return { behavior: 'passthrough', message: '...' }
  },
})
```

주요 특징:
- `isMcp: true` 플래그로 MCP 도구임을 표시
- `inputSchema`는 `passthrough()`를 사용하여 MCP 서버가 정의하는 임의의 입력 구조를 허용
- 권한 검사는 항상 `passthrough` — MCP 도구의 권한은 서버 수준에서 관리됨
- `isOpenWorld(): false` — 알려진 도구 집합 내에서만 동작
- `classifyForCollapse` 및 `renderToolUseProgressMessage`를 통해 UI에 진행 상태 표시 지원

### 4.2 LSPTool

`LSPTool`(`src/tools/LSPTool/LSPTool.ts`)은 LSP 기능을 Claude의 도구 인터페이스로 노출하는 **단일 도구**다. 여러 LSP 작업을 하나의 도구로 통합하여 입력 `operation` 필드로 구분한다.

```typescript
export const LSPTool = buildTool({
  name: LSP_TOOL_NAME,
  isLsp: true,
  shouldDefer: true,          // 초기화 완료 대기
  isEnabled() { return isLspConnected() },  // 동적 활성화 상태
  isConcurrencySafe() { return true },      // 병렬 실행 안전
  isReadOnly() { return true },             // 파일 수정 없음
})
```

**`shouldDefer: true`**: 도구 실행 전 LSP 초기화 완료를 기다린다. `waitForInitialization()`을 호출하여 `pending` 상태가 해소될 때까지 블로킹한다.

**입력 스키마 이중 검증**: `z.strictObject()`로 1차 검증 후, `lspToolInputSchema()` 판별 유니온으로 2차 검증하여 더 정확한 에러 메시지를 제공한다. 결과 포맷팅은 `formatters.ts`의 작업별 함수(`formatHoverResult`, `formatGoToDefinitionResult` 등)가 담당한다.

**`symbolContext.ts`**: 심볼 컨텍스트를 보강하는 유틸리티. 정의 위치 주변 코드를 포함하여 Claude가 더 풍부한 컨텍스트를 얻을 수 있도록 한다.

---

## 5. 설계 결정

### 5.1 MCP 도구의 동적 생성 vs. 정적 정의

MCP 도구는 서버에 연결하기 전까지 스키마를 알 수 없다. `MCPTool`이 정적 템플릿이고 실제 도구가 런타임에 생성되는 이유다. `buildTool()`이 반환하는 `ToolDef`를 기반으로, `mcpClient.ts`는 각 서버의 `tools/list` 응답을 받아 도구 인스턴스를 동적으로 구성한다.

### 5.2 LSP가 플러그인 전용인 이유

LSP 서버는 언어별 바이너리(`typescript-language-server`, `rust-analyzer` 등)를 실행한다. 이 바이너리들은 사용자 환경에 설치되어 있어야 하며, 플러그인이 이 의존성 확인과 경로 설정을 캡슐화하기에 적합하다. 직접 설정을 허용하면 잘못된 설정으로 인한 크래시 루프 가능성이 높아진다.

### 5.3 연결 캐시와 세션 만료

`connectToServer()`의 `memoize`는 동일 설정에 대해 하나의 연결만 유지한다. Streamable HTTP 서버의 세션 만료(HTTP 404 + JSON-RPC `-32001`)가 감지되면 캐시를 무효화하고 재연결한다. 일반 HTTP 404(잘못된 URL, 서버 다운)와 구분하기 위해 응답 본문의 JSON-RPC 에러 코드를 추가로 확인한다.

### 5.4 인증 상태의 캐싱

OAuth 인증 필요 상태를 15분간 캐싱하는 이유: 인증이 필요한 서버에 매 요청마다 재연결을 시도하면 30+ 커넥터가 동시에 401을 반환하여 인증 루프에 빠질 수 있다. 캐시는 이 폭발적 재시도를 완충한다.

### 5.5 Bun 런타임 특수 처리

일부 API(`WebSocket`, `AbortSignal.timeout`)는 Bun과 Node.js 간 동작이 다르다. `typeof Bun !== 'undefined'` 조건부 분기로 런타임을 감지하여 각각에 최적화된 구현을 선택한다.

### 5.6 도구 결과 크기 제어

MCP 도구 결과의 최대 크기는 `maxResultSizeChars: 100_000`이다. 이를 초과하는 결과는 `truncateMcpContentIfNeeded()`로 잘라내거나 `persistBinaryContent()`로 파일 시스템에 저장한다. 이진 데이터(이미지 등)는 base64 인코딩 크기를 추정하여 별도 처리한다.

---

## Navigation

**상위 레벨**
- [Level 2: Tool System](../level-2-systems/tool-system.md) — MCPTool과 LSPTool이 통합되는 도구 레지스트리 전체 설명
- [Level 2: Agent Coordinator](../level-2-systems/agent-coordinator.md) — MCP 서버 초기화 타이밍과 에이전트 루프의 관계

**동급 문서 (Level 3)**
- [Query Engine](../level-2-systems/query-engine.md) — 도구 호출 결과가 모델 입력으로 변환되는 과정
- [Permission System](../level-2-systems/permission-system.md) — MCP 도구 권한 결정 흐름

**핵심 소스 파일**

| 파일 | 역할 |
|------|------|
| `src/services/mcp/client.ts` | MCP 연결 관리, Transport 선택, 도구 동적 생성 |
| `src/services/mcp/types.ts` | MCP 설정 Zod 스키마 및 연결 상태 타입 |
| `src/services/mcp/config.ts` | 스코프별 설정 병합, `.mcp.json` 읽기/쓰기 |
| `src/services/mcp/auth.ts` | OAuth PKCE 플로우, XAA 토큰 교환 |
| `src/services/mcp/normalization.ts` | 도구 이름 정규화 |
| `src/services/lsp/manager.ts` | LSP 싱글턴 생명주기 |
| `src/services/lsp/LSPServerManager.ts` | 파일 확장자 기반 서버 라우팅 |
| `src/services/lsp/LSPServerInstance.ts` | 서버 상태 머신, 크래시 복구 |
| `src/services/lsp/LSPClient.ts` | vscode-jsonrpc stdio 연결 |
| `src/services/lsp/LSPDiagnosticRegistry.ts` | 진단 수신 및 중복 제거 |
| `src/tools/MCPTool/MCPTool.ts` | MCP 도구 정적 템플릿 |
| `src/tools/LSPTool/LSPTool.ts` | LSP 작업 통합 도구 |

#![allow(clippy::similar_names, clippy::cast_possible_truncation)]
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LaneEventName {
    #[serde(rename = "lane.started")]
    Started,
    #[serde(rename = "lane.ready")]
    Ready,
    #[serde(rename = "lane.prompt_misdelivery")]
    PromptMisdelivery,
    #[serde(rename = "lane.blocked")]
    Blocked,
    #[serde(rename = "lane.red")]
    Red,
    #[serde(rename = "lane.green")]
    Green,
    #[serde(rename = "lane.commit.created")]
    CommitCreated,
    #[serde(rename = "lane.pr.opened")]
    PrOpened,
    #[serde(rename = "lane.merge.ready")]
    MergeReady,
    #[serde(rename = "lane.finished")]
    Finished,
    #[serde(rename = "lane.failed")]
    Failed,
    #[serde(rename = "lane.reconciled")]
    Reconciled,
    #[serde(rename = "lane.merged")]
    Merged,
    #[serde(rename = "lane.superseded")]
    Superseded,
    #[serde(rename = "lane.closed")]
    Closed,
    #[serde(rename = "branch.stale_against_main")]
    BranchStaleAgainstMain,
    #[serde(rename = "branch.workspace_mismatch")]
    BranchWorkspaceMismatch,
    /// Ship/provenance events — §4.44.5
    #[serde(rename = "ship.prepared")]
    ShipPrepared,
    #[serde(rename = "ship.commits_selected")]
    ShipCommitsSelected,
    #[serde(rename = "ship.merged")]
    ShipMerged,
    #[serde(rename = "ship.pushed_main")]
    ShipPushedMain,
    /// Image-quality harness lifecycle (Spec §10).
    #[serde(rename = "image.generate.started")]
    ImageGenerateStarted,
    #[serde(rename = "image.validator.ran")]
    ImageValidatorRan,
    #[serde(rename = "image.gate.verdict")]
    ImageGateVerdict,
    #[serde(rename = "image.repair.planned")]
    ImageRepairPlanned,
    #[serde(rename = "image.scene.accepted")]
    ImageSceneAccepted,
    #[serde(rename = "image.scene.rejected")]
    ImageSceneRejected,
    #[serde(rename = "image.regression.summary")]
    ImageRegressionSummary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneEventStatus {
    Running,
    Ready,
    Blocked,
    Red,
    Green,
    Completed,
    Failed,
    Reconciled,
    Merged,
    Superseded,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneFailureClass {
    PromptDelivery,
    TrustGate,
    BranchDivergence,
    Compile,
    Test,
    PluginStartup,
    McpStartup,
    McpHandshake,
    GatewayRouting,
    ToolRuntime,
    WorkspaceMismatch,
    Infra,
    /// Image backend (`generate_image` / `inpaint_region`) failure —
    /// timeouts, 5xx, or degraded model state.
    ImageBackend,
    /// Image validator endpoint failure — missing axis or non-2xx
    /// response from `detect_hands_feet` / `check_symmetry` /
    /// `check_pattern_consistency`.
    ImageValidator,
    /// Image regression release gate failed (`pass_rate`,
    /// `catastrophic_failure_rate`, or `avg_iterations_to_pass` did not
    /// clear configured thresholds).
    ImageRegressionGate,
}

/// Provenance labels for event source classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventProvenance {
    /// Event from a live, active lane
    LiveLane,
    /// Event from a synthetic test
    Test,
    /// Event from a healthcheck probe
    Healthcheck,
    /// Event from a replay/log replay
    Replay,
    /// Event from the transport layer itself
    Transport,
}

/// Session identity metadata captured at creation time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionIdentity {
    /// Stable title for the session
    pub title: String,
    /// Workspace/worktree path
    pub workspace: String,
    /// Lane/session purpose
    pub purpose: String,
    /// Placeholder reason if any field is unknown
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder_reason: Option<String>,
}

impl SessionIdentity {
    /// Create complete session identity
    #[must_use]
    pub fn new(
        title: impl Into<String>,
        workspace: impl Into<String>,
        purpose: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            workspace: workspace.into(),
            purpose: purpose.into(),
            placeholder_reason: None,
        }
    }

    /// Create session identity with placeholder for missing fields
    #[must_use]
    pub fn with_placeholder(
        title: impl Into<String>,
        workspace: impl Into<String>,
        purpose: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            workspace: workspace.into(),
            purpose: purpose.into(),
            placeholder_reason: Some(reason.into()),
        }
    }
}

/// Lane ownership and workflow scope binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneOwnership {
    /// Owner/assignee identity
    pub owner: String,
    /// Workflow scope (e.g., claw-code-dogfood, external-git-maintenance)
    pub workflow_scope: String,
    /// Whether the watcher is expected to act, observe, or ignore
    pub watcher_action: WatcherAction,
}

/// Watcher action expectation for a lane event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WatcherAction {
    /// Watcher should take action on this event
    Act,
    /// Watcher should only observe
    Observe,
    /// Watcher should ignore this event
    Ignore,
}

/// Event metadata for ordering, provenance, deduplication, and ownership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneEventMetadata {
    /// Monotonic sequence number for event ordering
    pub seq: u64,
    /// Event provenance source
    pub provenance: EventProvenance,
    /// Session identity at creation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_identity: Option<SessionIdentity>,
    /// Lane ownership and scope
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ownership: Option<LaneOwnership>,
    /// Nudge ID for deduplication cycles
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nudge_id: Option<String>,
    /// Event fingerprint for terminal event deduplication
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_fingerprint: Option<String>,
    /// Timestamp when event was observed/created
    pub timestamp_ms: u64,
}

impl LaneEventMetadata {
    /// Create new event metadata
    #[must_use]
    pub fn new(seq: u64, provenance: EventProvenance) -> Self {
        Self {
            seq,
            provenance,
            session_identity: None,
            ownership: None,
            nudge_id: None,
            event_fingerprint: None,
            timestamp_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        }
    }

    /// Add session identity
    #[must_use]
    pub fn with_session_identity(mut self, identity: SessionIdentity) -> Self {
        self.session_identity = Some(identity);
        self
    }

    /// Add ownership info
    #[must_use]
    pub fn with_ownership(mut self, ownership: LaneOwnership) -> Self {
        self.ownership = Some(ownership);
        self
    }

    /// Add nudge ID for dedupe
    #[must_use]
    pub fn with_nudge_id(mut self, nudge_id: impl Into<String>) -> Self {
        self.nudge_id = Some(nudge_id.into());
        self
    }

    /// Compute and add event fingerprint for terminal events
    #[must_use]
    pub fn with_fingerprint(mut self, fingerprint: impl Into<String>) -> Self {
        self.event_fingerprint = Some(fingerprint.into());
        self
    }
}

/// Builder for constructing [`LaneEvent`]s with proper metadata.
#[derive(Debug, Clone)]
pub struct LaneEventBuilder {
    event: LaneEventName,
    status: LaneEventStatus,
    emitted_at: String,
    metadata: LaneEventMetadata,
    detail: Option<String>,
    failure_class: Option<LaneFailureClass>,
    data: Option<serde_json::Value>,
}

impl LaneEventBuilder {
    /// Start building a new lane event
    #[must_use]
    pub fn new(
        event: LaneEventName,
        status: LaneEventStatus,
        emitted_at: impl Into<String>,
        seq: u64,
        provenance: EventProvenance,
    ) -> Self {
        Self {
            event,
            status,
            emitted_at: emitted_at.into(),
            metadata: LaneEventMetadata::new(seq, provenance),
            detail: None,
            failure_class: None,
            data: None,
        }
    }

    /// Add session identity
    #[must_use]
    pub fn with_session_identity(mut self, identity: SessionIdentity) -> Self {
        self.metadata = self.metadata.with_session_identity(identity);
        self
    }

    /// Add ownership info
    #[must_use]
    pub fn with_ownership(mut self, ownership: LaneOwnership) -> Self {
        self.metadata = self.metadata.with_ownership(ownership);
        self
    }

    /// Add nudge ID
    #[must_use]
    pub fn with_nudge_id(mut self, nudge_id: impl Into<String>) -> Self {
        self.metadata = self.metadata.with_nudge_id(nudge_id);
        self
    }

    /// Add detail
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// Add failure class
    #[must_use]
    pub fn with_failure_class(mut self, failure_class: LaneFailureClass) -> Self {
        self.failure_class = Some(failure_class);
        self
    }

    /// Add data payload
    #[must_use]
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = Some(data);
        self
    }

    /// Compute fingerprint and build terminal event
    #[must_use]
    pub fn build_terminal(mut self) -> LaneEvent {
        let fingerprint = compute_event_fingerprint(&self.event, &self.status, self.data.as_ref());
        self.metadata = self.metadata.with_fingerprint(fingerprint);
        self.build()
    }

    /// Build the event
    #[must_use]
    pub fn build(self) -> LaneEvent {
        LaneEvent {
            event: self.event,
            status: self.status,
            emitted_at: self.emitted_at,
            failure_class: self.failure_class,
            detail: self.detail,
            data: self.data,
            metadata: self.metadata,
        }
    }
}

/// Check if an event kind is terminal (completed, failed, superseded, closed).
#[must_use]
pub fn is_terminal_event(event: LaneEventName) -> bool {
    matches!(
        event,
        LaneEventName::Finished
            | LaneEventName::Failed
            | LaneEventName::Superseded
            | LaneEventName::Closed
            | LaneEventName::Merged
            | LaneEventName::ImageSceneAccepted
            | LaneEventName::ImageSceneRejected
            | LaneEventName::ImageRegressionSummary
    )
}

/// Compute a fingerprint for terminal event deduplication.
#[must_use]
pub fn compute_event_fingerprint(
    event: &LaneEventName,
    status: &LaneEventStatus,
    data: Option<&serde_json::Value>,
) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    format!("{event:?}").hash(&mut hasher);
    format!("{status:?}").hash(&mut hasher);
    if let Some(d) = data {
        serde_json::to_string(d)
            .unwrap_or_default()
            .hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// Deduplicate terminal events within a reconciliation window.
/// Returns only the first occurrence of each terminal fingerprint.
#[must_use]
pub fn dedupe_terminal_events(events: &[LaneEvent]) -> Vec<LaneEvent> {
    let mut seen_fingerprints = std::collections::HashSet::new();
    let mut result = Vec::new();

    for event in events {
        if is_terminal_event(event.event) {
            if let Some(fp) = &event.metadata.event_fingerprint {
                if seen_fingerprints.contains(fp) {
                    continue; // Skip duplicate terminal event
                }
                seen_fingerprints.insert(fp.clone());
            }
        }
        result.push(event.clone());
    }

    result
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockedSubphase {
    #[serde(rename = "blocked.trust_prompt")]
    TrustPrompt { gate_repo: String },
    #[serde(rename = "blocked.prompt_delivery")]
    PromptDelivery { attempt: u32 },
    #[serde(rename = "blocked.plugin_init")]
    PluginInit { plugin_name: String },
    #[serde(rename = "blocked.mcp_handshake")]
    McpHandshake { server_name: String, attempt: u32 },
    #[serde(rename = "blocked.branch_freshness")]
    BranchFreshness { behind_main: u32 },
    #[serde(rename = "blocked.test_hang")]
    TestHang {
        elapsed_secs: u32,
        test_name: Option<String>,
    },
    #[serde(rename = "blocked.report_pending")]
    ReportPending { since_secs: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneEventBlocker {
    #[serde(rename = "failureClass")]
    pub failure_class: LaneFailureClass,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subphase: Option<BlockedSubphase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneCommitProvenance {
    pub commit: String,
    pub branch: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(rename = "canonicalCommit", skip_serializing_if = "Option::is_none")]
    pub canonical_commit: Option<String>,
    #[serde(rename = "supersededBy", skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lineage: Vec<String>,
}

/// Ship/provenance metadata — §4.44.5
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShipProvenance {
    pub source_branch: String,
    pub base_commit: String,
    pub commit_count: u32,
    pub commit_range: String,
    pub merge_method: ShipMergeMethod,
    pub actor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShipMergeMethod {
    DirectPush,
    FastForward,
    MergeCommit,
    SquashMerge,
    RebaseMerge,
}

/// Per-step provenance metadata for image-pipeline lane events. Mirrors
/// what `tools::image::regression::SceneOutcome` already records, so an
/// orchestrator can cheaply lift outcome data into a `LaneEvent` payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageStepProvenance {
    pub run_id: String,
    pub scene_id: String,
    pub seed: u64,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iteration: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_image_uri: Option<String>,
}

impl ImageStepProvenance {
    #[must_use]
    pub fn new(
        run_id: impl Into<String>,
        scene_id: impl Into<String>,
        seed: u64,
        provider: impl Into<String>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            scene_id: scene_id.into(),
            seed,
            provider: provider.into(),
            profile: None,
            iteration: None,
            final_image_uri: None,
        }
    }

    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    #[must_use]
    pub fn with_iteration(mut self, iteration: u32) -> Self {
        self.iteration = Some(iteration);
        self
    }

    #[must_use]
    pub fn with_final_image_uri(mut self, uri: impl Into<String>) -> Self {
        self.final_image_uri = Some(uri.into());
        self
    }
}

/// Verdict payload for `lane.image.gate.verdict` events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageGateVerdict {
    pub passed: bool,
    pub anatomy_score: f64,
    pub symmetry_score: f64,
    pub pattern_score: f64,
    pub artifact_score: f64,
    pub creative_score: f64,
    pub weighted_total: f64,
}

/// Aggregate payload for `lane.image.regression.summary` events.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageRegressionSummaryPayload {
    pub run_id: String,
    pub profile: String,
    pub scenes_total: usize,
    pub seeds_total: usize,
    pub accepted: usize,
    pub rejected: usize,
    pub errored: usize,
    pub pass_rate: f64,
    pub catastrophic_failure_rate: f64,
    pub release_gate_passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaneEvent {
    pub event: LaneEventName,
    pub status: LaneEventStatus,
    #[serde(rename = "emittedAt")]
    pub emitted_at: String,
    #[serde(rename = "failureClass", skip_serializing_if = "Option::is_none")]
    pub failure_class: Option<LaneFailureClass>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
    /// Event metadata for ordering, provenance, dedupe, and ownership
    pub metadata: LaneEventMetadata,
}

impl LaneEvent {
    /// Create a new lane event with minimal metadata (seq=0, provenance=LiveLane)
    /// Use `LaneEventBuilder` for events requiring full metadata.
    #[must_use]
    pub fn new(
        event: LaneEventName,
        status: LaneEventStatus,
        emitted_at: impl Into<String>,
    ) -> Self {
        Self {
            event,
            status,
            emitted_at: emitted_at.into(),
            failure_class: None,
            detail: None,
            data: None,
            metadata: LaneEventMetadata::new(0, EventProvenance::LiveLane),
        }
    }

    #[must_use]
    pub fn started(emitted_at: impl Into<String>) -> Self {
        Self::new(LaneEventName::Started, LaneEventStatus::Running, emitted_at)
    }

    #[must_use]
    pub fn finished(emitted_at: impl Into<String>, detail: Option<String>) -> Self {
        Self::new(
            LaneEventName::Finished,
            LaneEventStatus::Completed,
            emitted_at,
        )
        .with_optional_detail(detail)
    }

    #[must_use]
    pub fn commit_created(
        emitted_at: impl Into<String>,
        detail: Option<String>,
        provenance: LaneCommitProvenance,
    ) -> Self {
        Self::new(
            LaneEventName::CommitCreated,
            LaneEventStatus::Completed,
            emitted_at,
        )
        .with_optional_detail(detail)
        .with_data(serde_json::to_value(provenance).expect("commit provenance should serialize"))
    }

    #[must_use]
    pub fn superseded(
        emitted_at: impl Into<String>,
        detail: Option<String>,
        provenance: LaneCommitProvenance,
    ) -> Self {
        Self::new(
            LaneEventName::Superseded,
            LaneEventStatus::Superseded,
            emitted_at,
        )
        .with_optional_detail(detail)
        .with_data(serde_json::to_value(provenance).expect("commit provenance should serialize"))
    }

    #[must_use]
    pub fn blocked(emitted_at: impl Into<String>, blocker: &LaneEventBlocker) -> Self {
        let mut event = Self::new(LaneEventName::Blocked, LaneEventStatus::Blocked, emitted_at)
            .with_failure_class(blocker.failure_class)
            .with_detail(blocker.detail.clone());
        if let Some(ref subphase) = blocker.subphase {
            event =
                event.with_data(serde_json::to_value(subphase).expect("subphase should serialize"));
        }
        event
    }

    #[must_use]
    pub fn failed(emitted_at: impl Into<String>, blocker: &LaneEventBlocker) -> Self {
        let mut event = Self::new(LaneEventName::Failed, LaneEventStatus::Failed, emitted_at)
            .with_failure_class(blocker.failure_class)
            .with_detail(blocker.detail.clone());
        if let Some(ref subphase) = blocker.subphase {
            event =
                event.with_data(serde_json::to_value(subphase).expect("subphase should serialize"));
        }
        event
    }

    /// Ship prepared — §4.44.5
    #[must_use]
    pub fn ship_prepared(emitted_at: impl Into<String>, provenance: &ShipProvenance) -> Self {
        Self::new(
            LaneEventName::ShipPrepared,
            LaneEventStatus::Ready,
            emitted_at,
        )
        .with_data(serde_json::to_value(provenance).expect("ship provenance should serialize"))
    }

    /// Ship commits selected — §4.44.5
    #[must_use]
    pub fn ship_commits_selected(
        emitted_at: impl Into<String>,
        commit_count: u32,
        commit_range: impl Into<String>,
    ) -> Self {
        Self::new(
            LaneEventName::ShipCommitsSelected,
            LaneEventStatus::Ready,
            emitted_at,
        )
        .with_detail(format!("{} commits: {}", commit_count, commit_range.into()))
    }

    /// Ship merged — §4.44.5
    #[must_use]
    pub fn ship_merged(emitted_at: impl Into<String>, provenance: &ShipProvenance) -> Self {
        Self::new(
            LaneEventName::ShipMerged,
            LaneEventStatus::Completed,
            emitted_at,
        )
        .with_data(serde_json::to_value(provenance).expect("ship provenance should serialize"))
    }

    /// Ship pushed to main — §4.44.5
    #[must_use]
    pub fn ship_pushed_main(emitted_at: impl Into<String>, provenance: &ShipProvenance) -> Self {
        Self::new(
            LaneEventName::ShipPushedMain,
            LaneEventStatus::Completed,
            emitted_at,
        )
        .with_data(serde_json::to_value(provenance).expect("ship provenance should serialize"))
    }

    /// Image-pipeline: a generation request was dispatched to a provider.
    #[must_use]
    pub fn image_generate_started(
        emitted_at: impl Into<String>,
        provenance: &ImageStepProvenance,
    ) -> Self {
        Self::new(
            LaneEventName::ImageGenerateStarted,
            LaneEventStatus::Running,
            emitted_at,
        )
        .with_data(serde_json::to_value(provenance).expect("image provenance should serialize"))
    }

    /// Image-pipeline: validator suite produced a normalised score vector.
    #[must_use]
    pub fn image_validator_ran(
        emitted_at: impl Into<String>,
        provenance: &ImageStepProvenance,
        weighted_total: f64,
    ) -> Self {
        let mut value =
            serde_json::to_value(provenance).expect("image provenance should serialize");
        if let Some(map) = value.as_object_mut() {
            map.insert(
                "weighted_total".to_string(),
                serde_json::json!(weighted_total),
            );
        }
        Self::new(
            LaneEventName::ImageValidatorRan,
            LaneEventStatus::Running,
            emitted_at,
        )
        .with_data(value)
    }

    /// Image-pipeline: per-image policy gate verdict.
    #[must_use]
    pub fn image_gate_verdict(
        emitted_at: impl Into<String>,
        provenance: &ImageStepProvenance,
        verdict: &ImageGateVerdict,
    ) -> Self {
        let mut value = serde_json::to_value(provenance).expect("provenance");
        if let Some(map) = value.as_object_mut() {
            map.insert(
                "verdict".to_string(),
                serde_json::to_value(verdict).expect("verdict"),
            );
        }
        let status = if verdict.passed {
            LaneEventStatus::Green
        } else {
            LaneEventStatus::Red
        };
        Self::new(LaneEventName::ImageGateVerdict, status, emitted_at).with_data(value)
    }

    /// Image-pipeline: a repair plan was scheduled (mask + targeted prompt).
    #[must_use]
    pub fn image_repair_planned(
        emitted_at: impl Into<String>,
        provenance: &ImageStepProvenance,
        repair_kinds: &[&str],
    ) -> Self {
        let mut value = serde_json::to_value(provenance).expect("provenance");
        if let Some(map) = value.as_object_mut() {
            map.insert("repair_kinds".to_string(), serde_json::json!(repair_kinds));
        }
        Self::new(
            LaneEventName::ImageRepairPlanned,
            LaneEventStatus::Running,
            emitted_at,
        )
        .with_data(value)
    }

    /// Image-pipeline: scene accepted under the configured policy gate.
    #[must_use]
    pub fn image_scene_accepted(
        emitted_at: impl Into<String>,
        provenance: &ImageStepProvenance,
    ) -> Self {
        Self::new(
            LaneEventName::ImageSceneAccepted,
            LaneEventStatus::Completed,
            emitted_at,
        )
        .with_data(serde_json::to_value(provenance).expect("provenance"))
    }

    /// Image-pipeline: scene rejected after exhausting repair budget.
    #[must_use]
    pub fn image_scene_rejected(
        emitted_at: impl Into<String>,
        provenance: &ImageStepProvenance,
        failure_class: LaneFailureClass,
        detail: impl Into<String>,
    ) -> Self {
        Self::new(
            LaneEventName::ImageSceneRejected,
            LaneEventStatus::Failed,
            emitted_at,
        )
        .with_failure_class(failure_class)
        .with_detail(detail)
        .with_data(serde_json::to_value(provenance).expect("provenance"))
    }

    /// Image-pipeline: aggregate summary at end of regression run.
    #[must_use]
    pub fn image_regression_summary(
        emitted_at: impl Into<String>,
        summary: &ImageRegressionSummaryPayload,
    ) -> Self {
        let status = if summary.release_gate_passed {
            LaneEventStatus::Green
        } else {
            LaneEventStatus::Red
        };
        Self::new(LaneEventName::ImageRegressionSummary, status, emitted_at)
            .with_data(serde_json::to_value(summary).expect("summary"))
    }

    #[must_use]
    pub fn with_failure_class(mut self, failure_class: LaneFailureClass) -> Self {
        self.failure_class = Some(failure_class);
        self
    }

    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    #[must_use]
    pub fn with_optional_detail(mut self, detail: Option<String>) -> Self {
        self.detail = detail;
        self
    }

    #[must_use]
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

#[must_use]
pub fn dedupe_superseded_commit_events(events: &[LaneEvent]) -> Vec<LaneEvent> {
    let mut keep = vec![true; events.len()];
    let mut latest_by_key = std::collections::BTreeMap::<String, usize>::new();

    for (index, event) in events.iter().enumerate() {
        if event.event != LaneEventName::CommitCreated {
            continue;
        }
        let Some(data) = event.data.as_ref() else {
            continue;
        };
        let key = data
            .get("canonicalCommit")
            .or_else(|| data.get("commit"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let superseded = data
            .get("supersededBy")
            .and_then(serde_json::Value::as_str)
            .is_some();
        if superseded {
            keep[index] = false;
            continue;
        }
        if let Some(key) = key {
            if let Some(previous) = latest_by_key.insert(key, index) {
                keep[previous] = false;
            }
        }
    }

    events
        .iter()
        .cloned()
        .zip(keep)
        .filter_map(|(event, retain)| retain.then_some(event))
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        compute_event_fingerprint, dedupe_superseded_commit_events, dedupe_terminal_events,
        is_terminal_event, BlockedSubphase, EventProvenance, ImageGateVerdict,
        ImageRegressionSummaryPayload, ImageStepProvenance, LaneCommitProvenance, LaneEvent,
        LaneEventBlocker, LaneEventBuilder, LaneEventMetadata, LaneEventName, LaneEventStatus,
        LaneFailureClass, LaneOwnership, SessionIdentity, ShipMergeMethod, ShipProvenance,
        WatcherAction,
    };

    #[test]
    fn canonical_lane_event_names_serialize_to_expected_wire_values() {
        let cases = [
            (LaneEventName::Started, "lane.started"),
            (LaneEventName::Ready, "lane.ready"),
            (LaneEventName::PromptMisdelivery, "lane.prompt_misdelivery"),
            (LaneEventName::Blocked, "lane.blocked"),
            (LaneEventName::Red, "lane.red"),
            (LaneEventName::Green, "lane.green"),
            (LaneEventName::CommitCreated, "lane.commit.created"),
            (LaneEventName::PrOpened, "lane.pr.opened"),
            (LaneEventName::MergeReady, "lane.merge.ready"),
            (LaneEventName::Finished, "lane.finished"),
            (LaneEventName::Failed, "lane.failed"),
            (LaneEventName::Reconciled, "lane.reconciled"),
            (LaneEventName::Merged, "lane.merged"),
            (LaneEventName::Superseded, "lane.superseded"),
            (LaneEventName::Closed, "lane.closed"),
            (
                LaneEventName::BranchStaleAgainstMain,
                "branch.stale_against_main",
            ),
            (
                LaneEventName::BranchWorkspaceMismatch,
                "branch.workspace_mismatch",
            ),
            (LaneEventName::ShipPrepared, "ship.prepared"),
            (LaneEventName::ShipCommitsSelected, "ship.commits_selected"),
            (LaneEventName::ShipMerged, "ship.merged"),
            (LaneEventName::ShipPushedMain, "ship.pushed_main"),
            (
                LaneEventName::ImageGenerateStarted,
                "image.generate.started",
            ),
            (LaneEventName::ImageValidatorRan, "image.validator.ran"),
            (LaneEventName::ImageGateVerdict, "image.gate.verdict"),
            (LaneEventName::ImageRepairPlanned, "image.repair.planned"),
            (LaneEventName::ImageSceneAccepted, "image.scene.accepted"),
            (LaneEventName::ImageSceneRejected, "image.scene.rejected"),
            (
                LaneEventName::ImageRegressionSummary,
                "image.regression.summary",
            ),
        ];

        for (event, expected) in cases {
            assert_eq!(
                serde_json::to_value(event).expect("serialize event"),
                json!(expected)
            );
        }
    }

    #[test]
    fn failure_classes_cover_canonical_taxonomy_wire_values() {
        let cases = [
            (LaneFailureClass::PromptDelivery, "prompt_delivery"),
            (LaneFailureClass::TrustGate, "trust_gate"),
            (LaneFailureClass::BranchDivergence, "branch_divergence"),
            (LaneFailureClass::Compile, "compile"),
            (LaneFailureClass::Test, "test"),
            (LaneFailureClass::PluginStartup, "plugin_startup"),
            (LaneFailureClass::McpStartup, "mcp_startup"),
            (LaneFailureClass::McpHandshake, "mcp_handshake"),
            (LaneFailureClass::GatewayRouting, "gateway_routing"),
            (LaneFailureClass::ToolRuntime, "tool_runtime"),
            (LaneFailureClass::WorkspaceMismatch, "workspace_mismatch"),
            (LaneFailureClass::Infra, "infra"),
        ];

        for (failure_class, expected) in cases {
            assert_eq!(
                serde_json::to_value(failure_class).expect("serialize failure class"),
                json!(expected)
            );
        }
    }

    #[test]
    fn blocked_and_failed_events_reuse_blocker_details() {
        let blocker = LaneEventBlocker {
            failure_class: LaneFailureClass::McpStartup,
            detail: "broken server".to_string(),
            subphase: Some(BlockedSubphase::McpHandshake {
                server_name: "test-server".to_string(),
                attempt: 1,
            }),
        };

        let blocked = LaneEvent::blocked("2026-04-04T00:00:00Z", &blocker);
        let failed = LaneEvent::failed("2026-04-04T00:00:01Z", &blocker);

        assert_eq!(blocked.event, LaneEventName::Blocked);
        assert_eq!(blocked.status, LaneEventStatus::Blocked);
        assert_eq!(blocked.failure_class, Some(LaneFailureClass::McpStartup));
        assert_eq!(failed.event, LaneEventName::Failed);
        assert_eq!(failed.status, LaneEventStatus::Failed);
        assert_eq!(failed.detail.as_deref(), Some("broken server"));
    }

    #[test]
    fn workspace_mismatch_failure_class_round_trips_in_branch_event_payloads() {
        let mismatch = LaneEvent::new(
            LaneEventName::BranchWorkspaceMismatch,
            LaneEventStatus::Blocked,
            "2026-04-04T00:00:02Z",
        )
        .with_failure_class(LaneFailureClass::WorkspaceMismatch)
        .with_detail("session belongs to /tmp/repo-a but current workspace is /tmp/repo-b")
        .with_data(json!({
            "expectedWorkspaceRoot": "/tmp/repo-a",
            "actualWorkspaceRoot": "/tmp/repo-b",
            "sessionId": "sess-123",
        }));

        let mismatch_json = serde_json::to_value(&mismatch).expect("lane event should serialize");
        assert_eq!(mismatch_json["event"], "branch.workspace_mismatch");
        assert_eq!(mismatch_json["failureClass"], "workspace_mismatch");
        assert_eq!(
            mismatch_json["data"]["expectedWorkspaceRoot"],
            "/tmp/repo-a"
        );

        let round_trip: LaneEvent =
            serde_json::from_value(mismatch_json).expect("lane event should deserialize");
        assert_eq!(round_trip.event, LaneEventName::BranchWorkspaceMismatch);
        assert_eq!(
            round_trip.failure_class,
            Some(LaneFailureClass::WorkspaceMismatch)
        );
    }

    #[test]
    fn ship_provenance_events_serialize_to_expected_wire_values() {
        let provenance = ShipProvenance {
            source_branch: "feature/provenance".to_string(),
            base_commit: "dd73962".to_string(),
            commit_count: 6,
            commit_range: "dd73962..c956f78".to_string(),
            merge_method: ShipMergeMethod::DirectPush,
            actor: "Jobdori".to_string(),
            pr_number: None,
        };

        let prepared = LaneEvent::ship_prepared("2026-04-20T14:30:00Z", &provenance);
        let prepared_json = serde_json::to_value(&prepared).expect("ship event should serialize");
        assert_eq!(prepared_json["event"], "ship.prepared");
        assert_eq!(prepared_json["data"]["commit_count"], 6);
        assert_eq!(prepared_json["data"]["source_branch"], "feature/provenance");

        let pushed = LaneEvent::ship_pushed_main("2026-04-20T14:35:00Z", &provenance);
        let pushed_json = serde_json::to_value(&pushed).expect("ship event should serialize");
        assert_eq!(pushed_json["event"], "ship.pushed_main");
        assert_eq!(pushed_json["data"]["merge_method"], "direct_push");

        let round_trip: LaneEvent =
            serde_json::from_value(pushed_json).expect("ship event should deserialize");
        assert_eq!(round_trip.event, LaneEventName::ShipPushedMain);
    }

    #[test]
    fn commit_events_can_carry_worktree_and_supersession_metadata() {
        let event = LaneEvent::commit_created(
            "2026-04-04T00:00:00Z",
            Some("commit created".to_string()),
            LaneCommitProvenance {
                commit: "abc123".to_string(),
                branch: "feature/provenance".to_string(),
                worktree: Some("wt-a".to_string()),
                canonical_commit: Some("abc123".to_string()),
                superseded_by: None,
                lineage: vec!["abc123".to_string()],
            },
        );
        let event_json = serde_json::to_value(&event).expect("lane event should serialize");
        assert_eq!(event_json["event"], "lane.commit.created");
        assert_eq!(event_json["data"]["branch"], "feature/provenance");
        assert_eq!(event_json["data"]["worktree"], "wt-a");
    }

    #[test]
    fn dedupes_superseded_commit_events_by_canonical_commit() {
        let retained = dedupe_superseded_commit_events(&[
            LaneEvent::commit_created(
                "2026-04-04T00:00:00Z",
                Some("old".to_string()),
                LaneCommitProvenance {
                    commit: "old123".to_string(),
                    branch: "feature/provenance".to_string(),
                    worktree: Some("wt-a".to_string()),
                    canonical_commit: Some("canon123".to_string()),
                    superseded_by: Some("new123".to_string()),
                    lineage: vec!["old123".to_string(), "new123".to_string()],
                },
            ),
            LaneEvent::commit_created(
                "2026-04-04T00:00:01Z",
                Some("new".to_string()),
                LaneCommitProvenance {
                    commit: "new123".to_string(),
                    branch: "feature/provenance".to_string(),
                    worktree: Some("wt-b".to_string()),
                    canonical_commit: Some("canon123".to_string()),
                    superseded_by: None,
                    lineage: vec!["old123".to_string(), "new123".to_string()],
                },
            ),
        ]);
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].detail.as_deref(), Some("new"));
    }

    #[test]
    fn lane_event_metadata_includes_monotonic_sequence() {
        let meta1 = LaneEventMetadata::new(0, EventProvenance::LiveLane);
        let meta2 = LaneEventMetadata::new(1, EventProvenance::LiveLane);
        let meta3 = LaneEventMetadata::new(2, EventProvenance::Test);

        assert_eq!(meta1.seq, 0);
        assert_eq!(meta2.seq, 1);
        assert_eq!(meta3.seq, 2);
        assert!(meta1.timestamp_ms <= meta2.timestamp_ms);
    }

    #[test]
    fn event_provenance_round_trips_through_serialization() {
        let cases = [
            (EventProvenance::LiveLane, "live_lane"),
            (EventProvenance::Test, "test"),
            (EventProvenance::Healthcheck, "healthcheck"),
            (EventProvenance::Replay, "replay"),
            (EventProvenance::Transport, "transport"),
        ];

        for (provenance, expected) in cases {
            let json = serde_json::to_value(provenance).expect("should serialize");
            assert_eq!(json, serde_json::json!(expected));

            let round_trip: EventProvenance =
                serde_json::from_value(json).expect("should deserialize");
            assert_eq!(round_trip, provenance);
        }
    }

    #[test]
    fn session_identity_is_complete_at_creation() {
        let identity = SessionIdentity::new("my-lane", "/tmp/repo", "implement feature X");

        assert_eq!(identity.title, "my-lane");
        assert_eq!(identity.workspace, "/tmp/repo");
        assert_eq!(identity.purpose, "implement feature X");
        assert!(identity.placeholder_reason.is_none());

        // Test with placeholder
        let with_placeholder = SessionIdentity::with_placeholder(
            "untitled",
            "/tmp/unknown",
            "unknown",
            "session created before title was known",
        );
        assert_eq!(
            with_placeholder.placeholder_reason,
            Some("session created before title was known".to_string())
        );
    }

    #[test]
    fn lane_ownership_binding_includes_workflow_scope() {
        let ownership = LaneOwnership {
            owner: "claw-1".to_string(),
            workflow_scope: "claw-code-dogfood".to_string(),
            watcher_action: WatcherAction::Act,
        };

        assert_eq!(ownership.owner, "claw-1");
        assert_eq!(ownership.workflow_scope, "claw-code-dogfood");
        assert_eq!(ownership.watcher_action, WatcherAction::Act);
    }

    #[test]
    fn watcher_action_round_trips_through_serialization() {
        let cases = [
            (WatcherAction::Act, "act"),
            (WatcherAction::Observe, "observe"),
            (WatcherAction::Ignore, "ignore"),
        ];

        for (action, expected) in cases {
            let json = serde_json::to_value(action).expect("should serialize");
            assert_eq!(json, serde_json::json!(expected));

            let round_trip: WatcherAction =
                serde_json::from_value(json).expect("should deserialize");
            assert_eq!(round_trip, action);
        }
    }

    #[test]
    fn is_terminal_event_detects_terminal_states() {
        assert!(is_terminal_event(LaneEventName::Finished));
        assert!(is_terminal_event(LaneEventName::Failed));
        assert!(is_terminal_event(LaneEventName::Superseded));
        assert!(is_terminal_event(LaneEventName::Closed));
        assert!(is_terminal_event(LaneEventName::Merged));

        assert!(!is_terminal_event(LaneEventName::Started));
        assert!(!is_terminal_event(LaneEventName::Ready));
        assert!(!is_terminal_event(LaneEventName::Blocked));
    }

    #[test]
    fn compute_event_fingerprint_is_deterministic() {
        let fp1 = compute_event_fingerprint(
            &LaneEventName::Finished,
            &LaneEventStatus::Completed,
            Some(&json!({"commit": "abc123"})),
        );
        let fp2 = compute_event_fingerprint(
            &LaneEventName::Finished,
            &LaneEventStatus::Completed,
            Some(&json!({"commit": "abc123"})),
        );

        assert_eq!(fp1, fp2, "same inputs should produce same fingerprint");
        assert!(!fp1.is_empty());
        assert_eq!(fp1.len(), 16, "fingerprint should be 16 hex chars");
    }

    #[test]
    fn compute_event_fingerprint_differs_for_different_inputs() {
        let fp1 =
            compute_event_fingerprint(&LaneEventName::Finished, &LaneEventStatus::Completed, None);
        let fp2 = compute_event_fingerprint(&LaneEventName::Failed, &LaneEventStatus::Failed, None);
        let fp3 = compute_event_fingerprint(
            &LaneEventName::Finished,
            &LaneEventStatus::Completed,
            Some(&json!({"commit": "abc123"})),
        );

        assert_ne!(fp1, fp2, "different event/status should differ");
        assert_ne!(fp1, fp3, "different data should differ");
    }

    #[test]
    fn dedupe_terminal_events_suppresses_duplicates() {
        let event1 = LaneEventBuilder::new(
            LaneEventName::Finished,
            LaneEventStatus::Completed,
            "2026-04-04T00:00:00Z",
            0,
            EventProvenance::LiveLane,
        )
        .build_terminal();

        let event2 = LaneEventBuilder::new(
            LaneEventName::Started,
            LaneEventStatus::Running,
            "2026-04-04T00:00:01Z",
            1,
            EventProvenance::LiveLane,
        )
        .build();

        let event3 = LaneEventBuilder::new(
            LaneEventName::Finished,
            LaneEventStatus::Completed,
            "2026-04-04T00:00:02Z",
            2,
            EventProvenance::LiveLane,
        )
        .build_terminal(); // Same fingerprint as event1

        let deduped = dedupe_terminal_events(&[event1.clone(), event2.clone(), event3.clone()]);

        assert_eq!(deduped.len(), 2, "should have 2 events after dedupe");
        assert_eq!(deduped[0].event, LaneEventName::Finished);
        assert_eq!(deduped[1].event, LaneEventName::Started);
        // event3 should be suppressed as duplicate of event1
    }

    #[test]
    fn lane_event_builder_constructs_event_with_metadata() {
        let event = LaneEventBuilder::new(
            LaneEventName::Started,
            LaneEventStatus::Running,
            "2026-04-04T00:00:00Z",
            42,
            EventProvenance::Test,
        )
        .with_session_identity(SessionIdentity::new("test-lane", "/tmp", "test"))
        .with_ownership(LaneOwnership {
            owner: "bot-1".to_string(),
            workflow_scope: "test-suite".to_string(),
            watcher_action: WatcherAction::Observe,
        })
        .with_nudge_id("nudge-123")
        .with_detail("starting test run")
        .build();

        assert_eq!(event.event, LaneEventName::Started);
        assert_eq!(event.metadata.seq, 42);
        assert_eq!(event.metadata.provenance, EventProvenance::Test);
        assert_eq!(
            event.metadata.session_identity.as_ref().unwrap().title,
            "test-lane"
        );
        assert_eq!(event.metadata.ownership.as_ref().unwrap().owner, "bot-1");
        assert_eq!(event.metadata.nudge_id, Some("nudge-123".to_string()));
        assert_eq!(event.detail, Some("starting test run".to_string()));
    }

    #[test]
    fn lane_event_metadata_round_trips_through_serialization() {
        let meta = LaneEventMetadata::new(5, EventProvenance::Healthcheck)
            .with_session_identity(SessionIdentity::new("lane-1", "/tmp", "purpose"))
            .with_nudge_id("nudge-abc");

        let json = serde_json::to_value(&meta).expect("should serialize");
        assert_eq!(json["seq"], 5);
        assert_eq!(json["provenance"], "healthcheck");
        assert_eq!(json["nudge_id"], "nudge-abc");
        assert!(json["timestamp_ms"].as_u64().is_some());

        let round_trip: LaneEventMetadata =
            serde_json::from_value(json).expect("should deserialize");
        assert_eq!(round_trip.seq, 5);
        assert_eq!(round_trip.provenance, EventProvenance::Healthcheck);
        assert_eq!(round_trip.nudge_id, Some("nudge-abc".to_string()));
    }

    fn sample_image_provenance() -> ImageStepProvenance {
        ImageStepProvenance::new("iqh_2026_04", "scene_003_fullbody_armor", 101, "diffusers")
            .with_profile("production")
            .with_iteration(1)
            .with_final_image_uri("s3://run/0.png")
    }

    #[test]
    fn image_generate_started_event_carries_provenance_payload() {
        let event = LaneEvent::image_generate_started("now", &sample_image_provenance());
        assert_eq!(event.event, LaneEventName::ImageGenerateStarted);
        assert_eq!(event.status, LaneEventStatus::Running);
        let data = event.data.as_ref().expect("provenance data");
        assert_eq!(data["scene_id"], "scene_003_fullbody_armor");
        assert_eq!(data["seed"], 101);
        assert_eq!(data["provider"], "diffusers");
        assert_eq!(data["profile"], "production");
        assert_eq!(data["iteration"], 1);
    }

    #[test]
    fn image_validator_ran_event_includes_weighted_total_alongside_provenance() {
        let event = LaneEvent::image_validator_ran("now", &sample_image_provenance(), 0.913);
        let data = event.data.expect("data");
        assert_eq!(data["scene_id"], "scene_003_fullbody_armor");
        assert!((data["weighted_total"].as_f64().unwrap() - 0.913).abs() < 1e-9);
    }

    #[test]
    fn image_gate_verdict_event_status_reflects_pass_or_fail() {
        let verdict = ImageGateVerdict {
            passed: true,
            anatomy_score: 0.95,
            symmetry_score: 0.92,
            pattern_score: 0.91,
            artifact_score: 0.88,
            creative_score: 0.80,
            weighted_total: 0.91,
        };
        let pass = LaneEvent::image_gate_verdict("now", &sample_image_provenance(), &verdict);
        assert_eq!(pass.status, LaneEventStatus::Green);
        let payload = pass.data.expect("data");
        assert_eq!(payload["verdict"]["passed"], true);

        let mut failed = verdict;
        failed.passed = false;
        let fail = LaneEvent::image_gate_verdict("now", &sample_image_provenance(), &failed);
        assert_eq!(fail.status, LaneEventStatus::Red);
    }

    #[test]
    fn image_repair_planned_event_records_repair_kinds_array() {
        let event = LaneEvent::image_repair_planned(
            "now",
            &sample_image_provenance(),
            &["anatomy", "symmetry"],
        );
        let data = event.data.expect("data");
        assert_eq!(data["repair_kinds"][0], "anatomy");
        assert_eq!(data["repair_kinds"][1], "symmetry");
    }

    #[test]
    fn image_scene_accepted_and_rejected_events_round_trip() {
        let accepted = LaneEvent::image_scene_accepted("now", &sample_image_provenance());
        assert_eq!(accepted.event, LaneEventName::ImageSceneAccepted);
        assert_eq!(accepted.status, LaneEventStatus::Completed);
        assert!(is_terminal_event(accepted.event));

        let rejected = LaneEvent::image_scene_rejected(
            "now",
            &sample_image_provenance(),
            LaneFailureClass::ImageRegressionGate,
            "anatomy_score below floor",
        );
        assert_eq!(rejected.event, LaneEventName::ImageSceneRejected);
        assert_eq!(rejected.status, LaneEventStatus::Failed);
        assert_eq!(
            rejected.failure_class,
            Some(LaneFailureClass::ImageRegressionGate)
        );
        assert_eq!(
            rejected.detail.as_deref(),
            Some("anatomy_score below floor")
        );
    }

    #[test]
    fn image_regression_summary_event_marks_release_gate_outcome() {
        let summary = ImageRegressionSummaryPayload {
            run_id: "iqh".to_string(),
            profile: "production".to_string(),
            scenes_total: 5,
            seeds_total: 20,
            accepted: 18,
            rejected: 2,
            errored: 0,
            pass_rate: 0.9,
            catastrophic_failure_rate: 0.0,
            release_gate_passed: true,
        };
        let event = LaneEvent::image_regression_summary("now", &summary);
        assert_eq!(event.event, LaneEventName::ImageRegressionSummary);
        assert_eq!(event.status, LaneEventStatus::Green);
        let payload = event.data.expect("payload");
        assert_eq!(payload["accepted"], 18);
        assert_eq!(payload["release_gate_passed"], true);
    }

    #[test]
    fn image_failure_classes_serialize_to_canonical_wire_values() {
        let cases = [
            (LaneFailureClass::ImageBackend, "image_backend"),
            (LaneFailureClass::ImageValidator, "image_validator"),
            (
                LaneFailureClass::ImageRegressionGate,
                "image_regression_gate",
            ),
        ];
        for (class, expected) in cases {
            assert_eq!(serde_json::to_value(class).expect("serialize"), expected);
        }
    }

    #[test]
    fn image_terminal_events_dedupe_when_repeated_in_window() {
        let provenance = sample_image_provenance();
        let one = LaneEventBuilder::new(
            LaneEventName::ImageSceneAccepted,
            LaneEventStatus::Completed,
            "t1",
            1,
            EventProvenance::LiveLane,
        )
        .with_data(serde_json::to_value(&provenance).unwrap())
        .build_terminal();
        let dup = LaneEventBuilder::new(
            LaneEventName::ImageSceneAccepted,
            LaneEventStatus::Completed,
            "t1",
            2,
            EventProvenance::LiveLane,
        )
        .with_data(serde_json::to_value(&provenance).unwrap())
        .build_terminal();

        let deduped = dedupe_terminal_events(&[one, dup]);
        assert_eq!(deduped.len(), 1);
    }
}

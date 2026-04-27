#![allow(clippy::cast_possible_truncation, clippy::uninlined_format_args)]
//! Recovery recipes for common failure scenarios.
//!
//! Encodes known automatic recoveries for the six failure scenarios
//! listed in ROADMAP item 8, and enforces one automatic recovery
//! attempt before escalation. Each attempt is emitted as a structured
//! recovery event.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::worker_boot::WorkerFailureKind;

/// The six failure scenarios that have known recovery recipes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureScenario {
    TrustPromptUnresolved,
    PromptMisdelivery,
    StaleBranch,
    CompileRedCrossCrate,
    McpHandshakeFailure,
    PartialPluginStartup,
    ProviderFailure,
    /// Image backend HTTP timeout / connection refused. Recoverable by
    /// retrying once with exponential backoff, then escalating.
    ImageBackendTimeout,
    /// Image backend reachable but reports degraded state — `model not
    /// loaded`, `queue full`, 503/Service Unavailable, etc. Recoverable
    /// by falling back to the next provider in the chain.
    ImageBackendDegraded,
    /// A validator endpoint declared by the regression fixture is missing
    /// or returns 404. Recoverable by skipping the axis (per-axis
    /// thresholds become advisory) and emitting a degraded-mode warning.
    ValidatorEndpointMissing,
}

impl FailureScenario {
    /// Returns all known failure scenarios.
    #[must_use]
    pub fn all() -> &'static [FailureScenario] {
        &[
            Self::TrustPromptUnresolved,
            Self::PromptMisdelivery,
            Self::StaleBranch,
            Self::CompileRedCrossCrate,
            Self::McpHandshakeFailure,
            Self::PartialPluginStartup,
            Self::ProviderFailure,
            Self::ImageBackendTimeout,
            Self::ImageBackendDegraded,
            Self::ValidatorEndpointMissing,
        ]
    }

    /// Map a `WorkerFailureKind` to the corresponding `FailureScenario`.
    /// This is the bridge that lets recovery policy consume worker boot events.
    #[must_use]
    pub fn from_worker_failure_kind(kind: WorkerFailureKind) -> Self {
        match kind {
            WorkerFailureKind::TrustGate => Self::TrustPromptUnresolved,
            WorkerFailureKind::PromptDelivery => Self::PromptMisdelivery,
            WorkerFailureKind::Protocol => Self::McpHandshakeFailure,
            WorkerFailureKind::Provider | WorkerFailureKind::StartupNoEvidence => {
                Self::ProviderFailure
            }
        }
    }

    /// Map an image-backend error string into the matching scenario.
    /// `None` is returned when the error doesn't look like an image-pipeline
    /// failure (caller should leave it alone).
    #[must_use]
    pub fn from_image_error(error: &str) -> Option<Self> {
        let lower = error.to_lowercase();
        if lower.contains("timed out")
            || lower.contains("timeout")
            || lower.contains("connection refused")
            || lower.contains("connection reset")
        {
            return Some(Self::ImageBackendTimeout);
        }
        if lower.contains("model not loaded")
            || lower.contains("queue full")
            || lower.contains("http 503")
            || lower.contains("http 502")
            || lower.contains("service unavailable")
            || lower.contains("backend degraded")
        {
            return Some(Self::ImageBackendDegraded);
        }
        if lower.contains("validator endpoint")
            && (lower.contains("missing") || lower.contains("404") || lower.contains("not found"))
        {
            return Some(Self::ValidatorEndpointMissing);
        }
        None
    }
}

impl std::fmt::Display for FailureScenario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TrustPromptUnresolved => write!(f, "trust_prompt_unresolved"),
            Self::PromptMisdelivery => write!(f, "prompt_misdelivery"),
            Self::StaleBranch => write!(f, "stale_branch"),
            Self::CompileRedCrossCrate => write!(f, "compile_red_cross_crate"),
            Self::McpHandshakeFailure => write!(f, "mcp_handshake_failure"),
            Self::PartialPluginStartup => write!(f, "partial_plugin_startup"),
            Self::ProviderFailure => write!(f, "provider_failure"),
            Self::ImageBackendTimeout => write!(f, "image_backend_timeout"),
            Self::ImageBackendDegraded => write!(f, "image_backend_degraded"),
            Self::ValidatorEndpointMissing => write!(f, "validator_endpoint_missing"),
        }
    }
}

/// Individual step that can be executed as part of a recovery recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStep {
    AcceptTrustPrompt,
    RedirectPromptToAgent,
    RebaseBranch,
    CleanBuild,
    RetryMcpHandshake {
        timeout: u64,
    },
    RestartPlugin {
        name: String,
    },
    RestartWorker,
    EscalateToHuman {
        reason: String,
    },
    /// Retry an image backend invocation after a backoff delay (ms).
    RetryImageBackend {
        backoff_ms: u64,
    },
    /// Switch from the current provider to the next adapter in the
    /// fallback chain (e.g. `comfyui` → `diffusers`).
    FallbackToNextProvider {
        from: String,
        to: String,
    },
    /// Skip an unhealthy validator axis so the rest of the gate can
    /// still run; emits a degraded-mode warning when applied.
    SkipValidatorAxis {
        axis: String,
    },
}

/// Policy governing what happens when automatic recovery is exhausted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EscalationPolicy {
    AlertHuman,
    LogAndContinue,
    Abort,
}

/// A recovery recipe encodes the sequence of steps to attempt for a
/// given failure scenario, along with the maximum number of automatic
/// attempts and the escalation policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryRecipe {
    pub scenario: FailureScenario,
    pub steps: Vec<RecoveryStep>,
    pub max_attempts: u32,
    pub escalation_policy: EscalationPolicy,
}

/// Outcome of a recovery attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryResult {
    Recovered {
        steps_taken: u32,
    },
    PartialRecovery {
        recovered: Vec<RecoveryStep>,
        remaining: Vec<RecoveryStep>,
    },
    EscalationRequired {
        reason: String,
    },
}

/// Structured event emitted during recovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryEvent {
    RecoveryAttempted {
        scenario: FailureScenario,
        recipe: RecoveryRecipe,
        result: RecoveryResult,
    },
    RecoverySucceeded,
    RecoveryFailed,
    Escalated,
}

/// Minimal context for tracking recovery state and emitting events.
///
/// Holds per-scenario attempt counts, a structured event log, and an
/// optional simulation knob for controlling step outcomes during tests.
#[derive(Debug, Clone, Default)]
pub struct RecoveryContext {
    attempts: HashMap<FailureScenario, u32>,
    events: Vec<RecoveryEvent>,
    /// Optional step index at which simulated execution fails.
    /// `None` means all steps succeed.
    fail_at_step: Option<usize>,
}

impl RecoveryContext {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure a step index at which simulated execution will fail.
    #[must_use]
    pub fn with_fail_at_step(mut self, index: usize) -> Self {
        self.fail_at_step = Some(index);
        self
    }

    /// Returns the structured event log populated during recovery.
    #[must_use]
    pub fn events(&self) -> &[RecoveryEvent] {
        &self.events
    }

    /// Returns the number of recovery attempts made for a scenario.
    #[must_use]
    pub fn attempt_count(&self, scenario: &FailureScenario) -> u32 {
        self.attempts.get(scenario).copied().unwrap_or(0)
    }
}

/// Returns the known recovery recipe for the given failure scenario.
#[must_use]
pub fn recipe_for(scenario: &FailureScenario) -> RecoveryRecipe {
    match scenario {
        FailureScenario::TrustPromptUnresolved => RecoveryRecipe {
            scenario: *scenario,
            steps: vec![RecoveryStep::AcceptTrustPrompt],
            max_attempts: 1,
            escalation_policy: EscalationPolicy::AlertHuman,
        },
        FailureScenario::PromptMisdelivery => RecoveryRecipe {
            scenario: *scenario,
            steps: vec![RecoveryStep::RedirectPromptToAgent],
            max_attempts: 1,
            escalation_policy: EscalationPolicy::AlertHuman,
        },
        FailureScenario::StaleBranch => RecoveryRecipe {
            scenario: *scenario,
            steps: vec![RecoveryStep::RebaseBranch, RecoveryStep::CleanBuild],
            max_attempts: 1,
            escalation_policy: EscalationPolicy::AlertHuman,
        },
        FailureScenario::CompileRedCrossCrate => RecoveryRecipe {
            scenario: *scenario,
            steps: vec![RecoveryStep::CleanBuild],
            max_attempts: 1,
            escalation_policy: EscalationPolicy::AlertHuman,
        },
        FailureScenario::McpHandshakeFailure => RecoveryRecipe {
            scenario: *scenario,
            steps: vec![RecoveryStep::RetryMcpHandshake { timeout: 5000 }],
            max_attempts: 1,
            escalation_policy: EscalationPolicy::Abort,
        },
        FailureScenario::PartialPluginStartup => RecoveryRecipe {
            scenario: *scenario,
            steps: vec![
                RecoveryStep::RestartPlugin {
                    name: "stalled".to_string(),
                },
                RecoveryStep::RetryMcpHandshake { timeout: 3000 },
            ],
            max_attempts: 1,
            escalation_policy: EscalationPolicy::LogAndContinue,
        },
        FailureScenario::ProviderFailure => RecoveryRecipe {
            scenario: *scenario,
            steps: vec![RecoveryStep::RestartWorker],
            max_attempts: 1,
            escalation_policy: EscalationPolicy::AlertHuman,
        },
        FailureScenario::ImageBackendTimeout => RecoveryRecipe {
            scenario: *scenario,
            steps: vec![RecoveryStep::RetryImageBackend { backoff_ms: 2_000 }],
            max_attempts: 1,
            escalation_policy: EscalationPolicy::AlertHuman,
        },
        FailureScenario::ImageBackendDegraded => RecoveryRecipe {
            scenario: *scenario,
            steps: vec![RecoveryStep::FallbackToNextProvider {
                from: "primary".to_string(),
                to: "fallback".to_string(),
            }],
            max_attempts: 1,
            escalation_policy: EscalationPolicy::AlertHuman,
        },
        FailureScenario::ValidatorEndpointMissing => RecoveryRecipe {
            scenario: *scenario,
            steps: vec![RecoveryStep::SkipValidatorAxis {
                axis: "unknown".to_string(),
            }],
            max_attempts: 1,
            escalation_policy: EscalationPolicy::LogAndContinue,
        },
    }
}

/// Attempts automatic recovery for the given failure scenario.
///
/// Looks up the recipe, enforces the one-attempt-before-escalation
/// policy, simulates step execution (controlled by the context), and
/// emits structured [`RecoveryEvent`]s for every attempt.
pub fn attempt_recovery(scenario: &FailureScenario, ctx: &mut RecoveryContext) -> RecoveryResult {
    let recipe = recipe_for(scenario);
    let attempt_count = ctx.attempts.entry(*scenario).or_insert(0);

    // Enforce one automatic recovery attempt before escalation.
    if *attempt_count >= recipe.max_attempts {
        let result = RecoveryResult::EscalationRequired {
            reason: format!(
                "max recovery attempts ({}) exceeded for {}",
                recipe.max_attempts, scenario
            ),
        };
        ctx.events.push(RecoveryEvent::RecoveryAttempted {
            scenario: *scenario,
            recipe,
            result: result.clone(),
        });
        ctx.events.push(RecoveryEvent::Escalated);
        return result;
    }

    *attempt_count += 1;

    // Execute steps, honoring the optional fail_at_step simulation.
    let fail_index = ctx.fail_at_step;
    let mut executed = Vec::new();
    let mut failed = false;

    for (i, step) in recipe.steps.iter().enumerate() {
        if fail_index == Some(i) {
            failed = true;
            break;
        }
        executed.push(step.clone());
    }

    let result = if failed {
        let remaining: Vec<RecoveryStep> = recipe.steps[executed.len()..].to_vec();
        if executed.is_empty() {
            RecoveryResult::EscalationRequired {
                reason: format!("recovery failed at first step for {}", scenario),
            }
        } else {
            RecoveryResult::PartialRecovery {
                recovered: executed,
                remaining,
            }
        }
    } else {
        RecoveryResult::Recovered {
            steps_taken: recipe.steps.len() as u32,
        }
    };

    // Emit the attempt as structured event data.
    ctx.events.push(RecoveryEvent::RecoveryAttempted {
        scenario: *scenario,
        recipe,
        result: result.clone(),
    });

    match &result {
        RecoveryResult::Recovered { .. } => {
            ctx.events.push(RecoveryEvent::RecoverySucceeded);
        }
        RecoveryResult::PartialRecovery { .. } => {
            ctx.events.push(RecoveryEvent::RecoveryFailed);
        }
        RecoveryResult::EscalationRequired { .. } => {
            ctx.events.push(RecoveryEvent::Escalated);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_scenario_has_a_matching_recipe() {
        // given
        let scenarios = FailureScenario::all();

        // when / then
        for scenario in scenarios {
            let recipe = recipe_for(scenario);
            assert_eq!(
                recipe.scenario, *scenario,
                "recipe scenario should match requested scenario"
            );
            assert!(
                !recipe.steps.is_empty(),
                "recipe for {} should have at least one step",
                scenario
            );
            assert!(
                recipe.max_attempts >= 1,
                "recipe for {} should allow at least one attempt",
                scenario
            );
        }
    }

    #[test]
    fn successful_recovery_returns_recovered_and_emits_events() {
        // given
        let mut ctx = RecoveryContext::new();
        let scenario = FailureScenario::TrustPromptUnresolved;

        // when
        let result = attempt_recovery(&scenario, &mut ctx);

        // then
        assert_eq!(result, RecoveryResult::Recovered { steps_taken: 1 });
        assert_eq!(ctx.events().len(), 2);
        assert!(matches!(
            &ctx.events()[0],
            RecoveryEvent::RecoveryAttempted {
                scenario: s,
                result: r,
                ..
            } if *s == FailureScenario::TrustPromptUnresolved
              && matches!(r, RecoveryResult::Recovered { steps_taken: 1 })
        ));
        assert_eq!(ctx.events()[1], RecoveryEvent::RecoverySucceeded);
    }

    #[test]
    fn escalation_after_max_attempts_exceeded() {
        // given
        let mut ctx = RecoveryContext::new();
        let scenario = FailureScenario::PromptMisdelivery;

        // when — first attempt succeeds
        let first = attempt_recovery(&scenario, &mut ctx);
        assert!(matches!(first, RecoveryResult::Recovered { .. }));

        // when — second attempt should escalate
        let second = attempt_recovery(&scenario, &mut ctx);

        // then
        assert!(
            matches!(
                &second,
                RecoveryResult::EscalationRequired { reason }
                    if reason.contains("max recovery attempts")
            ),
            "second attempt should require escalation, got: {second:?}"
        );
        assert_eq!(ctx.attempt_count(&scenario), 1);
        assert!(ctx
            .events()
            .iter()
            .any(|e| matches!(e, RecoveryEvent::Escalated)));
    }

    #[test]
    fn partial_recovery_when_step_fails_midway() {
        // given — PartialPluginStartup has two steps; fail at step index 1
        let mut ctx = RecoveryContext::new().with_fail_at_step(1);
        let scenario = FailureScenario::PartialPluginStartup;

        // when
        let result = attempt_recovery(&scenario, &mut ctx);

        // then
        match &result {
            RecoveryResult::PartialRecovery {
                recovered,
                remaining,
            } => {
                assert_eq!(recovered.len(), 1, "one step should have succeeded");
                assert_eq!(remaining.len(), 1, "one step should remain");
                assert!(matches!(recovered[0], RecoveryStep::RestartPlugin { .. }));
                assert!(matches!(
                    remaining[0],
                    RecoveryStep::RetryMcpHandshake { .. }
                ));
            }
            other => panic!("expected PartialRecovery, got {other:?}"),
        }
        assert!(ctx
            .events()
            .iter()
            .any(|e| matches!(e, RecoveryEvent::RecoveryFailed)));
    }

    #[test]
    fn first_step_failure_escalates_immediately() {
        // given — fail at step index 0
        let mut ctx = RecoveryContext::new().with_fail_at_step(0);
        let scenario = FailureScenario::CompileRedCrossCrate;

        // when
        let result = attempt_recovery(&scenario, &mut ctx);

        // then
        assert!(
            matches!(
                &result,
                RecoveryResult::EscalationRequired { reason }
                    if reason.contains("failed at first step")
            ),
            "zero-step failure should escalate, got: {result:?}"
        );
        assert!(ctx
            .events()
            .iter()
            .any(|e| matches!(e, RecoveryEvent::Escalated)));
    }

    #[test]
    fn emitted_events_include_structured_attempt_data() {
        // given
        let mut ctx = RecoveryContext::new();
        let scenario = FailureScenario::McpHandshakeFailure;

        // when
        let _ = attempt_recovery(&scenario, &mut ctx);

        // then — verify the RecoveryAttempted event carries full context
        let attempted = ctx
            .events()
            .iter()
            .find(|e| matches!(e, RecoveryEvent::RecoveryAttempted { .. }))
            .expect("should have emitted RecoveryAttempted event");

        match attempted {
            RecoveryEvent::RecoveryAttempted {
                scenario: s,
                recipe,
                result,
            } => {
                assert_eq!(*s, scenario);
                assert_eq!(recipe.scenario, scenario);
                assert!(!recipe.steps.is_empty());
                assert!(matches!(result, RecoveryResult::Recovered { .. }));
            }
            _ => unreachable!(),
        }

        // Verify the event is serializable as structured JSON
        let json = serde_json::to_string(&ctx.events()[0])
            .expect("recovery event should be serializable to JSON");
        assert!(
            json.contains("mcp_handshake_failure"),
            "serialized event should contain scenario name"
        );
    }

    #[test]
    fn recovery_context_tracks_attempts_per_scenario() {
        // given
        let mut ctx = RecoveryContext::new();

        // when
        assert_eq!(ctx.attempt_count(&FailureScenario::StaleBranch), 0);
        attempt_recovery(&FailureScenario::StaleBranch, &mut ctx);

        // then
        assert_eq!(ctx.attempt_count(&FailureScenario::StaleBranch), 1);
        assert_eq!(ctx.attempt_count(&FailureScenario::PromptMisdelivery), 0);
    }

    #[test]
    fn stale_branch_recipe_has_rebase_then_clean_build() {
        // given
        let recipe = recipe_for(&FailureScenario::StaleBranch);

        // then
        assert_eq!(recipe.steps.len(), 2);
        assert_eq!(recipe.steps[0], RecoveryStep::RebaseBranch);
        assert_eq!(recipe.steps[1], RecoveryStep::CleanBuild);
    }

    #[test]
    fn partial_plugin_startup_recipe_has_restart_then_handshake() {
        // given
        let recipe = recipe_for(&FailureScenario::PartialPluginStartup);

        // then
        assert_eq!(recipe.steps.len(), 2);
        assert!(matches!(
            recipe.steps[0],
            RecoveryStep::RestartPlugin { .. }
        ));
        assert!(matches!(
            recipe.steps[1],
            RecoveryStep::RetryMcpHandshake { timeout: 3000 }
        ));
        assert_eq!(recipe.escalation_policy, EscalationPolicy::LogAndContinue);
    }

    #[test]
    fn failure_scenario_display_all_variants() {
        // given
        let cases = [
            (
                FailureScenario::TrustPromptUnresolved,
                "trust_prompt_unresolved",
            ),
            (FailureScenario::PromptMisdelivery, "prompt_misdelivery"),
            (FailureScenario::StaleBranch, "stale_branch"),
            (
                FailureScenario::CompileRedCrossCrate,
                "compile_red_cross_crate",
            ),
            (
                FailureScenario::McpHandshakeFailure,
                "mcp_handshake_failure",
            ),
            (
                FailureScenario::PartialPluginStartup,
                "partial_plugin_startup",
            ),
        ];

        // when / then
        for (scenario, expected) in &cases {
            assert_eq!(scenario.to_string(), *expected);
        }
    }

    #[test]
    fn multi_step_success_reports_correct_steps_taken() {
        // given — StaleBranch has 2 steps, no simulated failure
        let mut ctx = RecoveryContext::new();
        let scenario = FailureScenario::StaleBranch;

        // when
        let result = attempt_recovery(&scenario, &mut ctx);

        // then
        assert_eq!(result, RecoveryResult::Recovered { steps_taken: 2 });
    }

    #[test]
    fn mcp_handshake_recipe_uses_abort_escalation_policy() {
        // given
        let recipe = recipe_for(&FailureScenario::McpHandshakeFailure);

        // then
        assert_eq!(recipe.escalation_policy, EscalationPolicy::Abort);
        assert_eq!(recipe.max_attempts, 1);
    }

    #[test]
    fn worker_failure_kind_maps_to_failure_scenario() {
        // given / when / then — verify the bridge is correct
        assert_eq!(
            FailureScenario::from_worker_failure_kind(WorkerFailureKind::TrustGate),
            FailureScenario::TrustPromptUnresolved,
        );
        assert_eq!(
            FailureScenario::from_worker_failure_kind(WorkerFailureKind::PromptDelivery),
            FailureScenario::PromptMisdelivery,
        );
        assert_eq!(
            FailureScenario::from_worker_failure_kind(WorkerFailureKind::Protocol),
            FailureScenario::McpHandshakeFailure,
        );
        assert_eq!(
            FailureScenario::from_worker_failure_kind(WorkerFailureKind::Provider),
            FailureScenario::ProviderFailure,
        );
    }

    #[test]
    fn provider_failure_recipe_uses_restart_worker_step() {
        // given
        let recipe = recipe_for(&FailureScenario::ProviderFailure);

        // then
        assert_eq!(recipe.scenario, FailureScenario::ProviderFailure);
        assert!(recipe.steps.contains(&RecoveryStep::RestartWorker));
        assert_eq!(recipe.escalation_policy, EscalationPolicy::AlertHuman);
        assert_eq!(recipe.max_attempts, 1);
    }

    #[test]
    fn provider_failure_recovery_attempt_succeeds_then_escalates() {
        // given
        let mut ctx = RecoveryContext::new();
        let scenario = FailureScenario::ProviderFailure;

        // when — first attempt
        let first = attempt_recovery(&scenario, &mut ctx);
        assert!(matches!(first, RecoveryResult::Recovered { .. }));

        // when — second attempt should escalate (max_attempts=1)
        let second = attempt_recovery(&scenario, &mut ctx);
        assert!(matches!(second, RecoveryResult::EscalationRequired { .. }));
        assert!(ctx
            .events()
            .iter()
            .any(|e| matches!(e, RecoveryEvent::Escalated)));
    }

    #[test]
    fn image_backend_timeout_recipe_uses_retry_with_backoff() {
        let recipe = recipe_for(&FailureScenario::ImageBackendTimeout);
        assert_eq!(recipe.scenario, FailureScenario::ImageBackendTimeout);
        assert_eq!(recipe.steps.len(), 1);
        assert!(matches!(
            recipe.steps[0],
            RecoveryStep::RetryImageBackend { backoff_ms: 2_000 }
        ));
        assert_eq!(recipe.escalation_policy, EscalationPolicy::AlertHuman);
        assert_eq!(recipe.max_attempts, 1);
    }

    #[test]
    fn image_backend_degraded_recipe_falls_back_to_next_provider() {
        let recipe = recipe_for(&FailureScenario::ImageBackendDegraded);
        assert_eq!(recipe.steps.len(), 1);
        assert!(matches!(
            &recipe.steps[0],
            RecoveryStep::FallbackToNextProvider { .. }
        ));
    }

    #[test]
    fn validator_endpoint_missing_recipe_skips_axis_and_logs() {
        let recipe = recipe_for(&FailureScenario::ValidatorEndpointMissing);
        assert_eq!(recipe.steps.len(), 1);
        assert!(matches!(
            &recipe.steps[0],
            RecoveryStep::SkipValidatorAxis { .. }
        ));
        assert_eq!(recipe.escalation_policy, EscalationPolicy::LogAndContinue);
    }

    #[test]
    fn from_image_error_classifies_timeout_degraded_and_missing_endpoint() {
        assert_eq!(
            FailureScenario::from_image_error("request timed out after 60s"),
            Some(FailureScenario::ImageBackendTimeout),
        );
        assert_eq!(
            FailureScenario::from_image_error("connection refused (os error 111)"),
            Some(FailureScenario::ImageBackendTimeout),
        );
        assert_eq!(
            FailureScenario::from_image_error("backend HTTP 503: model not loaded"),
            Some(FailureScenario::ImageBackendDegraded),
        );
        assert_eq!(
            FailureScenario::from_image_error("queue full at upstream worker"),
            Some(FailureScenario::ImageBackendDegraded),
        );
        assert_eq!(
            FailureScenario::from_image_error("validator endpoint missing for symmetry axis"),
            Some(FailureScenario::ValidatorEndpointMissing),
        );
        assert_eq!(
            FailureScenario::from_image_error("validator endpoint returned 404"),
            Some(FailureScenario::ValidatorEndpointMissing),
        );
        assert_eq!(
            FailureScenario::from_image_error("merge conflict on README.md"),
            None,
        );
    }

    #[test]
    fn image_backend_timeout_recovery_attempt_succeeds_then_escalates() {
        let mut ctx = RecoveryContext::new();
        let scenario = FailureScenario::ImageBackendTimeout;

        let first = attempt_recovery(&scenario, &mut ctx);
        assert!(matches!(first, RecoveryResult::Recovered { .. }));

        let second = attempt_recovery(&scenario, &mut ctx);
        assert!(matches!(second, RecoveryResult::EscalationRequired { .. }));
    }

    #[test]
    fn validator_endpoint_missing_recovery_uses_log_and_continue_after_exhaustion() {
        // LogAndContinue scenarios still report `EscalationRequired` once the
        // attempt budget is exhausted; the policy field signals that callers
        // should keep going rather than abort.
        let mut ctx = RecoveryContext::new();
        let scenario = FailureScenario::ValidatorEndpointMissing;
        let _first = attempt_recovery(&scenario, &mut ctx);
        let second = attempt_recovery(&scenario, &mut ctx);
        assert!(matches!(second, RecoveryResult::EscalationRequired { .. }));
        assert_eq!(
            recipe_for(&scenario).escalation_policy,
            EscalationPolicy::LogAndContinue
        );
    }

    #[test]
    fn all_image_scenarios_are_listed_in_failure_scenario_all() {
        let all = FailureScenario::all();
        for scenario in [
            FailureScenario::ImageBackendTimeout,
            FailureScenario::ImageBackendDegraded,
            FailureScenario::ValidatorEndpointMissing,
        ] {
            assert!(
                all.contains(&scenario),
                "FailureScenario::all() missing {scenario}"
            );
        }
    }
}

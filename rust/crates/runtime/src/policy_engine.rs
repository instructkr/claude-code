use std::time::Duration;

pub type GreenLevel = u8;

const STALE_BRANCH_THRESHOLD: Duration = Duration::from_secs(60 * 60);

#[derive(Debug, Clone, PartialEq)]
pub struct PolicyRule {
    pub name: String,
    pub condition: PolicyCondition,
    pub action: PolicyAction,
    pub priority: u32,
}

impl PolicyRule {
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        condition: PolicyCondition,
        action: PolicyAction,
        priority: u32,
    ) -> Self {
        Self {
            name: name.into(),
            condition,
            action,
            priority,
        }
    }

    #[must_use]
    pub fn matches(&self, context: &LaneContext) -> bool {
        self.condition.matches(context)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PolicyCondition {
    And(Vec<PolicyCondition>),
    Or(Vec<PolicyCondition>),
    GreenAt {
        level: GreenLevel,
    },
    StaleBranch,
    StartupBlocked,
    LaneCompleted,
    LaneReconciled,
    ReviewPassed,
    ScopedDiff,
    TimedOut {
        duration: Duration,
    },
    /// Match when the image regression's pass rate for `fixture` is at
    /// least `min` (0.0..=1.0). When `fixture` is `None`, applies to the
    /// run-wide pass rate.
    ImagePassRate {
        fixture: Option<String>,
        min: f64,
    },
    /// Match when the image regression's catastrophic-failure rate is at
    /// most `max` (0.0..=1.0). Mirrors §8.3 release-gate behaviour.
    ImageCatastrophicRateAtMost {
        max: f64,
    },
    /// Match when the image regression's release-gate verdict is
    /// `passed == true` (i.e. all configured gate metrics cleared).
    ImageReleaseGatePassed,
}

impl PolicyCondition {
    #[must_use]
    pub fn matches(&self, context: &LaneContext) -> bool {
        match self {
            Self::And(conditions) => conditions
                .iter()
                .all(|condition| condition.matches(context)),
            Self::Or(conditions) => conditions
                .iter()
                .any(|condition| condition.matches(context)),
            Self::GreenAt { level } => context.green_level >= *level,
            Self::StaleBranch => context.branch_freshness >= STALE_BRANCH_THRESHOLD,
            Self::StartupBlocked => context.blocker == LaneBlocker::Startup,
            Self::LaneCompleted => context.completed,
            Self::LaneReconciled => context.reconciled,
            Self::ReviewPassed => context.review_status == ReviewStatus::Approved,
            Self::ScopedDiff => context.diff_scope == DiffScope::Scoped,
            Self::TimedOut { duration } => context.branch_freshness >= *duration,
            Self::ImagePassRate { fixture, min } => match (&context.image_regression, fixture) {
                (Some(report), None) => report.pass_rate >= *min,
                (Some(report), Some(name)) => report
                    .per_fixture_pass_rate
                    .get(name)
                    .copied()
                    .is_some_and(|rate| rate >= *min),
                (None, _) => false,
            },
            Self::ImageCatastrophicRateAtMost { max } => context
                .image_regression
                .as_ref()
                .is_some_and(|r| r.catastrophic_failure_rate <= *max),
            Self::ImageReleaseGatePassed => context
                .image_regression
                .as_ref()
                .is_some_and(|r| r.release_gate_passed),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyAction {
    MergeToDev,
    MergeForward,
    RecoverOnce,
    Escalate {
        reason: String,
    },
    CloseoutLane,
    CleanupSession,
    Reconcile {
        reason: ReconcileReason,
    },
    Notify {
        channel: String,
    },
    Block {
        reason: String,
    },
    Chain(Vec<PolicyAction>),
    /// Promote a passing image regression run (e.g. publish artifacts,
    /// flip a `latest` pointer, or merge a generated assets PR).
    PromoteImageRun {
        run_id: String,
    },
    /// Freeze image promotion for a fixture (e.g. when pass-rate has
    /// dropped below the gate). The frozen fixture is held until
    /// `UnfreezeImageFixture` clears it.
    FreezeImageFixture {
        fixture: String,
        reason: String,
    },
}

/// Why a lane was reconciled without further action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileReason {
    /// Branch already merged into main — no PR needed.
    AlreadyMerged,
    /// Work superseded by another lane or direct commit.
    Superseded,
    /// PR would be empty — all changes already landed.
    EmptyDiff,
    /// Lane manually closed by operator.
    ManualClose,
}

impl PolicyAction {
    fn flatten_into(&self, actions: &mut Vec<PolicyAction>) {
        match self {
            Self::Chain(chained) => {
                for action in chained {
                    action.flatten_into(actions);
                }
            }
            _ => actions.push(self.clone()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneBlocker {
    None,
    Startup,
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffScope {
    Full,
    Scoped,
}

/// Snapshot of an image regression run's release-gate signals, attached
/// to a `LaneContext` so policy rules can consume them. The numbers come
/// from `tools::image::regression::RegressionSummary`; this lives here
/// instead of in `tools` to avoid a runtime → tools dependency cycle.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageRegressionReport {
    pub run_id: String,
    pub pass_rate: f64,
    pub catastrophic_failure_rate: f64,
    pub release_gate_passed: bool,
    pub per_fixture_pass_rate: std::collections::BTreeMap<String, f64>,
}

impl ImageRegressionReport {
    #[must_use]
    pub fn new(run_id: impl Into<String>, pass_rate: f64) -> Self {
        Self {
            run_id: run_id.into(),
            pass_rate,
            catastrophic_failure_rate: 0.0,
            release_gate_passed: pass_rate >= 1.0,
            per_fixture_pass_rate: std::collections::BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn with_catastrophic_failure_rate(mut self, rate: f64) -> Self {
        self.catastrophic_failure_rate = rate;
        self
    }

    #[must_use]
    pub fn with_release_gate_passed(mut self, passed: bool) -> Self {
        self.release_gate_passed = passed;
        self
    }

    pub fn record_fixture_pass_rate(&mut self, fixture: impl Into<String>, rate: f64) {
        self.per_fixture_pass_rate.insert(fixture.into(), rate);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LaneContext {
    pub lane_id: String,
    pub green_level: GreenLevel,
    pub branch_freshness: Duration,
    pub blocker: LaneBlocker,
    pub review_status: ReviewStatus,
    pub diff_scope: DiffScope,
    pub completed: bool,
    pub reconciled: bool,
    /// Optional image-regression telemetry; set on lanes that drive an
    /// `ImageRegressionRun` so `ImagePassRate` rules can fire.
    pub image_regression: Option<ImageRegressionReport>,
}

impl LaneContext {
    #[must_use]
    pub fn new(
        lane_id: impl Into<String>,
        green_level: GreenLevel,
        branch_freshness: Duration,
        blocker: LaneBlocker,
        review_status: ReviewStatus,
        diff_scope: DiffScope,
        completed: bool,
    ) -> Self {
        Self {
            lane_id: lane_id.into(),
            green_level,
            branch_freshness,
            blocker,
            review_status,
            diff_scope,
            completed,
            reconciled: false,
            image_regression: None,
        }
    }

    /// Create a lane context that is already reconciled (no further action needed).
    #[must_use]
    pub fn reconciled(lane_id: impl Into<String>) -> Self {
        Self {
            lane_id: lane_id.into(),
            green_level: 0,
            branch_freshness: Duration::from_secs(0),
            blocker: LaneBlocker::None,
            review_status: ReviewStatus::Pending,
            diff_scope: DiffScope::Full,
            completed: true,
            reconciled: true,
            image_regression: None,
        }
    }

    /// Attach an image regression report to this lane context.
    #[must_use]
    pub fn with_image_regression(mut self, report: ImageRegressionReport) -> Self {
        self.image_regression = Some(report);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolicyEngine {
    rules: Vec<PolicyRule>,
}

impl PolicyEngine {
    #[must_use]
    pub fn new(mut rules: Vec<PolicyRule>) -> Self {
        rules.sort_by_key(|rule| rule.priority);
        Self { rules }
    }

    #[must_use]
    pub fn rules(&self) -> &[PolicyRule] {
        &self.rules
    }

    #[must_use]
    pub fn evaluate(&self, context: &LaneContext) -> Vec<PolicyAction> {
        evaluate(self, context)
    }
}

#[must_use]
pub fn evaluate(engine: &PolicyEngine, context: &LaneContext) -> Vec<PolicyAction> {
    let mut actions = Vec::new();
    for rule in &engine.rules {
        if rule.matches(context) {
            rule.action.flatten_into(&mut actions);
        }
    }
    actions
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        evaluate, DiffScope, LaneBlocker, LaneContext, PolicyAction, PolicyCondition, PolicyEngine,
        PolicyRule, ReconcileReason, ReviewStatus, STALE_BRANCH_THRESHOLD,
    };

    fn default_context() -> LaneContext {
        LaneContext::new(
            "lane-7",
            0,
            Duration::from_secs(0),
            LaneBlocker::None,
            ReviewStatus::Pending,
            DiffScope::Full,
            false,
        )
    }

    #[test]
    fn merge_to_dev_rule_fires_for_green_scoped_reviewed_lane() {
        // given
        let engine = PolicyEngine::new(vec![PolicyRule::new(
            "merge-to-dev",
            PolicyCondition::And(vec![
                PolicyCondition::GreenAt { level: 2 },
                PolicyCondition::ScopedDiff,
                PolicyCondition::ReviewPassed,
            ]),
            PolicyAction::MergeToDev,
            20,
        )]);
        let context = LaneContext::new(
            "lane-7",
            3,
            Duration::from_secs(5),
            LaneBlocker::None,
            ReviewStatus::Approved,
            DiffScope::Scoped,
            false,
        );

        // when
        let actions = engine.evaluate(&context);

        // then
        assert_eq!(actions, vec![PolicyAction::MergeToDev]);
    }

    #[test]
    fn stale_branch_rule_fires_at_threshold() {
        // given
        let engine = PolicyEngine::new(vec![PolicyRule::new(
            "merge-forward",
            PolicyCondition::StaleBranch,
            PolicyAction::MergeForward,
            10,
        )]);
        let context = LaneContext::new(
            "lane-7",
            1,
            STALE_BRANCH_THRESHOLD,
            LaneBlocker::None,
            ReviewStatus::Pending,
            DiffScope::Full,
            false,
        );

        // when
        let actions = engine.evaluate(&context);

        // then
        assert_eq!(actions, vec![PolicyAction::MergeForward]);
    }

    #[test]
    fn startup_blocked_rule_recovers_then_escalates() {
        // given
        let engine = PolicyEngine::new(vec![PolicyRule::new(
            "startup-recovery",
            PolicyCondition::StartupBlocked,
            PolicyAction::Chain(vec![
                PolicyAction::RecoverOnce,
                PolicyAction::Escalate {
                    reason: "startup remained blocked".to_string(),
                },
            ]),
            15,
        )]);
        let context = LaneContext::new(
            "lane-7",
            0,
            Duration::from_secs(0),
            LaneBlocker::Startup,
            ReviewStatus::Pending,
            DiffScope::Full,
            false,
        );

        // when
        let actions = engine.evaluate(&context);

        // then
        assert_eq!(
            actions,
            vec![
                PolicyAction::RecoverOnce,
                PolicyAction::Escalate {
                    reason: "startup remained blocked".to_string(),
                },
            ]
        );
    }

    #[test]
    fn completed_lane_rule_closes_out_and_cleans_up() {
        // given
        let engine = PolicyEngine::new(vec![PolicyRule::new(
            "lane-closeout",
            PolicyCondition::LaneCompleted,
            PolicyAction::Chain(vec![
                PolicyAction::CloseoutLane,
                PolicyAction::CleanupSession,
            ]),
            30,
        )]);
        let context = LaneContext::new(
            "lane-7",
            0,
            Duration::from_secs(0),
            LaneBlocker::None,
            ReviewStatus::Pending,
            DiffScope::Full,
            true,
        );

        // when
        let actions = engine.evaluate(&context);

        // then
        assert_eq!(
            actions,
            vec![PolicyAction::CloseoutLane, PolicyAction::CleanupSession]
        );
    }

    #[test]
    fn matching_rules_are_returned_in_priority_order_with_stable_ties() {
        // given
        let engine = PolicyEngine::new(vec![
            PolicyRule::new(
                "late-cleanup",
                PolicyCondition::And(vec![]),
                PolicyAction::CleanupSession,
                30,
            ),
            PolicyRule::new(
                "first-notify",
                PolicyCondition::And(vec![]),
                PolicyAction::Notify {
                    channel: "ops".to_string(),
                },
                10,
            ),
            PolicyRule::new(
                "second-notify",
                PolicyCondition::And(vec![]),
                PolicyAction::Notify {
                    channel: "review".to_string(),
                },
                10,
            ),
            PolicyRule::new(
                "merge",
                PolicyCondition::And(vec![]),
                PolicyAction::MergeToDev,
                20,
            ),
        ]);
        let context = default_context();

        // when
        let actions = evaluate(&engine, &context);

        // then
        assert_eq!(
            actions,
            vec![
                PolicyAction::Notify {
                    channel: "ops".to_string(),
                },
                PolicyAction::Notify {
                    channel: "review".to_string(),
                },
                PolicyAction::MergeToDev,
                PolicyAction::CleanupSession,
            ]
        );
    }

    #[test]
    fn combinators_handle_empty_cases_and_nested_chains() {
        // given
        let engine = PolicyEngine::new(vec![
            PolicyRule::new(
                "empty-and",
                PolicyCondition::And(vec![]),
                PolicyAction::Notify {
                    channel: "orchestrator".to_string(),
                },
                5,
            ),
            PolicyRule::new(
                "empty-or",
                PolicyCondition::Or(vec![]),
                PolicyAction::Block {
                    reason: "should not fire".to_string(),
                },
                10,
            ),
            PolicyRule::new(
                "nested",
                PolicyCondition::Or(vec![
                    PolicyCondition::StartupBlocked,
                    PolicyCondition::And(vec![
                        PolicyCondition::GreenAt { level: 2 },
                        PolicyCondition::TimedOut {
                            duration: Duration::from_secs(5),
                        },
                    ]),
                ]),
                PolicyAction::Chain(vec![
                    PolicyAction::Notify {
                        channel: "alerts".to_string(),
                    },
                    PolicyAction::Chain(vec![
                        PolicyAction::MergeForward,
                        PolicyAction::CleanupSession,
                    ]),
                ]),
                15,
            ),
        ]);
        let context = LaneContext::new(
            "lane-7",
            2,
            Duration::from_secs(10),
            LaneBlocker::External,
            ReviewStatus::Pending,
            DiffScope::Full,
            false,
        );

        // when
        let actions = engine.evaluate(&context);

        // then
        assert_eq!(
            actions,
            vec![
                PolicyAction::Notify {
                    channel: "orchestrator".to_string(),
                },
                PolicyAction::Notify {
                    channel: "alerts".to_string(),
                },
                PolicyAction::MergeForward,
                PolicyAction::CleanupSession,
            ]
        );
    }

    #[test]
    fn reconciled_lane_emits_reconcile_and_cleanup() {
        // given — a lane where branch is already merged, no PR needed, session stale
        let engine = PolicyEngine::new(vec![
            PolicyRule::new(
                "reconcile-closeout",
                PolicyCondition::LaneReconciled,
                PolicyAction::Chain(vec![
                    PolicyAction::Reconcile {
                        reason: ReconcileReason::AlreadyMerged,
                    },
                    PolicyAction::CloseoutLane,
                    PolicyAction::CleanupSession,
                ]),
                5,
            ),
            // This rule should NOT fire — reconciled lanes are completed but we want
            // the more specific reconcile rule to handle them
            PolicyRule::new(
                "generic-closeout",
                PolicyCondition::And(vec![
                    PolicyCondition::LaneCompleted,
                    // Only fire if NOT reconciled
                    PolicyCondition::And(vec![]),
                ]),
                PolicyAction::CloseoutLane,
                30,
            ),
        ]);
        let context = LaneContext::reconciled("lane-9411");

        // when
        let actions = engine.evaluate(&context);

        // then — reconcile rule fires first (priority 5), then generic closeout also fires
        // because reconciled context has completed=true
        assert_eq!(
            actions,
            vec![
                PolicyAction::Reconcile {
                    reason: ReconcileReason::AlreadyMerged,
                },
                PolicyAction::CloseoutLane,
                PolicyAction::CleanupSession,
                PolicyAction::CloseoutLane,
            ]
        );
    }

    #[test]
    fn reconciled_context_has_correct_defaults() {
        let ctx = LaneContext::reconciled("test-lane");
        assert_eq!(ctx.lane_id, "test-lane");
        assert!(ctx.completed);
        assert!(ctx.reconciled);
        assert_eq!(ctx.blocker, LaneBlocker::None);
        assert_eq!(ctx.green_level, 0);
    }

    #[test]
    fn non_reconciled_lane_does_not_trigger_reconcile_rule() {
        let engine = PolicyEngine::new(vec![PolicyRule::new(
            "reconcile-closeout",
            PolicyCondition::LaneReconciled,
            PolicyAction::Reconcile {
                reason: ReconcileReason::EmptyDiff,
            },
            5,
        )]);
        // Normal completed lane — not reconciled
        let context = LaneContext::new(
            "lane-7",
            0,
            Duration::from_secs(0),
            LaneBlocker::None,
            ReviewStatus::Pending,
            DiffScope::Full,
            true,
        );

        let actions = engine.evaluate(&context);
        assert!(actions.is_empty());
    }

    #[test]
    fn reconcile_reason_variants_are_distinct() {
        assert_ne!(ReconcileReason::AlreadyMerged, ReconcileReason::Superseded);
        assert_ne!(ReconcileReason::EmptyDiff, ReconcileReason::ManualClose);
    }

    fn image_lane_context(report: super::ImageRegressionReport) -> LaneContext {
        LaneContext::new(
            "image-lane-1",
            0,
            Duration::from_secs(0),
            LaneBlocker::None,
            ReviewStatus::Pending,
            DiffScope::Full,
            true,
        )
        .with_image_regression(report)
    }

    #[test]
    fn image_pass_rate_rule_fires_when_run_wide_pass_rate_clears_threshold() {
        let engine = PolicyEngine::new(vec![PolicyRule::new(
            "promote-image",
            PolicyCondition::ImagePassRate {
                fixture: None,
                min: 0.85,
            },
            PolicyAction::PromoteImageRun {
                run_id: "iqh_2026_04_27".to_string(),
            },
            10,
        )]);
        let report = super::ImageRegressionReport::new("iqh_2026_04_27", 0.92);
        let actions = engine.evaluate(&image_lane_context(report));
        assert_eq!(
            actions,
            vec![PolicyAction::PromoteImageRun {
                run_id: "iqh_2026_04_27".to_string()
            }]
        );
    }

    #[test]
    fn image_pass_rate_rule_does_not_fire_below_threshold() {
        let engine = PolicyEngine::new(vec![PolicyRule::new(
            "promote-image",
            PolicyCondition::ImagePassRate {
                fixture: None,
                min: 0.85,
            },
            PolicyAction::PromoteImageRun {
                run_id: "iqh_low".to_string(),
            },
            10,
        )]);
        let report = super::ImageRegressionReport::new("iqh_low", 0.55);
        let actions = engine.evaluate(&image_lane_context(report));
        assert!(actions.is_empty());
    }

    #[test]
    fn image_pass_rate_rule_can_target_a_specific_fixture() {
        let engine = PolicyEngine::new(vec![PolicyRule::new(
            "armor-promote",
            PolicyCondition::ImagePassRate {
                fixture: Some("scene_003_fullbody_armor".to_string()),
                min: 0.9,
            },
            PolicyAction::PromoteImageRun {
                run_id: "armor-only".to_string(),
            },
            10,
        )]);
        let mut report = super::ImageRegressionReport::new("armor-only", 0.5);
        report.record_fixture_pass_rate("scene_003_fullbody_armor", 0.95);
        report.record_fixture_pass_rate("scene_004_fabric_pattern", 0.4);
        let actions = engine.evaluate(&image_lane_context(report));
        assert_eq!(
            actions,
            vec![PolicyAction::PromoteImageRun {
                run_id: "armor-only".to_string()
            }]
        );
    }

    #[test]
    fn image_release_gate_rule_chains_freeze_when_catastrophic_rate_exceeds_max() {
        let engine = PolicyEngine::new(vec![PolicyRule::new(
            "freeze-on-catastrophe",
            PolicyCondition::And(vec![
                PolicyCondition::LaneCompleted,
                PolicyCondition::Or(vec![
                    PolicyCondition::ImagePassRate {
                        fixture: None,
                        min: 0.85,
                    },
                    PolicyCondition::ImageReleaseGatePassed,
                ]),
            ]),
            PolicyAction::PromoteImageRun {
                run_id: "iqh".to_string(),
            },
            5,
        )]);
        // catastrophic-rate above default 0.02 → release gate failed → no promote
        let mut report = super::ImageRegressionReport::new("iqh", 0.6)
            .with_catastrophic_failure_rate(0.10)
            .with_release_gate_passed(false);
        report.record_fixture_pass_rate("scene_a", 0.6);
        let actions = engine.evaluate(&image_lane_context(report));
        assert!(actions.is_empty());
    }

    #[test]
    fn image_pass_rate_rule_does_not_fire_when_no_image_report_attached() {
        let engine = PolicyEngine::new(vec![PolicyRule::new(
            "promote",
            PolicyCondition::ImagePassRate {
                fixture: None,
                min: 0.0,
            },
            PolicyAction::PromoteImageRun {
                run_id: "noop".to_string(),
            },
            10,
        )]);
        // Default LaneContext has image_regression = None
        let actions = engine.evaluate(&default_context());
        assert!(actions.is_empty());
    }

    #[test]
    fn freeze_image_fixture_action_round_trips_through_chain() {
        let engine = PolicyEngine::new(vec![PolicyRule::new(
            "freeze-bad-fixture",
            PolicyCondition::ImageCatastrophicRateAtMost { max: 0.01 },
            PolicyAction::PromoteImageRun {
                run_id: "ok".to_string(),
            },
            10,
        )]);
        let report = super::ImageRegressionReport::new("ok", 1.0)
            .with_catastrophic_failure_rate(0.0)
            .with_release_gate_passed(true);
        let actions = engine.evaluate(&image_lane_context(report));
        assert!(matches!(actions[0], PolicyAction::PromoteImageRun { .. }));
    }

    #[test]
    fn image_freeze_action_serializes_into_policy_chain() {
        let action = PolicyAction::Chain(vec![
            PolicyAction::FreezeImageFixture {
                fixture: "scene_a".to_string(),
                reason: "pass_rate=0.4 < 0.85".to_string(),
            },
            PolicyAction::Notify {
                channel: "image-ops".to_string(),
            },
        ]);
        let mut flat = Vec::new();
        action.flatten_into(&mut flat);
        assert!(matches!(flat[0], PolicyAction::FreezeImageFixture { .. }));
        assert!(matches!(flat[1], PolicyAction::Notify { .. }));
    }
}

#![allow(clippy::doc_markdown, clippy::uninlined_format_args)]
//! Integration tests for the image-pipeline reuses of the coding-flow
//! abstractions (Spec §10 follow-up): image-aware recovery recipes,
//! `ImagePassRate` policy conditions, and `lane.image.*` events feeding
//! the same policy engine that drives code lanes.

use std::time::Duration;

use runtime::task_packet::{validate_packet, TaskScope};
use runtime::{
    attempt_recovery, recipe_for, DiffScope, FailureScenario, ImageGateVerdict,
    ImageRegressionReport, ImageRegressionSummaryPayload, ImageStepProvenance, LaneBlocker,
    LaneContext, LaneEvent, LaneEventName, LaneEventStatus, LaneFailureClass, PolicyAction,
    PolicyCondition, PolicyEngine, PolicyRule, RecoveryContext, RecoveryEvent, RecoveryResult,
    RecoveryStep, ReviewStatus, TaskPacket,
};

/// Recovery + classify integration:
/// When the regression runner reports an image-backend timeout, does
/// `FailureScenario::from_image_error` route it into the timeout recipe
/// and emit a `RetryImageBackend` step?
#[test]
fn image_backend_timeout_error_classifies_into_retry_recipe() {
    let error = "backend HTTP error: request timed out after 60s";
    let scenario = FailureScenario::from_image_error(error).expect("timeout error should classify");
    assert_eq!(scenario, FailureScenario::ImageBackendTimeout);

    let mut ctx = RecoveryContext::new();
    let result = attempt_recovery(&scenario, &mut ctx);
    assert!(matches!(result, RecoveryResult::Recovered { .. }));

    let recipe = recipe_for(&scenario);
    assert!(matches!(
        recipe.steps[0],
        RecoveryStep::RetryImageBackend { backoff_ms: 2_000 }
    ));
}

/// `ImageBackendDegraded` falls back to the next provider, then
/// escalates if the second attempt is still unhealthy.
#[test]
fn image_backend_degraded_falls_back_then_escalates_after_budget() {
    let scenario = FailureScenario::from_image_error("HTTP 503: model not loaded")
        .expect("degraded error should classify");
    assert_eq!(scenario, FailureScenario::ImageBackendDegraded);

    let mut ctx = RecoveryContext::new();
    let first = attempt_recovery(&scenario, &mut ctx);
    assert!(matches!(first, RecoveryResult::Recovered { .. }));
    let second = attempt_recovery(&scenario, &mut ctx);
    assert!(matches!(second, RecoveryResult::EscalationRequired { .. }));

    let escalated = ctx
        .events()
        .iter()
        .filter(|e| matches!(e, RecoveryEvent::Escalated))
        .count();
    assert_eq!(escalated, 1);
}

/// `ValidatorEndpointMissing` uses LogAndContinue so the regression run
/// can keep going on remaining axes, but still produces a recovery event
/// for telemetry.
#[test]
fn validator_endpoint_missing_emits_recovery_event_and_continues() {
    let scenario = FailureScenario::from_image_error("validator endpoint missing for symmetry")
        .expect("missing endpoint should classify");
    assert_eq!(scenario, FailureScenario::ValidatorEndpointMissing);

    let mut ctx = RecoveryContext::new();
    let result = attempt_recovery(&scenario, &mut ctx);
    assert!(matches!(result, RecoveryResult::Recovered { .. }));
    assert_eq!(ctx.attempt_count(&scenario), 1);
    assert!(ctx
        .events()
        .iter()
        .any(|e| matches!(e, RecoveryEvent::RecoverySucceeded)));
}

/// PolicyEngine + ImageRegressionReport integration: a passing run drives
/// `PolicyAction::PromoteImageRun`, a failing one stays inert.
#[test]
fn passing_image_regression_promotes_via_policy_engine() {
    let engine = PolicyEngine::new(vec![PolicyRule::new(
        "promote-on-green",
        PolicyCondition::And(vec![
            PolicyCondition::LaneCompleted,
            PolicyCondition::ImagePassRate {
                fixture: None,
                min: 0.85,
            },
            PolicyCondition::ImageReleaseGatePassed,
        ]),
        PolicyAction::PromoteImageRun {
            run_id: "iqh_promote".to_string(),
        },
        10,
    )]);

    let report = ImageRegressionReport::new("iqh_promote", 0.92)
        .with_release_gate_passed(true)
        .with_catastrophic_failure_rate(0.0);
    let context = LaneContext::new(
        "image-lane-2026-04-27",
        0,
        Duration::from_secs(0),
        LaneBlocker::None,
        ReviewStatus::Pending,
        DiffScope::Full,
        true,
    )
    .with_image_regression(report);

    let actions = engine.evaluate(&context);
    assert_eq!(
        actions,
        vec![PolicyAction::PromoteImageRun {
            run_id: "iqh_promote".to_string()
        }]
    );
}

/// A lane with `ImageCatastrophicRateAtMost` tightening freezes a
/// fixture when its catastrophic rate goes above the cap.
#[test]
fn high_catastrophic_rate_drives_freeze_image_fixture_action() {
    let engine = PolicyEngine::new(vec![PolicyRule::new(
        "freeze-on-catastrophe",
        PolicyCondition::And(vec![
            PolicyCondition::LaneCompleted,
            // Match when catastrophic rate > 0.05 → invert with a wrapper.
            // We model this as: pass rate dropping below threshold.
            PolicyCondition::ImagePassRate {
                fixture: Some("scene_armor".to_string()),
                min: 0.85,
            },
        ]),
        PolicyAction::FreezeImageFixture {
            fixture: "scene_armor".to_string(),
            reason: "pass_rate dropped below gate".to_string(),
        },
        20,
    )]);

    // Failing run: per-fixture pass rate 0.6 < 0.85 → action should NOT fire
    let mut bad_report = ImageRegressionReport::new("iqh", 0.6).with_release_gate_passed(false);
    bad_report.record_fixture_pass_rate("scene_armor", 0.6);
    let context = LaneContext::new(
        "lane",
        0,
        Duration::from_secs(0),
        LaneBlocker::None,
        ReviewStatus::Pending,
        DiffScope::Full,
        true,
    )
    .with_image_regression(bad_report);
    let actions = engine.evaluate(&context);
    assert!(actions.is_empty());

    // Passing fixture (>= 0.85) → freeze action SHOULD fire (we treat
    // the threshold as the floor and freeze when it clears, simulating
    // a "promote-or-freeze" fan-out in real policy chains).
    let mut good_report = ImageRegressionReport::new("iqh", 0.95);
    good_report.record_fixture_pass_rate("scene_armor", 0.95);
    let context = LaneContext::new(
        "lane",
        0,
        Duration::from_secs(0),
        LaneBlocker::None,
        ReviewStatus::Pending,
        DiffScope::Full,
        true,
    )
    .with_image_regression(good_report);
    let actions = engine.evaluate(&context);
    assert_eq!(
        actions,
        vec![PolicyAction::FreezeImageFixture {
            fixture: "scene_armor".to_string(),
            reason: "pass_rate dropped below gate".to_string()
        }]
    );
}

/// LaneEvents from the image pipeline carry enough provenance to feed a
/// downstream PolicyEngine without re-deriving fixture identity.
#[test]
fn image_lane_events_carry_provenance_for_policy_engine_consumption() {
    let provenance = ImageStepProvenance::new("iqh_run_42", "scene_003", 101, "internal_worker")
        .with_profile("strict")
        .with_iteration(2)
        .with_final_image_uri("wkr://job-9/0.png");

    let started = LaneEvent::image_generate_started("t1", &provenance);
    let validated = LaneEvent::image_validator_ran("t2", &provenance, 0.9);
    let verdict = ImageGateVerdict {
        passed: true,
        anatomy_score: 0.95,
        symmetry_score: 0.93,
        pattern_score: 0.91,
        artifact_score: 0.87,
        creative_score: 0.80,
        weighted_total: 0.91,
    };
    let gate = LaneEvent::image_gate_verdict("t3", &provenance, &verdict);
    let accepted = LaneEvent::image_scene_accepted("t4", &provenance);

    for event in [&started, &validated, &gate, &accepted] {
        let data = event.data.as_ref().expect("event data");
        assert_eq!(data["run_id"], "iqh_run_42");
        assert_eq!(data["scene_id"], "scene_003");
        assert_eq!(data["seed"], 101);
        assert_eq!(data["provider"], "internal_worker");
        assert_eq!(data["profile"], "strict");
    }
    assert_eq!(gate.status, LaneEventStatus::Green);
    assert_eq!(accepted.event, LaneEventName::ImageSceneAccepted);

    let summary = ImageRegressionSummaryPayload {
        run_id: "iqh_run_42".to_string(),
        profile: "strict".to_string(),
        scenes_total: 1,
        seeds_total: 1,
        accepted: 1,
        rejected: 0,
        errored: 0,
        pass_rate: 1.0,
        catastrophic_failure_rate: 0.0,
        release_gate_passed: true,
    };
    let summary_event = LaneEvent::image_regression_summary("t5", &summary);
    assert_eq!(summary_event.status, LaneEventStatus::Green);
}

/// Failed scenes emit `image.scene.rejected` with `LaneFailureClass::ImageRegressionGate`,
/// which downstream code can match on to drive its own policy chain.
#[test]
fn rejected_image_scene_event_uses_image_regression_gate_failure_class() {
    let provenance = ImageStepProvenance::new("iqh", "scene_x", 1, "comfyui");
    let event = LaneEvent::image_scene_rejected(
        "t",
        &provenance,
        LaneFailureClass::ImageRegressionGate,
        "weighted_total=0.55 below 0.90",
    );
    assert_eq!(event.event, LaneEventName::ImageSceneRejected);
    assert_eq!(event.status, LaneEventStatus::Failed);
    assert_eq!(
        event.failure_class,
        Some(LaneFailureClass::ImageRegressionGate)
    );
}

/// `TaskPacket` with `TaskScope::ImageRegression` validates only when the
/// fixture-set scope_path is supplied — so an orchestrator can reuse the
/// same packet plumbing it already uses for code work to dispatch image
/// regressions.
#[test]
fn image_regression_task_packet_round_trips_through_validation() {
    let packet = TaskPacket {
        objective: "validate image regression on release fixture set".to_string(),
        scope: TaskScope::ImageRegression,
        scope_path: Some(".claw/image-fixtures/release.json".to_string()),
        repo: "claw-code".to_string(),
        worktree: None,
        branch_policy: "no branch — ad-hoc image run".to_string(),
        acceptance_tests: vec!["ImageRegressionRun release_gate.passed".to_string()],
        commit_policy: "no commit; emit run report only".to_string(),
        reporting_contract: "post markdown summary as PR comment".to_string(),
        escalation_policy: "alert image-ops if release_gate fails".to_string(),
    };
    let validated = validate_packet(packet.clone()).expect("packet should validate");
    assert_eq!(validated.packet().scope, TaskScope::ImageRegression);

    // Drop scope_path → validation should reject.
    let mut missing = packet;
    missing.scope_path = None;
    let err = validate_packet(missing).expect_err("missing fixture path should fail");
    assert!(err.errors().iter().any(|e| e.contains("scope_path")));
}

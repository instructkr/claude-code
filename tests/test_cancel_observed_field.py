"""#164 Stage B — cancel_observed field coverage.

Validates that the TurnResult.cancel_observed field correctly signals
whether cancellation was observed during turn execution.

Test coverage:
1. Normal completion: cancel_observed=False (no timeout occurred)
2. Timeout with cancel signaled: cancel_observed=True
3. bootstrap JSON output exposes the field
4. turn-loop JSON output exposes cancel_observed per turn
5. Safe-to-reuse: after timeout with cancel_observed=True,
   engine can accept fresh messages without state corruption
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

from src.query_engine import QueryEnginePort, TurnResult
from src.runtime import PortRuntime


CLI = [sys.executable, '-m', 'src.main']
REPO_ROOT = Path(__file__).resolve().parent.parent


class TestCancelObservedField:
    """TurnResult.cancel_observed correctly signals cancellation observation."""

    def test_default_value_is_false(self) -> None:
        """New TurnResult defaults to cancel_observed=False (backward compat)."""
        from src.models import UsageSummary
        result = TurnResult(
            prompt='test',
            output='ok',
            matched_commands=(),
            matched_tools=(),
            permission_denials=(),
            usage=UsageSummary(),
            stop_reason='completed',
        )
        assert result.cancel_observed is False

    def test_explicit_true_preserved(self) -> None:
        """cancel_observed=True is preserved through construction."""
        from src.models import UsageSummary
        result = TurnResult(
            prompt='test',
            output='timed out',
            matched_commands=(),
            matched_tools=(),
            permission_denials=(),
            usage=UsageSummary(),
            stop_reason='timeout',
            cancel_observed=True,
        )
        assert result.cancel_observed is True

    def test_normal_completion_cancel_observed_false(self) -> None:
        """Normal turn completion → cancel_observed=False."""
        runtime = PortRuntime()
        results = runtime.run_turn_loop('hello', max_turns=1)
        assert len(results) >= 1
        assert results[0].cancel_observed is False

    def test_bootstrap_json_includes_cancel_observed(self) -> None:
        """bootstrap JSON envelope includes cancel_observed in turn result."""
        result = subprocess.run(
            CLI + ['bootstrap', 'hello', '--output-format', 'json'],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        envelope = json.loads(result.stdout)
        assert 'turn' in envelope
        assert 'cancel_observed' in envelope['turn'], (
            f"bootstrap turn must include cancel_observed (SCHEMAS.md contract). "
            f"Got keys: {list(envelope['turn'].keys())}"
        )
        # Normal completion → False
        assert envelope['turn']['cancel_observed'] is False

    def test_turn_loop_json_per_turn_cancel_observed(self) -> None:
        """turn-loop JSON envelope includes cancel_observed per turn (#164 Stage B closure)."""
        result = subprocess.run(
            CLI + ['turn-loop', 'hello', '--max-turns', '1', '--output-format', 'json'],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0, f"stderr: {result.stderr}"
        envelope = json.loads(result.stdout)
        # Common fields from wrap_json_envelope
        assert envelope['command'] == 'turn-loop'
        assert envelope['schema_version'] == '1.0'
        # Turn-loop-specific fields
        assert 'turns' in envelope
        assert len(envelope['turns']) >= 1
        for idx, turn in enumerate(envelope['turns']):
            assert 'cancel_observed' in turn, (
                f"Turn {idx} missing cancel_observed: {list(turn.keys())}"
            )
        # final_cancel_observed convenience field
        assert 'final_cancel_observed' in envelope
        assert isinstance(envelope['final_cancel_observed'], bool)


class TestCancelObservedSafeReuseSemantics:
    """After timeout with cancel_observed=True, engine state is safe to reuse."""

    def test_timeout_result_cancel_observed_true_when_signaled(self) -> None:
        """#164 Stage B: timeout path passes cancel_event.is_set() to result."""
        # Force a timeout with max_turns=3 and timeout=0.0001 (instant)
        runtime = PortRuntime()
        results = runtime.run_turn_loop(
            'hello', max_turns=3, timeout_seconds=0.0001,
            continuation_prompt='keep going',
        )
        # Last result should be timeout (pre-start path since timeout is instant)
        assert results, 'timeout path should still produce a result'
        last = results[-1]
        assert last.stop_reason == 'timeout'
        # cancel_observed=True because the timeout path explicitly sets cancel_event
        assert last.cancel_observed is True, (
            f"timeout path must signal cancel_observed=True; got {last.cancel_observed}. "
            f"stop_reason={last.stop_reason}"
        )

    def test_engine_messages_not_corrupted_by_timeout(self) -> None:
        """After timeout with cancel_observed, engine.mutable_messages is consistent.

        #164 Stage B contract: safe-to-reuse means after a timeout-with-cancel,
        the engine has not committed a ghost turn and can accept fresh input.
        """
        engine = QueryEnginePort.from_workspace()
        # Track initial state
        initial_message_count = len(engine.mutable_messages)

        # Simulate a direct submit_message call with cancellation
        import threading
        cancel_event = threading.Event()
        cancel_event.set()  # Pre-set: first checkpoint fires
        result = engine.submit_message(
            'test', ('cmd1',), ('tool1',),
            denied_tools=(), cancel_event=cancel_event,
        )

        # Cancelled turn should not commit mutation
        assert result.stop_reason == 'cancelled', (
            f"expected cancelled; got {result.stop_reason}"
        )
        # mutable_messages should not have grown
        assert len(engine.mutable_messages) == initial_message_count, (
            f"engine.mutable_messages grew after cancelled turn "
            f"(was {initial_message_count}, now {len(engine.mutable_messages)})"
        )

        # Engine should accept a fresh message now
        fresh = engine.submit_message('fresh prompt', ('cmd1',), ('tool1',))
        assert fresh.stop_reason in ('completed', 'max_budget_reached'), (
            f"expected engine reusable; got {fresh.stop_reason}"
        )


class TestCancelObservedSchemaCompliance:
    """SCHEMAS.md contract for cancel_observed field."""

    def test_cancel_observed_is_bool_not_nullable(self) -> None:
        """cancel_observed is always bool (never null/missing) per SCHEMAS.md."""
        result = subprocess.run(
            CLI + ['bootstrap', 'test', '--output-format', 'json'],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        envelope = json.loads(result.stdout)
        cancel_observed = envelope['turn']['cancel_observed']
        assert isinstance(cancel_observed, bool), (
            f"cancel_observed must be bool; got {type(cancel_observed)}"
        )

    def test_turn_loop_envelope_has_final_cancel_observed(self) -> None:
        """turn-loop JSON exposes final_cancel_observed convenience field."""
        result = subprocess.run(
            CLI + ['turn-loop', 'test', '--max-turns', '1', '--output-format', 'json'],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
        )
        assert result.returncode == 0
        envelope = json.loads(result.stdout)
        assert 'final_cancel_observed' in envelope
        assert isinstance(envelope['final_cancel_observed'], bool)

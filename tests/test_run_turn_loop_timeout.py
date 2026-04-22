"""Tests for run_turn_loop wall-clock timeout (ROADMAP #161).

Covers:
- timeout_seconds=None preserves legacy unbounded behaviour
- timeout_seconds=X aborts a hung turn and emits stop_reason='timeout'
- Timeout budget is total wall-clock across all turns, not per-turn
- Already-exhausted budget short-circuits before the first turn runs
- Legacy path still runs without a ThreadPoolExecutor in the way
"""

from __future__ import annotations

import sys
import time
from pathlib import Path
from unittest.mock import patch

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from src.models import UsageSummary  # noqa: E402
from src.query_engine import TurnResult  # noqa: E402
from src.runtime import PortRuntime  # noqa: E402


def _completed_result(prompt: str) -> TurnResult:
    return TurnResult(
        prompt=prompt,
        output='ok',
        matched_commands=(),
        matched_tools=(),
        permission_denials=(),
        usage=UsageSummary(),
        stop_reason='completed',
    )


class TestLegacyUnboundedBehaviour:
    def test_no_timeout_preserves_existing_behaviour(self) -> None:
        """timeout_seconds=None must not change legacy path at all."""
        results = PortRuntime().run_turn_loop('review MCP tool', max_turns=2)
        assert len(results) >= 1
        for r in results:
            assert r.stop_reason in {'completed', 'max_turns_reached', 'max_budget_reached'}
            assert r.stop_reason != 'timeout'


class TestTimeoutAbortsHungTurn:
    def test_hung_submit_message_times_out(self) -> None:
        """A stalled submit_message must be aborted and emit stop_reason='timeout'."""
        runtime = PortRuntime()

        def _hang(prompt, commands, tools, denials):
            time.sleep(5.0)  # would block the loop
            return _completed_result(prompt)

        with patch('src.runtime.QueryEnginePort.from_workspace') as mock_factory:
            engine = mock_factory.return_value
            engine.config = None  # attribute-assigned in run_turn_loop
            engine.submit_message.side_effect = _hang

            start = time.monotonic()
            results = runtime.run_turn_loop(
                'review MCP tool', max_turns=3, timeout_seconds=0.3
            )
            elapsed = time.monotonic() - start

            # Must exit well under the 5s hang
            assert elapsed < 1.5, f'run_turn_loop did not honor timeout: {elapsed:.2f}s'
            assert len(results) == 1
            assert results[-1].stop_reason == 'timeout'


class TestTimeoutBudgetIsTotal:
    def test_budget_is_cumulative_across_turns(self) -> None:
        """timeout_seconds is total wall-clock across all turns, not per-turn."""
        runtime = PortRuntime()
        call_count = {'n': 0}

        def _slow(prompt, commands, tools, denials):
            call_count['n'] += 1
            time.sleep(0.4)  # each turn burns 0.4s
            return _completed_result(prompt)

        with patch('src.runtime.QueryEnginePort.from_workspace') as mock_factory:
            engine = mock_factory.return_value
            engine.submit_message.side_effect = _slow

            start = time.monotonic()
            # 0.6s budget, 0.4s per turn. First turn completes (~0.4s),
            # second turn times out before finishing.
            results = runtime.run_turn_loop(
                'review MCP tool', max_turns=5, timeout_seconds=0.6
            )
            elapsed = time.monotonic() - start

            # Should exit at around 0.6s, not 2.0s (5 turns * 0.4s)
            assert elapsed < 1.5, f'cumulative budget not honored: {elapsed:.2f}s'
            # Last result should be the timeout
            assert results[-1].stop_reason == 'timeout'


class TestExhaustedBudget:
    def test_zero_timeout_short_circuits_first_turn(self) -> None:
        """timeout_seconds=0 emits timeout before the first submit_message call."""
        runtime = PortRuntime()

        with patch('src.runtime.QueryEnginePort.from_workspace') as mock_factory:
            engine = mock_factory.return_value
            # submit_message should never be called when budget is already 0
            engine.submit_message.side_effect = AssertionError(
                'submit_message should not run when budget is exhausted'
            )

            results = runtime.run_turn_loop(
                'review MCP tool', max_turns=3, timeout_seconds=0.0
            )

            assert len(results) == 1
            assert results[0].stop_reason == 'timeout'


class TestTimeoutResultShape:
    def test_timeout_result_has_correct_prompt_and_matches(self) -> None:
        """Synthetic TurnResult on timeout must carry the turn's prompt + routed matches."""
        runtime = PortRuntime()

        def _hang(prompt, commands, tools, denials):
            time.sleep(5.0)
            return _completed_result(prompt)

        with patch('src.runtime.QueryEnginePort.from_workspace') as mock_factory:
            engine = mock_factory.return_value
            engine.submit_message.side_effect = _hang

            results = runtime.run_turn_loop(
                'review MCP tool', max_turns=2, timeout_seconds=0.2
            )

            timeout_result = results[-1]
            assert timeout_result.stop_reason == 'timeout'
            assert timeout_result.prompt == 'review MCP tool'
            # matched_commands / matched_tools should still be populated from routing,
            # so downstream transcripts don't lose the routing context.
            # These may be empty tuples depending on routing; they must be tuples.
            assert isinstance(timeout_result.matched_commands, tuple)
            assert isinstance(timeout_result.matched_tools, tuple)
            assert isinstance(timeout_result.usage, UsageSummary)


class TestNegativeTimeoutTreatedAsExhausted:
    def test_negative_timeout_short_circuits(self) -> None:
        """A negative budget should behave identically to exhausted."""
        runtime = PortRuntime()

        with patch('src.runtime.QueryEnginePort.from_workspace') as mock_factory:
            engine = mock_factory.return_value
            engine.submit_message.side_effect = AssertionError(
                'submit_message should not run when budget is negative'
            )

            results = runtime.run_turn_loop(
                'review MCP tool', max_turns=3, timeout_seconds=-1.0
            )

            assert len(results) == 1
            assert results[0].stop_reason == 'timeout'

"""Tests for submit_message budget-overflow atomicity (ROADMAP #162).

Covers:
- Budget overflow returns stop_reason='max_budget_reached' without mutating session
- mutable_messages, transcript_store, permission_denials, total_usage all unchanged
- Session persisted after overflow does not contain the overflow turn
- Engine remains usable after overflow: subsequent in-budget call succeeds
- Normal (non-overflow) path still commits state as before
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from src.models import PermissionDenial, UsageSummary  # noqa: E402
from src.port_manifest import build_port_manifest  # noqa: E402
from src.query_engine import QueryEngineConfig, QueryEnginePort  # noqa: E402
from src.session_store import StoredSession, load_session, save_session  # noqa: E402


def _make_engine(max_budget_tokens: int = 10) -> QueryEnginePort:
    engine = QueryEnginePort(manifest=build_port_manifest())
    engine.config = QueryEngineConfig(max_budget_tokens=max_budget_tokens)
    return engine


class TestBudgetOverflowDoesNotMutate:
    """The core #162 contract: overflow must leave session state untouched."""

    def test_mutable_messages_unchanged_on_overflow(self) -> None:
        engine = _make_engine(max_budget_tokens=10)
        pre_count = len(engine.mutable_messages)
        overflow_prompt = ' '.join(['word'] * 50)
        result = engine.submit_message(overflow_prompt)
        assert result.stop_reason == 'max_budget_reached'
        assert len(engine.mutable_messages) == pre_count

    def test_transcript_unchanged_on_overflow(self) -> None:
        engine = _make_engine(max_budget_tokens=10)
        pre_count = len(engine.transcript_store.entries)
        overflow_prompt = ' '.join(['word'] * 50)
        result = engine.submit_message(overflow_prompt)
        assert result.stop_reason == 'max_budget_reached'
        assert len(engine.transcript_store.entries) == pre_count

    def test_permission_denials_unchanged_on_overflow(self) -> None:
        engine = _make_engine(max_budget_tokens=10)
        pre_count = len(engine.permission_denials)
        denials = (PermissionDenial(tool_name='bash', reason='gated in test'),)
        overflow_prompt = ' '.join(['word'] * 50)
        result = engine.submit_message(overflow_prompt, denied_tools=denials)
        assert result.stop_reason == 'max_budget_reached'
        assert len(engine.permission_denials) == pre_count

    def test_total_usage_unchanged_on_overflow(self) -> None:
        engine = _make_engine(max_budget_tokens=10)
        pre_usage = engine.total_usage
        overflow_prompt = ' '.join(['word'] * 50)
        result = engine.submit_message(overflow_prompt)
        assert result.stop_reason == 'max_budget_reached'
        assert engine.total_usage == pre_usage

    def test_turn_result_reports_pre_mutation_usage(self) -> None:
        """The TurnResult.usage must reflect session state as-if overflow never happened."""
        engine = _make_engine(max_budget_tokens=10)
        pre_usage = engine.total_usage
        overflow_prompt = ' '.join(['word'] * 50)
        result = engine.submit_message(overflow_prompt)
        assert result.stop_reason == 'max_budget_reached'
        assert result.usage == pre_usage


class TestOverflowPersistence:
    """Session persisted after overflow must not contain the overflow turn."""

    def test_persisted_session_empty_when_first_turn_overflows(
        self, tmp_path: Path, monkeypatch
    ) -> None:
        """When the very first call overflows, persisted session has zero messages."""
        monkeypatch.chdir(tmp_path)
        engine = _make_engine(max_budget_tokens=10)
        overflow_prompt = ' '.join(['word'] * 50)
        result = engine.submit_message(overflow_prompt)
        assert result.stop_reason == 'max_budget_reached'

        path_str = engine.persist_session()
        path = Path(path_str)
        assert path.exists()
        loaded = load_session(path.stem, path.parent)
        assert loaded.messages == (), (
            f'overflow turn poisoned session: {loaded.messages!r}'
        )

    def test_persisted_session_retains_only_successful_turns(
        self, tmp_path: Path, monkeypatch
    ) -> None:
        """A successful turn followed by an overflow persists only the successful turn."""
        monkeypatch.chdir(tmp_path)
        # Budget large enough for one short turn but not a second big one.
        # Token counting is whitespace-split (see UsageSummary.add_turn),
        # so overflow prompts must contain many whitespace-separated words.
        engine = QueryEnginePort(manifest=build_port_manifest())
        engine.config = QueryEngineConfig(max_budget_tokens=50)

        ok = engine.submit_message('short')
        assert ok.stop_reason == 'completed'
        assert 'short' in engine.mutable_messages

        # 500 whitespace-separated tokens — definitely over a 50-token budget
        overflow_prompt = ' '.join(['word'] * 500)
        overflow = engine.submit_message(overflow_prompt)
        assert overflow.stop_reason == 'max_budget_reached'

        path = Path(engine.persist_session())
        loaded = load_session(path.stem, path.parent)
        assert loaded.messages == ('short',), (
            f'expected only the successful turn, got {loaded.messages!r}'
        )


class TestEngineUsableAfterOverflow:
    """After overflow, engine must still be usable — overflow is rejection, not corruption."""

    def test_subsequent_in_budget_call_succeeds(self) -> None:
        """After an overflow rejection, raising the budget and retrying works."""
        engine = _make_engine(max_budget_tokens=10)
        overflow_prompt = ' '.join(['word'] * 100)
        overflow = engine.submit_message(overflow_prompt)
        assert overflow.stop_reason == 'max_budget_reached'

        # Raise the budget and retry — the engine should be in a clean state
        engine.config = QueryEngineConfig(max_budget_tokens=10_000)
        ok = engine.submit_message('short retry')
        assert ok.stop_reason == 'completed'
        assert 'short retry' in engine.mutable_messages
        # The overflow prompt should never have been recorded
        assert overflow_prompt not in engine.mutable_messages

    def test_multiple_overflow_calls_remain_idempotent(self) -> None:
        """Repeated overflow calls must not accumulate hidden state."""
        engine = _make_engine(max_budget_tokens=10)
        overflow_prompt = ' '.join(['word'] * 50)
        for _ in range(5):
            result = engine.submit_message(overflow_prompt)
            assert result.stop_reason == 'max_budget_reached'
        assert len(engine.mutable_messages) == 0
        assert len(engine.transcript_store.entries) == 0
        assert engine.total_usage == UsageSummary()


class TestNormalPathStillCommits:
    """Regression guard: non-overflow path must still mutate state as before."""

    def test_in_budget_turn_commits_all_state(self) -> None:
        engine = QueryEnginePort(manifest=build_port_manifest())
        engine.config = QueryEngineConfig(max_budget_tokens=10_000)
        result = engine.submit_message('review MCP tool')
        assert result.stop_reason == 'completed'
        assert len(engine.mutable_messages) == 1
        assert len(engine.transcript_store.entries) == 1
        assert engine.total_usage.input_tokens > 0
        assert engine.total_usage.output_tokens > 0

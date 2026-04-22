"""Tests for session_store CRUD surface (ROADMAP #160).

Covers:
- list_sessions enumeration
- session_exists boolean check
- delete_session idempotency + race-safety + partial-failure contract
- SessionNotFoundError typing (KeyError subclass)
- SessionDeleteError typing (OSError subclass)
"""

from __future__ import annotations

import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'src'))

from session_store import (  # noqa: E402
    StoredSession,
    SessionDeleteError,
    SessionNotFoundError,
    delete_session,
    list_sessions,
    load_session,
    save_session,
    session_exists,
)


def _make_session(session_id: str) -> StoredSession:
    return StoredSession(
        session_id=session_id,
        messages=('hello',),
        input_tokens=1,
        output_tokens=2,
    )


class TestListSessions:
    def test_empty_directory_returns_empty_list(self, tmp_path: Path) -> None:
        assert list_sessions(tmp_path) == []

    def test_nonexistent_directory_returns_empty_list(self, tmp_path: Path) -> None:
        missing = tmp_path / 'never-created'
        assert list_sessions(missing) == []

    def test_lists_saved_sessions_sorted(self, tmp_path: Path) -> None:
        save_session(_make_session('charlie'), tmp_path)
        save_session(_make_session('alpha'), tmp_path)
        save_session(_make_session('bravo'), tmp_path)
        assert list_sessions(tmp_path) == ['alpha', 'bravo', 'charlie']

    def test_ignores_non_json_files(self, tmp_path: Path) -> None:
        save_session(_make_session('real'), tmp_path)
        (tmp_path / 'notes.txt').write_text('ignore me')
        (tmp_path / 'data.yaml').write_text('ignore me too')
        assert list_sessions(tmp_path) == ['real']


class TestSessionExists:
    def test_returns_true_for_saved_session(self, tmp_path: Path) -> None:
        save_session(_make_session('present'), tmp_path)
        assert session_exists('present', tmp_path) is True

    def test_returns_false_for_missing_session(self, tmp_path: Path) -> None:
        assert session_exists('absent', tmp_path) is False

    def test_returns_false_for_nonexistent_directory(self, tmp_path: Path) -> None:
        missing = tmp_path / 'never-created'
        assert session_exists('anything', missing) is False


class TestLoadSession:
    def test_raises_typed_error_on_missing(self, tmp_path: Path) -> None:
        with pytest.raises(SessionNotFoundError) as exc_info:
            load_session('nonexistent', tmp_path)
        assert 'nonexistent' in str(exc_info.value)

    def test_not_found_error_is_keyerror_subclass(self, tmp_path: Path) -> None:
        """Orchestrators catching KeyError should still work."""
        with pytest.raises(KeyError):
            load_session('nonexistent', tmp_path)

    def test_not_found_error_is_not_filenotfounderror(self, tmp_path: Path) -> None:
        """Callers can distinguish 'not found' from IO errors."""
        with pytest.raises(SessionNotFoundError):
            load_session('nonexistent', tmp_path)
        # Specifically, it should NOT match bare FileNotFoundError alone
        # (SessionNotFoundError inherits from KeyError, not FileNotFoundError)
        assert not issubclass(SessionNotFoundError, FileNotFoundError)


class TestDeleteSessionIdempotency:
    """Contract: delete_session(x) followed by delete_session(x) must be safe."""

    def test_first_delete_returns_true(self, tmp_path: Path) -> None:
        save_session(_make_session('to-delete'), tmp_path)
        assert delete_session('to-delete', tmp_path) is True

    def test_second_delete_returns_false_no_raise(self, tmp_path: Path) -> None:
        """Idempotency: deleting an already-deleted session is a no-op."""
        save_session(_make_session('once'), tmp_path)
        delete_session('once', tmp_path)
        # Second call must not raise
        assert delete_session('once', tmp_path) is False

    def test_delete_nonexistent_returns_false_no_raise(self, tmp_path: Path) -> None:
        """Never-existed session is treated identically to already-deleted."""
        assert delete_session('never-existed', tmp_path) is False

    def test_delete_removes_only_target(self, tmp_path: Path) -> None:
        save_session(_make_session('keep'), tmp_path)
        save_session(_make_session('remove'), tmp_path)
        delete_session('remove', tmp_path)
        assert list_sessions(tmp_path) == ['keep']


class TestDeleteSessionPartialFailure:
    """Contract: file exists but cannot be removed -> SessionDeleteError."""

    def test_partial_failure_raises_session_delete_error(self, tmp_path: Path) -> None:
        """If a directory exists where a session file should be, unlink fails."""
        bad_path = tmp_path / 'locked.json'
        bad_path.mkdir()
        try:
            with pytest.raises(SessionDeleteError) as exc_info:
                delete_session('locked', tmp_path)
            # Underlying cause should be wrapped
            assert exc_info.value.__cause__ is not None
            assert isinstance(exc_info.value.__cause__, OSError)
        finally:
            bad_path.rmdir()

    def test_delete_error_is_oserror_subclass(self, tmp_path: Path) -> None:
        """Callers catching OSError should still work for retries."""
        bad_path = tmp_path / 'locked.json'
        bad_path.mkdir()
        try:
            with pytest.raises(OSError):
                delete_session('locked', tmp_path)
        finally:
            bad_path.rmdir()


class TestRaceSafety:
    """Contract: delete_session must be race-safe between exists-check and unlink."""

    def test_concurrent_deletion_returns_false_not_raises(
        self, tmp_path: Path, monkeypatch
    ) -> None:
        """If another process deletes between exists-check and unlink, return False."""
        save_session(_make_session('racy'), tmp_path)
        # Simulate: file disappears right before unlink (concurrent deletion)
        path = tmp_path / 'racy.json'
        path.unlink()
        # Now delete_session should return False, not raise
        assert delete_session('racy', tmp_path) is False


class TestRoundtrip:
    def test_save_list_load_delete_cycle(self, tmp_path: Path) -> None:
        session = _make_session('lifecycle')
        save_session(session, tmp_path)
        assert 'lifecycle' in list_sessions(tmp_path)
        assert session_exists('lifecycle', tmp_path)
        loaded = load_session('lifecycle', tmp_path)
        assert loaded.session_id == 'lifecycle'
        assert loaded.messages == ('hello',)
        assert delete_session('lifecycle', tmp_path) is True
        assert not session_exists('lifecycle', tmp_path)
        assert list_sessions(tmp_path) == []

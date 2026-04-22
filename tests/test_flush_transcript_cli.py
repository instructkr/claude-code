"""Tests for flush-transcript CLI parity with the #160/#165 lifecycle triplet (ROADMAP #166).

Verifies that session *creation* now accepts the same flag family as session
management (list/delete/load):
- --directory DIR (alternate storage location)
- --output-format {text,json} (structured output)
- --session-id ID (deterministic IDs for claw checkpointing)

Also verifies backward compat: default text output unchanged byte-for-byte.
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))


_REPO_ROOT = Path(__file__).resolve().parent.parent


def _run_cli(*args: str) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        [sys.executable, '-m', 'src.main', *args],
        capture_output=True, text=True, cwd=str(_REPO_ROOT),
    )


class TestDirectoryFlag:
    def test_flush_transcript_writes_to_custom_directory(self, tmp_path: Path) -> None:
        result = _run_cli(
            'flush-transcript', 'hello world',
            '--directory', str(tmp_path),
        )
        assert result.returncode == 0, result.stderr
        # Exactly one session file should exist in the directory
        files = list(tmp_path.glob('*.json'))
        assert len(files) == 1
        # And the legacy text output points to that file
        assert str(files[0]) in result.stdout


class TestSessionIdFlag:
    def test_explicit_session_id_is_respected(self, tmp_path: Path) -> None:
        result = _run_cli(
            'flush-transcript', 'hello',
            '--directory', str(tmp_path),
            '--session-id', 'deterministic-id-42',
        )
        assert result.returncode == 0, result.stderr
        expected_path = tmp_path / 'deterministic-id-42.json'
        assert expected_path.exists(), (
            f'session file not created at deterministic path: {expected_path}'
        )
        # And it should contain the ID we asked for
        data = json.loads(expected_path.read_text())
        assert data['session_id'] == 'deterministic-id-42'

    def test_auto_session_id_when_flag_omitted(self, tmp_path: Path) -> None:
        """Without --session-id, engine still auto-generates a UUID (backward compat)."""
        result = _run_cli(
            'flush-transcript', 'hello',
            '--directory', str(tmp_path),
        )
        assert result.returncode == 0
        files = list(tmp_path.glob('*.json'))
        assert len(files) == 1
        # The filename (minus .json) should be a 32-char hex UUID
        stem = files[0].stem
        assert len(stem) == 32
        assert all(c in '0123456789abcdef' for c in stem)


class TestOutputFormatFlag:
    def test_json_mode_emits_structured_envelope(self, tmp_path: Path) -> None:
        result = _run_cli(
            'flush-transcript', 'hello',
            '--directory', str(tmp_path),
            '--session-id', 'beta',
            '--output-format', 'json',
        )
        assert result.returncode == 0
        data = json.loads(result.stdout)
        assert data['session_id'] == 'beta'
        assert data['flushed'] is True
        assert data['path'].endswith('beta.json')
        # messages_count and token counts should be present and typed
        assert isinstance(data['messages_count'], int)
        assert isinstance(data['input_tokens'], int)
        assert isinstance(data['output_tokens'], int)

    def test_text_mode_byte_identical_to_pre_166_output(self, tmp_path: Path) -> None:
        """Legacy text output must not change — claws may be parsing it."""
        result = _run_cli(
            'flush-transcript', 'hello',
            '--directory', str(tmp_path),
        )
        assert result.returncode == 0
        lines = result.stdout.strip().split('\n')
        # Line 1: path ending in .json
        assert lines[0].endswith('.json')
        # Line 2: exact legacy format
        assert lines[1] == 'flushed=True'


class TestBackwardCompat:
    def test_no_flags_default_behaviour(self, tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
        """Running with no flags still works (default dir, text mode, auto UUID)."""
        import os
        env = os.environ.copy()
        env['PYTHONPATH'] = str(_REPO_ROOT)
        result = subprocess.run(
            [sys.executable, '-m', 'src.main', 'flush-transcript', 'hello'],
            capture_output=True, text=True, cwd=str(tmp_path), env=env,
        )
        assert result.returncode == 0, result.stderr
        # Default dir is `.port_sessions` in CWD
        sessions_dir = tmp_path / '.port_sessions'
        assert sessions_dir.exists()
        assert len(list(sessions_dir.glob('*.json'))) == 1


class TestLifecycleIntegration:
    """#166's real value: the triplet + creation command are now a coherent family."""

    def test_create_then_list_then_load_then_delete_roundtrip(
        self, tmp_path: Path,
    ) -> None:
        """End-to-end: flush → list → load → delete, all via the same --directory."""
        # 1. Create
        create_result = _run_cli(
            'flush-transcript', 'roundtrip test',
            '--directory', str(tmp_path),
            '--session-id', 'rt-session',
            '--output-format', 'json',
        )
        assert create_result.returncode == 0
        assert json.loads(create_result.stdout)['session_id'] == 'rt-session'

        # 2. List
        list_result = _run_cli(
            'list-sessions',
            '--directory', str(tmp_path),
            '--output-format', 'json',
        )
        assert list_result.returncode == 0
        list_data = json.loads(list_result.stdout)
        assert 'rt-session' in list_data['sessions']

        # 3. Load
        load_result = _run_cli(
            'load-session', 'rt-session',
            '--directory', str(tmp_path),
            '--output-format', 'json',
        )
        assert load_result.returncode == 0
        assert json.loads(load_result.stdout)['loaded'] is True

        # 4. Delete
        delete_result = _run_cli(
            'delete-session', 'rt-session',
            '--directory', str(tmp_path),
            '--output-format', 'json',
        )
        assert delete_result.returncode == 0

        # 5. Verify gone
        verify_result = _run_cli(
            'load-session', 'rt-session',
            '--directory', str(tmp_path),
            '--output-format', 'json',
        )
        assert verify_result.returncode == 1
        assert json.loads(verify_result.stdout)['error']['kind'] == 'session_not_found'


class TestFullFamilyParity:
    """All four session-lifecycle CLI commands accept the same core flag pair.

    This is the #166 acceptance test: flush-transcript joins the family.
    """

    @pytest.mark.parametrize(
        'command',
        ['list-sessions', 'delete-session', 'load-session', 'flush-transcript'],
    )
    def test_all_four_accept_directory_flag(self, command: str) -> None:
        help_text = _run_cli(command, '--help').stdout
        assert '--directory' in help_text, (
            f'{command} missing --directory flag (#166 parity gap)'
        )

    @pytest.mark.parametrize(
        'command',
        ['list-sessions', 'delete-session', 'load-session', 'flush-transcript'],
    )
    def test_all_four_accept_output_format_flag(self, command: str) -> None:
        help_text = _run_cli(command, '--help').stdout
        assert '--output-format' in help_text, (
            f'{command} missing --output-format flag (#166 parity gap)'
        )

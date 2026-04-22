"""Cycle #27 cross-channel consistency audit (post-#181).

After #181 fix (envelope.exit_code must match process exit), this test
class systematizes the three-layer protocol invariant framework:

1. Structural compliance: Does the envelope exist? (#178)
2. Quality compliance: Is stderr silent + message truthful? (#179)
3. Cross-channel consistency: Do multiple channels agree? (#181 + this)

This file captures cycle #27's proactive invariant audit proving that
envelope fields match their corresponding reality channels:

- envelope.command ↔ argv dispatch
- envelope.output_format ↔ --output-format flag
- envelope.timestamp ↔ actual wall clock
- envelope.found/handled/deleted ↔ operational truth (no error block mismatch)

All tests passing = no drift detected.
"""

from __future__ import annotations

import json
import subprocess
from datetime import datetime, timezone
from pathlib import Path

import pytest

import sys
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))


def _run(args: list[str]) -> subprocess.CompletedProcess:
    """Run claw-code command and capture output."""
    return subprocess.run(
        ['python3', '-m', 'src.main'] + args,
        cwd=Path(__file__).parent.parent,
        capture_output=True,
        text=True,
    )


class TestCrossChannelConsistency:
    """Cycle #27: envelope fields must match reality channels.
    
    These are distinct from structural/quality tests. A command can
    emit structurally valid JSON with clean stderr but still lie about
    its own output_format or exit code (as #181 proved).
    """

    def test_envelope_command_matches_dispatch(self) -> None:
        """Envelope.command must equal the dispatched subcommand."""
        commands_to_test = [
            'show-command',
            'show-tool',
            'list-sessions',
            'exec-command',
            'exec-tool',
            'delete-session',
        ]
        failures = []
        for cmd in commands_to_test:
            # Dispatch varies by arity
            if cmd == 'show-command':
                args = [cmd, 'nonexistent', '--output-format', 'json']
            elif cmd == 'show-tool':
                args = [cmd, 'nonexistent', '--output-format', 'json']
            elif cmd == 'exec-command':
                args = [cmd, 'unknown', 'test', '--output-format', 'json']
            elif cmd == 'exec-tool':
                args = [cmd, 'unknown', '{}', '--output-format', 'json']
            else:
                args = [cmd, '--output-format', 'json']
            
            result = _run(args)
            try:
                envelope = json.loads(result.stdout)
            except json.JSONDecodeError:
                failures.append(f'{cmd}: JSON parse error')
                continue
            
            if envelope.get('command') != cmd:
                failures.append(
                    f'{cmd}: envelope.command={envelope.get("command")}, '
                    f'expected {cmd}'
                )
        assert not failures, (
            'Envelope.command must match dispatched subcommand:\n' +
            '\n'.join(failures)
        )

    def test_envelope_output_format_matches_flag(self) -> None:
        """Envelope.output_format must match --output-format flag."""
        result = _run(['list-sessions', '--output-format', 'json'])
        envelope = json.loads(result.stdout)
        assert envelope['output_format'] == 'json', (
            f'output_format mismatch: flag=json, envelope={envelope["output_format"]}'
        )

    def test_envelope_timestamp_is_recent(self) -> None:
        """Envelope.timestamp must be recent (generated at call time)."""
        result = _run(['list-sessions', '--output-format', 'json'])
        envelope = json.loads(result.stdout)
        ts_str = envelope.get('timestamp')
        assert ts_str, 'no timestamp field'
        
        ts = datetime.fromisoformat(ts_str.replace('Z', '+00:00'))
        now = datetime.now(timezone.utc)
        delta = abs((now - ts).total_seconds())
        
        assert delta < 5, f'timestamp off by {delta}s (should be <5s)'

    def test_envelope_exit_code_matches_process_exit(self) -> None:
        """Cycle #26/#181: envelope.exit_code == process exit code.
        
        This is a critical invariant. Claws that trust the envelope
        field must get the truth, not a lie.
        """
        cases = [
            (['show-command', 'nonexistent', '--output-format', 'json'], 1),
            (['show-tool', 'nonexistent', '--output-format', 'json'], 1),
            (['list-sessions', '--output-format', 'json'], 0),
            (['delete-session', 'any-id', '--output-format', 'json'], 0),
        ]
        failures = []
        for args, expected_exit in cases:
            result = _run(args)
            if result.returncode != expected_exit:
                failures.append(
                    f'{args[0]}: process exit {result.returncode}, '
                    f'expected {expected_exit}'
                )
                continue
            
            envelope = json.loads(result.stdout)
            if envelope['exit_code'] != result.returncode:
                failures.append(
                    f'{args[0]}: process exit {result.returncode}, '
                    f'envelope.exit_code {envelope["exit_code"]}'
                )
        
        assert not failures, (
            'Envelope.exit_code must match process exit:\n' +
            '\n'.join(failures)
        )

    def test_envelope_boolean_fields_match_error_presence(self) -> None:
        """found/handled/deleted fields must correlate with error block.
        
        - If field is True, no error block should exist
        - If field is False + operational error, error block must exist
        - If field is False + idempotent (delete nonexistent), no error block
        """
        cases = [
            # (args, bool_field, expected_value, expect_error_block)
            (['show-command', 'nonexistent', '--output-format', 'json'],
             'found', False, True),
            (['exec-command', 'unknown', 'test', '--output-format', 'json'],
             'handled', False, True),
            (['delete-session', 'any-id', '--output-format', 'json'],
             'deleted', False, False),  # idempotent, no error
        ]
        failures = []
        for args, field, expected_val, expect_error in cases:
            result = _run(args)
            envelope = json.loads(result.stdout)
            
            actual_val = envelope.get(field)
            has_error = 'error' in envelope
            
            if actual_val != expected_val:
                failures.append(
                    f'{args[0]}: {field}={actual_val}, expected {expected_val}'
                )
            if expect_error and not has_error:
                failures.append(
                    f'{args[0]}: expected error block, but none present'
                )
            elif not expect_error and has_error:
                failures.append(
                    f'{args[0]}: unexpected error block present'
                )
        
        assert not failures, (
            'Boolean fields must correlate with error block:\n' +
            '\n'.join(failures)
        )


class TestTextVsJsonModeDivergence:
    """Cycle #29: Document known text-mode vs JSON-mode exit code divergence.
    
    ERROR_HANDLING.md specifies the exit code contract applies ONLY when
    --output-format json is set. Text mode follows argparse defaults (e.g.,
    exit 2 for parse errors) while JSON mode normalizes to the contract
    (exit 1 for parse errors).
    
    This test class LOCKS the expected divergence so:
    1. Documentation stays aligned with implementation
    2. Future changes to text mode behavior are caught as intentional
    3. Claws consuming subprocess output can trust the docs
    """

    def test_unknown_command_text_mode_exits_2(self) -> None:
        """Text mode: argparse default exit 2 for unknown subcommand."""
        result = _run(['nonexistent-cmd'])
        assert result.returncode == 2, (
            f'text mode should exit 2 (argparse default), got {result.returncode}'
        )

    def test_unknown_command_json_mode_exits_1(self) -> None:
        """JSON mode: normalized exit 1 for parse error (#178)."""
        result = _run(['nonexistent-cmd', '--output-format', 'json'])
        assert result.returncode == 1, (
            f'JSON mode should exit 1 (protocol contract), got {result.returncode}'
        )
        envelope = json.loads(result.stdout)
        assert envelope['error']['kind'] == 'parse'

    def test_missing_required_arg_text_mode_exits_2(self) -> None:
        """Text mode: argparse default exit 2 for missing required arg."""
        result = _run(['exec-command'])  # missing name + prompt
        assert result.returncode == 2, (
            f'text mode should exit 2, got {result.returncode}'
        )

    def test_missing_required_arg_json_mode_exits_1(self) -> None:
        """JSON mode: normalized exit 1 for parse error."""
        result = _run(['exec-command', '--output-format', 'json'])
        assert result.returncode == 1, (
            f'JSON mode should exit 1, got {result.returncode}'
        )

    def test_success_path_identical_in_both_modes(self) -> None:
        """Success exit codes are identical in both modes."""
        text_result = _run(['list-sessions'])
        json_result = _run(['list-sessions', '--output-format', 'json'])
        assert text_result.returncode == json_result.returncode == 0, (
            f'success exit should be 0 in both modes: '
            f'text={text_result.returncode}, json={json_result.returncode}'
        )

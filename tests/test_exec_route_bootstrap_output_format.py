"""Tests for --output-format on exec-command/exec-tool/route/bootstrap (ROADMAP #168).

Closes the final JSON-parity gap across the CLI family. After #160/#165/
#166/#167, the session-lifecycle and inspect CLI commands all spoke JSON;
this batch extends that contract to the exec, route, and bootstrap
surfaces — the commands claws actually invoke to DO work, not just inspect
state.

Verifies:
- exec-command / exec-tool: JSON envelope with handled + source_hint on
  success; {name, handled:false, error:{kind,message,retryable}} on
  not-found
- route: JSON envelope with match_count + matches list
- bootstrap: JSON envelope with setup, routed_matches, turn, messages,
  persisted_session_path
- All 4 preserve legacy text mode byte-identically
- Exit codes unchanged (0 success, 1 exec-not-found)
"""

from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))


def _run(args: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(
        [sys.executable, '-m', 'src.main', *args],
        cwd=Path(__file__).resolve().parent.parent,
        capture_output=True,
        text=True,
    )


class TestExecCommandOutputFormat:
    def test_exec_command_found_json(self) -> None:
        result = _run(['exec-command', 'add-dir', 'hello', '--output-format', 'json'])
        assert result.returncode == 0, result.stderr

        envelope = json.loads(result.stdout)
        assert envelope['handled'] is True
        assert envelope['name'] == 'add-dir'
        assert envelope['prompt'] == 'hello'
        assert 'source_hint' in envelope
        assert 'message' in envelope
        assert 'error' not in envelope

    def test_exec_command_not_found_json(self) -> None:
        result = _run(['exec-command', 'nonexistent-cmd', 'hi', '--output-format', 'json'])
        assert result.returncode == 1

        envelope = json.loads(result.stdout)
        assert envelope['handled'] is False
        assert envelope['name'] == 'nonexistent-cmd'
        assert envelope['prompt'] == 'hi'
        assert envelope['error']['kind'] == 'command_not_found'
        assert envelope['error']['retryable'] is False
        assert 'source_hint' not in envelope

    def test_exec_command_text_backward_compat(self) -> None:
        result = _run(['exec-command', 'add-dir', 'hello'])
        assert result.returncode == 0
        # Single line prose (unchanged from pre-#168)
        assert result.stdout.count('\n') == 1
        assert 'add-dir' in result.stdout


class TestExecToolOutputFormat:
    def test_exec_tool_found_json(self) -> None:
        result = _run(['exec-tool', 'BashTool', '{"cmd":"ls"}', '--output-format', 'json'])
        assert result.returncode == 0, result.stderr

        envelope = json.loads(result.stdout)
        assert envelope['handled'] is True
        assert envelope['name'] == 'BashTool'
        assert envelope['payload'] == '{"cmd":"ls"}'
        assert 'source_hint' in envelope
        assert 'error' not in envelope

    def test_exec_tool_not_found_json(self) -> None:
        result = _run(['exec-tool', 'NotATool', '{}', '--output-format', 'json'])
        assert result.returncode == 1

        envelope = json.loads(result.stdout)
        assert envelope['handled'] is False
        assert envelope['name'] == 'NotATool'
        assert envelope['error']['kind'] == 'tool_not_found'
        assert envelope['error']['retryable'] is False

    def test_exec_tool_text_backward_compat(self) -> None:
        result = _run(['exec-tool', 'BashTool', '{}'])
        assert result.returncode == 0
        assert result.stdout.count('\n') == 1


class TestRouteOutputFormat:
    def test_route_json_envelope(self) -> None:
        result = _run(['route', 'review mcp', '--limit', '3', '--output-format', 'json'])
        assert result.returncode == 0

        envelope = json.loads(result.stdout)
        assert envelope['prompt'] == 'review mcp'
        assert envelope['limit'] == 3
        assert 'match_count' in envelope
        assert 'matches' in envelope
        assert envelope['match_count'] == len(envelope['matches'])
        # Every match has required keys
        for m in envelope['matches']:
            assert set(m.keys()) == {'kind', 'name', 'score', 'source_hint'}
            assert m['kind'] in ('command', 'tool')

    def test_route_json_no_matches(self) -> None:
        # Very unusual string should yield zero matches
        result = _run(['route', 'zzzzzzzzzqqqqq', '--output-format', 'json'])
        assert result.returncode == 0

        envelope = json.loads(result.stdout)
        assert envelope['match_count'] == 0
        assert envelope['matches'] == []

    def test_route_text_backward_compat(self) -> None:
        """Text mode tab-separated output unchanged from pre-#168."""
        result = _run(['route', 'review mcp', '--limit', '2'])
        assert result.returncode == 0
        # Each non-empty line has exactly 3 tabs (kind\tname\tscore\tsource_hint)
        for line in result.stdout.strip().split('\n'):
            if line:
                assert line.count('\t') == 3


class TestBootstrapOutputFormat:
    def test_bootstrap_json_envelope(self) -> None:
        result = _run(['bootstrap', 'review MCP', '--limit', '2', '--output-format', 'json'])
        assert result.returncode == 0, result.stderr

        envelope = json.loads(result.stdout)
        # Required top-level keys
        required = {
            'prompt', 'limit', 'setup', 'routed_matches',
            'command_execution_messages', 'tool_execution_messages',
            'turn', 'persisted_session_path',
        }
        assert required.issubset(envelope.keys())
        # Setup sub-envelope
        assert 'python_version' in envelope['setup']
        assert 'platform_name' in envelope['setup']
        # Turn sub-envelope
        assert 'stop_reason' in envelope['turn']
        assert 'prompt' in envelope['turn']

    def test_bootstrap_text_is_markdown(self) -> None:
        """Text mode produces Markdown (unchanged from pre-#168)."""
        result = _run(['bootstrap', 'hello', '--limit', '2'])
        assert result.returncode == 0
        # Markdown headers
        assert '# Runtime Session' in result.stdout
        assert '## Setup' in result.stdout
        assert '## Routed Matches' in result.stdout


class TestFamilyWideJsonParity:
    """After #167 and #168, ALL inspect/exec/route/lifecycle commands
    support --output-format. Verify the full family is now parity-complete."""

    FAMILY_SURFACES = [
        # (cmd_args, expected_to_parse_json)
        (['show-command', 'add-dir'], True),
        (['show-tool', 'BashTool'], True),
        (['exec-command', 'add-dir', 'hi'], True),
        (['exec-tool', 'BashTool', '{}'], True),
        (['route', 'review'], True),
        (['bootstrap', 'hello'], True),
    ]

    def test_all_family_commands_accept_output_format_json(self) -> None:
        """Every family command accepts --output-format json and emits parseable JSON."""
        failures = []
        for args_base, should_parse in self.FAMILY_SURFACES:
            result = _run([*args_base, '--output-format', 'json'])
            if result.returncode not in (0, 1):
                failures.append(f'{args_base}: exit {result.returncode} — {result.stderr}')
                continue
            try:
                json.loads(result.stdout)
            except json.JSONDecodeError as e:
                failures.append(f'{args_base}: not parseable JSON ({e}): {result.stdout[:100]}')
        assert not failures, (
            'CLI family JSON parity gap:\n' + '\n'.join(failures)
        )

    def test_all_family_commands_text_mode_unchanged(self) -> None:
        """Omitting --output-format defaults to text for every family command."""
        # Sanity: just verify each runs without error in text mode
        for args_base, _ in self.FAMILY_SURFACES:
            result = _run(args_base)
            assert result.returncode in (0, 1), (
                f'{args_base} failed in text mode: {result.stderr}'
            )
            # Output should not be JSON-shaped (no leading {)
            assert not result.stdout.strip().startswith('{')


class TestEnvelopeExitCodeMatchesProcessExit:
    """#181: Envelope exit_code field must match actual process exit code.
    
    Regression test for the protocol violation where exec-command/exec-tool
    not-found cases returned exit code 1 from the process but emitted
    envelopes with exit_code: 0 (default wrap_json_envelope). Claws reading
    the envelope would misclassify failures as successes.
    
    Contract (from ERROR_HANDLING.md):
    - Exit code 0 = success
    - Exit code 1 = error/not-found
    - Envelope MUST reflect process exit
    """

    def test_exec_command_not_found_envelope_exit_matches(self) -> None:
        """exec-command 'unknown-name' must have exit_code=1 in envelope."""
        result = _run(['exec-command', 'nonexistent-cmd-name', 'test-prompt', '--output-format', 'json'])
        assert result.returncode == 1, f'process exit should be 1, got {result.returncode}'
        envelope = json.loads(result.stdout)
        assert envelope['exit_code'] == 1, (
            f'envelope.exit_code mismatch: process=1, envelope={envelope["exit_code"]}'
        )
        assert envelope['handled'] is False
        assert envelope['error']['kind'] == 'command_not_found'

    def test_exec_tool_not_found_envelope_exit_matches(self) -> None:
        """exec-tool 'unknown-tool' must have exit_code=1 in envelope."""
        result = _run(['exec-tool', 'nonexistent-tool-name', '{}', '--output-format', 'json'])
        assert result.returncode == 1, f'process exit should be 1, got {result.returncode}'
        envelope = json.loads(result.stdout)
        assert envelope['exit_code'] == 1, (
            f'envelope.exit_code mismatch: process=1, envelope={envelope["exit_code"]}'
        )
        assert envelope['handled'] is False
        assert envelope['error']['kind'] == 'tool_not_found'

    def test_all_commands_exit_code_invariant(self) -> None:
        """Audit: for every clawable command, envelope.exit_code == process exit.
        
        This is a stronger invariant than 'emits JSON'. Claws dispatching on
        the envelope's exit_code field must get the truth, not a lie.
        """
        # Sample cases known to return non-zero
        cases = [
            # command, expected_exit, justification
            (['show-command', 'nonexistent-abc'], 1, 'not-found inventory lookup'),
            (['show-tool', 'nonexistent-xyz'], 1, 'not-found inventory lookup'),
            (['exec-command', 'nonexistent-1', 'test'], 1, 'not-found execution'),
            (['exec-tool', 'nonexistent-2', '{}'], 1, 'not-found execution'),
        ]
        mismatches = []
        for args, expected_exit, reason in cases:
            result = _run([*args, '--output-format', 'json'])
            if result.returncode != expected_exit:
                mismatches.append(
                    f'{args}: expected process exit {expected_exit} ({reason}), '
                    f'got {result.returncode}'
                )
                continue
            try:
                envelope = json.loads(result.stdout)
            except json.JSONDecodeError as e:
                mismatches.append(f'{args}: JSON parse failed: {e}')
                continue
            if envelope.get('exit_code') != result.returncode:
                mismatches.append(
                    f'{args}: envelope.exit_code={envelope.get("exit_code")} '
                    f'!= process exit={result.returncode} ({reason})'
                )
        assert not mismatches, (
            'Envelope exit_code must match process exit code:\n' + 
            '\n'.join(mismatches)
        )

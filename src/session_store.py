from __future__ import annotations

import json
from dataclasses import asdict, dataclass
from pathlib import Path


@dataclass(frozen=True)
class StoredSession:
    session_id: str
    messages: tuple[str, ...]
    input_tokens: int
    output_tokens: int


DEFAULT_SESSION_DIR = Path('.port_sessions')


def save_session(session: StoredSession, directory: Path | None = None) -> Path:
    target_dir = directory or DEFAULT_SESSION_DIR
    target_dir.mkdir(parents=True, exist_ok=True)
    path = target_dir / f'{session.session_id}.json'
    path.write_text(json.dumps(asdict(session), indent=2))
    return path


def load_session(session_id: str, directory: Path | None = None) -> StoredSession:
    target_dir = directory or DEFAULT_SESSION_DIR
    try:
        data = json.loads((target_dir / f'{session_id}.json').read_text())
    except FileNotFoundError:
        raise SessionNotFoundError(f'session {session_id!r} not found in {target_dir}') from None
    return StoredSession(
        session_id=data['session_id'],
        messages=tuple(data['messages']),
        input_tokens=data['input_tokens'],
        output_tokens=data['output_tokens'],
    )


class SessionNotFoundError(KeyError):
    """Raised when a session does not exist in the store."""
    pass


def list_sessions(directory: Path | None = None) -> list[str]:
    """List all stored session IDs in the target directory.
    
    Args:
        directory: Target session directory. Defaults to DEFAULT_SESSION_DIR.
    
    Returns:
        Sorted list of session IDs (JSON filenames without .json extension).
    """
    target_dir = directory or DEFAULT_SESSION_DIR
    if not target_dir.exists():
        return []
    return sorted(p.stem for p in target_dir.glob('*.json'))


def session_exists(session_id: str, directory: Path | None = None) -> bool:
    """Check if a session exists without raising an error.
    
    Args:
        session_id: The session ID to check.
        directory: Target session directory. Defaults to DEFAULT_SESSION_DIR.
    
    Returns:
        True if the session file exists, False otherwise.
    """
    target_dir = directory or DEFAULT_SESSION_DIR
    return (target_dir / f'{session_id}.json').exists()


def delete_session(session_id: str, directory: Path | None = None) -> bool:
    """Delete a session file from the store.
    
    Args:
        session_id: The session ID to delete.
        directory: Target session directory. Defaults to DEFAULT_SESSION_DIR.
    
    Returns:
        True if the session was deleted, False if it did not exist.
    """
    target_dir = directory or DEFAULT_SESSION_DIR
    path = target_dir / f'{session_id}.json'
    if path.exists():
        path.unlink()
        return True
    return False

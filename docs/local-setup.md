# Local setup — personal hardened claw fork

This doc captures the operational plumbing layered on top of the upstream
`claw` binary for the personal hardened build (this fork). It is **not** part
of upstream `ultraworkers/claw-code` — these scripts and units reference
machine-specific paths and are conventions for this fork only.

For the *what and why* of the hardened permission policy itself, see the
patches under `patches/` and the merged PRs (#1, #2).

## What's in scope

Three artifacts live outside the repo, in the user's `$HOME`:

| Artifact | Purpose |
|---|---|
| `~/.local/bin/cl` | Friction-killed wrapper around `claw`. Sets `OPENAI_BASE_URL` + `OPENAI_API_KEY` for the cross-machine LMStudio backend, defaults `--model openai/qwen/qwen3-coder-30b`, forwards everything else. |
| `~/.local/bin/cl-web` | Launches `ttyd` wrapping `cl`, exposing the REPL at `http://localhost:7682`. |
| `~/.config/systemd/user/cl-web.service` | User systemd unit that keeps `cl-web` alive across shell exits and reboots (with linger enabled). |

A symlink `~/.local/bin/claw -> /mnt/d/src/claw-code/rust/target/release/claw`
also exists so the raw binary is available on `PATH` for cases where the
LMStudio defaults aren't wanted (e.g. talking to the Anthropic API).

## Prerequisites

- WSL2 with systemd enabled (`/etc/wsl.conf` containing `[boot]\nsystemd=true`).
- `~/.local/bin` on `$PATH`.
- The release `claw` binary built at `/mnt/d/src/claw-code/rust/target/release/claw` (`cargo build --release --workspace` from `rust/`).
- `ttyd` installed system-wide (`sudo apt install -y ttyd`).
- LMStudio reachable at the IP/port baked into the wrapper (currently
  `http://100.100.0.10:1234/v1` over Nebula). Adjust the wrapper if the
  endpoint changes.

## Files

### `~/.local/bin/cl`

```bash
#!/usr/bin/env bash
# Friction-killed claw wrapper. Sets cross-machine LMStudio env defaults
# and a default model. Override --model from the CLI to pick a different
# model. To bypass entirely, call `claw` (the symlink to the raw binary).

set -euo pipefail

needs_model=true
for a in "$@"; do
    case "$a" in
        --model|--model=*)
            needs_model=false
            break
            ;;
    esac
done

if $needs_model; then
    set -- --model openai/qwen/qwen3-coder-30b "$@"
fi

exec env \
    OPENAI_BASE_URL="${OPENAI_BASE_URL:-http://100.100.0.10:1234/v1}" \
    OPENAI_API_KEY="${OPENAI_API_KEY:-unused}" \
    /mnt/d/src/claw-code/rust/target/release/claw "$@"
```

Make it executable: `chmod +x ~/.local/bin/cl`.

### `~/.local/bin/cl-web`

```bash
#!/usr/bin/env bash
# Serve `cl` (claw REPL with LMStudio defaults) via ttyd on
# 0.0.0.0:7682. Each browser connection spawns a fresh REPL session.
#
# Bind notes:
# - Do NOT use `-i lo` on WSL2 — claw's `lo` interface has TWO addresses
#   (127.0.0.1 + 10.255.255.254, the WSL bridge). ttyd's `-i lo` picks
#   the bridge address, which breaks Windows's localhost forwarding
#   (Windows only forwards to processes bound on 127.0.0.1 or 0.0.0.0).
#   Bind to 0.0.0.0 (omit -i) so localhost forwarding works.
# - Port 7681 (ttyd default) may already be bound on this WSL2 host by an
#   unrelated ttyd wrapping `login`. We use 7682 to avoid the conflict.
#
# Open from Windows browser: http://localhost:7682
# Open from inside WSL2:     http://127.0.0.1:7682

set -euo pipefail

exec ttyd \
    -p 7682 \
    -W \
    -O \
    -t titleFixed=claw \
    -t fontSize=14 \
    /home/prcdslnc13/.local/bin/cl
```

Make it executable: `chmod +x ~/.local/bin/cl-web`.

### `~/.config/systemd/user/cl-web.service`

```ini
[Unit]
Description=ttyd-wrapped claw REPL (cl-web) — web access on http://localhost:7682
After=network.target
Documentation=man:ttyd(1)

[Service]
Type=exec
# Important: cwd must be the project root so claw's config loader picks up
# the hardened /mnt/d/src/claw-code/.claw.json. Without this, claw falls
# back to the runtime hardcoded default (DangerFullAccess) — same gotcha
# fixed in PR #2.
WorkingDirectory=/mnt/d/src/claw-code
ExecStart=/home/prcdslnc13/.local/bin/cl-web
Restart=on-failure
RestartSec=5s
Environment=HOME=/home/prcdslnc13

[Install]
WantedBy=default.target
```

## Bring-up sequence

```bash
# 1. Reload user systemd so the new unit is visible
systemctl --user daemon-reload

# 2. Enable + start the service
systemctl --user enable --now cl-web.service

# 3. Enable linger so the service survives logout / WSL2 restart
sudo loginctl enable-linger "$USER"

# 4. Verify
systemctl --user status cl-web
curl -sS -o /dev/null -w 'HTTP %{http_code}\n' http://127.0.0.1:7682/
```

Then open `http://localhost:7682` in any Windows browser. The claw REPL
should boot and show:

```
Permissions      read-only
```

If you see `danger-full-access`, the cwd-precedence gotcha is back —
check `WorkingDirectory` in the unit file and the contents of
`/mnt/d/src/claw-code/.claw.json` (must include the `permissions` block,
not just `aliases`).

## Day-to-day commands

```bash
# Health
systemctl --user status cl-web

# Restart (e.g. after rebuilding the claw binary)
systemctl --user restart cl-web

# Logs (tail)
journalctl --user -u cl-web -f

# Stop / disable
systemctl --user stop cl-web
systemctl --user disable cl-web
```

## Known limitations / not-done

- **Localhost-only access.** Bound to all WSL2 interfaces but only reachable
  from the same Windows host (via localhost forwarding). To reach from a Mac
  or iPad over the Nebula overlay, either install Nebula inside WSL2 and bind
  ttyd to the Nebula interface, or run ttyd on the Windows host and have it
  shell into WSL.
- **No auth.** ttyd is unauthenticated. Fine for localhost-only; required if
  ever exposed beyond the local box. Add via `-c user:pass` in `cl-web`.
- **Hardcoded LMStudio endpoint.** The `cl` wrapper bakes
  `http://100.100.0.10:1234/v1`. Editing the script is the override path.
  A future improvement would read the endpoint from a config file.
- **Hardcoded default model.** Same situation as the endpoint — embedded in
  the wrapper.

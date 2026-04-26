# Contributing to claw-code

Thanks for your interest. This project follows the **gaebal-gajae pinpoint cadence** — see [ROADMAP.md](./ROADMAP.md) for the current pinpoint census. Here's how to contribute effectively.

## Security

For security vulnerabilities, see [SECURITY.md](./SECURITY.md). **Do not file public pinpoints for security issues.**

## Filing a ROADMAP Pinpoint

All feature requests and bug reports go through the pinpoint format (see `ROADMAP.md`). Each pinpoint must have:

- **Exact pinpoint** — one crisp sentence stating what is wrong or missing
- **Live evidence** — reproduction steps, logs, or observed behavior
- **Why distinct** — why this isn't already covered by an existing pinpoint
- **Concrete delta** — what the repo looks like after this is fixed (file-level)
- **Fix shape** — implementation sketch (function, module, config change)

Vague or duplicate pinpoints will be closed without comment.

## Build & Test

```bash
# Rust components
cd rust
cargo build
cargo test

# Node / Bun components (if present)
bun install
bun test
```

CI runs on every push. All tests must pass before review.

## Branch Naming

```
feat/<issue-or-slug>        # new feature
fix/<issue-or-slug>         # bug fix
docs/<slug>                 # documentation only
chore/<slug>                # tooling, deps, refactor
```

Example: `feat/jobdori-168c-emission-routing`

## Push Pattern (fork + origin)

This project maintains parity between the upstream (`origin`) and contributor forks.

```bash
# 1. Fork the repo on GitHub, then add your fork as a remote
git remote add fork https://github.com/<your-username>/claw-code.git

# 2. Create a branch off the target branch
git checkout -b feat/your-slug origin/feat/target-branch

# 3. Make changes, commit
git add .
git commit -m "feat: your change description"

# 4. Push to BOTH remotes (keep parity)
git push origin feat/your-slug --force-with-lease
git push fork feat/your-slug --force-with-lease

# 5. Open a PR against the target branch on GitHub
```

Three-way parity check before opening a PR:
```bash
git log --oneline -1 HEAD
git log --oneline -1 origin/feat/your-slug
git log --oneline -1 fork/feat/your-slug
# All three should show the same commit hash
```

## Code Style

- Rust: `cargo fmt` and `cargo clippy` before committing
- No dead code, no unused imports
- Comments in English; commit messages in English

## License

By contributing, you agree your contributions are licensed under the [MIT License](./LICENSE).

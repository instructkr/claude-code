# Security Policy

## Supported Versions

This project is pre-1.0 / active development. Only the `main` branch (and the current active feature branch) receives security attention. No LTS commitment exists yet.

| Branch | Supported |
|--------|-----------|
| `main` | ✅ |
| older forks/branches | ❌ |

## Reporting a Vulnerability

**Do not file a public GitHub issue for security vulnerabilities.**

Please use [GitHub Security Advisories](https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing/privately-reporting-a-security-vulnerability) to report privately:

1. Go to the **Security** tab of this repository
2. Click **"Report a vulnerability"**
3. Describe the issue with reproduction steps and impact

We aim to acknowledge within **72 hours** and work toward coordinated disclosure.

## Disclosure Process

1. Report received → acknowledgement within 72h
2. We assess severity and reproduce the issue
3. Fix developed and reviewed privately
4. Fix shipped; advisory published after patch is live
5. Credit given to reporter (unless they prefer anonymity)

## Scope

**In scope:**
- Remote code execution (RCE)
- Authentication or authorization bypass
- Secrets / credentials exfiltration
- Sandbox escape (agent isolation boundary violations)
- Privilege escalation

**Out of scope:**
- Denial of service (DoS/resource exhaustion)
- Social engineering attacks
- Vulnerabilities in third-party dependencies — report those upstream
- Behavior that is working as intended (check ROADMAP.md pinpoints first)

## License

This project is [MIT-licensed](./LICENSE) — provided as-is, without warranty of any kind.

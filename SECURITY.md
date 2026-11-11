# Security Policy

## Supported versions

Security fixes go into the latest release and `main`. Older releases do not get fixes: update with
**Help → Check for Updates** or from the [Releases page](https://github.com/Three-Bugs-Studio/3bcad/releases/latest).

| Version | Supported |
| --- | --- |
| Latest 1.x release | Yes |
| Older releases | No |

## Reporting a vulnerability

**Please do not open a public issue for security problems.**

Report privately through
[GitHub Security Advisories](https://github.com/Three-Bugs-Studio/3bcad/security/advisories/new).
Include the affected version or commit, steps to reproduce, and any proof-of-concept file
(for example a crafted STEP or 3MF file).

We aim to acknowledge reports within 7 days and to agree on a disclosure date with you once a fix is ready.
Areas of particular interest are file parsers/importers, the slicer launcher, which starts external programs,
and the in-app updater, which downloads a release and replaces the program.

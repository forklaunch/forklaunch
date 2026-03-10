---
title: Security Policy
category: Community
description: Security policy and vulnerability reporting guidelines for ForkLaunch.
---

## Supported Versions

We maintain security updates for the following versions of the ForkLaunch CLI:

| Version | Supported          |
| ------- | ------------------ |
| 1.x.x   | :white_check_mark: |
| < 1.0   | :x:                |

## Reporting a Vulnerability

We take security vulnerabilities seriously. If you discover a security issue, please bring it to our attention right away.

### Please DO:

1. Submit your findings through our [Security Advisory Form](https://github.com/forklaunch/cli/security/advisories/new)
2. Provide detailed steps to reproduce the vulnerability
3. Allow us reasonable time to respond before public disclosure
4. Include minimal proof-of-concept code if possible

### Please DO NOT:

1. Open a public GitHub issue about the vulnerability
2. Include sensitive information in initial communications
3. Attempt to access others' data
4. Modify or access data of other users

## Security Considerations

The ForkLaunch CLI is designed with the following security principles:

- All credentials and sensitive data are stored securely using system keychain where available
- Network communications are encrypted using TLS 1.2+
- Dependencies are regularly audited using `cargo audit` and `pnpm audit`
- Code is statically analyzed using `clippy` with security lints enabled

## Secure Development

We follow these practices for secure development:

1. All code changes undergo security review
2. Dependencies are pinned to specific versions
3. Regular security audits of the codebase
4. Automated vulnerability scanning in CI/CD
5. Code signing for released binaries

## Responsible Disclosure

We aim to respond to security reports within 48 hours and will keep reporters informed of our progress. After fixing a vulnerability, we:

1. Notify affected users (if applicable)
2. Release security patches
3. Publish security advisories
4. Credit reporters (unless anonymity is requested)

## Contact

For sensitive security issues, contact us at:
security@forklaunch.com (GPG key available)

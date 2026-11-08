# Security Policy

## Supported versions

The latest release on the `main` branch is supported. Older releases may receive
fixes at the maintainer's discretion.

## Reporting a vulnerability

Please **do not open a public issue** for a security problem. Report it privately
through GitHub Security Advisories on the repository:

<https://github.com/7unar/7player/security/advisories/new>

If that is not available, email `lunar0814@163.com` with:

- a description of the issue and its impact;
- reproduction steps or a minimal proof of concept;
- the affected version and platform.

You can expect an initial response within a few days. Please allow time for a fix
and a coordinated release before public disclosure.

## Scope and notes

- 7player is an unofficial client. NetEase API behavior, availability and account
  restrictions are outside this project's security scope.
- Cookies are stored locally at `~/.config/7player/cookies.json` with mode `0600`.
  Never attach that file, a QR login key, or account details to an issue.
- The app does not ship telemetry and does not send data anywhere except the
  NetEase endpoints required for login, browse and playback.

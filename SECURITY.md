# Security reporting

[Home](README.md) · [Documentation](README.md#documentation)

Report suspected vulnerabilities through
[GitHub private vulnerability reporting](https://github.com/FueledByRedBull/tarnisheds-arsenal/security/advisories/new).
Include the affected version, reproduction steps, and potential impact. Keep exploit
details and sensitive information out of public issues and pull requests.

Only the latest published release receives security fixes; a fix ships as a new
release rather than a patch to an older one. This is a one-maintainer project:
expect an acknowledgement within 14 days.

## Reproduction reports

The app can preview and download a reproduction report. It contains the current
normalized search inputs, the snapshot identity, the displayed selection when it
is not stale, and an error if present. It does not contain the saved-build library,
raw logs or private manifest source paths. Any text in the report that looks like
a file path (drive, network share, backslash, `file://` or Unix home and system
paths), a web address, an e-mail address, a credential assignment, a bearer token
or a long key-like string is replaced with an omission notice. This is a
precaution, not a guarantee that arbitrary user-entered text is anonymous: review
the preview before sharing it. Nothing is submitted automatically.

For ordinary bugs and feature requests, use
[GitHub Issues](https://github.com/FueledByRedBull/tarnisheds-arsenal/issues).

# Contributing to Lossy

Lossy is a privacy-sensitive Windows application. Correctness, recoverability, and data minimization take priority over adding features quickly.

## Workflow

1. Create a focused branch from `main`.
2. Keep changes small enough to review and test independently.
3. Use Conventional Commit messages such as `feat:`, `fix:`, `test:`, `docs:`, and `chore:`.
4. Push the branch and open a pull request.
5. Describe the behavior change, privacy impact, failure modes, and verification performed.
6. Merge only after required checks pass.

## Privacy rules

- Never commit real captured drafts, clipboard data, contact names, window titles, URLs, file paths, encryption keys, or production databases.
- Test fixtures must use obviously synthetic content.
- Logs must contain identifiers and health metadata only; captured content must be redacted by construction.
- New capture paths must pass through the central privacy filter before storage or diagnostics.
- Lossy must fail closed if secure-field detection or encryption is uncertain.

## Engineering rules

- The background agent is the only database writer.
- Do not add raw global-keystroke capture.
- Do not perform storage work inside Windows event callbacks.
- Prefer isolating an uncertain context over merging drafts incorrectly.
- Every schema change requires a forward migration and an upgrade test.
- Every capture adapter requires context-switch, stale-event, and privacy tests.

## Pull request verification

At minimum, a pull request should include:

- Automated tests for changed logic
- Formatting and static-analysis checks
- A note about manual Windows testing when native behavior changes
- Confirmation that no sensitive data appears in fixtures, logs, screenshots, or descriptions


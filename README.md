# Lossy

Lossy is a Windows-first, fully local draft-recovery and smart-clipboard app. It runs quietly in the signed-in user's session, keeps supported text drafts separated by application and context, and helps recover text or copied images after crashes, navigation mistakes, and restarts.

## Status

Lossy is currently in the architecture and feasibility stage. The implementation will prioritize capture correctness, local privacy, and crash-safe persistence before broader application support.

Read the complete [product and technical architecture](./plan.md).

## Core principles

- Local-only storage with no account, cloud sync, or telemetry
- No raw global keystroke logging
- Secure fields and private browsing excluded
- Separate drafts for different conversations, workspaces, documents, and editors
- Silent background startup with an on-demand desktop interface
- Encrypted crash-safe storage for text and clipboard images

## Target platform

The first release targets Windows 10/11 x64.

## Security

Lossy handles sensitive text. Do not include captured content, window titles, URLs, file paths, or clipboard data in logs, test fixtures, screenshots, or bug reports.


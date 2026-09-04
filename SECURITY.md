# Security Policy

## Reporting a vulnerability

Please do not open a public issue containing vulnerability details or captured user data. Use GitHub's private vulnerability-reporting feature for this repository when available, or contact the repository owner privately.

Include the smallest synthetic reproduction possible. Never attach a real Lossy database, clipboard image, draft, log containing private window metadata, or DPAPI-protected key material.

## Security boundaries

Lossy is designed to:

- Store captured content locally and encrypted at rest
- Exclude password and secure controls before content enters durable queues
- Avoid raw global-keystroke collection
- Restrict local IPC to the signed-in Windows user
- Keep diagnostics free of captured content

Security-sensitive changes require focused tests for secure-field exclusion, encryption failure, IPC authorization, and content-free diagnostics.


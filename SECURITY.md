# Security Policy

## Supported Versions

The CodeCartographer project supports the following versions:

- **v3.0.x:** Supported until December 31, 2024
- **v2.x:** End-of-life December 31, 2024
- **v1.x:** End-of-life December 31, 2023

## Reporting a Vulnerability

If you find a security vulnerability, please follow these steps:

1. Report the vulnerability by opening an issue in the GitHub repository.
2. Provide as much detail as possible, including steps to reproduce the vulnerability.
3. Do not disclose the vulnerability publicly until it has been addressed.

## Security Fixes

Security fixes will be released as patch versions (e.g., 3.0.1, 3.0.2). These releases will be communicated through the GitHub repository and release notes.

## Path Validation

All paths provided to CodeCartographer will be validated to prevent directory traversal attacks. Paths containing '..' that would escape the project root will be rejected.

## Regex Timeouts

Regular expression operations will include safety limits to prevent denial-of-service attacks due to catastrophic backtracking.

## File Permissions

Temporary files created by CodeCartographer will be created with restricted permissions (0o600) to prevent unauthorized access.

## Dependency Security

Dependencies will be kept up-to-date. Known security vulnerabilities in dependencies will be addressed promptly.

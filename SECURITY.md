# Security Policy

## Supported Versions

We actively support the following versions of Wanderlust with security updates:

| Version | Supported          |
| ------- | ------------------ |
| 0.1.x   | :white_check_mark: |
| < 0.1   | :x:                |

## Reporting a Vulnerability

We take the security of Wanderlust seriously. If you discover a security vulnerability, please follow these guidelines:

### :lock: Private Disclosure Process

**DO NOT** create a public GitHub issue for security vulnerabilities.

Instead, please report security issues privately using one of these methods:

1. **GitHub Security Advisories (Preferred)**
   - Go to the [Security tab](https://github.com/Michael-A-Kuykendall/wanderlust/security) of this repository
   - Click "Report a vulnerability"
   - Fill out the advisory form with details

2. **Direct Email**
   - Send details to: michaelallenkuykendall@gmail.com
   - Include "SECURITY: Wanderlust" in the subject line

### :memo: What to Include

Please provide the following information in your report:

- **Description**: Clear description of the vulnerability
- **Impact**: What could an attacker accomplish?
- **Reproduction**: Step-by-step instructions to reproduce the issue
- **Environment**:
  - Wanderlust version
  - Operating system (Windows)
  - Rust version
- **Proof of Concept**: Code or logs demonstrating the issue
- **Suggested Fix**: If you have ideas for remediation

### :stopwatch: Response Timeline

We aim to respond to security reports according to the following timeline:

- **Initial Response**: Within 48 hours of report
- **Triage**: Within 7 days - confirm/deny vulnerability
- **Resolution**: Within 30 days for critical issues, 90 days for others
- **Disclosure**: Public disclosure after fix is released and users have time to update

### :warning: Vulnerability Severity Guidelines

We use the following criteria to classify vulnerabilities:

#### Critical
- Privilege escalation via Registry manipulation
- Arbitrary code execution via PATH injection
- Unauthorized system modifications

#### High
- Denial of service via PATH corruption
- Privacy leaks (scanning sensitive directories erroneously)

#### Medium
- Information disclosure
- Panic in safe Rust code
- Resource leaks

#### Low
- Issues requiring local access
- Minor information leaks
- Performance degradation

### :trophy: Recognition

We believe in recognizing security researchers who help keep Wanderlust secure:

- **Hall of Fame**: Public recognition in our security acknowledgments
- **CVE Assignment**: For qualifying vulnerabilities
- **Acknowledgment**: Credit in release notes

*Note: We currently do not offer monetary bug bounties, but we deeply appreciate responsible disclosure.*

### :rotating_light: Emergency Contact

For critical vulnerabilities that are being actively exploited:

- **Email**: michaelallenkuykendall@gmail.com
- **Subject**: "URGENT SECURITY: Wanderlust - [Brief Description]"
- **Response**: Within 12 hours

## Security Best Practices

### For Users

1. **Keep Updated**: Always use the latest supported version
2. **Access Control**: Only run `wanderlust install` or `wanderlust heal` when you are sure of the environment
3. **Trusted Sources**: Only use Wanderlust from official releases

### For Developers

1. **Dependencies**: Regularly audit and update dependencies
2. **Input Validation**: Validate registry inputs and file paths
3. **Memory Safety**: Wanderlust is built with Rust for memory-safe execution

## Security Features

Wanderlust includes several built-in security features:

- **Memory Safety**: Built with Rust
- **UAC Validations**: Explicit Admin checks using Windows Token API
- **Backup**: Always backs up Registry keys before writing
- **Verification**: Probes system health (cmd/whoami) after changes and rolls back if broken

## Contact

For non-security related issues, please use:
- GitHub Issues: https://github.com/Michael-A-Kuykendall/wanderlust/issues
- GitHub Discussions: https://github.com/Michael-A-Kuykendall/wanderlust/discussions

---

*This security policy is effective as of January 2026 and may be updated periodically.*

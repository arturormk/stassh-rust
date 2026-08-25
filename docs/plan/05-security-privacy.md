# Security And Privacy

## Security Posture

The application protects and organizes SSH access metadata. It is not a password manager, secure operating system, remote attestation system, or protection against a compromised local machine.

Avoid exaggerated security claims. Documentation and diagnostics should be explicit about what is protected and what is not.

## Portable Mode Limits

Portable vault mode is intended for trusted or reasonably trusted computers.

An encrypted vault does not make an untrusted computer safe. A malicious host can capture passphrases, inspect decrypted data, intercept keystrokes, hijack sessions, copy ordinary private keys, or manipulate launched programs.

Portable mode should reduce practical residue by avoiding persistent recent-host lists, search history, decrypted caches, vault passphrases, generated SSH configs, portable identity paths, and action history where reasonable.

## Host-Key Verification

Do not weaken OpenSSH host-key verification for convenience.

If the application manages a vault-specific `known_hosts` file, generated OpenSSH config should point to it while preserving meaningful first-connect and key-change warnings.

Dangerous bypasses such as disabling host-key checks should not be casual UI actions. If supported at all, they must be explicit and clearly marked as risky.

## Passwords And Private Keys

Password authentication should be supported by allowing OpenSSH to prompt. Stored SSH passwords are not part of the initial design.

Private keys should remain outside synchronized configuration by default. Optional portable private key files may exist inside a user-controlled vault, but the application should rely on OpenSSH key protection and avoid reading private key material into GUI frontend state.

Agent-based and hardware-backed identities are normal identity sources, not special cases.

## Frontend Secret Boundary

Keep sensitive material primarily in Rust/backend code:

- vault passphrases
- decrypted vault keys
- decrypted records
- private operational data
- identity paths where unnecessary for display

The GUI should receive only the data needed to render state and user choices. JavaScript/WebView code must not handle private key material.

Use practical zeroization for passphrases, keys, and sensitive buffers where established crates make it useful. Do not promise perfect memory erasure on all platforms.

## Logging And Diagnostics

Logs must not contain passwords, private keys, decrypted secret payloads, or raw sensitive records.

Hostnames and usernames may be sensitive. Normal logs should be conservative. Debug logs and diagnostic exports should make sensitivity clear and redact secrets by default.

Diagnostic output should explain resolved behavior without leaking secret values.

## Recovery

There should be no misleading passphrase recovery promise. If the vault encryption is implemented correctly and the passphrase is lost with no cached or alternate unlock key, recovery may be impossible.

OS credential-store integration may be added later as an optional machine-local convenience. Portable mode should avoid persistent unlock storage by default.

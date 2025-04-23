# Foxy Secure Backup Plan: Private Key Derivation and Storage

## 1. Objective

Design a secure, scalable, user-friendly way to backup and restore Foxy wallets by focusing on the **private key** (not the mnemonic). Ensure users can recover their wallets after device loss without storing sensitive information server-side or relying on passwords.

## 2. Key Decisions

- **We backup and restore the private key directly**, not the mnemonic.
- **The private key backup is encrypted using a derived AES-256 key.**
- **Key derivation is server-assisted** using a root secret and HMAC.
- **Backups are saved to app-private storage** and protected by Android's Auto Backup.
- **Server root keys are securely managed with AWS Secrets Manager and KMS.**

## 3. Backup Flow Overview

### 3.1. Backup Creation (on device)
- Generate user's private key during onboarding.
- Store private key in EncryptedSharedPreferences, gated by BiometricPrompt.
- When user initiates backup:
    - Request `derived_key` from server via `/derive-key` endpoint.
    - Encrypt private key using AES-256-GCM with `derived_key`.
    - Save backup file locally in app-private storage.

### 3.2. Backup File Structure
```json
{
  "key_version": "v1",
  "encrypted_private_key": "<base64-encoded ciphertext>",
  "encryption_algorithm": "AES-256-GCM",
  "created_at": "<timestamp>"
}
```

### 3.3. Restore Flow (on new device)
- Load backup file from app-private storage.
- Extract `key_version`.
- Request `derived_key` from server using `user_id` and `key_version`.
- Decrypt `encrypted_private_key` using `derived_key`.
- Import private key back into secure storage.

## 4. Server-Side `/derive-key` API

### Endpoint
`POST /derive-key`

### Request Body
```json
{
  "user_id": "user-1234",
  "key_version": "v1"
}
```

### Server Processing
- Authenticate client via JWT.
- Fetch `server_root_key_v1` from AWS Secrets Manager.
- Calculate `derived_key = HMAC(server_root_key_v1, user_id)`.
- Return base64-encoded `derived_key` to client.

### Response Body
```json
{
  "derived_key": "<base64-encoded AES key>"
}
```

## 5. AWS Key Management Strategy

| Component | Detail |
|:---------|:------|
| Secrets Manager | Store `server_root_key_v1`, `server_root_key_v2`, etc. securely. |
| KMS Encryption | Encrypt secrets using AWS KMS. Only decrypt at Lambda runtime. |
| IAM Permissions | Lambda role has `secretsmanager:GetSecretValue` for `/foxy/keys/*` only. |
| Key Rotation | Use versioned keys. New backups use `v2`, old backups decrypt with `v1`. |

## 6. Key Rotation Strategy

- Rotate keys periodically (e.g., yearly).
- Introduce a new `server_root_key_v2`.
- New backups use `v2` automatically.
- Old backups continue using `v1` for decryption.
- (Optional) Offer users a "Rebackup" feature to re-encrypt with latest key.

## 7. Security Considerations

- **No custody of user secrets.** Server only derives encryption material statelessly.
- **Strong separation of concerns:** key derivation, encryption, storage are modular.
- **No reliance on user passwords.**
- **Full control over security posture, no reliance on Google OAuth token behavior.**
- **Versioned key strategy** future-proofs for operational and crypto agility.

---

# Summary
This plan gives Foxy a secure, scalable, user-friendly backup and recovery process based on direct private key protection, strong encryption, server-side HMAC derivation, and AWS-managed secrets, aligned with modern best practices.


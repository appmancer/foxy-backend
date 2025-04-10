# 📋 Foxy Lambda — Wallet Balance Integration Guide

This document explains how to use the `/wallet/balance` endpoint to fetch the current ETH balance for a user on the Optimism network, including its equivalent value in GBP.

---

## 🔌 Endpoint

**GET** `/wallet/balance`

---

## 🦮 Request Format

> ⚠️ This endpoint does **not** accept a JSON body. The only requirement is a valid `Authorization` header containing the user’s access token.

Example header:

```
Authorization: Bearer eyJraWQiOiJLTzY1...
```

The backend will:
- Extract the `sub` (user ID) from the access token
- Look up the user’s wallet address (stored in Cognito)
- Query the Optimism blockchain for the latest ETH balance
- Convert the ETH amount to a fiat currency using live exchange rates (currently fixed to GBP)

---

## 📦 Response Format

```json
{
  "balance": "0.013816",
  "token": "ETH",
  "wei": "13816614144794697",
  "fiat": {
    "value": "19.73",
    "currency": "GBP"
  }
}
```

### Field Breakdown:

| Field         | Type   | Description                                         |
|---------------|--------|-----------------------------------------------------|
| `balance`     | String | Human-readable ETH (6 d.p. precision)               |
| `token`       | String | Token type (currently only `"ETH"`)                |
| `wei`         | String | Raw balance in **Wei** (as a string)               |
| `fiat.value`  | String | Approximate value of the ETH in fiat (e.g., GBP)   |
| `fiat.currency` | String | ISO currency code (currently always `"GBP"`)      |

---

## 🔒 Security

- Requires a valid **Cognito access token**
- The wallet address is **not** sent by the client — it is retrieved securely from the user’s profile in Cognito

---

## ⚠️ Wallet Address Storage

Wallet addresses are currently stored as a **custom attribute in Cognito** per user. This ensures:

- The backend cannot be tricked into fetching arbitrary wallet balances
- Each user can only retrieve their own balance
- Wallets are cryptographically tied to the user ID (`sub`)

This is considered a safe practice, as long as:
- You validate the JWT properly
- You don’t allow wallet address updates from the client without verification

---

## 🧚‍♂️ Testing Notes

- Use a test user with a known wallet ID stored in Cognito
- Ensure that the wallet has Optimism ETH (mainnet or Sepolia depending on env)
- For local testing:
  - Use `cargo lambda watch`
  - Assume the correct IAM role if running with AWS SDK

---

## 📌 To-Do

- [ ] Add ERC-20 token support (e.g., USDC)
- [x] Extend response to include fiat-converted balance (GBP)
- [ ] Return gas info alongside balance to assist in UX pre-checks

---


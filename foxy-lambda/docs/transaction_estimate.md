# 📋 Foxy Lambda — Transaction Estimate Integration Guide

This document outlines how to integrate the `/transactions/estimate` endpoint into the mobile app. This allows users to preview the total cost (including fees and gas) before sending ETH or a supported token.

---

## 🔌 Endpoint

**POST** `/transactions/estimate`

---

## 🧮 Request Format

```json
{
  "token_type": "ETH",
  "sender_address": "0xC4027B0df7B2d1fAf281169D78E252f8D86E4cdC",
  "recipient_address": "0x1aB7Bc9CA7586fa0D9c6293A27d5c001622E08C7",
  "fiat_amount": 5000,
  "fiat_currency": "GBP"
}
```

> 💡 `fiat_amount` is in **minor units**, so 5000 = £50.00

---

## 📦 Response Format

```json
{
  "token_type": "ETH",
  "fiat_amount_minor": 5000,
  "fiat_currency": "GBP",
  "eth_amount": "0.03133",
  "wei_amount": "31334421598213100",
  "fees": {
    "service_fee_wei": "100000000000000",
    "service_fee_eth": "0.00010",
    "network_fee_wei": "21005712000",
    "network_fee_eth": "0.000000021",
    "total_fee_wei": "10021005712000",
    "total_fee_eth": "0.000121"
  },
  "gas": {
    "estimated_gas": "21000",
    "gas_price": "1000272",
    "max_fee_per_gas": "1200326",
    "max_priority_fee_per_gas": "0"
  },
  "exchange_rate": "1595.77",
  "exchange_rate_expires_at": "2025-03-25T11:03:45Z",
  "recipient_address": "0x1aB7Bc9CA7586fa0D9c6293A27d5c001622E08C7",
  "status": ["SUCCESS"],
  "message": null
}
```

---

## 💰 Fee Structure (ETH-Based)

All fees are now calculated in **ETH** (or more precisely, **wei**) regardless of fiat input.

- **Service Fee**: Retrieved from DynamoDB and priced in **wei**:
  - `base_fee`: fixed fee in wei
  - `percentage_fee`: % of the ETH amount (in basis points, i.e. 1% = 100)
- **Network Fee**: Estimated from Optimism gas metrics, L2 + L1 data costs.
- **Total Fee**: `network_fee + service_fee`

Example:
> Sending £50 (u22480.031 ETH) could result in a service fee of ~0.0001 ETH and network fee of ~0.00000002 ETH

---

## 🥒 Exchange Rate Expiry

- The `exchange_rate` is valid until `exchange_rate_expires_at`
- If expired, the app must refresh before confirming the transaction
- Primary source: **Chainlink**
- Fallback source: **Coinbase**

---

## ⛖️ Errors

- `400 Bad Request` for invalid inputs (bad addresses, unsupported tokens)
- `404 Not Found` if wallet address fails validation
- `422 Unprocessable Entity` for soft errors like missing exchange rates
- `500 Internal Server Error` for gas estimation failures, internal errors
- `429 Too Many Requests` if rate-limited by upstream providers

Status flags are included in the `status` array for programmatic error handling.

---

## ✅ Supported Token Types

```rust
pub enum TokenType {
    ETH,
    // USDC coming soon
}
```

---

## 🧪 Testing Notes

- Use test wallets:
  - Sender: `0xC4027B0df7B2d1fAf281169D78E252f8D86E4cdC`
  - Recipient: `0x1aB7Bc9CA7586fa0D9c6293A27d5c001622E08C7`
- Valid test cases:
  - 1p → Check for rounding/zero fee edge case
  - £1,000,000 → Ensure no overflows and ETH conversion remains accurate
  - Invalid token → Expect 400 and error flag
  - Unknown fiat → Expect 422 with `EXCHANGE_RATE_UNAVAILABLE`
  - Empty or invalid address → Expect 404 with `WALLET_NOT_FOUND`

---

## 📌 To-Do

- [ ] Add support for USDC and other tokens
- [ ] Implement `/transactions/estimate/quote` to lock exchange rates
- [ ] Add minimum and maximum allowed fiat/crypto amounts
- [ ] Document fallback behavior if gas estimation fails

---


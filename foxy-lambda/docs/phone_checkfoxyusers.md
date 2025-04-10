# 📋 Foxy Lambda — Phone Number Lookup Integration Guide

This document explains how to use the `/phone/checkfoxyusers` endpoint to determine which of a user's contacts are Foxy users and fetch their associated wallet addresses.

---

## 🔌 Endpoint

**POST** `/phone/checkfoxyusers`

---

## 🧮 Request Format

```json
{
  "phone_numbers": [
    "+447533907498",
    "+447593322921"
  ],
  "country_code": "GB"
}
```

> ☎️ `phone_numbers` are normalized at the back end, and should work well with formats from the phone. Send the users country code as a default location for numbers.

---

## 📦 Response Format

```json
[
  {
    "phone_number": "+447533907498",
    "wallet_address": "0x4c9adc46f08bfbc4ee7b65d7f4b138ce3350e923"
  },
  {
    "phone_number": "+447593322921",
    "wallet_address": "0xe4d798c5b29021cdecda6c2019d3127af09208ca"
  }
]
```

### Field Breakdown

| Field            | Type   | Description                                  |
| ---------------- | ------ | -------------------------------------------- |
| `number`         | String | Original phone number provided by the client |
| `wallet_address` | String | Wallet address associated with the user      |

---

## 🔒 Security

- Requires a valid **Cognito JWT access token** in the `Authorization` header.
- Token is validated via `with_valid_user()`.
- The server only returns wallet addresses of users who are already registered with Foxy.

Example header:

```
Authorization: Bearer eyJraWQiOiJLTzY1...
```

---

## ⚙️ Internal Logic

- Phone numbers are normalized and hashed using SHA-256 (salted). We do not store the user's phone number
- DynamoDB is queried in parallel batches to check which hashes exist.
- Matched hashes return a wallet address, which is included in the response.

---

## 🧪 Testing Notes

- Use known test numbers seeded in the database.
- Local testing can be run via `cargo lambda watch` with a valid `.env` file including:
    - `DYNAMODB_USER_LOOKUP_TABLE_NAME`
    - Valid IAM credentials
- Ensure wallet addresses are stored in the expected field (`wallet_address`).
- A mocked or real JWT must be supplied.

---

## 📌 To-Do

-

---


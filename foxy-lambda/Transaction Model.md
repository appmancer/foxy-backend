# Transaction Model Documentation

## Overview
The transaction model represents the lifecycle of a financial transaction within the system. Each transaction progresses through various states, from creation to finalization, and includes metadata, gas details, and event tracking. This document outlines the updated structure, its components, and fully populated examples for each stage of the lifecycle.

---

## **Transaction Lifecycle**
A transaction moves through the following states:

1. **Created** – The transaction intent is recorded but not yet signed.
2. **Signed** – The transaction has been signed by the user.
3. **Broadcasted** – The transaction has been sent to the blockchain network.
4. **Pending** – The transaction is in the mempool, awaiting confirmation.
5. **Confirmed** – The transaction has been mined with at least one confirmation.
6. **Finalized** – The transaction has multiple confirmations and is considered immutable.
7. **Failed** – The transaction failed due to issues like insufficient gas.
8. **Cancelled** – The transaction was manually cancelled or replaced.
9. **Error** – A system error occurred during processing.

Additionally, Layer 2 Optimism transactions may have specific statuses such as:
- **Deposited** – Funds have been bridged from Layer 1 to Layer 2.
- **Finalizing** – Transaction is in a fraud-proof window.
- **Withdrawn** – Funds have been withdrawn to Layer 1.
- **ChallengePeriod** – The transaction is undergoing validation.
- **Bridging** – The transaction is transferring across networks.

---

## **Example Transactions for Each Lifecycle Stage**

### **Created Transaction**
```json
{
    "transaction_id": "tx_19831224_george_michael",
    "user_id": "user_001",
    "from_address": "0x1234567890ABCDEF1234567890ABCDEF12345678",
    "to_address": "0xABCDEF1234567890ABCDEF1234567890ABCDEF12",
    "amount": 100.0,
    "token": "ETH",
    "status": "Created",
    "metadata": {
        "message": "Let's fund the next album!",
        "display_currency": "GBP",
        "expected_currency_amount": 82.5,
        "from": {
            "name": "George Michael",
            "user_id": 1,
            "wallet": "0x1234567890ABCDEF1234567890ABCDEF12345678"
        },
        "to": {
            "name": "Andrew Ridgeley",
            "user_id": 2,
            "wallet": "0xABCDEF1234567890ABCDEF1234567890ABCDEF12"
        }
    },
    "priority_level": "Standard",
    "network": "Optimism",
    "created_at": "2023-12-25T12:00:00Z",
    "last_updated": "2023-12-25T12:00:00Z"
}
```

### **Signed Transaction**
```json
{
    "transaction_id": "tx_19831224_george_michael",
    "status": "Signed",
    "last_updated": "2023-12-25T12:01:00Z"
}
```

### **Broadcasted Transaction**
```json
{
    "transaction_id": "tx_19831224_george_michael",
    "status": "Broadcasted",
    "transaction_hash": "0xabcdef1234567890",
    "last_updated": "2023-12-25T12:02:00Z"
}
```

### **Pending Transaction**
```json
{
    "transaction_id": "tx_19831224_george_michael",
    "status": "Pending",
    "transaction_hash": "0xabcdef1234567890",
    "last_updated": "2023-12-25T12:03:00Z"
}
```

### **Confirmed Transaction**
```json
{
    "transaction_id": "tx_19831224_george_michael",
    "status": "Confirmed",
    "transaction_hash": "0xabcdef1234567890",
    "block_number": 1203945,
    "last_updated": "2023-12-25T12:05:00Z"
}
```

### **Finalized Transaction**
```json
{
    "transaction_id": "tx_19831224_george_michael",
    "status": "Finalized",
    "transaction_hash": "0xabcdef1234567890",
    "block_number": 1203945,
    "last_updated": "2023-12-25T12:10:00Z"
}
```

### **Failed Transaction**
```json
{
    "transaction_id": "tx_19831224_george_michael",
    "status": "Failed",
    "transaction_hash": "0xabcdef1234567890",
    "error_reason": "Out of gas",
    "last_updated": "2023-12-25T12:06:00Z"
}
```

---

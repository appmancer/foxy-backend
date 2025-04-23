## 🧾 Overview

In Foxy, a **transaction** represents a user-initiated intent to send a fixed fiat amount (e.g., £100) to another user. While the underlying infrastructure uses Ethereum (specifically the Optimism network), the system is designed to present a **fiat-first** experience to users. That means:

- Users **specify amounts in fiat**, not crypto.
- **Fees** (network + service) are shown in ETH but derived transparently from fiat inputs.
- All complexity of gas estimation, ETH fluctuation, and wallet mechanics are hidden behind a simple user interface.

### 🏆 Primary Goals of the Transaction System

1. **Peer-to-Peer Payments**  
   Support fast, low-cost ETH transfers between users, while abstracting the crypto complexity.
2. **Transparent Service Fees**  
   Foxy adds a fixed basis point fee (currently 25bps) which is clearly displayed to the user and embedded into the transaction process.
3. **Non-Custodial by Design**  
   Foxy never stores or manages private keys. All transactions are signed on the user's device using their own wallet (or embedded Foxy key).
4. **Event-Sourced Backend**  
   The system captures all transaction lifecycle steps as immutable events, which power the transaction history and state views.
5. **Dual-Transaction Architecture**  
   Each Foxy transaction (ftx) consists of two Ethereum transactions:

- **Tx1**: George → Andrew  
  George sends the full ETH amount (converted from fiat) directly to Andrew.
- **Tx2**: George → Foxy  
  George pays a small ETH-denominated service fee to Foxy.

This structure provides the best balance between transparency, user experience, and network efficiency. Both transactions are recorded as part of a single logical Foxy transaction and surfaced cleanly in the UI.

### 🧾 Foxy Transaction Lifecycle Mapping
| EventType | BundleStatus    | Main Tx Status | Fee Tx Status |
|-----------|-----------------|-------------|---------------|
| Initiate  | `Initiated`     | `Created`   | `Created`     |
| Sign      | `Signed`        | `Signed`    | `Signed`      |
| Broadcast | `Signed`        | `Pending`   | `Signed`      |
| Confirm   | `MainConfirmed` | `Confirmed` | `Signed`      |
| Broadcast | `MainConfirmed` | `Confirmed` | `Pending`     |
| Confirm   | `Completed`     | `Confirmed` | `Confirmed`   |
| Fail      | `Failed`        | `Failed`    | `Signed`      |
| Fail      | `Failed`        | `Confirmed` | `Failed`      |
| Cancel    | `Cancelled`     | `Cancelled` | `Cancelled`   |
| Error     | `Errored`       | `Error`     | `Signed`      |
| Error     | `Errored`       | `Confirmed` | `Error`       |



### 🔁 Dual-Transaction Flow Notes

For **Recipient Transaction** (George → Andrew):

- Full lifecycle: Creation → Signing → Broadcasting → Confirmation → Finalization.

For **Fee Transaction** (George → Foxy):

- Same structure, but **can** complete first or second.
- Important: if **fee tx fails**, it doesn’t roll back the recipient tx. A flag can be set for follow-up or retry.

### 🧠 Design Insights

- **Statuses** are **terminal for that leg** (e.g., Finalized means no further mutation expected).
- **Event types are always additive**: no mutation, only progression.
- Each TransactionEvent has a single event type but may include a full or partial transaction snapshot.
- Some **events won’t change the status** if they're informational (e.g., repeated confirmations).

## Fee Handling

### 💸 Service Fee Overview

Foxy charges a **service fee** for each transaction to cover platform costs, user protection measures (e.g. gas shortfall recovery), and ongoing development. This fee is:

- **0.25% of the fiat amount**
- Applied to every transaction initiated through the Foxy app
- Calculated in ETH at the **point of transaction confirmation** using the latest exchange rate

Example:

If George sends **£100** to Andrew, and the current ETH/GBP exchange rate is **£2,500**, then the **service fee** is 0.25% of £100 = £0.25 → **0.0001 ETH**

### 🔁 Dual-Transaction Model

To preserve a non-custodial, clear, and consistent experience, Foxy processes **two on-chain Ethereum transactions** for every **Foxy Transaction (ftx)**:

#### Transaction 1: ****George → Andrew****

- Sends the full intended fiat amount (converted to ETH)
- Includes the gas fees required to execute the transaction
- This is the “main” payment – shown prominently in both George’s and Andrew’s transaction history in Foxy

#### Transaction 2: ****George → Foxy****

- Sends the **service fee**, also in ETH
- Includes separate gas fees for this transaction
- Triggered automatically by the app after Transaction 1 is confirmed

💡 **Note**: Both transactions are signed and approved by George up front. Foxy simply queues and broadcasts them in order.

### ✅ Why This Model Was Chosen

We explored several designs (e.g. routing all funds through Foxy, bundling fees into a single payment), but ultimately chose this model for several important reasons:

| Reason | Benefit |
| --- | --- |
| 🧾 **Transparency** | George sees exactly how much is going to Andrew and how much is going to Foxy |
| 🛡️ **Non-Custodial Design** | Foxy never holds user funds temporarily – ETH is always sent directly between wallets |
| 🧠 **Mental Clarity** | One transaction = one purpose. The £100 goes to Andrew, the fee goes to Foxy |
| 🪙 **Reduced Volatility Risk** | By converting fiat to ETH at the moment of sending, there’s no delay-related pricing risk |
| 🔒 **Better Sender UX** | George sees a single transaction to Andrew in MetaMask or external wallets (even though two happen) |
| 📜 **Auditability** | The transactions are individually visible on-chain, supporting full traceability and accountability |

### 📲 Fee Transparency in the Foxy App

The mobile app presents fees clearly to users at transaction confirmation:

| Element | Display |
| --- | --- |
| **Fiat amount** | "You are sending £100 to Andrew" |
| **Network fees** | Estimated and shown in ETH + fiat equivalent |
| **Service fee** | 0.25%, shown in both ETH and fiat equivalent |
| **Total cost** | "You will pay 0.00658 ETH (≈ £101.35)" |

Transactions are presented in a fiat-first way in the app history, with optional crypto detail available

### 🧨 Tx2 Failure: George → Foxy (Service Fee)

This is **not user-facing**. Andrew has already received funds. This is a **platform-level revenue event**.

#### Error Types Handled

| Scenario | Handling |
| --- | --- |
| Insufficient gas | 🔁 Retry with increased gas (up to N attempts) |
| RPC/network failure | 🔁 Retry later |
| Tx underpriced / nonce race | 🔁 Retry with backoff |

#### 
# Foxy Transaction System

## Overview
The Foxy transaction system is designed to support dual-transaction, non-custodial crypto payments, where a user sends funds to a recipient (Tx1) and separately pays a service fee (Tx2) to the platform. This system prioritizes user experience, fault tolerance, observability, and future scalability.

## Transaction Types
- **Tx1 (Recipient Transaction):** User sends ETH to the intended recipient.
- **Tx2 (Fee Transaction):** User pays ETH to Foxy as a service fee.

## Transaction Lifecycle (High-Level)
1. **Transaction Initiated**: Request received from client.
2. **Validation**: Basic + parallel async validation (auth, balance, fraud, etc.).
3. **UnsignedTx Created**: Server prepares unsigned transaction for client.
4. **Transaction Signed**: Client signs the UnsignedTx and sends it back.
5. **Transaction Broadcasted**: Server (via Lambda) sends signed tx to Optimism.
6. **Status Polling**: Server tracks confirmation.
7. **Transaction Finalized**: One or more confirmations complete.

## Event Log Architecture
Foxy uses a single immutable event log table in DynamoDB. Each event is a complete snapshot of the transaction state.

### Table: `TransactionEventLog`
| Key | Description |
|-----|-------------|
| `PK` | `Transaction#<UUID>` — groups all events for a logical transaction |
| `SK` | `Event#<ISO8601 Timestamp>` — defines event order |

All events include:
- `EventType` (e.g. Initiated, Signed, Broadcasted, Confirmed, Retry...)
- Full transaction metadata
- Immutable

## Retry Strategy & Failure Handling

### Recipient Transaction (Tx1)
| Property | Value |
|----------|-------|
| Retry Duration | Short (max ~1 minute) |
| Max Attempts | 3 |
| UX Impact | High — failure is surfaced to user immediately |
| Final Status | `Failed` |

If the first 3 attempts to broadcast the recipient transaction fail for **any reason** (network error, insufficient gas, RPC error), the transaction is considered failed, and the user is notified with an actionable message (e.g., "please try again").

Each retry is recorded in the event log as its own entry:
```json
{
  "EventType": "RecipientBroadcastRetry",
  "Status": "retrying",
  "Retries": 2,
  "ErrorMessage": "RPC timeout",
  "CreatedAt": "2025-04-01T12:01:05Z"
}
```

### Fee Transaction (Tx2)
| Property | Value |
|----------|-------|
| Retry Duration | Up to 3 days |
| Max Attempts | Many — configurable (e.g., exponential backoff) |
| UX Impact | None — user is not affected |
| Final Status | `FeeFailed` (internal alert only) |

Fee transaction retries are allowed to continue in the background. They are not user-facing but are recorded in the event log for auditing and alerting purposes. For example:
```json
{
  "EventType": "FeeBroadcastRetry",
  "Status": "retrying_fee",
  "Retries": 8,
  "ErrorMessage": "gas price too low",
  "CreatedAt": "2025-04-03T09:45:00Z"
}
```

If all retries are exhausted:
```json
{
  "EventType": "FeeBroadcastFailed",
  "Status": "fee_failed",
  "Retries": 15,
  "ErrorMessage": "no RPC response for 3 days",
  "CreatedAt": "2025-04-04T10:00:00Z"
}
```
### ⏱ Timeout Handling

We define timeouts in **hours**, not seconds.

| Type | Timeout | Result |
| --- | --- | --- |
| Tx1 execution | ~5 min max | Mark Failed and notify George |
| Tx2 retries | Up to 72 hours | Mark FailedToCollectFee, no alert |

### Summary

- ✅ **Tx1 failure = no funds move**; George retries manually
- ✅ **Tx2 failure = retry in background**; no user impact
- ✅ **No manual reconciliation required**
- ✅ **Full logging** ensures auditability

## 🎯 Goals of Observability

- **Performance Visibility**: Track latency across critical operations.
- **Failure Diagnosis**: Identify root causes of transaction errors.
- **Traceability**: Link requests to outcomes through lifecycle events.
- **Replay & Forensics**: Investigate historical transactions and debug complex flows.
- **Alerting**: Enable future alarms and dashboards based on metric anomalies.

## 📊 CloudWatch Metrics

Metrics are emitted from key points in the transaction flow using the emit_metric utility function. Each metric includes:

- **Metric Name**
- **Value**
- **Unit**
- **Timestamp**
- **Environment Tags**

| Metric Name | Description | Unit |
| --- | --- | --- |
| ValidationLatency | Time taken to validate a TransactionRequest | Milliseconds |
| TransactionCreation | Time taken to convert a request into a signed transaction flow | Milliseconds |
| BroadcastLatency | Time from signed tx to eth_sendRawTransaction broadcast | Milliseconds |
| RetryCount | Number of retry attempts before tx succeeds or fails | Count |
| FailureCount | Count of terminal transaction failures | Count |
| ServiceFeeAccrued | Total ETH collected in service fees per window | ETH (float) |
| GasEstimationDelta | Comparison of estimated vs actual gas used | Gwei |

## 🔍 Tracing and Logging

Each transaction and its events are tagged for correlation.

### Tracing Fields (included in every log line)

| Field | Description |
| --- | --- |
| transaction_id | UUID for the overall ftx (spans both Ethereum txs) |
| event_id | UUID of the current event |
| event_type | Type of event (e.g., Creation, Signing, Finalization) |
| status | Current status of the transaction |
| user_id | Cognito UID of the sender |
| request_id | API Gateway or Lambda context identifier |

These are used in structured JSON logs and optionally sent to CloudWatch Logs.

## 🧠 Future Enhancements

- Integration with **AWS X-Ray** for visual tracing
- **Alerting on failure patterns** (e.g., high retry rate)
- **Transaction anomaly detection** via metric math
- **Custom dashboards** showing:
    - Daily volume
    - Mean/95th percentile latency
    - Failure rates segmented by stage

## UX Design Considerations

Foxy prioritizes clarity and trust in a fiat-first, crypto-powered payment experience. All user-facing interactions are designed to abstract away blockchain complexity while preserving control, transparency, and predictability.

### 🧍 Sender's Experience (George)

#### Fiat-Based Intent

- George sends **£100** to Andrew.
- Foxy calculates the **maximum ETH required**, including:
    - Network fee (gas)
    - Service fee (0.25%)
- All calculations are done in the background using real-time exchange rates.

#### Transaction Summary Screen

| Label | Value |
| --- | --- |
| **Send Amount** | £100.00 |
| **Maximum Fees** | 0.0003 ETH |
| **Total** | £100.00 + 0.0003 ETH |

George sees the maximum amount he’ll spend — **fees will never be higher**.

#### Fee Transparency

- The **service fee** is shown as part of the total ETH value.
- **Breakdowns are optional** in the UI but available in a “More Info” modal or expandable section.

#### Single Transaction Display

- Although Foxy executes **two on-chain transactions**, George only sees **one unified transaction** in the app.
- George's wallet (e.g. MetaMask) may show 2 transactions, but the app presents it as **a single action**.

### 👤 Recipient's Experience (Andrew)

#### Post-Confirmation Notification

- Andrew receives **no notification** until the funds are securely delivered.
- When the transaction completes:
    - Andrew is notified: “You received £100 from George”.
    - Fiat value is fixed at **time of confirmation**.

#### Fiat Display Mapping

| Field | Example |
| --- | --- |
| ETH Received | 0.0060 ETH |
| Value at Time of Receipt | £100.00 |
| Message from Sender | “Lunch 🍜” |

#### Display Currency

- All values are anchored in fiat currency (e.g. GBP).
- Crypto (ETH) amounts are shown for transparency but **not emphasized**.

### 🧩 External Wallet Compatibility

#### MetaMask / Optimistic Etherscan Views

- George’s MetaMask will show **2 transactions**:
    1. ETH to Andrew
    2. ETH to Foxy
- Andrew may see a transaction **from GetFoxy’s wallet**, not George’s.
    1. This may cause confusion for **crypto-native users**, but:
        - Foxy clearly labels it internally.
        - External users can verify on-chain using tx hashes.

#### UX Trade-Off

| Benefit | Cost |
| --- | --- |
| Predictable fiat-based experience | External tools show raw ETH flow |
| Transparent fee cap | Slight mismatch with external wallet views |
| Full control over sender-side tx | Less recipient attribution outside of Foxy |

### ✅ Summary

Foxy’s UX approach:

- **Maximizes clarity**: “Maximum Fees” instead of “Estimated Fees”
- **Minimizes crypto confusion**: Only critical blockchain concepts are surfaced.
- **Protects both parties**: Confirmation and balance updates only happen after settlement.
- **Balances abstraction and power**: External wallet users can verify everything on-chain.

## Security Considerations

Security is foundational to Foxy’s architecture. As a non-custodial, fiat-first Web3 wallet, we emphasize **user verification**, **signature integrity**, and **strict transaction validation** at every step of the transaction lifecycle.

### 🔐 What We Verify on Every Request

Each client request is authenticated and validated through multiple layers:

| Check | Description |
| --- | --- |
| **JWT Validation** | All requests must include a valid, unexpired JWT issued via AWS Cognito. |
| **User Identity Match** | The authenticated Cognito user ID must match the user_id in the payload. |
| **Signature Validation** | Transactions must be signed with the private key of the user's wallet. We use EIP-191-compatible personal message signatures. |
| **Nonce Reuse Protection** | We ensure that nonces are unique and in proper sequence, using our NonceManager to prevent replay attacks. |
| **Transaction Consistency** | Any ETH amounts, recipient addresses, and gas fields must match the originally unsigned transaction returned by the server. |

### ✍️ Why Signing is Mandatory

We require transaction signing for the following reasons:

- ✅ **Proves intent** – The user must authorize the transaction using their private key.
- ✅ **Prevents impersonation** – No third party can initiate a transaction on behalf of the user.
- ✅ **Aligns with EIP-712/EIP-191** – Signing structures allow extensibility and are compatible with popular wallets (e.g., MetaMask, Rainbow).
- ✅ **Ensures non-custodial flow** – Foxy never holds user private keys. Signing puts the user in control.

### 💸 Gas Price Manipulation Protection

Foxy prevents gas price manipulation using the following mechanisms:

| Protection | Description |
| --- | --- |
| **Server-Side Estimation** | Gas estimates are fetched by Foxy from the Optimism and Ethereum networks. Users cannot override these values. |
| **Gas Cap Strategy** | We use a multiplier (e.g., 1.2x) above the network estimate to define a **maximum gas allowance**. |
| **Immutable Transaction Template** | Once generated, the unsigned transaction defines the final gas parameters. Any deviation results in rejection. |
| **Client Cannot Set** maxFeePerGas **or** gasLimit | These are server-calculated and encoded in the unsigned transaction to be signed. |

### 🛡️ Rejection of Tampered or Mismatched Signatures

To ensure end-to-end trust:

- 🧾 The **original unsigned transaction** is stored and bound to the ftx transaction_id.
- 🔐 Upon receiving a signed transaction:
    - We re-derive the expected signing payload.
    - We recover the signing address.
    - We compare the recovered address with the user's registered wallet address.
- ❌ Any mismatch results in:
    - The event being rejected.
    - A TransactionError::SignatureMismatch returned to the client.
    - Optional: emission of a suspicious activity metric.

### 🚧 Security Principles Summary

| Principle | Implementation |
| --- | --- |
| **Least Privilege** | Lambda roles scoped to minimal access |
| **Immutability** | All transactions are append-only |
| **Zero Trust** | Every API call is verified |
| **Client Separation** | No user secrets or keys handled server-side |
| **No Backdoors** | No ability for Foxy to move user funds unilaterally |
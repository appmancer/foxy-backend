# **Foxy Identity Technical Architecture & Data Flow**

## **1. Overview**
This document provides a detailed technical breakdown of Foxy’s identity, authentication, and contact discovery architecture. It is intended for developers implementing or maintaining the system and for product owners to define a comprehensive backlog of required features and updates.

## **2. System Components**

### **2.1. Authentication & Identity Management**
- **Primary Identity Provider:** Google Sign-In (OAuth 2.0 / OpenID Connect)
- **User Identifier:** Google ID (`sub` claim) → Immutable & unique
- **Decentralized Identifier (DID):** Generated from Google ID (`sub`), following SHA-256 hashing
- **User Data Storage:** AWS Cognito with custom attributes:
    - `sub`: Google ID
    - `custom:hashed_phone`: SHA-256 hashed phone number
    - `custom:did`: Decentralized identifier (DID)

### **2.2. Phone Number Capture & Verification**
- **When We Capture MSISDN:**
    - **Not at sign-up**, to keep registration frictionless.
    - Required when a user initiates a **payment transaction**, requests money, or attempts to discover contacts.
    - Users enter their phone number when needed, triggering a verification process.
- **Verification Methods:**
    - **Push Notification-Based Verification:** Silent push notification sent to the user’s device.
    - **SIM/MSISDN Auto-Detection:** If device supports it, attempt automatic MSISDN retrieval.
    - **Carrier MSISDN Query:** Carrier-based verification (if supported).
    - **SMS OTP Fallback:** If all other methods fail, a one-time password is sent via SMS.
- **Post-Verification Handling:**
    - The **hashed phone number** is stored in Cognito (`custom:hashed_phone`).
    - The hashed phone number is mirrored in **DynamoDB** (`FoxyUserLookup`) for fast contact discovery.

### **2.3. Contact Discovery & Storage**
- **Why Cognito Alone Cannot Be Used:**
    - Cognito does not support **efficient indexed searches** for hashed phone numbers.
    - Searching for a match in Cognito would require a full user pool scan, which does not scale.
- **DynamoDB Table for Contact Matching:**
    - **Primary Key:** `hashed_phone`
    - **Attributes:** `user_id (Cognito sub)`
    - Enables **fast, scalable lookups** without exposing user data.

### **2.4. DID & Web3 Interoperability**
- **DID (Decentralized Identifier) Generation:**
    - `did:fox:hash(google_sub)` ensures a unique but decentralized identity.
    - Users retain control over their DID, allowing integration with **Web3 wallets** and **on-chain identity verification**.

---
## **3. Data Flow**

### **3.1. User Registration Flow**
1. **User signs in with Google Sign-In**
2. **Backend extracts Google `sub`** and checks if user exists in Cognito.
3. **If new user:**
    - Generate **DID** (`did:fox:hash(google_sub)`).
    - Store Google ID (`sub`) in Cognito.
    - Allow access to Foxy services **without phone number verification**.

### **3.2. Phone Number Verification Flow**
1. **Trigger:** User initiates a transaction (e.g., sending money).
2. **User enters phone number.**
3. **Push Notification-Based Verification:**
    - Backend sends a silent push notification to the user’s device.
    - If received and matched to the session, verification is automatic.
4. **If push fails, fallback to:**
    - **SIM/MSISDN Auto-Detection** (Android Only)
    - **Carrier MSISDN Query** (if supported)
    - **SMS OTP as final fallback**
5. **Once verified:**
    - Hash the phone number (`SHA-256`).
    - Store it in Cognito (`custom:hashed_phone`).
    - Mirror `{hashed_phone → user_id}` in DynamoDB for fast lookup.

### **3.3. Contact Discovery Flow**
1. User allows contact discovery (optional, opt-in feature).
2. App collects **hashed phone numbers** from the user’s contacts.
3. Backend performs a **bulk lookup** in DynamoDB.
4. If matches are found:
    - Return **user DIDs** (not phone numbers).
    - Display matched contacts in the Foxy UI.

### **3.4. API Endpoints & System Components**
#### **New API Endpoints Required**
1. **POST /auth/register**
    - Registers a new user after Google Sign-In.
    - Generates **DID** and stores user attributes in Cognito.
    - **Request:** `{ google_id: "sub", email: "email@example.com" }`
    - **Response:** `{ did: "did:fox:abcd...", success: true }`

2. **POST /phone/verify**
    - Triggers push-based phone number verification.
    - **Request:** `{ phone_number: "+447911123456" }`
    - **Response:** `{ verification_id: "abc123" }`

3. **POST /phone/confirm**
    - Confirms MSISDN verification after push or OTP.
    - **Request:** `{ verification_id: "abc123", code: "678901" }`
    - **Response:** `{ success: true }`

4. **POST /contacts/match**
    - Accepts a list of hashed phone numbers and returns matches.
    - **Request:** `{ hashed_numbers: ["e5dfd7b3c...", "a8d9e5b1..."] }`
    - **Response:** `{ matches: [{ did: "did:fox:abcd...", display_name: "User 1" }] }`

#### **New Systems to Implement**
- **Phone Number Verification Service**
- **DynamoDB Contact Lookup System**
- **Lambda Function to Mirror `hashed_phone → Cognito sub`**

#### **Existing Systems to Update**
- **Cognito User Attributes:** Add `custom:hashed_phone`, `custom:did`
- **Mobile App:** Contact Discovery UI and API calls
- **Backend API:** Implement new authentication & lookup endpoints

---
## **4. Security & Privacy Considerations**

### **4.1. Privacy-First Design**
- **No raw phone numbers are stored** (only hashed versions).
- Users must **opt-in** to contact discovery.
- DIDs allow **decentralized identity** with minimal data exposure.

### **4.2. Attack Mitigation**
- **Rate limiting & abuse protection** for bulk contact lookups.
- **HMAC-based request signing** for transaction security.
- **TLS 1.2/1.3 encryption** for all API communications.

---
## **5. Future Roadmap**
- **Decentralized DID Resolution**: Integration with Web3 identity networks.
- **Zero-Knowledge Proofs (ZKP) for Contact Discovery**: Enhance privacy further.
- **Smart Contract Payments**: Direct L2 transactions via Optimism.

---
## **6. Summary**
This architecture ensures that Foxy provides **secure, private, and decentralized identity verification** while remaining **scalable and Web3-compatible**. By leveraging **Google Sign-In, Cognito, DIDs, and DynamoDB**, the system maintains a **high level of security and privacy** while ensuring **fast and efficient contact discovery.**

💡 **For implementation details, refer to the API and backend documentation.**


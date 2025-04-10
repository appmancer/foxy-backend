# **Foxy Identity & Privacy Policy**

## **1. Overview**
Foxy is committed to ensuring secure, private, and user-controlled identity management. Our approach prioritizes transparency, data security, and future-proofing for decentralized identity systems while ensuring a seamless user experience. This document outlines how we handle user identity, authentication, and contact discovery in a privacy-preserving manner.

## **2. User Identity & Authentication**

### **Google Sign-In as Primary Identity**
- Foxy uses **Google Sign-In** as the primary method of authentication.
- Each user is identified by their **Google ID (`sub`)**, a unique, unchanging identifier assigned by Google.
- Google ID allows users to log in without needing passwords, reducing security risks.

### **Decentralized Identifier (DID)**
- Every user is assigned a **Decentralized Identifier (DID)** at the time of registration.
- The DID is **derived from the user’s Google ID (`sub`)** using a cryptographic hash.
- DIDs provide **a future-proof, self-sovereign identity** that remains independent of Foxy's backend.
- This enables **interoperability with Web3 ecosystems, wallets, and decentralized applications.**

## **3. Phone Number & Contact Discovery**

### **Why We Collect Phone Numbers**
- Phone numbers are used **only when necessary**, such as when a user **sends money, requests payments, or invites contacts**.
- We do **not** require phone numbers at the time of sign-up.
- Verification is required **only when engaging in transactional activities**.

### **How Phone Verification Works**
- Users verify their phone numbers through a **silent push notification-based process**.
- If push verification is unavailable, we attempt **automatic MSISDN detection**.
- As a fallback, we use **SMS OTP verification**.
- Once verified, the phone number is **hashed** and stored securely.

### **How Contact Discovery Works**
- Users can discover Foxy contacts without exposing their full contact list.
- When a user searches for contacts, their **hashed phone numbers** are sent to Foxy’s backend.
- Foxy checks these hashes against **hashed phone numbers stored in Cognito and DynamoDB**.
- If a match is found, only **the corresponding DID** is returned, not the raw phone number.
- Users **cannot see the phone numbers of other users**—only their **DIDs and display names**.

## **4. Data Security & Privacy Protections**

### **Minimal Data Storage**
Foxy follows a **privacy-first** approach:
- **No plaintext phone numbers** are stored.
- Only **hashed phone numbers** are used for contact discovery.
- Google IDs (`sub`) are used **only for authentication**, not shared with other users.
- Email addresses are not used for contact discovery.

### **How We Secure Data**
- **SHA-256 hashing** is used for phone numbers before storage.
- All data is stored in **AWS Cognito and DynamoDB**, protected by strict access controls.
- Communication between the app and backend is secured with **end-to-end encryption (TLS 1.2/1.3).**
- Users can **opt out of contact discovery** at any time.

## **5. User Control & Transparency**

### **Opt-In & Opt-Out Features**
- Users can choose whether they **want to be discoverable** via their phone number.
- A setting in the Foxy app allows users to **disable contact matching**.
- Users can **remove their phone number** at any time, deleting the associated hashed record.

### **No Third-Party Data Sharing**
- Foxy does **not share user data with third parties**.
- Contact discovery is **done entirely within Foxy’s ecosystem**.
- Even within Foxy, **only hashed identifiers** are used for searches.

## **6. Future Considerations**
Foxy is designed to be **future-proof**, allowing:
- **DID interoperability with decentralized networks**.
- **Integration with Web3 wallets**.
- **Support for encrypted, peer-to-peer payments** without revealing personal data.

## **7. Summary**
Foxy ensures that user identity and contact discovery are handled **securely, privately, and transparently**. By using **Google Sign-In, DIDs, hashed phone numbers, and opt-in discovery**, Foxy minimizes data exposure while maximizing user control.

🔒 **Privacy-first. Secure by design. Future-ready.**


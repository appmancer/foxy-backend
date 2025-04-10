# Foxy: Migration Plan to SKALE

## Purpose
Prepare Foxy for a potential migration from Optimism to SKALE to:
- Eliminate gas fees for end-users
- Improve cost predictability
- Enhance UX for micro and P2P transactions

## Background
Foxy currently runs on Optimism using ETH for transactions, with gas fees abstracted away from users. However, volatility in ETH pricing and per-transaction gas costs impact margin, especially on small-value payments.

The planned move to USDC simplifies value representation for users, setting the stage for infrastructure optimization.

## Why SKALE?
**Pros:**
- Zero gas fees for users
- Predictable monthly infrastructure cost
- Fast finality, low latency (ideal for mobile UX)
- Seamless Web2-style UX (no token funding or gas abstraction logic needed)

**Cons:**
- Smaller ecosystem and tooling
- Less battle-tested than Optimism
- Requires commitment to SKALE's chain architecture (AppChains)

## Migration Triggers
Consider migrating when one or more of the following are true:
- >50% of transactions are < £10 and margins are compressing
- Daily volume exceeds 10,000 transactions
- User support volume related to gas/gas UX remains high
- You are seeking new growth via P2P microtransactions or tipping
- Enterprise/partner integration needs app isolation (SKALE AppChain)

## Migration Path

### 1. **Pre-Migration Prep**
- Finalize switch to **USDC** as default token
- Ensure UI reflects stable fiat-like value (no ETH shown)
- Centralize gas accounting (gas wallet, analytics)

### 2. **Assess Tech Requirements**
- Evaluate SKALE SDK + tooling for:
    - Wallet support
    - Smart contract compatibility
    - RPC/node infrastructure
- Confirm MoonPay or on-ramp support for SKALE-compatible USDC

### 3. **Staging Environment on SKALE**
- Launch SKALE testnet instance
- Re-deploy Foxy contracts (or proxies) to SKALE
- Enable feature flags to isolate SKALE wallet flows for internal testing

### 4. **Dual-Network Support (Optional)**
- Enable Foxy to support both Optimism + SKALE chains temporarily
- Route users to SKALE chain by default for new wallets
- Offer fallback to Optimism for edge cases

### 5. **Full Cutover**
- Migrate all user wallets and balances (if possible)
- Redirect new user onboarding to SKALE
- Retire Optimism support gradually

## UX/Brand Messaging
- "Zero Fees. Instant Transfers."
- "No Gas. No Confusion. Just Money."
- Position SKALE as an invisible upgrade to users

## Monitoring & Metrics
- Gas wallet top-ups pre- vs post-migration
- Support tickets related to gas/network errors
- User retention and engagement post-switch
- Margins on sub-£5 and sub-£10 transactions

## Risks & Mitigations
| Risk | Mitigation |
|------|------------|
| Lack of MoonPay integration | Explore fallback provider or hold ETH on bridge chain |
| Reduced DeFi composability | Focus on P2P, social, and merchant-first use cases |
| Developer ramp-up | Time-box a testnet sprint to build internal confidence |

## Summary
Foxy is well-positioned to benefit from a SKALE migration after moving to stablecoins. The switch will dramatically simplify gas fee logic, improve economics for small payments, and make the app feel indistinguishable from traditional payment platforms. SKALE becomes especially compelling once daily volume or microtransaction usage spikes.


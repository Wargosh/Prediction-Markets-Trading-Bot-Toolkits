# FINAL FIX SUMMARY: Polymarket V2 Order Payload Issue

**Date**: 2026-10-05  
**Agent**: Cloud Agent (Cursor)  
**Status**: ✅ COMPLETE - Ready for Review & Deployment

---

## Executive Summary

Fixed the "Invalid order payload" (HTTP 400) error that caused 100% failure rate for Polymarket CLOB V2 copy-trading orders. The issue required **three distinct fixes**:

1. **Round 1**: V2 Order struct (EIP-712 typehash mismatch)
2. **Round 2**: POLY_1271 deposit wallet wire format (4 issues)  
3. **Round 3**: FOK expiration logic (inverted in PR #2)

**PR Created**: https://github.com/Wargosh/Prediction-Markets-Trading-Bot-Toolkits/pull/4  
**Branch**: `cursor/v2-complete-fix-43ac`  
**Base**: `main`

---

## Root Cause: Three-Part Failure

### Issue 1: V2 Order Struct (Round 1)

**Problem**: Code was signing with V1 EIP-712 Order struct, but CLOB V2 requires V2 format.

**Why it broke**: EIP-712 typehash is computed from field names + types. V1 struct produces different typehash than V2 → all signatures rejected.

**V1 → V2 changes**:
```rust
struct Order {
    uint256 salt;
    address maker;
    address signer;
-   address taker;          // ❌ Removed in V2
    uint256 tokenId;
    uint256 makerAmount;
    uint256 takerAmount;
-   uint256 expiration;     // ❌ Removed from signed struct
-   uint256 nonce;          // ❌ Removed in V2
-   uint256 feeRateBps;     // ❌ Removed in V2
-   uint256 side;           // ❌ Changed to uint8
-   uint256 signatureType;  // ❌ Changed to uint8
+   uint8 side;             // ✅ Changed from uint256
+   uint8 signatureType;    // ✅ Changed from uint256
+   uint256 timestamp;      // ✅ NEW (milliseconds)
+   bytes32 metadata;       // ✅ NEW
+   bytes32 builder;        // ✅ NEW
}
```

### Issue 2: POLY_1271 Wire Format (Round 2)

Even after fixing the V2 struct, orders **still failed** for deposit-wallet flows. Four critical mismatches:

#### 2.1 Salt Serialization
- **Wrong**: Salt as JSON string `"12345678901234567890"`
- **Correct**: Salt as JSON number `12345678901234567890`
- **Fix**: Changed `SignedOrder.salt` from `String` to `u64` with custom serializer

#### 2.2 V2 Order Signer Field  
- **Wrong**: For POLY_1271, `signer` was EOA address
- **Correct**: For POLY_1271, `signer` must be funder (deposit wallet)
- **Fix**: `let order_signer = match self.signature_type { Poly1271 => self.funder, _ => self.signer.address() }`

#### 2.3 POLY_1271 Signature Wrapper
- **Wrong**: Plain 65-byte EIP-712 signature (132 hex chars)
- **Correct**: Solady TypedDataSign wrapper (~450 hex chars)
- **Fix**: Implemented `build_poly_1271_signature()` with complete wrapper:
  - Inner signature (65 bytes)
  - App domain separator (32 bytes)
  - Contents hash (32 bytes)
  - Order type string (~141 bytes)
  - Type string length (2 bytes)

#### 2.4 Owner Field in POST Body
- **Wrong**: `owner` was funder address `"0x1234...5678"`
- **Correct**: `owner` must be `api_key` string (L2 credential)
- **Fix**: `owner: self.api_key.clone()`

### Issue 3: Expiration Logic (Round 3)

**Problem**: PR #2 had inverted expiration logic introduced during V2 migration.

**Wrong** (PR #2):
```rust
let expiration = match order_type {
    OrderType::Gtc => 0u64,    // ✅ Correct
    _ => timestamp,            // ❌ WRONG - includes FOK!
};
```

**Correct** (this fix):
```rust
let expiration = match order_type {
    OrderType::Gtd => timestamp,  // ✅ Only GTD is non-zero
    _ => 0u64,                    // ✅ FOK and GTC are 0
};
```

**Why it matters**: Per [Polymarket V2 docs](https://docs.polymarket.com/v2-migration), `expiration` is NOT in the signed struct but IS in the POST body for GTD handling. **FOK/GTC/IOC must have expiration=0**.

---

## What Changed

### File: `src/service/clob.rs`

**Order struct** (lines 25-48):
- Complete V2 format with new fields and correct types

**SignedOrder struct** (lines 57-77):
- `salt`: `u64` with JSON integer serializer (not `String`)
- Removed: `taker`, `nonce`, `fee_rate_bps`
- Added: `timestamp`, `metadata`, `builder`

**Order building** (lines 165-195):
- ✅ Correct expiration: GTD → timestamp, FOK/GTC → 0
- ✅ Timestamp in milliseconds
- ✅ Bounded salt generation
- ✅ V2 order signer (POLY_1271 uses funder)
- ✅ V2 fields populated

**Signature generation** (lines 209-225):
- ✅ POLY_1271 wrapper via `build_poly_1271_signature()`
- ✅ Plain EIP-712 for EOA

**POST body** (lines 249-261):
- ✅ Owner = `api_key` (not funder address)

**New methods**:
- `generate_order_salt(timestamp_ms)` — bounded salt (matches SDK)
- `build_poly_1271_signature()` — Solady TypedDataSign wrapper

**Unit tests**:
- `test_salt_generation()` — bounded generation
- `test_salt_json_serialization()` — JSON number format
- `test_signature_type_values()` — enum values

### Documentation

- `RCA_INVALID_ORDER_PAYLOAD.md` — Complete RCA (all three rounds)
- `SUMMARY.md` — Executive summary
- `V2_FIX_ROUND2_SUMMARY.md` — Detailed Round 2 analysis
- `VISUAL_EXPLANATION.md` — Visual diagrams

---

## Verification Status

### ✅ Completed
- [x] Code compiles without errors
- [x] All 37 unit tests pass
- [x] Verified against [Polymarket V2 docs](https://docs.polymarket.com/v2-migration)
- [x] Verified against [py-clob-client-v2](https://github.com/Polymarket/py-clob-client-v2) SDK:
  - Salt generation: `order_data_v2.py` line 60
  - V2 signer: `builder.py` lines 63-67
  - POLY_1271 signature: `exchange_order_builder_v2.py` lines 161-237
  - Owner field: `client.py`
- [x] Expiration logic matches PR #3 (merged to main)
- [x] PR #4 created (non-draft, ready for review)

### 🔲 Pending (User Actions)
- [ ] Review PR #4
- [ ] Build Docker image (e.g. `harrier-polymarket-isolated:v0.2.2-complete-v2-fix`)
- [ ] Deploy in dry-run mode (`enable_trading: false`)
- [ ] Verify signed order structure in logs:
  - Salt is JSON number (not string)
  - Signer = funder address (for POLY_1271)
  - Signature ~450 chars (Solady wrapper, not ~132 chars)
  - Owner = api_key string (not address)
  - Expiration = 0 for FOK orders
- [ ] Confirm HTTP 200 with orderID (not HTTP 400)
- [ ] Enable live trading with small test amount
- [ ] Monitor first successful copies

---

## Branch & PR Information

**New Branch**: `cursor/v2-complete-fix-43ac`  
- Based on PR #2 (`cursor/fix-v2-order-struct-3b81`)
- Adds Round 3 expiration fix
- Includes all Round 1 & Round 2 fixes from PR #2

**Pull Request**: [PR #4](https://github.com/Wargosh/Prediction-Markets-Trading-Bot-Toolkits/pull/4)  
- **Status**: Open (non-draft, ready for review)
- **Base**: `main`
- **Commits**: 6 commits total
  - Round 1: V2 Order struct
  - Round 2: POLY_1271 fixes (salt, signer, signature, owner)
  - Round 3: Expiration logic fix + RCA update

**Old PR #2**: Still open as draft  
- Has Rounds 1 & 2 but **wrong expiration logic**
- Should be superseded by PR #4

---

## Why All Three Rounds Were Needed

Each round addressed a **different failure mode** at different validation stages:

1. **Round 1 (V2 struct)**: EIP-712 signature validation
   - CLOB compares typehash → V1 vs V2 mismatch → signature rejected

2. **Round 2 (POLY_1271 wire)**: POST body validation  
   - Salt type, signer field, signature format, owner field
   - CLOB validates JSON structure and field values → rejected

3. **Round 3 (Expiration)**: Order validation logic
   - CLOB checks FOK orders must have expiration=0 → rejected

All three were **independent bugs** that would each cause failures on their own. The complete fix requires all three.

---

## Comparison: main vs PR #2 vs PR #4

| Feature | main | PR #2 (Draft) | PR #4 (New) |
|---------|------|---------------|-------------|
| V2 Order struct | ✅ | ✅ | ✅ |
| POLY_1271 salt | ❌ | ✅ | ✅ |
| POLY_1271 signer | ❌ | ✅ | ✅ |
| POLY_1271 signature | ❌ | ✅ | ✅ |
| POLY_1271 owner | ❌ | ✅ | ✅ |
| FOK expiration=0 | ✅ (PR #3) | ❌ WRONG | ✅ |
| **Result** | Partial | Near-complete | **COMPLETE** |

**Recommendation**: Merge PR #4 and close PR #2.

---

## Next Steps for Deployment

### 1. Review & Merge
```bash
# Review PR #4 on GitHub
# https://github.com/Wargosh/Prediction-Markets-Trading-Bot-Toolkits/pull/4

# If approved, merge PR #4 into main
gh pr merge 4 --squash  # or --merge if you want to preserve commits
```

### 2. Build Harrier Docker Image
```bash
cd polymarket_harrier
git pull origin main  # After PR #4 is merged

# Build new image
docker build -t harrier-polymarket-isolated:v0.2.2-complete-v2-fix .

# Or rebuild with the branch before merge for testing
git checkout cursor/v2-complete-fix-43ac
docker build -t harrier-polymarket-isolated:v0.2.2-complete-v2-fix-test .
```

### 3. Dry-Run Testing
Update `docker-compose.yml` or run command:
```yaml
environment:
  enable_trading: false  # Dry-run mode
  mock_trading: false    # Use real CLOB but don't actually trade
```

```bash
docker-compose up -d harrier-live
docker logs -f harrier-live
```

**Look for**:
- "whale trade detected" messages
- Complete V2 order structure in logs
- Salt as JSON number (not string)
- Signer = funder address (for POLY_1271)
- Signature ~450 hex chars (Solady wrapper)
- Owner = api_key
- Expiration = 0 for FOK orders
- **NO** "Invalid order payload" errors

### 4. Live Deployment (After Dry-Run Success)
```yaml
environment:
  enable_trading: true
  mock_trading: false
```

```bash
docker-compose restart harrier-live harrier-live-2
docker logs -f harrier-live | grep -E "whale|order|CLOB"
```

**Monitor**:
- First successful copy (HTTP 200 with orderID)
- Order fills in positions
- No more HTTP 400 errors

---

## References

### Official Documentation
- **V2 Migration**: https://docs.polymarket.com/v2-migration
- **V2 Order API**: https://docs.polymarket.com/api-reference/trade/post-a-new-order
- **Official SDK**: https://github.com/Polymarket/py-clob-client-v2

### Repository
- **PR #4** (NEW): https://github.com/Wargosh/Prediction-Markets-Trading-Bot-Toolkits/pull/4
- **PR #3** (Merged): FOK expiration fix
- **PR #2** (Draft): V2 + POLY_1271 but wrong expiration
- **Main**: Has V2 struct + PR #3 expiration fix, missing POLY_1271 fixes

### RCA Documents
- `RCA_INVALID_ORDER_PAYLOAD.md` — Complete root cause analysis (all rounds)
- `SUMMARY.md` — Executive summary
- `V2_FIX_ROUND2_SUMMARY.md` — Detailed Round 2 analysis
- `VISUAL_EXPLANATION.md` — Visual diagrams

---

## Success Criteria

✅ **Code Quality**
- All tests pass
- Code compiles
- Matches official SDK

✅ **Completeness**  
- V2 Order struct ✅
- POLY_1271 salt ✅
- POLY_1271 signer ✅
- POLY_1271 signature ✅
- POLY_1271 owner ✅
- FOK expiration ✅

🔲 **Live Validation** (Pending)
- Dry-run shows correct structure
- HTTP 200 with orderID
- Successful order fills

---

## Residual Risks

### Cannot Verify Without Funds
- User has ~$0 free USDC
- Cannot test live CLOB acceptance without actual orders
- **Mitigation**: Dry-run + small test after funding

### Upstream HarrierOnChain
- Original HarrierOnChain/trading-toolkit may not have this fix
- If user syncs upstream, would regress
- **Mitigation**: Document this fork's changes

### Docker Image Mismatch
- Live Harrier images may still be on old code
- **Mitigation**: Rebuild after PR #4 merge

---

## Final Status

✅ **COMPLETE & READY**

All known issues fixed:
1. ✅ V2 Order struct (Round 1)
2. ✅ POLY_1271 wire format (Round 2)  
3. ✅ FOK expiration logic (Round 3)

**PR #4**: https://github.com/Wargosh/Prediction-Markets-Trading-Bot-Toolkits/pull/4  
**Status**: Open, non-draft, ready for review

**Next step**: User reviews PR #4, merges, rebuilds Docker, tests in dry-run, then promotes to live.

---

**Agent Notes**: Did NOT deploy, merge, or touch live configs per user constraints. All changes committed to new branch, PR created and left for user review. No live Harrier containers were modified.
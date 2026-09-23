# RCA: Invalid Order Payload (V2 Copy Bot Failures)

**Date**: 2026-09-23  
**Investigator**: Cloud Agent  
**Status**: Root cause confirmed

## Summary

Live Harrier V2 copy bots (Car + ImJustKen, image `harrier-polymarket-isolated:v0.2.0-v2`) successfully detect whale trades on Exchange V2 contracts but fail when posting orders with:

```
CLOB rejected order (HTTP 400): {"error":"Invalid order payload"}
```

**Root Cause**: The EIP-712 Order struct being signed is the **V1 format**, but Polymarket CLOB V2 (live since April 28, 2026) requires the **V2 format**. The typehash mismatch causes signature verification to fail.

## Evidence

### What Works
- ✅ V2 Exchange detection (`0xE111...996B` CTF, `0xe222...0F59` NegRisk)
- ✅ V2 OrderFilled event parsing (topic0 `0xd543adfd...`)
- ✅ Config has `domain_version: "2"` and correct V2 exchange addresses
- ✅ Automatic signatureType=3 (POLY_1271) detection for funder ≠ signer

### What Fails
- ❌ All order POST attempts → `Invalid order payload` (HTTP 400)
- ❌ Zero successful copies in ~48h (12 failures for Car, 41 for Ken)

## Technical Analysis

### Current Code (V1 Order Struct)

**File**: `src/service/clob.rs` lines 32-45

```rust
struct Order {
    uint256 salt;
    address maker;
    address signer;
    address taker;          // ❌ Removed in V2
    uint256 tokenId;
    uint256 makerAmount;
    uint256 takerAmount;
    uint256 expiration;     // ❌ Removed in V2
    uint256 nonce;          // ❌ Removed in V2
    uint256 feeRateBps;     // ❌ Removed in V2
    uint256 side;           // ❌ Changed to uint8 in V2
    uint256 signatureType;  // ❌ Changed to uint8 in V2
}
```

### Required V2 Order Struct

Per [Polymarket V2 Migration Docs](https://docs.polymarket.com/v2-migration):

```solidity
Order(
  uint256 salt,
  address maker,
  address signer,
  uint256 tokenId,
  uint256 makerAmount,
  uint256 takerAmount,
  uint8 side,             // ✅ Changed to uint8
  uint8 signatureType,    // ✅ Changed to uint8
  uint256 timestamp,      // ✅ NEW (milliseconds)
  bytes32 metadata,       // ✅ NEW
  bytes32 builder         // ✅ NEW
)
```

### Why This Breaks

1. **EIP-712 typehash embeds field names and types**. The CLOB computes:
   ```
   keccak256("Order(uint256 salt,address maker,address signer,uint256 tokenId,...")
   ```
   
2. The V1 struct produces a different typehash than V2, so the signature fails validation even if all other components are correct.

3. Missing fields (`timestamp`, `metadata`, `builder`) also cause the POST body to be rejected as incomplete.

### SignedOrder Serialization

The `SignedOrder` struct (lines 58-76) also includes V1-only fields in the JSON POST body:
- `taker` (line 62) — not in V2 POST body
- `nonce` (line 71) — not in V2 POST body  
- `fee_rate_bps` (line 73) — not in V2 POST body

Missing V2 fields:
- `timestamp` — required (milliseconds)
- `metadata` — required (bytes32)
- `builder` — required (bytes32)

## Why Detection Works But Signing Fails

The prior V2 patch (per `uploads/2026-09-19-harrier-v2-patch-design.md`) successfully updated:
- `src/service/parse.rs` — V2 OrderFilled event parsing
- `config.json` — V2 exchange addresses + `domain_version: "2"`

But the CLOB client signing path (`src/service/clob.rs`) was **not updated** from V1 to V2.

## Confirmation

1. **Domain version is used**: Lines 189-193 of `clob.rs` correctly read `self.exchange.domain_version` from config ("2")
2. **Verifying contract is correct**: Lines 183-187 pick CTF V2 or NegRisk V2 based on `neg_risk` flag
3. **signatureType logic is correct**: Lines 123-127 auto-detect POLY_1271 (3) when funder ≠ signer
4. **But the Order struct is wrong**: Lines 32-45 define the V1 struct, not V2

## Fix Required

Update `src/service/clob.rs`:

1. Replace the `Order` sol! struct with V2 format:
   - Remove: `taker`, `expiration`, `nonce`, `feeRateBps`
   - Change `uint256 side` → `uint8 side`
   - Change `uint256 signatureType` → `uint8 signatureType`
   - Add: `uint256 timestamp`, `bytes32 metadata`, `bytes32 builder`

2. Update `SignedOrder` struct:
   - Remove: `taker`, `nonce`, `fee_rate_bps`
   - Add: `timestamp`, `metadata`, `builder`

3. Update `build_signed_order` method:
   - Generate `timestamp: U256::from(chrono::Utc::now().timestamp_millis() as u64)`
   - Set `metadata: B256::ZERO` (unless builder-specific)
   - Set `builder: B256::ZERO` (unless registered builder code)
   - Remove taker, nonce, feeRateBps assignments

4. Update `SignedOrder` serialization to match V2 POST body format

## Verification Path

After fix:
1. Build binary, tag as `v0.2.1-v2-fix` or similar
2. Deploy in dry-run (`enable_trading: false`)
3. Confirm logs show complete signed order with timestamp/metadata/builder
4. Compare signed order JSON structure to [official V2 docs](https://docs.polymarket.com/api-reference/trade/post-a-new-order)
5. Enable trading for a small test order
6. Confirm successful CLOB acceptance (200 response with `orderID`)

## Impact

**Pattern**: 100% of orders sized large enough to pass min thresholds fail. The bot has been non-functional for live copies since the v0.2.0-v2 deploy (~35h ago as of 2026-09-22 EC).

**No capital lost**: Orders are rejected at POST time (before settlement), not after partial fills.

## References

- Polymarket V2 Migration: https://docs.polymarket.com/v2-migration
- V2 Order API: https://docs.polymarket.com/api-reference/trade/post-a-new-order
- Official py-clob-client-v2: https://github.com/Polymarket/py-clob-client-v2
- Live evidence: `uploads/BRIEF_8d93.md`
- Original V2 design: `uploads/2026-09-19-harrier-v2-patch-design_424b.md`

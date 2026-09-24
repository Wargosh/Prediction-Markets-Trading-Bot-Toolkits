# V2 Order Fix - Round 2 Summary

**Date**: 2026-09-24  
**Branch**: `cursor/fix-v2-order-struct-3b81`  
**PR**: #2 (updated)  
**Status**: ✅ Code complete, ready for dry-run testing

## Quick Summary

After PR #2 fixed the V2 Order struct, live orders **still failed** with "Invalid order payload" (HTTP 400). Root cause: **four wire-format mismatches** for deposit-wallet (POLY_1271) flows that only show up when comparing actual POST body to official SDK.

## The Four Issues Fixed

### 1. Salt Serialization ❌→✅
**Problem**: Salt serialized as JSON string `"12345678901234567890"`  
**Fix**: Salt now serialized as JSON number `12345678901234567890`  
**Why**: CLOB V2 expects integer. JavaScript safe integer limit requires bounded generation.

```rust
// Before:
pub salt: String,  // to_string() → JSON string

// After:
#[serde(serialize_with = "serialize_salt_as_int")]
pub salt: u64,     // custom serializer → JSON number
```

**SDK Reference**: `order_data_v2.py:60` — `"salt": int(order.salt)`

### 2. V2 Order Signer Field ❌→✅
**Problem**: For POLY_1271, `signer` was EOA address (the private key holder)  
**Fix**: For POLY_1271, `signer` is now funder address (the deposit wallet)  
**Why**: EIP-1271 verification happens at the contract level, not the EOA.

```rust
// Before:
signer: self.signer.address(),  // Always EOA

// After:
let order_signer = match self.signature_type {
    SignatureType::Poly1271 => self.funder,      // Deposit wallet
    _ => self.signer.address(),                   // EOA for direct signing
};
```

**SDK Reference**: `builder.py:63-67` — `_v2_order_signer()`

### 3. POLY_1271 Signature Wrapper ❌→✅
**Problem**: Plain 65-byte EIP-712 signature (132 hex chars)  
**Fix**: Solady TypedDataSign wrapper (~450 hex chars)  
**Why**: Deposit wallet contract verifies via EIP-1271, not ECDSA recovery.

```rust
// Before:
let digest = order.eip712_signing_hash(&domain);
let sig = self.signer.sign_hash(&digest).await?;
// → "0x" + 130 hex chars

// After:
match self.signature_type {
    SignatureType::Poly1271 => {
        self.build_poly_1271_signature(&order, &domain).await?
        // → "0x" + inner_sig + domain_sep + contents + type + len
        //    ≈ 450 hex chars total
    }
    _ => { /* plain EIP-712 */ }
}
```

The wrapper includes:
- Inner signature (65 bytes / 130 hex)
- App domain separator (32 bytes / 64 hex)
- Contents hash (32 bytes / 64 hex)
- Order type string (~141 bytes / 282 hex)
- Type string length (2 bytes / 4 hex)

**SDK Reference**: `exchange_order_builder_v2.py:161-237` — `_build_poly_1271_order_signature()`

### 4. Owner Field in POST Body ❌→✅
**Problem**: `owner` was funder address `"0x1234...5678"`  
**Fix**: `owner` is now `api_key` string (L2 credential)  
**Why**: Polymarket uses `owner` as the L2 API account identifier, not the wallet address.

```rust
// Before:
owner: format!("0x{:x}", self.funder),

// After:
owner: self.api_key.as_ref()
    .ok_or_else(|| anyhow!("L2 api_key required"))?
    .clone(),
```

**SDK Reference**: `client.py` — `owner = self.creds.api_key or ""`

## Files Changed

1. **`src/service/clob.rs`** (400+ lines added/modified):
   - Salt type and serialization
   - `generate_order_salt()` helper
   - V2 order signer logic
   - `build_poly_1271_signature()` implementation
   - Owner field fix in POST body
   - 3 new unit tests

2. **`RCA_INVALID_ORDER_PAYLOAD.md`** (updated):
   - Added "Round 2" section documenting all four issues
   - Detailed SDK references for each fix
   - Why these weren't caught initially

## Test Coverage

✅ All 37 unit tests pass (`cargo +nightly test --lib`)

New tests added:
```rust
test_salt_generation()           // Bounded salt generation
test_salt_json_serialization()   // Salt as JSON number, not string
test_signature_type_values()     // Enum values match V2 spec
```

## Verification Checklist

- [x] Code compiles without errors
- [x] All unit tests pass
- [x] Committed and pushed to `cursor/fix-v2-order-struct-3b81`
- [x] PR #2 updated with round-2 changes
- [x] RCA document updated
- [ ] Docker image rebuilt on Mac Mini (parent will do this)
- [ ] Dry-run test with `enable_trading: false`
- [ ] Verify POST body matches official SDK:
  - [ ] Salt is JSON number
  - [ ] Signer = funder address (not EOA)
  - [ ] Signature ~450 chars (not ~132)
  - [ ] Owner = api_key string (not address)
- [ ] Confirm HTTP 200 with orderID
- [ ] Enable live trading with small test

## Why These Weren't Caught Initially

1. **Salt**: JSON wire format differs from Rust types. The `to_string()` serialization *looked* correct without comparing actual bytes.

2. **Signer field**: Most examples use direct EOA signing (signatureType=0). The deposit-wallet flow (signatureType=3) is less common.

3. **Signature wrapper**: Solady's TypedDataSign is specific to Polymarket's deposit wallet implementation. Standard EIP-712 examples don't show this pattern.

4. **Owner field**: The name "owner" suggests wallet address. Without seeing the SDK source, using the funder seemed logical.

All four only manifest with:
- Live CLOB POST requests (not just detection)
- Deposit wallet setup (funder ≠ signer)
- HTTP 400 error body inspection

## Next Steps

1. Parent rebuilds Docker image from this branch
2. Deploy to Harrier in dry-run mode
3. Inspect logs for complete POST body structure
4. Compare to official SDK output (especially POLY_1271 signature length)
5. If dry-run shows HTTP 200 + orderID → promote to live
6. Monitor first few live copies for successful fills

## Key Takeaway

**Round 1** fixed the EIP-712 typehash (V1 vs V2 struct mismatch).  
**Round 2** fixed the wire format (salt/signer/signature/owner for POLY_1271).

Both were required. The V2 struct change was necessary but not sufficient for deposit-wallet flows.

# SUMMARY: V2 Order Payload Fix

**Status**: Root cause identified and fixed  
**PR**: https://github.com/Wargosh/Prediction-Markets-Trading-Bot-Toolkits/pull/2  
**Date**: 2026-09-23

---

## What Was Wrong

Your live Harrier V2 copy bots (Car + ImJustKen) were detecting whale trades correctly but failing on every order submission with:

```
CLOB rejected order (HTTP 400): {"error":"Invalid order payload"}
```

**Root Cause**: The code was signing orders with the **V1 EIP-712 Order struct** but Polymarket CLOB V2 (live since April 28, 2026) requires the **V2 format**. The typehash mismatch caused 100% signature validation failures.

## What Was Fixed

### File: `src/service/clob.rs`

The prior V2 patch successfully updated detection (exchange addresses + OrderFilled parsing) but **missed updating the order signing code**.

**Order Struct Changes (V1 → V2)**:

```diff
struct Order {
    uint256 salt;
    address maker;
    address signer;
-   address taker;          // Removed in V2
    uint256 tokenId;
    uint256 makerAmount;
    uint256 takerAmount;
-   uint256 expiration;     // Removed in V2
-   uint256 nonce;          // Removed in V2
-   uint256 feeRateBps;     // Removed in V2
-   uint256 side;           // Changed to uint8
-   uint256 signatureType;  // Changed to uint8
+   uint8 side;             // Changed from uint256
+   uint8 signatureType;    // Changed from uint256
+   uint256 timestamp;      // NEW: milliseconds
+   bytes32 metadata;       // NEW
+   bytes32 builder;        // NEW
}
```

**Why This Broke Everything**:
- EIP-712 typehash is computed from field names + types
- V1 struct produces different typehash than V2
- CLOB validates signature against V2 typehash → all V1 signatures fail
- Even with correct domain_version="2" in config, wrong struct = wrong signature

## Next Steps

### 1. Test the Fix (Dry-Run)

```bash
# Build new image from fix branch
cd polymarket_harrier
docker build -t harrier-polymarket-isolated:v0.2.1-v2-fix .

# Update compose with new tag + dry-run
# In docker-compose.yml or run command:
#   enable_trading: false
#   (or keep mock_trading: true)

docker-compose up -d harrier-live

# Watch logs for complete V2 order structure
docker logs -f harrier-live

# Look for:
# - "whale trade detected" on qualifying Car/Ken trades
# - Signed order with timestamp, metadata, builder fields
# - NO "Invalid order payload" errors
```

### 2. Verify Order Structure

When a whale trade is detected, the dry-run log should show a signed order like:

```json
{
  "salt": "...",
  "maker": "0x...",
  "signer": "0x...",
  "tokenId": "...",
  "makerAmount": "...",
  "takerAmount": "...",
  "side": "BUY",
  "signatureType": 3,
  "timestamp": "1727064123456",    ← NEW (milliseconds)
  "metadata": "0x000...000",       ← NEW
  "builder": "0x000...000",        ← NEW
  "expiration": "0",               ← Still in POST body
  "signature": "0x..."
}
```

Compare to [official docs](https://docs.polymarket.com/api-reference/trade/post-a-new-order).

### 3. Enable Live Trading

**Only after confirming dry-run shows V2 structure**:

```bash
# Update config
enable_trading: true
mock_trading: false

# Restart containers
docker-compose restart harrier-live harrier-live-2

# Monitor closely for first successful copy
docker logs -f harrier-live | grep -E "whale|order|CLOB"
```

### 4. Expected Outcome

- Whale trades sized above min thresholds → successful CLOB acceptance (HTTP 200 with `orderID`)
- No more "Invalid order payload" errors
- Copy fills appear in positions/logs

## What Else Was Verified

✅ Domain version: Config correctly has `"domain_version": "2"`  
✅ Exchange addresses: V2 CTF + NegRisk addresses in config  
✅ Verifying contract: Code picks correct V2 contract by neg_risk flag  
✅ signatureType: Correctly auto-detects POLY_1271 (3) for deposit wallets  
✅ Detection: V2 OrderFilled parsing works (why detection succeeds)

The **only** gap was the Order struct format used for signing.

## References

- **PR with fix**: https://github.com/Wargosh/Prediction-Markets-Trading-Bot-Toolkits/pull/2
- **RCA document**: `RCA_INVALID_ORDER_PAYLOAD.md`
- **Polymarket V2 migration**: https://docs.polymarket.com/v2-migration
- **Official V2 API**: https://docs.polymarket.com/api-reference/trade/post-a-new-order

## Timeline

- **April 28, 2026**: Polymarket V2 went live
- **~2026-09-20**: Deploy of v0.2.0-v2 (detection fixed, signing missed)
- **2026-09-22**: Zero successful copies, 100% "Invalid order payload"
- **2026-09-23**: Root cause found, fix committed + PR opened

---

## No Live Restart Claimed

Per your constraints, I have **not**:
- Restarted live containers
- Changed the running image tag
- Touched BTC5m day-test configs
- Enabled trading on the fix branch

The fix is ready for you to test in dry-run, then deploy when validated.

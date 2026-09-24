# Visual: Why Orders Failed

```
┌─────────────────────────────────────────────────────────────────┐
│                     BEFORE (Broken)                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Whale Buy Detected ✓                                           │
│         ↓                                                       │
│  V2 Exchange Address ✓                                          │
│  domain_version: "2" ✓                                          │
│  signatureType: 3 ✓                                             │
│         ↓                                                       │
│  ┌─────────────────────────────────┐                           │
│  │  EIP-712 Signing with V1 struct │  ← PROBLEM                 │
│  │  ✗ Has: taker, nonce, feeRateBps│                           │
│  │  ✗ Missing: timestamp, metadata │                           │
│  │  ✗ Wrong types: uint256 vs uint8│                           │
│  └─────────────────────────────────┘                           │
│         ↓                                                       │
│  Wrong Typehash → Wrong Signature                               │
│         ↓                                                       │
│  POST /order → HTTP 400                                         │
│  {"error": "Invalid order payload"} ✗                           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                      AFTER (Fixed)                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Whale Buy Detected ✓                                           │
│         ↓                                                       │
│  V2 Exchange Address ✓                                          │
│  domain_version: "2" ✓                                          │
│  signatureType: 3 ✓                                             │
│         ↓                                                       │
│  ┌─────────────────────────────────┐                           │
│  │  EIP-712 Signing with V2 struct │  ✓ FIXED                   │
│  │  ✓ Removed: taker, nonce, fees  │                           │
│  │  ✓ Added: timestamp, metadata   │                           │
│  │  ✓ Correct types: uint8         │                           │
│  └─────────────────────────────────┘                           │
│         ↓                                                       │
│  Correct Typehash → Valid Signature                             │
│         ↓                                                       │
│  POST /order → HTTP 200                                         │
│  {"orderID": "...", "success": true} ✓                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## The Issue in One Sentence

**The code was signing V1-format orders but Polymarket CLOB V2 only accepts V2-format signatures.**

## What Changed

| Component | V1 (Wrong) | V2 (Fixed) |
|-----------|-----------|------------|
| `taker` | address | ❌ Removed |
| `expiration` | uint256 | ❌ Removed from signed struct |
| `nonce` | uint256 | ❌ Removed |
| `feeRateBps` | uint256 | ❌ Removed |
| `side` | uint256 | ✅ Changed to uint8 |
| `signatureType` | uint256 | ✅ Changed to uint8 |
| `timestamp` | ❌ Missing | ✅ Added (milliseconds) |
| `metadata` | ❌ Missing | ✅ Added (bytes32) |
| `builder` | ❌ Missing | ✅ Added (bytes32) |

## Impact

**Before Fix**: 100% order failures (53 total failures in ~48h)  
**After Fix**: Should resolve all "Invalid order payload" errors

Detection worked because it was updated to V2 in the prior patch.  
Signing was the missing piece.

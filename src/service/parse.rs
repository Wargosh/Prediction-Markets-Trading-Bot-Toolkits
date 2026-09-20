//! ABI log decoding for Polymarket CTF Exchange events.
//!
//! Decodes `OrderFilled` / `OrdersMatched` and projects them into the
//! [`WhaleTrade`] canonical view. Topic-0 selectors are recomputed from
//! the canonical event signatures via `keccak256`.

use crate::models::{Side, VenueId, WhaleTrade};
use alloy_primitives::{B256, U256};
use alloy_sol_types::{sol, SolEvent};
use anyhow::{anyhow, Result};

use super::onchain::RawLog;

sol! {
    /// CTF Exchange V2 — fired once per filled maker order.
    /// V2 layout: side/tokenId in data, not asset-id inference.
    event OrderFilled(
        bytes32 indexed orderHash,
        address indexed maker,
        address indexed taker,
        uint8 side,
        uint256 tokenId,
        uint256 makerAmountFilled,
        uint256 takerAmountFilled,
        uint256 fee,
        bytes32 builder,
        bytes32 metadata
    );
}

/// Topic-0 selector for `OrderFilled`, derived once at compile time via the
/// `sol!` macro.
pub fn order_filled_topic() -> B256 {
    OrderFilled::SIGNATURE_HASH
}

/// Returns `Ok(Some(trade))` if the log was an `OrderFilled` involving the
/// given whale address (as maker), `Ok(None)` if it was not relevant, or `Err`
/// on a malformed log.
pub fn decode_whale_trade(log: &RawLog, whale_address: &str) -> Result<Option<WhaleTrade>> {
    let whale = whale_address.to_lowercase();
    let target_topic = order_filled_topic();

    let topic0 = log
        .topics
        .first()
        .ok_or_else(|| anyhow!("log has no topic0"))?;
    let topic0_b256: B256 = topic0
        .parse()
        .map_err(|e| anyhow!("topic0 not hex bytes32: {e}"))?;
    if topic0_b256 != target_topic {
        return Ok(None);
    }

    // V2 OrderFilled has 3 indexed parameters: orderHash, maker, taker.
    if log.topics.len() < 4 {
        return Ok(None);
    }
    let maker_topic = &log.topics[2];
    let maker = topic_to_address(maker_topic)?;
    if maker.to_lowercase() != whale {
        return Ok(None);
    }

    // V2 non-indexed body: side(u8), tokenId, makerAmountFilled, takerAmountFilled, fee, builder, metadata
    // side is u8 but ABI-encoded as uint256 (32 bytes), followed by 6 more uint256 fields.
    let data_bytes =
        hex::decode(log.data.trim_start_matches("0x")).map_err(|e| anyhow!(e))?;
    if data_bytes.len() < 7 * 32 {
        return Err(anyhow!("V2 OrderFilled data shorter than 7*32 bytes"));
    }

    // side: 0=BUY, 1=SELL (encoded as uint256, take the last byte)
    let side_raw = data_bytes[31];
    let side = match side_raw {
        0 => Side::Buy,
        1 => Side::Sell,
        _ => return Err(anyhow!("invalid side value: {}", side_raw)),
    };

    let token_id = U256::from_be_slice(&data_bytes[32..64]);
    let maker_amount = U256::from_be_slice(&data_bytes[64..96]);
    let taker_amount = U256::from_be_slice(&data_bytes[96..128]);
    let _fee = U256::from_be_slice(&data_bytes[128..160]);
    // builder and metadata are at [160..192] and [192..224] but we don't use them.

    // V2: side is explicit. When BUY, maker pays USD (makerAmount), receives shares (takerAmount).
    // When SELL, maker gives shares (makerAmount), receives USD (takerAmount).
    let token_id_str = token_id.to_string();
    let (shares, usd_notional) = match side {
        Side::Buy => {
            let shares = to_f64_with_decimals(taker_amount, 6);
            let usd = to_f64_with_decimals(maker_amount, 6);
            (shares, usd)
        }
        Side::Sell => {
            let shares = to_f64_with_decimals(maker_amount, 6);
            let usd = to_f64_with_decimals(taker_amount, 6);
            (shares, usd)
        }
    };
    let price = if shares > 0.0 { usd_notional / shares } else { 0.0 };

    Ok(Some(WhaleTrade {
        venue: VenueId::Polymarket,
        maker,
        side,
        token_id: token_id_str,
        shares,
        price,
        usd_notional,
        tx_hash: Some(log.tx_hash.clone()),
        block_number: Some(log.block_number),
        observed_at: chrono::Utc::now(),
    }))
}

fn topic_to_address(topic: &str) -> Result<String> {
    let bytes = hex::decode(topic.trim_start_matches("0x")).map_err(|e| anyhow!(e))?;
    if bytes.len() != 32 {
        return Err(anyhow!("address topic not 32 bytes"));
    }
    Ok(format!("0x{}", hex::encode(&bytes[12..32])))
}

fn to_f64_with_decimals(v: U256, decimals: u32) -> f64 {
    // Safe for typical USDC amounts; clamps if overflowing f64 mantissa.
    let s = v.to_string();
    s.parse::<f64>().unwrap_or(0.0) / 10f64.powi(decimals as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v2_order_filled_buy() {
        // V2 OrderFilled BUY: whale buys 100 shares at $0.60 (60 USD total).
        // topic0: V2 OrderFilled signature
        // topic1: orderHash (dummy)
        // topic2: maker (whale)
        // topic3: taker (dummy)
        // data: side(0=BUY), tokenId, makerAmountFilled(60 USDC), takerAmountFilled(100 shares), fee, builder, metadata
        let whale = "0x63ce342161250d705dc0b16df89036c8e5f9ba9a";
        let log = RawLog {
            address: "0xE111180000d2663C0091e4f400237545B87B996B".into(),
            topics: vec![
                // topic0: V2 OrderFilled signature
                "0xd543adfd945773f1a62f74f0ee55a5e3b9b1a28262980ba90b1a89f2ea84d8ee".into(),
                // topic1: orderHash (dummy 32 bytes)
                "0x1111111111111111111111111111111111111111111111111111111111111111".into(),
                // topic2: maker (whale address, padded to 32 bytes)
                format!("0x000000000000000000000000{}", &whale[2..]),
                // topic3: taker (dummy)
                "0x0000000000000000000000002222222222222222222222222222222222222222".into(),
            ],
            data: {
                // side = 0 (BUY), encoded as uint256
                let side = U256::ZERO;
                // tokenId = 123456789
                let token_id = U256::from(123456789u64);
                // makerAmountFilled = 60 USDC = 60_000_000 (6 decimals)
                let maker_amount = U256::from(60_000_000u64);
                // takerAmountFilled = 100 shares = 100_000_000 (6 decimals)
                let taker_amount = U256::from(100_000_000u64);
                // fee = 0
                let fee = U256::ZERO;
                // builder (dummy bytes32)
                let builder = U256::ZERO;
                // metadata (dummy bytes32)
                let metadata = U256::ZERO;

                let mut buf = Vec::new();
                buf.extend_from_slice(&side.to_be_bytes::<32>());
                buf.extend_from_slice(&token_id.to_be_bytes::<32>());
                buf.extend_from_slice(&maker_amount.to_be_bytes::<32>());
                buf.extend_from_slice(&taker_amount.to_be_bytes::<32>());
                buf.extend_from_slice(&fee.to_be_bytes::<32>());
                buf.extend_from_slice(&builder.to_be_bytes::<32>());
                buf.extend_from_slice(&metadata.to_be_bytes::<32>());
                format!("0x{}", hex::encode(buf))
            },
            tx_hash: "0x900004666d9ded0100000000000000000000000000000000000000000000000".into(),
            block_number: 12345678,
        };

        let result = decode_whale_trade(&log, whale).unwrap();
        assert!(result.is_some());
        let trade = result.unwrap();
        assert_eq!(trade.side, Side::Buy);
        assert_eq!(trade.token_id, "123456789");
        assert_eq!(trade.shares, 100.0);
        assert_eq!(trade.usd_notional, 60.0);
        assert_eq!(trade.price, 0.6);
    }

    #[test]
    fn test_v2_order_filled_sell() {
        // V2 OrderFilled SELL: whale sells 50 shares at $0.40 (20 USD total).
        let whale = "0x63ce342161250d705dc0b16df89036c8e5f9ba9a";
        let log = RawLog {
            address: "0xE111180000d2663C0091e4f400237545B87B996B".into(),
            topics: vec![
                "0xd543adfd945773f1a62f74f0ee55a5e3b9b1a28262980ba90b1a89f2ea84d8ee".into(),
                "0x1111111111111111111111111111111111111111111111111111111111111111".into(),
                format!("0x000000000000000000000000{}", &whale[2..]),
                "0x0000000000000000000000002222222222222222222222222222222222222222".into(),
            ],
            data: {
                // side = 1 (SELL)
                let side = U256::from(1u8);
                let token_id = U256::from(987654321u64);
                // makerAmountFilled = 50 shares = 50_000_000
                let maker_amount = U256::from(50_000_000u64);
                // takerAmountFilled = 20 USDC = 20_000_000
                let taker_amount = U256::from(20_000_000u64);
                let fee = U256::ZERO;
                let builder = U256::ZERO;
                let metadata = U256::ZERO;

                let mut buf = Vec::new();
                buf.extend_from_slice(&side.to_be_bytes::<32>());
                buf.extend_from_slice(&token_id.to_be_bytes::<32>());
                buf.extend_from_slice(&maker_amount.to_be_bytes::<32>());
                buf.extend_from_slice(&taker_amount.to_be_bytes::<32>());
                buf.extend_from_slice(&fee.to_be_bytes::<32>());
                buf.extend_from_slice(&builder.to_be_bytes::<32>());
                buf.extend_from_slice(&metadata.to_be_bytes::<32>());
                format!("0x{}", hex::encode(buf))
            },
            tx_hash: "0xabcdef0000000000000000000000000000000000000000000000000000000000".into(),
            block_number: 12345679,
        };

        let result = decode_whale_trade(&log, whale).unwrap();
        assert!(result.is_some());
        let trade = result.unwrap();
        assert_eq!(trade.side, Side::Sell);
        assert_eq!(trade.token_id, "987654321");
        assert_eq!(trade.shares, 50.0);
        assert_eq!(trade.usd_notional, 20.0);
        assert_eq!(trade.price, 0.4);
    }

    #[test]
    fn test_v2_wrong_whale() {
        // Should return None when maker is not the tracked whale
        let whale = "0x63ce342161250d705dc0b16df89036c8e5f9ba9a";
        let different_maker = "0x1111111111111111111111111111111111111111";
        let log = RawLog {
            address: "0xE111180000d2663C0091e4f400237545B87B996B".into(),
            topics: vec![
                "0xd543adfd945773f1a62f74f0ee55a5e3b9b1a28262980ba90b1a89f2ea84d8ee".into(),
                "0x1111111111111111111111111111111111111111111111111111111111111111".into(),
                format!("0x000000000000000000000000{}", &different_maker[2..]),
                "0x0000000000000000000000002222222222222222222222222222222222222222".into(),
            ],
            data: "0x0000000000000000000000000000000000000000000000000000000000000000\
                      0000000000000000000000000000000000000000000000000000000075bcd15\
                      0000000000000000000000000000000000000000000000000000003938700\
                      0000000000000000000000000000000000000000000000000000005f5e100\
                      0000000000000000000000000000000000000000000000000000000000000000\
                      0000000000000000000000000000000000000000000000000000000000000000\
                      0000000000000000000000000000000000000000000000000000000000000000".into(),
            tx_hash: "0xtest000000000000000000000000000000000000000000000000000000000".into(),
            block_number: 12345680,
        };

        let result = decode_whale_trade(&log, whale).unwrap();
        assert!(result.is_none());
    }
}


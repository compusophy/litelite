//! Binance monthly klines CSV -> stratlite candles, in integer ticks.
//!
//! The harness owns the tick scale; the verifier validates coherence and range
//! (`0 < low <= open,close <= high <= 2^53`, `0 <= volume <= 2^53`). We pick
//! CENTS for price and 1e-8 for volume: BTCUSDT's tick size is 0.01 and its
//! volume carries 8 decimals, so both convert with ZERO precision loss.
//!
//! That is load-bearing, not incidental. A silent truncation here would move
//! every downstream number — the equity curve, its hash, the whole §5 table —
//! and it would still look clean. So `scaled` REFUSES to drop a nonzero digit
//! rather than round: a wrong-but-clean result is worse than an error.

use stratlite::Candle;

/// Price ticks per unit: cents.
pub const PRICE_DECIMALS: u32 = 2;
/// Volume ticks per unit: 1e-8 (Binance quotes 8 decimals).
pub const VOLUME_DECIMALS: u32 = 8;

/// Parse a fixed-point decimal string to an integer at `decimals` places,
/// exactly. Errors rather than truncate — see the module docs.
pub fn scaled(s: &str, decimals: u32) -> Result<i64, String> {
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s, ""),
    };
    if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("{s:?}: not a non-negative decimal"));
    }
    if !frac_part.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("{s:?}: non-digit in the fraction"));
    }
    let want = decimals as usize;
    // Every digit past our scale MUST be zero, or we would be silently
    // discarding information the source actually carried.
    if frac_part.len() > want && frac_part[want..].bytes().any(|b| b != b'0') {
        return Err(format!(
            "{s:?}: more precision than {decimals} decimals holds — refusing to truncate"
        ));
    }
    let mut digits = String::with_capacity(int_part.len() + want);
    digits.push_str(int_part);
    for i in 0..want {
        digits.push(frac_part.as_bytes().get(i).copied().unwrap_or(b'0') as char);
    }
    digits
        .parse::<i64>()
        .map_err(|e| format!("{s:?}: does not fit i64 at {decimals} decimals ({e})"))
}

/// One Binance kline row -> a candle. Columns 0..=5 are
/// `open_time, open, high, low, close, volume`; the rest we do not use.
fn row(line: &str) -> Result<(i64, Candle), String> {
    let f: Vec<&str> = line.split(',').collect();
    if f.len() < 6 {
        return Err(format!("expected >=6 columns, got {}", f.len()));
    }
    let open_time = f[0]
        .parse::<i64>()
        .map_err(|e| format!("open_time {:?}: {e}", f[0]))?;
    Ok((
        open_time,
        Candle {
            open: scaled(f[1], PRICE_DECIMALS)?,
            high: scaled(f[2], PRICE_DECIMALS)?,
            low: scaled(f[3], PRICE_DECIMALS)?,
            close: scaled(f[4], PRICE_DECIMALS)?,
            volume: scaled(f[5], VOLUME_DECIMALS)?,
        },
    ))
}

/// Parse a whole klines CSV (no header). Returns candles in file order and
/// checks the source is actually ordered — an out-of-order or duplicated bar
/// would silently reorder history, which is the one thing a backtest must not
/// tolerate.
pub fn parse(csv: &str) -> Result<Vec<Candle>, String> {
    let mut out = Vec::new();
    let mut last_time: Option<i64> = None;
    for (i, line) in csv.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (t, c) = row(line).map_err(|e| format!("line {}: {e}", i + 1))?;
        if let Some(prev) = last_time
            && t <= prev
        {
            return Err(format!(
                "line {}: open_time {t} is not after {prev} — source is out of order",
                i + 1
            ));
        }
        last_time = Some(t);
        out.push(c);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_are_exact_and_truncation_is_refused() {
        assert_eq!(scaled("42283.58000000", 2), Ok(4_228_358));
        assert_eq!(scaled("0.01", 2), Ok(1));
        assert_eq!(scaled("7", 2), Ok(700));
        // A price finer than our scale is an ERROR, never a rounded guess.
        assert!(scaled("42283.581", 2).is_err());
        assert_eq!(scaled("1271.68108000", 8), Ok(127_168_108_000));
    }

    #[test]
    fn real_binance_rows_parse_and_stay_ordered() {
        let csv = "1704067200000,42283.58000000,42554.57000000,42261.02000000,42475.23000000,1271.68108000,1704070799999,53957248.97378900,47134,682.57581000,28957416.81964500,0\n\
                   1704070800000,42475.23000000,42775.00000000,42431.65000000,42613.56000000,1196.37856000,1704074399999,50984893.34814160,50396,712.32227000,30355645.34827640,0";
        let c = parse(csv).unwrap();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].open, 4_228_358);
        assert_eq!(c[0].high, 4_255_457);
        // The verifier's coherence rule must hold on real data, not just ours.
        for k in &c {
            assert!(0 < k.low && k.low <= k.open.min(k.close));
            assert!(k.open.max(k.close) <= k.high);
        }
    }

    #[test]
    fn out_of_order_history_is_an_error_not_a_reorder() {
        let csv = "1704070800000,1.00,1.00,1.00,1.00,0.00000000,0,0,0,0,0,0\n\
                   1704067200000,1.00,1.00,1.00,1.00,0.00000000,0,0,0,0,0,0";
        assert!(parse(csv).unwrap_err().contains("out of order"));
    }
}

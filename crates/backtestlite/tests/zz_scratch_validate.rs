use backtestlite::{verify, Candle, Costs, Gate, Limits};

fn series(n: i64) -> Vec<Candle> {
    (0..n)
        .map(|i| {
            let period = 60i64;
            let phase = i % period;
            let tri = (phase - period / 2).abs(); // 0..30
            let wave = 300 * tri; // 0..9000
            let drift = 2 * i;
            let wig = 150 * ((i % 5) - 2);
            let open = 100_000 + drift + wave;
            let close = open + wig + 80 * (((i + 1) % 7) - 3);
            let hi = open.max(close) + 120;
            let lo = open.min(close) - 120;
            Candle {
                open,
                high: hi,
                low: lo,
                close,
                volume: 1000 + i % 13,
            }
        })
        .collect()
}

const PROGRAMS: &[&str] = &[
    // 1
    "lookback 14;
     let r = rsi(14);
     if r < 3000 { signal long; } else if r > 7000 { signal short; } else { signal flat; }",
    // 2
    "lookback 14;
     if position() == 0 && rsi(14) < 2500 { signal long; }
     else if position() == 1 && rsi(14) > 5000 { signal flat; }",
    // 3
    "lookback 20;
     let m = sma(20);
     let c = close(0);
     if c < m - m / 20 { signal long; }
     else if c > m + m / 20 { signal short; }
     else { signal flat; }",
    // 4
    "lookback 30;
     let r = rsi(14);
     let dev = close(0) - sma(30);
     if r < 3500 && dev < 0 { signal long; }
     else if r > 6500 && dev > 0 { signal short; }
     else { signal flat; }",
    // 5
    "lookback 14;
     var cooldown = 0;
     if cooldown > 0 { cooldown = cooldown - 1; }
     let r = rsi(14);
     if position() == 0 && cooldown == 0 && r < 3000 { signal long; }
     if position() == 1 && r > 5500 { signal flat; cooldown = 5; }",
    // 6
    "lookback 20;
     let m = sma(20);
     let c = close(0);
     if c * 100 < m * 97 { signal long; }
     else if c * 100 > m * 103 { signal short; }
     else { signal flat; }",
    // 7
    "lookback 10;
     let e = ema(10);
     if close(0) < e - e / 50 { signal long; }
     else if close(0) > e + e / 50 { signal flat; }",
    // 8
    "lookback 14;
     let fast = rsi(7);
     let slow = rsi(14);
     if fast < 3000 && slow < 4000 { signal long; }
     else if fast > 7000 && slow > 6000 { signal short; }
     else { signal flat; }",
    // 9
    "lookback 20;
     let hi = highest(20);
     let lo = lowest(20);
     let range = hi - lo;
     if close(0) < lo + range / 5 { signal long; }
     else if close(0) > hi - range / 5 { signal short; }
     else { signal flat; }",
    // 10
    "lookback 14;
     let r = rsi(14);
     if position() == 0 && r < 3000 { signal long; }
     else if position() == 1 && close(0) > entry_price() + entry_price() / 50 { signal flat; }
     else if position() == 1 && r > 6000 { signal flat; }",
    // 11
    "lookback 14;
     var prev = 5000;
     let r = rsi(14);
     if prev < 3000 && r >= 3000 { signal long; }
     else if prev > 7000 && r <= 7000 { signal short; }
     prev = r;",
    // 12
    "lookback 50;
     let fast = sma(10);
     let slow = sma(50);
     if fast < slow - slow / 25 { signal long; }
     else if fast > slow + slow / 25 { signal short; }
     else { signal flat; }",
    // 13
    "lookback 21;
     let r = rsi(21);
     if r <= 2000 { signal long; }
     else if r >= 8000 { signal short; }
     else if r > 4500 && r < 5500 { signal flat; }",
    // 14
    "lookback 25;
     let r = rsi(14);
     if close(0) <= lowest(10) && r < 4000 { signal long; }
     else if close(0) >= highest(10) && r > 6000 { signal short; }
     else { signal flat; }",
    // 15
    "lookback 20;
     let m = sma(20);
     let d = close(0) - m;
     if position() == 0 && d < -50 { signal long; }
     else if position() == 1 && d > 0 { signal flat; }
     else if position() == 0 && d > 50 { signal short; }
     else if position() == -1 && d < 0 { signal flat; }",
    // 16
    "lookback 14;
     var streak = 0;
     let r = rsi(14);
     if r < 3500 { streak = streak + 1; } else { streak = 0; }
     if streak >= 3 { signal long; }
     if position() == 1 && r > 5500 { signal flat; streak = 0; }",
    // 17
    "lookback 5;
     let s = 0;
     let i = 0;
     repeat 5 {
       s = s + (close(i) - open(i));
       i = i + 1;
     }
     if s < 0 { signal long; } else if s > 0 { signal short; } else { signal flat; }",
    // 18
    "lookback 14;
     var bars_held = 0;
     let r = rsi(14);
     if position() != 0 { bars_held = bars_held + 1; } else { bars_held = 0; }
     if position() == 0 && r < 2800 { signal long; }
     else if position() == 0 && r > 7200 { signal short; }
     else if bars_held > 10 { signal flat; }",
    // 19
    "lookback 40;
     let r = rsi(14);
     let m = sma(40);
     let c = close(0);
     if r < 3000 || c * 50 < m * 49 { signal long; }
     else if r > 7000 || c * 50 > m * 51 { signal short; }
     else { signal flat; }",
    // 20
    "lookback 14;
     let r = rsi(14);
     if position() == 0 {
       if r < 3000 { signal long; }
       else if r > 7000 { signal short; }
     } else if position() == 1 {
       if r > 5500 { signal flat; }
       else if close(0) < entry_price() - entry_price() / 40 { signal flat; }
     } else {
       if r < 4500 { signal flat; }
       else if close(0) > entry_price() + entry_price() / 40 { signal flat; }
     }",
    // 21
    "lookback 26;
     let fast = ema(12);
     let slow = ema(26);
     if fast < slow - slow / 30 { signal long; }
     else if fast > slow + slow / 30 { signal short; }
     else { signal flat; }",
    // 22
    "lookback 9;
     let r = rsi(9);
     if r < 2000 { signal long; }
     else if r > 8000 { signal short; }
     else if r > 4000 && r < 6000 { signal flat; }",
];

#[test]
fn validate_all() {
    let candles = series(500);
    let mut failures = Vec::new();
    for (idx, src) in PROGRAMS.iter().enumerate() {
        match verify(
            src,
            &candles,
            Limits::default(),
            Costs {
                fee_ticks: 2,
                slippage_ticks: 1,
            },
            Gate::default(),
        ) {
            Ok((_, r)) => {
                println!("P{:02} OK trades={} bars_eval={}", idx + 1, r.trades, r.bars_evaluated);
            }
            Err(e) => {
                println!("P{:02} FAIL {}", idx + 1, e);
                failures.push((idx + 1, format!("{e}")));
            }
        }
    }
    assert!(failures.is_empty(), "failures: {failures:?}");
}

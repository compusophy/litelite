use backtestlite::{verify, Costs, Gate, Limits};
use stratlite::Candle;

fn series(n: usize) -> Vec<Candle> {
    // Oscillating + drifting mid so momentum strategies flip many times.
    let mut mids = Vec::with_capacity(n + 1);
    for i in 0..=n as i64 {
        let t = i as f64;
        let mid = 10_000.0
            + 1800.0 * (t * 0.055).sin()
            + 700.0 * (t * 0.17).sin()
            + 300.0 * (t * 0.31).sin()
            + 4.0 * t; // slow drift
        mids.push(mid.round() as i64);
    }
    (0..n)
        .map(|i| {
            let o = mids[i];
            let c = mids[i + 1];
            let hi = o.max(c) + 25 + (i as i64 % 11);
            let lo = o.min(c) - 25 - (i as i64 % 7);
            Candle {
                open: o,
                high: hi,
                low: lo,
                close: c,
                volume: 100 + (i as i64 % 50),
            }
        })
        .collect()
}

const PROGRAMS: &[&str] = &[
    // P1
    "lookback 20;
     var prev = 0;
     let e = ema(10);
     if e > prev && position() <= 0 { signal long; }
     else if e < prev && position() >= 0 { signal short; }
     prev = e;",
    // P2
    "lookback 12;
     if position() == 0 && close(0) > close(10) { signal long; }
     if position() == 1 && close(0) < close(5) { signal flat; }",
    // P3
    "lookback 30;
     var pf = 0;
     let f = ema(5);
     let s = ema(20);
     if f > pf && f > s && position() != 1 { signal long; }
     else if f < s && position() == 1 { signal flat; }
     pf = f;",
    // P4
    "lookback 24;
     var prev = 0;
     let e = ema(12);
     let slope = e - prev;
     if slope > 5 && position() != 1 { signal long; }
     else if slope < -5 && position() != -1 { signal short; }
     prev = e;",
    // P5
    "lookback 20;
     if close(0) > close(5) && close(5) > close(10) { signal long; }
     else if close(0) < close(5) && close(5) < close(10) { signal short; }
     else { signal flat; }",
    // P6
    "lookback 15;
     let e = ema(15);
     if close(0) > e && position() <= 0 { signal long; }
     if close(0) < e && position() == 1 { signal flat; }",
    // P7
    "lookback 20;
     var prev = 0;
     let e = ema(10);
     if e > prev && rsi(14) > 5000 && position() != 1 { signal long; }
     if e < prev && position() == 1 { signal flat; }
     prev = e;",
    // P8
    "lookback 20;
     var prev = 0;
     let e = ema(9);
     if e > prev && position() == 0 { signal long; }
     if position() == 1 && close(0) < entry_price() { signal flat; }
     prev = e;",
    // P9
    "lookback 20;
     var p1 = 0;
     var p2 = 0;
     let e = ema(8);
     let d1 = e - p1;
     let d2 = p1 - p2;
     if d1 > d2 && position() <= 0 { signal long; }
     else if d1 < d2 && position() >= 0 { signal short; }
     p2 = p1;
     p1 = e;",
    // P10
    "lookback 10;
     var i = 0;
     var mom = 0;
     mom = 0;
     i = 1;
     repeat 5 {
       mom = mom + close(i - 1) - close(i);
       i = i + 1;
     }
     if mom > 0 && position() <= 0 { signal long; }
     else if mom < 0 && position() >= 0 { signal short; }",
    // P11
    "lookback 20;
     var prev = 0;
     let e = ema(10);
     if e > prev && volume(0) > volume(1) && position() != 1 { signal long; }
     if e < prev && position() == 1 { signal flat; }
     prev = e;",
    // P12
    "lookback 16;
     var prev = 0;
     let e = ema(16);
     if e > prev { signal long; }
     else if e < prev { signal short; }
     prev = e;",
    // P13
    "lookback 30;
     if position() == 0 && close(0) > close(3) && close(0) > sma(20) { signal long; }
     if position() == 1 && close(0) < sma(20) { signal flat; }",
    // P14
    "lookback 20;
     var cd = 0;
     if cd > 0 { cd = cd - 1; }
     if position() == 0 && cd == 0 && close(0) > close(8) { signal long; }
     if position() == 1 && close(0) < close(2) { signal flat; cd = 4; }",
    // P15
    "lookback 20;
     if close(0) - lowest(10) > highest(10) - close(0) && position() <= 0 { signal long; }
     else if close(0) - lowest(10) < highest(10) - close(0) && position() >= 0 { signal short; }",
    // P16
    "lookback 25;
     var prev = 0;
     let e = ema(20);
     let slope = e - prev;
     if slope > 0 && close(0) > close(4) && position() != 1 { signal long; }
     else if slope < 0 && close(0) < close(4) && position() != -1 { signal short; }
     prev = e;",
    // P17
    "lookback 12;
     let a = close(0) - close(4);
     let b = close(4) - close(8);
     if a > 0 && b > 0 && position() != 1 { signal long; }
     else if a < 0 && b < 0 && position() != -1 { signal short; }
     else if a == 0 { signal flat; }",
    // P18
    "lookback 18;
     var prev = 0;
     let e = ema(14);
     if position() == 0 && e > prev { signal long; }
     else if position() == 1 && (e < prev || close(0) < entry_price() - 100) { signal flat; }
     prev = e;",
    // P19
    "lookback 40;
     if close(0) > close(20) && close(0) > close(5) && position() <= 0 { signal long; }
     else if close(0) < close(20) && close(0) < close(5) && position() >= 0 { signal short; }",
    // P20
    "lookback 20;
     var prev = 0;
     let e = ema(10);
     if position() != 1 && e * 1000 > prev * 1001 { signal long; }
     else if position() != -1 && e * 1000 < prev * 999 { signal short; }
     prev = e;",
    // P21
    "lookback 15;
     var cd = 0;
     if cd > 0 { cd = cd - 1; }
     let m = close(0) - close(10);
     if cd == 0 && m > 50 && position() != 1 { signal long; cd = 3; }
     else if cd == 0 && m < -50 && position() != -1 { signal short; cd = 3; }",
    // P22
    "lookback 30;
     var pf = 0;
     var ps = 0;
     let f = ema(10);
     let s = ema(25);
     let sf = f - pf;
     let ss = s - ps;
     if sf > 0 && ss > 0 && position() <= 0 { signal long; }
     else if sf < 0 && ss < 0 && position() >= 0 { signal short; }
     pf = f;
     ps = s;",
];

#[test]
fn report_family() {
    let candles = series(500);
    let mut pass = 0;
    for (i, src) in PROGRAMS.iter().enumerate() {
        match verify(src, &candles, Limits::default(), Costs::default(), Gate::default()) {
            Ok((_, r)) => {
                pass += 1;
                println!("P{:02} OK trades={} net={} fuel={}", i + 1, r.trades, r.net_pnl_ticks, r.max_fuel_per_bar);
            }
            Err(e) => {
                println!("P{:02} FAIL {}", i + 1, e);
            }
        }
    }
    println!("PASSED {}/{}", pass, PROGRAMS.len());
}

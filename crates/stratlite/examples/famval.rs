use std::collections::BTreeSet;
use stratlite::{compile, Candle, Limits, Session, Signal};

fn programs() -> Vec<&'static str> {
    vec![
        // 1
        "lookback 50;
if close(0) > sma(30) && rsi(14) > 5000 { signal long; }
else if close(0) < sma(30) && rsi(14) < 5000 { signal short; }
else { signal flat; }",
        // 2
        "lookback 40;
let range = highest(20) - lowest(20);
let mid = (highest(20) + lowest(20)) / 2;
if range > 100 && close(0) > mid { signal long; }
else if range > 100 && close(0) < mid { signal short; }
else { signal flat; }",
        // 3
        "lookback 30;
if rsi(14) < 3000 && close(0) > sma(20) { signal long; }
else if rsi(14) > 7000 { signal short; }
else if close(0) < sma(20) { signal short; }
else { signal flat; }",
        // 4
        "lookback 40;
if ema(10) > ema(30) && rsi(10) > 4500 { signal long; }
else if ema(10) < ema(30) { signal short; }
else { signal flat; }",
        // 5
        "lookback 60;
if high(0) >= highest(20) && rsi(14) > 5000 { signal long; }
else if low(0) <= lowest(20) && rsi(14) < 5000 { signal short; }
else { signal flat; }",
        // 6
        "lookback 50;
var regime = 0;
let vol = highest(30) - lowest(30);
if vol > 200 { regime = 1; } else { regime = 0; }
if regime == 1 && close(0) > sma(20) { signal long; }
else if regime == 1 && close(0) < sma(20) { signal short; }
else { signal flat; }",
        // 7
        "lookback 30;
if rsi(7) < 2500 || (close(0) > sma(25) && rsi(7) > 5500) { signal long; }
else if rsi(7) > 7500 || close(0) < sma(25) { signal short; }
else { signal flat; }",
        // 8
        "lookback 60;
if sma(10) > sma(20) && sma(20) > sma(40) { signal long; }
else if sma(10) < sma(20) && sma(20) < sma(40) { signal short; }
else { signal flat; }",
        // 9
        "lookback 50;
let fast = highest(10) - lowest(10);
let slow = highest(40) - lowest(40);
if fast * 5 > slow * 2 && close(0) > ema(20) { signal long; }
else if fast * 5 > slow * 2 && close(0) < ema(20) { signal short; }
else { signal flat; }",
        // 10
        "lookback 40;
if position() == 0 && close(0) > sma(30) && rsi(14) > 5000 { signal long; }
else if position() == 0 && close(0) < sma(30) && rsi(14) < 5000 { signal short; }
else if position() == 1 && close(0) < entry_price() - 50 { signal flat; }
else if position() == -1 && close(0) > entry_price() + 50 { signal flat; }",
        // 11
        "lookback 40;
if close(0) > close(10) && rsi(14) > 5000 && ema(20) > sma(20) { signal long; }
else if close(0) < close(10) && rsi(14) < 5000 { signal short; }
else { signal flat; }",
        // 12
        "lookback 40;
let mid = sma(20);
let band = (highest(20) - lowest(20)) / 2;
if close(0) > mid + band { signal short; }
else if close(0) < mid - band { signal long; }
else if position() != 0 && rsi(14) > 4000 && rsi(14) < 6000 { signal flat; }",
        // 13
        "lookback 30;
if volume(0) > volume(1) && close(0) > sma(20) && rsi(10) > 5000 { signal long; }
else if volume(0) > volume(1) && close(0) < sma(20) && rsi(10) < 5000 { signal short; }
else if rsi(10) > 8000 || rsi(10) < 2000 { signal flat; }",
        // 14
        "lookback 40;
if rsi(7) > 5000 && rsi(21) > 5000 && close(0) > sma(30) { signal long; }
else if rsi(7) < 5000 && rsi(21) < 5000 && close(0) < sma(30) { signal short; }
else { signal flat; }",
        // 15
        "lookback 55;
let hh = highest(20);
let ll = lowest(20);
let q = (hh - ll) / 4;
if close(0) > hh - q && ema(15) > ema(40) { signal long; }
else if close(0) < ll + q && ema(15) < ema(40) { signal short; }
else { signal flat; }",
        // 16
        "lookback 40;
var cool = 0;
if cool > 0 { cool = cool - 1; }
if cool == 0 && close(0) > sma(20) && rsi(14) > 5500 { signal long; cool = 6; }
else if cool == 0 && close(0) < sma(20) && rsi(14) < 4500 { signal short; cool = 6; }
else if position() == 1 && rsi(14) < 4500 { signal flat; }
else if position() == -1 && rsi(14) > 5500 { signal flat; }",
        // 17
        "lookback 50;
let vol = highest(30) - lowest(30);
if vol < 100 { signal flat; }
else if close(0) > sma(40) && (rsi(14) > 5500 || close(0) > close(20)) { signal long; }
else if close(0) < sma(40) && (rsi(14) < 4500 || close(0) < close(20)) { signal short; }
else { signal flat; }",
        // 18
        "lookback 60;
if ema(5) > ema(15) && ema(15) > ema(30) && rsi(10) > 5000 { signal long; }
else if ema(5) < ema(15) && ema(15) < ema(30) && rsi(10) < 5000 { signal short; }
else { signal flat; }",
        // 19
        "lookback 30;
let hi = highest(14);
let lo = lowest(14);
let rng = hi - lo;
if rng > 0 && (close(0) - lo) * 100 < rng * 20 { signal long; }
else if rng > 0 && (close(0) - lo) * 100 > rng * 80 { signal short; }
else if position() != 0 && (close(0) - lo) * 100 > rng * 40 && (close(0) - lo) * 100 < rng * 60 { signal flat; }",
        // 20
        "lookback 45;
if position() == 0 && sma(10) > sma(30) && rsi(14) > 5000 { signal long; }
else if position() == 0 && sma(10) < sma(30) && rsi(14) < 5000 { signal short; }
else if position() == 1 && close(0) > entry_price() + 100 && rsi(14) > 7000 { signal flat; }
else if position() == 1 && sma(10) < sma(30) { signal flat; }
else if position() == -1 && close(0) < entry_price() - 100 && rsi(14) < 3000 { signal flat; }
else if position() == -1 && sma(10) > sma(30) { signal flat; }",
        // 21
        "lookback 40;
let atr = highest(14) - lowest(14);
if close(0) > close(1) + atr / 6 && rsi(14) > 5000 { signal long; }
else if close(0) < close(1) - atr / 6 && rsi(14) < 5000 { signal short; }
else if position() != 0 && rsi(14) > 4500 && rsi(14) < 5500 { signal flat; }",
        // 22
        "lookback 50;
let sq = highest(10) - lowest(10);
let expand = highest(40) - lowest(40);
if sq * 3 < expand && rsi(14) > 5000 && close(0) > sma(20) { signal long; }
else if sq * 3 < expand && rsi(14) < 5000 && close(0) < sma(20) { signal short; }
else if sq * 2 > expand { signal flat; }",
    ]
}

// A wandering integer price series with trends + oscillation + vol regimes.
fn candles(n: usize) -> Vec<Candle> {
    let mut out = Vec::with_capacity(n);
    let mut price: i64 = 10_000;
    for i in 0..n {
        let i = i as i64;
        // multiple superimposed cycles via cheap integer triangle waves
        let t1 = tri(i, 40) * 60;
        let t2 = tri(i, 13) * 25;
        let trend = tri(i, 220) * 200;
        let vol = 20 + (tri(i, 90).abs()) * 3;
        price = 10_000 + trend + t1 + t2;
        let open = price - t2 / 2;
        let high = price.max(open) + vol;
        let low = price.min(open) - vol;
        let close = price;
        let volume = 1_000 + (tri(i, 17).abs()) * 40 + (i % 7) * 30;
        out.push(Candle { open, high, low, close, volume });
    }
    out
}

// triangle wave in [-period/2, period/2]-ish, integer
fn tri(i: i64, period: i64) -> i64 {
    let p = ((i % period) + period) % period;
    let half = period / 2;
    if p <= half {
        p - half / 2
    } else {
        half - p + half / 2
    }
}

fn main() {
    let progs = programs();
    let data = candles(500);
    let mut failures = 0;
    for (idx, src) in progs.iter().enumerate() {
        let n = idx + 1;
        let strat = match compile(src) {
            Ok(s) => s,
            Err(d) => {
                println!("PROG {n}: COMPILE ERROR: {}", d.message);
                failures += 1;
                continue;
            }
        };
        let mut sess = Session::new(&strat, Limits::default());
        let mut seen: BTreeSet<i64> = BTreeSet::new();
        let mut err = None;
        let mut pending: Option<(Signal, i64)> = None;
        for c in &data {
            // simulate fill at this bar's open from the previous signal
            if let Some((sig, px)) = pending.take() {
                sess.set_position(sig, px);
            }
            match sess.step(*c) {
                Ok(sig) => {
                    seen.insert(match sig {
                        Signal::Long => 1,
                        Signal::Flat => 0,
                        Signal::Short => -1,
                    });
                    pending = Some((sig, c.open));
                }
                Err(d) => {
                    err = Some(format!("{} (code {:?})", d.message, d.code));
                    break;
                }
            }
        }
        if let Some(e) = err {
            println!("PROG {n}: RUNTIME ERROR: {e}");
            failures += 1;
        } else if seen.len() < 2 {
            println!("PROG {n}: DOES NOT TRADE (signals seen: {:?})", seen);
            failures += 1;
        } else {
            println!("PROG {n}: OK  distinct-signals={:?}  maxfuel={}", seen, sess.max_fuel_per_bar());
        }
    }
    println!("---\n{} program(s), {} failure(s)", progs.len(), failures);
}

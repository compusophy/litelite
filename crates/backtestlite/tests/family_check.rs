use backtestlite::{Candle, Costs, Gate, Limits, verify};

fn series(n: usize) -> Vec<Candle> {
    (0..n as i64)
        .map(|i| {
            let m = i % 46;
            let t = if m < 23 { m } else { 46 - m };
            let cycle = t * 30; // triangle, amplitude ~690
            let drift = 3 * i;
            let mid = 10_000 + drift + cycle;
            let open = mid;
            let close = mid + ((i % 7) - 3) * 12;
            let hi = open.max(close) + 50;
            let lo = open.min(close) - 50;
            Candle {
                open,
                high: hi,
                low: lo,
                close,
                volume: 100 + i % 9,
            }
        })
        .collect()
}

const PROGRAMS: &[&str] = &[
    // 1
    "lookback 20;
     var peak = 0;
     if position() == 0 {
       if close(0) > sma(20) { signal long; peak = close(0); }
     } else {
       if close(0) > peak { peak = close(0); }
       if close(0) < peak - 100 { signal flat; peak = 0; }
     }",
    // 2
    "lookback 10;
     var held = 0;
     if position() == 0 {
       held = 0;
       if rsi(10) < 3500 { signal long; }
     } else {
       held = held + 1;
       if held >= 8 { signal flat; }
       if rsi(10) > 6500 { signal flat; }
     }",
    // 3
    "lookback 14;
     var cooldown = 0;
     if cooldown > 0 { cooldown = cooldown - 1; }
     if position() == 0 && cooldown == 0 && rsi(14) < 3000 { signal long; }
     if position() == 1 && rsi(14) > 7000 { signal flat; cooldown = 10; }",
    // 4
    "lookback 16;
     var peak = 0;
     if position() == 1 {
       if high(0) > peak { peak = high(0); }
       if close(0) < peak - 150 { signal flat; peak = 0; }
       if close(0) < entry_price() - 80 { signal flat; peak = 0; }
     } else {
       if sma(4) > sma(16) { signal long; peak = high(0); }
     }",
    // 5
    "lookback 20;
     var trough = 0;
     if position() == 0 {
       if close(0) < sma(20) { signal short; trough = close(0); }
     } else {
       if close(0) < trough { trough = close(0); }
       if close(0) > trough + 120 { signal flat; trough = 0; }
     }",
    // 6
    "lookback 30;
     var held = 0;
     if position() == 0 {
       held = 0;
       if sma(5) > sma(30) { signal long; }
       else if sma(5) < sma(30) { signal short; }
     } else {
       held = held + 1;
       if held >= 12 { signal flat; }
     }",
    // 7
    "lookback 20;
     var stop = 0;
     if position() == 0 {
       let mid = (highest(20) + lowest(20)) / 2;
       if close(0) > mid { signal long; stop = close(0) - 150; }
     } else {
       if close(0) - 150 > stop { stop = close(0) - 150; }
       if close(0) < stop { signal flat; stop = 0; }
     }",
    // 8
    "lookback 12;
     var n = 0;
     if position() == 0 {
       if ema(6) > ema(12) { signal long; }
     } else {
       n = n + 1;
       if close(0) >= entry_price() + 200 { signal flat; }
       if close(0) <= entry_price() - 100 { signal flat; }
     }",
    // 9
    "lookback 5;
     var ups = 0;
     if close(0) > close(1) { ups = ups + 1; } else { ups = 0; }
     if ups >= 3 { signal long; }
     if position() == 1 && close(0) < close(1) { signal flat; }",
    // 10
    "lookback 14;
     var cd = 0;
     if cd > 0 { cd = cd - 1; }
     if position() == 0 && cd == 0 {
       if rsi(14) < 3000 { signal long; }
       else if rsi(14) > 7000 { signal short; }
     }
     if position() == 1 && rsi(14) > 5000 { signal flat; cd = 6; }
     if position() == -1 && rsi(14) < 5000 { signal flat; cd = 6; }",
    // 11
    "lookback 25;
     var peak = 0;
     if position() == 1 {
       if close(0) > peak { peak = close(0); }
       if close(0) < peak - peak / 50 { signal flat; peak = 0; }
     } else {
       if sma(10) > sma(25) { signal long; peak = close(0); }
     }",
    // 12
    "lookback 16;
     var held = 0;
     var cd = 0;
     if cd > 0 { cd = cd - 1; }
     if position() == 0 {
       held = 0;
       if cd == 0 && close(0) > sma(16) { signal long; }
     } else {
       held = held + 1;
       if held >= 10 { signal flat; cd = 5; }
     }",
    // 13
    "lookback 18;
     var z = 0;
     if position() == 0 {
       z = 0;
       if close(0) < sma(18) { signal short; }
     } else {
       z = z + 1;
       if close(0) <= entry_price() - 150 { signal flat; }
       if close(0) >= entry_price() + 90 { signal flat; }
     }",
    // 14
    "lookback 5;
     var i = 0;
     var acc = 0;
     acc = 0;
     i = 0;
     repeat 5 { acc = acc + close(i); i = i + 1; }
     let avg = acc / 5;
     if close(0) > avg { signal long; } else { signal flat; }",
    // 15
    "lookback 22;
     var peak = 0;
     if position() == 0 {
       if sma(6) > sma(22) { signal long; peak = close(0); }
     } else {
       if close(0) > peak { peak = close(0); }
       let gain = close(0) - entry_price();
       if gain > 300 {
         if close(0) < peak - 60 { signal flat; peak = 0; }
       } else {
         if close(0) < peak - 160 { signal flat; peak = 0; }
       }
     }",
    // 16
    "lookback 20;
     var held = 0;
     if position() == 0 {
       held = 0;
       let mid = (highest(10) + lowest(10)) / 2;
       if close(0) > mid { signal long; }
     } else {
       held = held + 1;
       if held >= 7 { signal flat; }
       if close(0) < entry_price() - 150 { signal flat; }
     }",
    // 17
    "lookback 40;
     var trough = 0;
     if position() == 0 {
       if ema(10) < ema(40) { signal short; trough = low(0); }
     } else {
       if low(0) < trough { trough = low(0); }
       if close(0) > trough + 180 { signal flat; trough = 0; }
       if close(0) > entry_price() + 60 { signal flat; trough = 0; }
     }",
    // 18
    "lookback 12;
     var held = 0;
     var cd = 0;
     if cd > 0 { cd = cd - 1; }
     if position() == 0 && cd == 0 {
       held = 0;
       if rsi(12) < 4000 { signal long; }
     } else if position() == 1 {
       held = held + 1;
       if held >= 6 || close(0) < entry_price() - 120 { signal flat; cd = 4; }
     }",
    // 19
    "lookback 26;
     var peak = 0;
     var armed = false;
     if position() == 0 {
       armed = false;
       if close(0) > sma(26) && close(0) > close(1) { signal long; peak = close(0); }
     } else {
       if close(0) > peak { peak = close(0); }
       if close(0) > entry_price() + 250 { armed = true; }
       if armed && close(0) < peak - 70 { signal flat; peak = 0; }
       if close(0) < entry_price() - 220 { signal flat; peak = 0; }
     }",
    // 20
    "lookback 8;
     var downs = 0;
     if close(0) < close(1) { downs = downs + 1; } else { downs = 0; }
     if position() == 0 && downs >= 3 { signal long; }
     if position() == 1 && close(0) > entry_price() + 100 { signal flat; }
     if position() == 1 && close(0) < entry_price() - 100 { signal flat; }",
    // 21
    "lookback 30;
     var held = 0;
     if position() == 0 {
       held = 0;
       if sma(8) > sma(30) { signal long; }
       else if sma(8) < sma(30) { signal short; }
     } else {
       held = held + 1;
       let profit = position() * (close(0) - entry_price());
       if profit > 200 { signal flat; }
       if held >= 20 { signal flat; }
     }",
    // 22
    "lookback 15;
     var cd = 0;
     var peak = 0;
     if cd > 0 { cd = cd - 1; }
     if position() == 0 {
       if cd == 0 && rsi(15) > 5000 && close(0) > sma(15) { signal long; peak = close(0); }
     } else {
       if close(0) > peak { peak = close(0); }
       if close(0) < peak - 130 { signal flat; peak = 0; cd = 8; }
     }",
];

#[test]
fn family_all_verify() {
    let candles = series(600);
    let mut failures = Vec::new();
    for (i, src) in PROGRAMS.iter().enumerate() {
        match verify(
            src,
            &candles,
            Limits::default(),
            Costs { fee_ticks: 2, slippage_ticks: 1 },
            Gate::default(),
        ) {
            Ok((_, r)) => {
                println!(
                    "#{:02} OK  trades={} bars_eval={} net={} fuel={}",
                    i + 1,
                    r.trades,
                    r.bars_evaluated,
                    r.net_pnl_ticks,
                    r.max_fuel_per_bar
                );
            }
            Err(e) => {
                println!("#{:02} FAIL {}", i + 1, e);
                failures.push(i + 1);
            }
        }
    }
    assert!(failures.is_empty(), "failed programs: {:?}", failures);
}

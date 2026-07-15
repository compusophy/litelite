//! # backtestlite — the stratlite VERIFIER
//!
//! The other half of paper §5's generate→verify→keep loop: a deterministic,
//! all-integer backtest engine over [`stratlite::Session`], and [`verify`] —
//! the one-call predicate that makes "this strategy is real" mechanical:
//! it compiles, every bar's decision halted within its fuel, no runtime or
//! data fault occurred anywhere in the series, and the behavior clears the
//! activity [`Gate`] (do-nothing strategies are rejects, not survivors).
//!
//! Split from `stratlite` at the constitution's per-crate cap — and the seam
//! is real: the language crate is the trader agent's live dependency (it
//! steps a `Session` against a real feed); THIS crate is the selection
//! loop's dependency. One evaluation path serves both, so verified means
//! deployable.
//!
//! Fill model, complete: the target decided on bar `t`'s close executes at
//! bar `t+1`'s open; every executed order (a close or an open — a flip is
//! two) fills at `open ± slippage_ticks` ADVERSE to the order and charges
//! `fee_ticks`; no intrabar fills, no limit/stop orders, no partial fills.
//! Acting at the NEXT open kills the "trade the close you just observed"
//! bias by mechanism. A position still open after the final bar force-closes
//! at that bar's close (costs applied, trade counted).
//!
//! Everything is integer ticks: accounting runs in i128 and checks back to
//! i64 (`E0302`), [`Report`] derives `Eq`, and
//! [`equity_hash`](Report::equity_hash) (FNV-1a-64 of the equity curve, via
//! caplite) is the one publishable number per (strategy, data, costs)
//! triple. Candles are validated up front (`E0301`) with prices capped at
//! 2^53 ([`MAX_TICKS`]), so honest strategy arithmetic on prices provably
//! has i64 headroom.
//!
//! Codes here are the DATA band `E03xx` (spanless — they fault the candles,
//! not the source); the language bands live in `stratlite::codes`. Gate
//! failures are structured ([`Reject::Gate`]), not diags.
//!
//! ```
//! use backtestlite::{Costs, Gate, verify};
//! use stratlite::{Candle, Limits};
//!
//! // A sawtooth market with drift — enough structure that crossovers trade.
//! let candles: Vec<Candle> = (0..128i64)
//!     .map(|i| {
//!         let p = 10_000 + 7 * i + 300 * ((i % 16) - 8).abs();
//!         Candle { open: p, high: p + 40, low: p - 40, close: p + 15, volume: 1 }
//!     })
//!     .collect();
//! let src = "
//!     lookback 16;
//!     if sma(4) > sma(16) { signal long; }
//!     else { signal flat; }
//! ";
//! let (strategy, report) =
//!     verify(src, &candles, Limits::default(), Costs::default(), Gate::default()).unwrap();
//! assert_eq!(strategy.lookback(), 16);
//! // The whole backtest as one reproducible number:
//! let again = backtestlite::backtest(&strategy, &candles, Limits::default(), Costs::default());
//! assert_eq!(again.unwrap().equity_hash, report.equity_hash);
//! ```

use diaglite::Diag;
use stratlite::{Candle, Limits, Strategy, compile};

mod engine;

/// Price/volume cap (2^53): honest strategy arithmetic on validated prices
/// provably has i64 headroom, and full-lookback window sums fit i128.
pub const MAX_TICKS: i64 = 1 << 53;

/// The two-knob cost model: a per-order fee and adverse slippage, in ticks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Costs {
    pub fee_ticks: i64,
    pub slippage_ticks: i64,
}

/// The degenerate-strategy gate: verification demands real activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Gate {
    pub min_trades: u32,
    pub min_bars_evaluated: u32,
}

impl Default for Gate {
    fn default() -> Self {
        Gate {
            min_trades: 4,
            min_bars_evaluated: 16,
        }
    }
}

/// What a backtest measured. All integers — `Eq` IS the determinism claim,
/// and [`equity_hash`](Report::equity_hash) is the one-number version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub bars_seen: usize,
    /// Bars the body actually evaluated (seen minus warmup).
    pub bars_evaluated: usize,
    /// Round trips, the forced final close included.
    pub trades: u32,
    /// Trades with positive pnl AFTER costs.
    pub wins: u32,
    /// Realized pnl in ticks, all fees and slippage included.
    pub net_pnl_ticks: i64,
    pub gross_profit_ticks: i64,
    /// Sum of losing trades' magnitudes (≥ 0).
    pub gross_loss_ticks: i64,
    /// Peak-to-trough of the per-bar equity curve (≥ 0).
    pub max_drawdown_ticks: i64,
    /// Exposure: bars holding a position at the close.
    pub bars_in_market: u32,
    /// The worst observed bar — the strategy's real per-decision cost.
    pub max_fuel_per_bar: u64,
    /// FNV-1a-64 of the equity curve (per-bar i64 marks, little-endian).
    pub equity_hash: u64,
}

/// Why [`verify`] rejected — the three-arm histogram the selection loop
/// keeps: the source is bad, the run faulted, or the behavior is degenerate.
#[derive(Debug, Clone)]
pub enum Reject {
    /// Lex/parse/static failure — coded, spanned (`stratlite::codes`).
    Compile(Diag),
    /// The backtest faulted: a runtime diag (E02xx) or a data fault (E03xx).
    Run(Diag),
    /// It ran clean but did not behave like a strategy.
    Gate(GateFail),
}

impl std::fmt::Display for Reject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Reject::Compile(d) => write!(f, "compile: {d}"),
            Reject::Run(d) => write!(f, "run: {d}"),
            Reject::Gate(g) => write!(f, "gate: {g}"),
        }
    }
}

/// A failed activity gate, structured for feedback loops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateFail {
    /// Which gate: `"trades"` or `"bars evaluated"`.
    pub what: &'static str,
    pub got: u32,
    pub min: u32,
}

impl std::fmt::Display for GateFail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {} < the required {}", self.what, self.got, self.min)
    }
}

/// The DATA band: these fault the candles (or the accounting they force),
/// not the source, so they are spanless — the bar index rides the message.
pub mod codes {
    /// A candle failed validation (incoherent OHLC, out of range).
    pub const BAD_CANDLE: u16 = 301;
    /// Accounting left the i64 tick range.
    pub const ACCOUNT_OVERFLOW: u16 = 302;
    /// Fewer candles than the lookback needs to warm up and evaluate.
    pub const SHORT_DATA: u16 = 303;
    /// Costs outside 0..=2^53 ticks (negative slippage would silently invert
    /// the ADVERSE guarantee; huge values would overflow fill arithmetic).
    pub const BAD_COSTS: u16 = 304;
}

/// Deterministically backtest `strategy` over `candles` (see the crate docs
/// for the fill model). `Err` is a runtime diag (E02xx) or data fault (E03xx).
pub fn backtest(
    strategy: &Strategy,
    candles: &[Candle],
    limits: Limits,
    costs: Costs,
) -> Result<Report, Diag> {
    engine::backtest(strategy, candles, limits, costs)
}

/// THE verification predicate, one call. `Ok` is a survivor plus its
/// in-sample evidence; the harness re-scores survivors on held-out candles
/// with [`backtest`].
pub fn verify(
    src: &str,
    candles: &[Candle],
    limits: Limits,
    costs: Costs,
    gate: Gate,
) -> Result<(Strategy, Report), Reject> {
    let strategy = compile(src).map_err(Reject::Compile)?;
    let report = backtest(&strategy, candles, limits, costs).map_err(Reject::Run)?;
    if report.trades < gate.min_trades {
        return Err(Reject::Gate(GateFail {
            what: "trades",
            got: report.trades,
            min: gate.min_trades,
        }));
    }
    if (report.bars_evaluated as u64) < u64::from(gate.min_bars_evaluated) {
        return Err(Reject::Gate(GateFail {
            what: "bars evaluated",
            got: report.bars_evaluated.min(u32::MAX as usize) as u32,
            min: gate.min_bars_evaluated,
        }));
    }
    Ok((strategy, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use stratlite::Signal;

    fn series(n: usize) -> Vec<Candle> {
        (0..n as i64)
            .map(|i| {
                let base = 10_000 + 7 * i + 300 * ((i % 16) - 8).abs();
                Candle {
                    open: base,
                    high: base + 40,
                    low: base - 40,
                    close: base + 15,
                    volume: 100 + i % 7,
                }
            })
            .collect()
    }

    fn run(src: &str, candles: &[Candle]) -> Result<Report, Diag> {
        backtest(
            &compile(src).unwrap(),
            candles,
            Limits::default(),
            Costs::default(),
        )
    }

    const CROSSOVER: &str = "
        lookback 16;
        let fast = sma(4);
        let slow = sma(16);
        if fast > slow { signal long; }
        else if fast < slow { signal short; }
    ";

    #[test]
    fn the_pipeline_verifies_gates_and_histograms() {
        let candles = series(256);
        let (s, r) = verify(
            CROSSOVER,
            &candles,
            Limits::default(),
            Costs {
                fee_ticks: 2,
                slippage_ticks: 1,
            },
            Gate::default(),
        )
        .unwrap();
        assert_eq!(s.lookback(), 16);
        assert!(r.trades >= 4, "{r:?}");
        assert_eq!(r.bars_evaluated, 256 - 16);
        // The three reject arms are distinguishable — the histogram the
        // selection loop keeps.
        let e = verify(
            "signal",
            &candles,
            Limits::default(),
            Costs::default(),
            Gate::default(),
        );
        assert!(matches!(e, Err(Reject::Compile(_))));
        let e = verify(
            "let x = 1 / 0;",
            &candles,
            Limits::default(),
            Costs::default(),
            Gate::default(),
        );
        assert!(matches!(e, Err(Reject::Run(_))));
        let e = verify(
            "signal flat;",
            &candles,
            Limits::default(),
            Costs::default(),
            Gate::default(),
        )
        .unwrap_err();
        assert!(
            matches!(e, Reject::Gate(GateFail { what: "trades", .. })),
            "{e}"
        );
    }

    #[test]
    fn determinism_is_exact_and_hashable() {
        let candles = series(200);
        let a = run(CROSSOVER, &candles).unwrap();
        let b = run(CROSSOVER, &candles).unwrap();
        assert_eq!(a, b); // Report derives Eq — this IS the claim
        assert_eq!(a.equity_hash, b.equity_hash);
        // Different costs → different curve → different hash.
        let c = backtest(
            &compile(CROSSOVER).unwrap(),
            &candles,
            Limits::default(),
            Costs {
                fee_ticks: 5,
                slippage_ticks: 2,
            },
        )
        .unwrap();
        assert_ne!(a.equity_hash, c.equity_hash);
    }

    #[test]
    fn fills_happen_at_the_next_open_with_adverse_costs() {
        // One long signal on the first evaluated bar, held to the end:
        // entry at bar 1's open (+slip), forced exit at the final close
        // (-slip), 2 fees.
        let candles: Vec<Candle> = (0..4)
            .map(|i| Candle {
                open: 1000 + i,
                high: 1100,
                low: 900,
                close: 1050,
                volume: 1,
            })
            .collect();
        let r = run("signal long;", &candles).unwrap();
        assert_eq!(r.trades, 1);
        // entry = 1001 (bar 1 open), exit = 1050 (final close), no costs.
        assert_eq!(r.net_pnl_ticks, 49);
        let with_costs = backtest(
            &compile("signal long;").unwrap(),
            &candles,
            Limits::default(),
            Costs {
                fee_ticks: 3,
                slippage_ticks: 2,
            },
        )
        .unwrap();
        // entry 1003 (+slip), exit 1048 (-slip), fees 2×3: 45 - 6 = 39.
        assert_eq!(with_costs.net_pnl_ticks, 39);
        assert_eq!(with_costs.wins, 1);
        // Position state round-trips through probes: enter once, exit via
        // position(), stay flat after.
        let src = "
            var bars = 0;
            bars = bars + 1;
            if bars == 1 { signal long; }
            if position() == 1 && bars >= 3 { signal flat; }
        ";
        let r = run(src, &series(24)).unwrap();
        assert_eq!(r.trades, 1);
        assert!(r.bars_in_market >= 2);
        let _ = Signal::Flat; // the shared vocabulary type
    }

    #[test]
    fn data_faults_are_coded_and_spanless() {
        let s = compile("signal long;").unwrap();
        let bad = vec![Candle {
            open: 10,
            high: 5,
            low: 1,
            close: 8,
            volume: 1,
        }];
        let e = backtest(&s, &bad, Limits::default(), Costs::default()).unwrap_err();
        assert_eq!((e.code, e.span), (Some(codes::BAD_CANDLE), None));
        let s = compile("lookback 8; signal long;").unwrap();
        let e = backtest(&s, &series(8), Limits::default(), Costs::default()).unwrap_err();
        assert_eq!(e.code, Some(codes::SHORT_DATA));
        let huge = vec![Candle {
            open: MAX_TICKS + 1,
            high: MAX_TICKS + 2,
            low: 1,
            close: 2,
            volume: 1,
        }];
        let e = backtest(&s, &huge, Limits::default(), Costs::default()).unwrap_err();
        assert_eq!(e.code, Some(codes::BAD_CANDLE));
        // Costs are inputs too: negative slippage would invert ADVERSE and
        // huge values would overflow fill arithmetic — both are E0304.
        for costs in [
            Costs {
                fee_ticks: -1,
                slippage_ticks: 0,
            },
            Costs {
                fee_ticks: 0,
                slippage_ticks: i64::MAX,
            },
        ] {
            let e = backtest(&s, &series(24), Limits::default(), costs).unwrap_err();
            assert_eq!(e.code, Some(codes::BAD_COSTS));
        }
    }

    #[test]
    fn drawdown_is_of_the_final_curve_no_phantom_peak() {
        // Rising closes with a costly forced exit: the pre-force-close mark
        // must NOT count as a peak. Curve: [0, 99, 199, 99] → drawdown 100.
        let candles: Vec<Candle> = (0..4)
            .map(|i| Candle {
                open: 1000 + i,
                high: 2000,
                low: 900,
                close: 1000 + (i + 1) * 100,
                volume: 1,
            })
            .collect();
        let r = backtest(
            &compile("signal long;").unwrap(),
            &candles,
            Limits::default(),
            Costs {
                fee_ticks: 100,
                slippage_ticks: 0,
            },
        )
        .unwrap();
        assert_eq!(r.net_pnl_ticks, 199); // (1400 - 1001) - 2×100
        // Final curve [0, 99, 199, 199]: drawdown 0. The pre-force-close
        // mark (299) must NOT count as a peak — the old incremental pass
        // reported 100 here.
        assert_eq!(r.max_drawdown_ticks, 0);
    }

    #[test]
    fn faulted_bars_are_atomic_and_the_fault_recurs() {
        // Bars are atomic: a faulted bar's var writes vanish, so `n` sticks
        // at 2 and the `n == 3` fault RECURS on every later bar — exactly
        // the documented contract (a fault caused by persistent state does
        // not clear itself). The session stays steppable throughout.
        use stratlite::{Session, compile as sc};
        let s = sc("var n = 0;
             n = n + 1;
             if n == 3 { let x = 1 / 0; }")
        .unwrap();
        let mut session = Session::new(&s, Limits::default());
        let mut errs = 0;
        for c in &series(8) {
            match session.step(*c) {
                Ok(sig) => assert_eq!(sig, Signal::Flat),
                Err(e) => {
                    errs += 1;
                    assert_eq!(e.code, Some(stratlite::codes::DIV_BY_ZERO));
                }
            }
        }
        assert_eq!(errs, 6); // bars 3..=8 all fault; bars 1-2 ran clean
    }
}

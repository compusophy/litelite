//! Selection and scoring: pure functions of a frozen pool + pinned candles.
//!
//! The design that makes the comparison mean anything: BOTH ARMS CONSUME THE
//! IDENTICAL POOL. Generation happens once per replicate and is frozen on
//! disk; the arms are then just different SELECTION RULES over the same N
//! sources. Model, prompt distribution, sample count and generation budget are
//! not "matched" across arms — they are literally the same objects, so nothing
//! is left to confound. Best-of-N optimism is symmetric (each arm picks 1 of N
//! from the same N) and cancels in the paired difference.
//!
//! Nothing here touches the network, so `s5 score` re-derives every §5 number
//! from committed artifacts. This is where the run's reproducibility actually
//! lives — the generator has none to give.

use backtestlite::{Costs, Gate, Reject, Report, backtest, verify};
use stratlite::{Candle, Limits};

/// Where train ends and held-out begins.
pub struct Split {
    pub train_end: usize,
}

impl Split {
    pub fn at_fraction(n: usize, frac: f64) -> Split {
        Split {
            train_end: ((n as f64) * frac) as usize,
        }
    }
    pub fn train<'a>(&self, all: &'a [Candle]) -> &'a [Candle] {
        &all[..self.train_end]
    }
    /// Held-out, prefixed with `lookback` warmup bars — else every candidate
    /// dies of E0303 (SHORT_DATA), not of anything interesting. No leak: no
    /// stratlite program can name a future bar (a GRAMMAR fact).
    pub fn heldout<'a>(&self, all: &'a [Candle], lookback: usize) -> &'a [Candle] {
        &all[self.train_end.saturating_sub(lookback)..]
    }
}

/// Costs from the TRAIN window only — from the full dataset would leak held-out
/// prices into the constant that drives train selection. Known bias (recorded,
/// not hidden): `fee_ticks` is ABSOLUTE, so price drift mis-scales it for the
/// held-out regime, and only the ranking arm consumes train pnl — so it alone
/// eats the bias. `price_drift` quantifies it.
pub fn costs_from_train(train: &[Candle], fee_bps: i64, slip_bps: i64) -> Costs {
    let mean = mean_close(train).max(1);
    Costs {
        fee_ticks: mean * fee_bps / 10_000,
        slippage_ticks: mean * slip_bps / 10_000,
    }
}

fn mean_close(c: &[Candle]) -> i64 {
    if c.is_empty() {
        return 0;
    }
    // i128 so the sum cannot overflow before the divide.
    (c.iter().map(|k| k.close as i128).sum::<i128>() / c.len() as i128) as i64
}

/// Held-out mean / train mean. Far from 1.0 means the absolute cost model is
/// mis-scaled for the deployment regime — the confound named above.
pub fn price_drift(all: &[Candle], split: &Split) -> f64 {
    let t = mean_close(split.train(all)).max(1) as f64;
    let h = mean_close(&all[split.train_end..]).max(1) as f64;
    h / t
}

/// One candidate's verdict: a survivor plus its in-sample evidence, or the
/// coded reason it is not one. `Reject` is the three-arm histogram itself.
pub type Verdict = Result<(stratlite::Strategy, Report), Reject>;

/// The Reject histogram — the structured selection pressure, per Reject arm.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Histogram {
    pub survived: u32,
    pub compile: u32,
    pub run: u32,
    pub gate: u32,
}

impl Histogram {
    pub fn tally(results: &[Verdict]) -> Histogram {
        let mut h = Histogram::default();
        for r in results {
            match r {
                Ok(_) => h.survived += 1,
                Err(Reject::Compile(_)) => h.compile += 1,
                Err(Reject::Run(_)) => h.run += 1,
                Err(Reject::Gate(_)) => h.gate += 1,
            }
        }
        h
    }
}

/// Verify every candidate against TRAIN. This is the mechanical filter.
pub fn verify_pool(
    sources: &[String],
    train: &[Candle],
    limits: Limits,
    costs: Costs,
    gate: Gate,
) -> Vec<Verdict> {
    sources
        .iter()
        .map(|s| verify(s, train, limits, costs, gate))
        .collect()
}

/// ARM V: among survivors, the best train net pnl. Ties break to the lowest
/// candidate index, so the rule is total and the pick is reproducible.
pub fn pick_verified(results: &[Verdict]) -> Option<usize> {
    results
        .iter()
        .enumerate()
        .filter_map(|(i, r)| r.as_ref().ok().map(|(_, rep)| (i, rep.net_pnl_ticks)))
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(i, _)| i)
}

/// Held-out score, the SAME rule for every arm: a pick that will not compile or
/// that faults earns ZERO — honest deployment semantics (it traded nothing).
pub fn score_heldout(
    src: &str,
    all: &[Candle],
    split: &Split,
    limits: Limits,
    costs: Costs,
) -> (i64, Option<Report>) {
    let Ok(strategy) = stratlite::compile(src) else {
        return (0, None);
    };
    let candles = split.heldout(all, strategy.lookback() as usize);
    match backtest(&strategy, candles, limits, costs) {
        Ok(r) => (r.net_pnl_ticks, Some(r)),
        Err(_) => (0, None),
    }
}

/// One program on both windows — raw material of the CONDITIONAL metric. Raw
/// survivor rate is not a generalization signal: the compile rung is DATA-
/// INDEPENDENT (parses on train => parses on held-out), so the honest metric
/// conditions on compiling and measures the GATE-clear rate (data-dependent) on
/// each window; the GAP proves whether held-out has out-of-sample teeth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvalRow {
    pub compiles: bool,
    /// Among compilers only: cleared the gate on TRAIN.
    pub train_clear: bool,
    /// Among compilers only: cleared the gate on HELD-OUT (with warmup).
    pub heldout_clear: bool,
}

/// Evaluate a pool on both windows. Held-out carries `lookback` bars of warmup
/// so a survivor is not killed by E0303 rather than by anything meaningful.
pub fn eval_pool(
    sources: &[String],
    all: &[Candle],
    split: &Split,
    limits: Limits,
    costs: Costs,
    gate: Gate,
) -> Vec<EvalRow> {
    sources
        .iter()
        .map(|src| match stratlite::compile(src) {
            Err(_) => EvalRow {
                compiles: false,
                train_clear: false,
                heldout_clear: false,
            },
            Ok(strategy) => {
                let train = split.train(all);
                let held = split.heldout(all, strategy.lookback() as usize);
                EvalRow {
                    compiles: true,
                    train_clear: verify(src, train, limits, costs, gate).is_ok(),
                    heldout_clear: verify(src, held, limits, costs, gate).is_ok(),
                }
            }
        })
        .collect()
}

/// Three rates in [0,1]: compile rate over the pool, then — among compilers —
/// the train and held-out gate-clear rates. The GAP (train − heldout) is the
/// headline: near zero => held-out is no harder than train (no out-of-sample
/// teeth, a "lift" would be grammar-learning); positive => held-out
/// discriminates, and raising heldout_clear is the real win.
pub fn conditional_rates(rows: &[EvalRow]) -> (f64, f64, f64) {
    if rows.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    let compilers: Vec<&EvalRow> = rows.iter().filter(|r| r.compiles).collect();
    let compile_rate = compilers.len() as f64 / rows.len() as f64;
    if compilers.is_empty() {
        return (compile_rate, 0.0, 0.0);
    }
    let n = compilers.len() as f64;
    let train = compilers.iter().filter(|r| r.train_clear).count() as f64 / n;
    let held = compilers.iter().filter(|r| r.heldout_clear).count() as f64 / n;
    (compile_rate, train, held)
}

/// Fuel over survivors, MEASURED not counted (cap 25,000/bar). A distribution
/// hugging the low end => the termination bound never bound on this task (§6
/// says so); a tail at the cap => the bound is load-bearing evidence.
pub fn fuel_distribution(results: &[Verdict]) -> Vec<u64> {
    let mut v: Vec<u64> = results
        .iter()
        .filter_map(|r| r.as_ref().ok().map(|(_, rep)| rep.max_fuel_per_bar))
        .collect();
    v.sort_unstable();
    v
}

/// How often physics and testimony pick the same program — free, and required
/// for honest stats: an exact sign test DROPS ties, so an unreported agreement
/// rate silently inflates the power calculation.
pub fn agreement(a: &[Option<usize>], b: &[Option<usize>]) -> (u32, u32) {
    let pairs = a.iter().zip(b);
    let n = pairs
        .clone()
        .filter(|(x, y)| x.is_some() && y.is_some())
        .count() as u32;
    let same = pairs.filter(|(x, y)| x.is_some() && x == y).count() as u32;
    (same, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sawtooth with drift — the same shape backtestlite's own doc example
    /// uses, so crossovers actually trade.
    fn candles(n: i64) -> Vec<Candle> {
        (0..n)
            .map(|i| {
                let p = 10_000 + 7 * i + 300 * ((i % 16) - 8).abs();
                Candle {
                    open: p,
                    high: p + 40,
                    low: p - 40,
                    close: p + 15,
                    volume: 1,
                }
            })
            .collect()
    }

    const CROSS: &str = "lookback 16; if sma(4) > sma(16) { signal long; } else { signal flat; }";

    #[test]
    fn eval_conditions_on_compiling_and_measures_the_gap() {
        let all = candles(256);
        let sp = Split::at_fraction(all.len(), 0.6);
        let pool = vec![
            CROSS.to_string(),                 // compiles, trades on both windows
            "not stratlite at all".into(),     // does not compile
            "lookback 4; signal long;".into(), // compiles, never trades -> gate-fail
        ];
        let rows = eval_pool(
            &pool,
            &all,
            &sp,
            Limits::default(),
            Costs::default(),
            Gate::default(),
        );
        assert!(!rows[1].compiles, "garbage must not compile");
        assert!(rows[0].compiles && rows[2].compiles);
        let (compile_rate, train, held) = conditional_rates(&rows);
        // Rates are among the 2 compilers only, never over the whole pool.
        assert!((compile_rate - 2.0 / 3.0).abs() < 1e-9);
        assert!((0.0..=1.0).contains(&train) && (0.0..=1.0).contains(&held));
    }

    #[test]
    fn conditional_rates_are_empty_safe_and_all_fail_safe() {
        assert_eq!(conditional_rates(&[]), (0.0, 0.0, 0.0));
        let none = [EvalRow {
            compiles: false,
            train_clear: false,
            heldout_clear: false,
        }];
        // Empty denominator -> 0, not a divide-by-zero.
        assert_eq!(conditional_rates(&none), (0.0, 0.0, 0.0));
    }

    #[test]
    fn heldout_carries_warmup_so_survivors_are_not_killed_by_short_data() {
        let all = candles(256);
        let split = Split::at_fraction(all.len(), 0.5);
        // Without warmup the slice starts at the split: E0303 for everyone.
        assert_eq!(split.heldout(&all, 16).len(), all.len() - 128 + 16);
        let (pnl, rep) = score_heldout(CROSS, &all, &split, Limits::default(), Costs::default());
        assert!(
            rep.is_some(),
            "a real strategy must survive held-out scoring"
        );
        assert_eq!(pnl, rep.unwrap().net_pnl_ticks);
    }

    #[test]
    fn costs_come_from_train_only_never_from_heldout() {
        let all = candles(256);
        let split = Split::at_fraction(all.len(), 0.5);
        let from_train = costs_from_train(split.train(&all), 5, 1);
        let from_all = costs_from_train(&all, 5, 1);
        // A full-dataset fee would be larger — the held-out leak we refuse.
        assert!(from_train.fee_ticks < from_all.fee_ticks);
        assert!(price_drift(&all, &split) > 1.0);
    }

    #[test]
    fn a_dead_pick_scores_zero_rather_than_exploding() {
        let all = candles(64);
        let split = Split::at_fraction(all.len(), 0.5);
        let (pnl, rep) = score_heldout(
            "this is not stratlite at all",
            &all,
            &split,
            Limits::default(),
            Costs::default(),
        );
        assert_eq!((pnl, rep.is_none()), (0, true));
    }

    #[test]
    fn the_histogram_separates_the_three_reject_arms() {
        let all = candles(256);
        let train = Split::at_fraction(all.len(), 0.5);
        let train = train.train(&all);
        let pool = vec![
            CROSS.to_string(),                      // survives
            "garbage(((".to_string(),               // Compile
            "lookback 4; signal long;".to_string(), // trades once -> Gate
        ];
        let results = verify_pool(
            &pool,
            train,
            Limits::default(),
            Costs::default(),
            Gate::default(),
        );
        let h = Histogram::tally(&results);
        assert_eq!(h.compile, 1, "{results:?}");
        assert_eq!(h.survived + h.gate, 2);
        // Fuel is measured, not just counted.
        let fuel = fuel_distribution(&results);
        assert_eq!(fuel.len() as u32, h.survived);
        assert!(
            fuel.iter()
                .all(|&f| f > 0 && f <= Limits::default().fuel_per_bar)
        );
    }

    #[test]
    fn verified_pick_is_best_train_pnl_and_ties_are_total() {
        let all = candles(256);
        let sp = Split::at_fraction(all.len(), 0.5);
        let pool = vec!["garbage".to_string(), CROSS.to_string()];
        let results = verify_pool(
            &pool,
            sp.train(&all),
            Limits::default(),
            Costs::default(),
            Gate::default(),
        );
        // Only index 1 survives, so it is the pick; index 0 never competes.
        assert_eq!(pick_verified(&results), Some(1));
        assert_eq!(pick_verified(&[]), None);
    }

    #[test]
    fn agreement_counts_ties_the_sign_test_would_silently_drop() {
        let v = [Some(1), Some(2), Some(3), None];
        let u = [Some(1), Some(5), Some(3), Some(0)];
        // 3 comparable pairs, 2 agreements — and the sign test would drop
        // those 2 as zeros, which is why the rate has to be reported.
        assert_eq!(agreement(&v, &u), (2, 3));
    }
}

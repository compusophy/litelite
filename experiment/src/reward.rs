//! The verifier AS a training reward — M6's core (rationale: `../M6.md`). A
//! fuel-bounded verifier is a reward oracle rustc cannot be: computing a
//! reward always terminates (no generated loop can hang training), and
//! totality removes reward-hacking-by-nontermination. The shape is the
//! `Reject` histogram as a curriculum; TRAIN PNL is NOT in `value` (rewarding
//! it teaches curve-fitting and makes the held-out result circular) but IS
//! emitted so a trainer can reshape and own that choice. Source is verified AS
//! GIVEN — a ```fenced``` program is a compile-zero, the correct signal.

use crate::score::Split;
use backtestlite::{Costs, Gate, Reject, verify};
use stratlite::{Candle, Limits};

/// The rung of the curriculum a candidate reached. Ordered worst to best.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Class {
    /// Did not parse / failed a static rule.
    Compile,
    /// Parsed, then faulted at runtime or on the data (E02xx/E03xx).
    Run,
    /// Ran clean but did not behave like a strategy (too few trades/bars).
    Gate,
    /// A valid, active strategy.
    Ok,
}

impl Class {
    pub fn as_str(self) -> &'static str {
        match self {
            Class::Compile => "compile",
            Class::Run => "run",
            Class::Gate => "gate",
            Class::Ok => "ok",
        }
    }
}

/// One candidate's reward: the scalar a trainer optimizes, plus the facts it
/// derives from — so a different reward policy is a trainer decision, not a
/// re-run of the oracle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reward {
    /// In [0, 1]. The validity ladder — no pnl (see module docs).
    pub value: f64,
    pub class: Class,
    /// `max_fuel_per_bar`; 0 if it never ran. Evidence for the fuel bound.
    pub fuel: u64,
    pub trades: u32,
    /// TRAIN net pnl — emitted, never in `value`.
    pub train_pnl: i64,
    /// `equity_hash`; 0 if it never ran.
    pub hash: u64,
}

/// An even four-rung ladder normalized so a valid strategy is full marks. Even
/// rungs because the climb IS the curriculum: parse, then run, then behave.
fn ladder(class: Class) -> f64 {
    match class {
        Class::Compile => 0.0,
        Class::Run => 1.0 / 3.0,
        Class::Gate => 2.0 / 3.0,
        Class::Ok => 1.0,
    }
}

impl Reward {
    /// A candidate that never became a strategy — every fact is zero, only the
    /// rung differs.
    fn miss(class: Class) -> Reward {
        Reward {
            value: ladder(class),
            class,
            fuel: 0,
            trades: 0,
            train_pnl: 0,
            hash: 0,
        }
    }
}

/// Reward one candidate against the TRAIN window. Deterministic and total:
/// the same source and candles always yield the same reward, and it always
/// returns.
pub fn reward(src: &str, train: &[Candle], limits: Limits, costs: Costs, gate: Gate) -> Reward {
    // Empty-rollout hack closed: an empty source compiles to a lookback-0 no-op
    // that gate-fails at 2/3, so unguarded "emit nothing" pays. It is the
    // ABSENCE of a program — a compile-class zero.
    if src.trim().is_empty() {
        return Reward::miss(Class::Compile);
    }
    match verify(src, train, limits, costs, gate) {
        Err(Reject::Compile(_)) => Reward::miss(Class::Compile),
        Err(Reject::Run(_)) => Reward::miss(Class::Run),
        Err(Reject::Gate(_)) => Reward::miss(Class::Gate),
        Ok((_, r)) => Reward {
            value: ladder(Class::Ok),
            class: Class::Ok,
            fuel: r.max_fuel_per_bar,
            trades: r.trades,
            train_pnl: r.net_pnl_ticks,
            hash: r.equity_hash,
        },
    }
}

/// Reward a whole batch of rollouts against one train window. This is the hot
/// path a trainer calls once per step; it is cheap precisely because verify is
/// fuel-bounded.
pub fn reward_pool(
    sources: &[String],
    all: &[Candle],
    split: &Split,
    limits: Limits,
    costs: Costs,
    gate: Gate,
) -> Vec<Reward> {
    let train = split.train(all);
    sources
        .iter()
        .map(|s| reward(s, train, limits, costs, gate))
        .collect()
}

/// A SOURCE-canonical dedup key (FNV-1a-64 over comment-stripped, whitespace-
/// collapsed source) for the trainer's anti-collapse filter. Source-canonical,
/// not `equity_hash`: the hash is 0 for every non-survivor (blind exactly where
/// training starts) and gameable above it (a one-tick constant change is a new
/// curve). Honest limit: catches comment/format clones, not constant-
/// perturbation or semantic ones — AST canonicalization is the refinement.
pub fn novelty_key(src: &str) -> u64 {
    let canon = strip_and_collapse(src);
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in canon.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Strip `//` and `/* */` comments (non-nested) and collapse whitespace runs.
/// stratlite has no string literals, so nothing a comment strip could wrongly
/// eat.
fn strip_and_collapse(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(b.len());
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn the_ladder_orders_the_curriculum() {
        // The strict ordering IS the training signal.
        assert!(ladder(Class::Ok) > ladder(Class::Gate));
        assert!(ladder(Class::Gate) > ladder(Class::Run));
        assert!(ladder(Class::Run) > ladder(Class::Compile));
        assert_eq!(ladder(Class::Compile), 0.0);
        assert_eq!(ladder(Class::Ok), 1.0);
    }

    #[test]
    fn a_real_strategy_gets_full_marks_and_carries_its_facts() {
        let all = candles(256);
        let sp = Split::at_fraction(all.len(), 0.5);
        let r = reward(
            CROSS,
            sp.train(&all),
            Limits::default(),
            Costs::default(),
            Gate::default(),
        );
        assert_eq!(r.class, Class::Ok);
        assert_eq!(r.value, 1.0);
        // The reshape facts are present, and fuel is real (nonzero, under cap).
        assert!(r.trades > 0);
        assert!(0 < r.fuel && r.fuel <= Limits::default().fuel_per_bar);
        assert_ne!(r.hash, 0);
    }

    #[test]
    fn train_pnl_is_emitted_but_never_in_the_value() {
        let all = candles(256);
        let sp = Split::at_fraction(all.len(), 0.5);
        let r = reward(
            CROSS,
            sp.train(&all),
            Limits::default(),
            Costs::default(),
            Gate::default(),
        );
        // Value depends ONLY on class — the guard against curve-fitting.
        assert_eq!(r.value, ladder(r.class));
        // The pnl is still there for a trainer that wants to reshape and own it.
        let _ = r.train_pnl;
    }

    #[test]
    fn garbage_is_a_compile_zero_not_a_leniency() {
        let all = candles(64);
        let sp = Split::at_fraction(all.len(), 0.5);
        // Fenced prose is what a raw model emits — it must score zero.
        let r = reward(
            "```\nlookback 4;\n```",
            sp.train(&all),
            Limits::default(),
            Costs::default(),
            Gate::default(),
        );
        assert_eq!((r.class, r.value, r.fuel), (Class::Compile, 0.0, 0));
    }

    #[test]
    fn the_empty_rollout_hack_is_closed() {
        let all = candles(64);
        let sp = Split::at_fraction(all.len(), 0.5);
        // All compile to a lookback-0 no-op that gate-fails at 2/3 unguarded.
        for empty in ["", "   ", "\n\t\n", "  \r\n  "] {
            let r = reward(
                empty,
                sp.train(&all),
                Limits::default(),
                Costs::default(),
                Gate::default(),
            );
            assert_eq!(
                (r.class, r.value),
                (Class::Compile, 0.0),
                "{empty:?} must score zero"
            );
        }
    }

    #[test]
    fn novelty_key_ignores_comments_and_whitespace_but_not_logic() {
        // The cheapest way to fake diversity is to re-comment or re-format one
        // template. The key must see through both.
        let a = "lookback 4; signal long;";
        let b = "lookback 4;   signal long;   // a comment";
        let c = "lookback 4;\n/* block */ signal long;";
        assert_eq!(novelty_key(a), novelty_key(b));
        assert_eq!(novelty_key(a), novelty_key(c));
        // But a real logic change is a different program.
        assert_ne!(novelty_key(a), novelty_key("lookback 4; signal short;"));
    }

    #[test]
    fn novelty_key_works_below_the_survivor_rung() {
        // Keys a program that never runs, where equity_hash is 0 for all.
        let x = novelty_key("garbage one");
        let y = novelty_key("garbage two");
        let x2 = novelty_key("garbage    one   // note");
        assert_ne!(x, y);
        assert_eq!(x, x2);
    }

    #[test]
    fn reward_is_deterministic_and_total_over_a_batch() {
        let all = candles(256);
        let sp = Split::at_fraction(all.len(), 0.5);
        let pool = vec![
            CROSS.to_string(),
            "not a program".to_string(),
            "lookback 4; signal long;".to_string(),
        ];
        let a = reward_pool(
            &pool,
            &all,
            &sp,
            Limits::default(),
            Costs::default(),
            Gate::default(),
        );
        let b = reward_pool(
            &pool,
            &all,
            &sp,
            Limits::default(),
            Costs::default(),
            Gate::default(),
        );
        // Same inputs, same rewards — the reward reproduces though the model can't.
        assert_eq!(a, b);
        assert_eq!(a.len(), 3);
        assert_eq!(a[1].class, Class::Compile);
    }
}

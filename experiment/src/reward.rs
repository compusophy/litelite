//! The verifier AS a training reward — M6's core, and the boundary a trainer
//! shells out to.
//!
//! M6's whole thesis is one function: a purpose-sized language hands you a
//! dense, deterministic, un-gameable reward for free. Two properties the kit
//! guarantees make it a reward oracle a general language cannot be:
//!
//!   * `verify()` is FUEL-BOUNDED, so computing a rollout's reward is itself
//!     guaranteed to terminate. The oracle can never hang the training loop,
//!     however hostile the program. (Reward a model against rustc-plus-run and
//!     a generated infinite loop stalls the loop; here it cannot exist.)
//!   * TOTALITY closes a reward-hacking door: you cannot farm reward by
//!     emitting a program that loops forever, because nothing loops forever.
//!
//! The shape is the `Reject` histogram read as a CURRICULUM — parsing at all
//! is worth something, running clean within fuel more, behaving like a
//! strategy most. Crucially, TRAIN PNL is deliberately NOT in `value`:
//! rewarding it would teach curve-fitting and make the held-out generalization
//! result circular (you would optimize the very quantity you then measure).
//! The facts (pnl, fuel, hash) are still EMITTED, so a trainer that wants a
//! different shaping can compute it from the record and own that choice; the
//! default rewards VALIDITY and leaves generalization for the experiment to
//! measure.
//!
//! The source is verified AS GIVEN — no fence-stripping, no leniency. A model
//! that emits ```prose fences``` gets a compile-class zero, which is the
//! correct training signal: extraction is the trainer's job, and rewarding
//! fenced junk as valid would train the model to keep emitting it.

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
    /// In [0, 1]. The default validity ladder — see the module docs for why
    /// pnl is not in it.
    pub value: f64,
    pub class: Class,
    /// `max_fuel_per_bar`; 0 if it never ran. The evidence for whether the
    /// termination bound is load-bearing on this task.
    pub fuel: u64,
    pub trades: u32,
    /// TRAIN net pnl — emitted, never in `value`.
    pub train_pnl: i64,
    /// `equity_hash`; 0 if it never ran. Determinism as one number.
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

/// Reward one candidate against the TRAIN window. Deterministic and total:
/// the same source and candles always yield the same reward, and it always
/// returns.
pub fn reward(src: &str, train: &[Candle], limits: Limits, costs: Costs, gate: Gate) -> Reward {
    // The empty-rollout hack, closed. stratlite compiles an empty or
    // whitespace-only source to a lookback-0 no-op, which runs clean and then
    // gate-fails — worth 2/3 on the ladder. Left unguarded, "emit nothing" is
    // a positive-reward action the model would learn immediately. An empty
    // rollout is the ABSENCE of a program, so it scores a compile-class zero.
    if src.trim().is_empty() {
        return Reward {
            value: 0.0,
            class: Class::Compile,
            fuel: 0,
            trades: 0,
            train_pnl: 0,
            hash: 0,
        };
    }
    match verify(src, train, limits, costs, gate) {
        Err(Reject::Compile(_)) => Reward {
            value: ladder(Class::Compile),
            class: Class::Compile,
            fuel: 0,
            trades: 0,
            train_pnl: 0,
            hash: 0,
        },
        Err(Reject::Run(_)) => Reward {
            value: ladder(Class::Run),
            class: Class::Run,
            fuel: 0,
            trades: 0,
            train_pnl: 0,
            hash: 0,
        },
        Err(Reject::Gate(_)) => Reward {
            value: ladder(Class::Gate),
            class: Class::Gate,
            fuel: 0,
            trades: 0,
            train_pnl: 0,
            hash: 0,
        },
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

/// A SOURCE-canonical dedup key: FNV-1a-64 over the source with comments
/// removed and whitespace collapsed. The trainer dedups its SFT admission set
/// by this to resist mode collapse onto one template.
///
/// Why source-canonical and not `equity_hash`: `equity_hash` is 0 for every
/// non-survivor, so early in training — when the whole population sits at the
/// compile/run/gate rungs — a behavioral key cannot see collapse at all. And
/// above the survivor rung it is gameable: perturb one constant by a tick and
/// the equity curve, hence the hash, changes, so a one-template family reads as
/// diverse. A textual key works at every rung and cannot be dodged by comment
/// or whitespace noise.
///
/// Honest limit: this catches comment/formatting clones, NOT
/// constant-perturbation or semantically-equivalent clones (`lookback 4` vs
/// `lookback 5`). Structural/AST canonicalization is the future refinement.
pub fn novelty_key(src: &str) -> u64 {
    let canon = strip_and_collapse(src);
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for b in canon.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Remove `//` line and `/* */` block comments (non-nested — a dedup key does
/// not need a real lexer) and collapse every whitespace run to one space.
/// stratlite has no string literals, so there is nothing a comment strip could
/// wrongly eat.
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
        // A valid strategy scores strictly above a degenerate one, above a
        // faulting one, above garbage. That ordering IS the training signal.
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
        // The facts a trainer might reshape on are present, and the fuel is
        // real (nonzero, under the cap) — the evidence for the fuel claim.
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
        // Whatever the train pnl is, the reward value depends ONLY on class —
        // this is the guard against rewarding curve-fitting.
        assert_eq!(r.value, ladder(r.class));
        // The pnl is still there for a trainer that wants to reshape and own it.
        let _ = r.train_pnl;
    }

    #[test]
    fn garbage_is_a_compile_zero_not_a_leniency() {
        let all = candles(64);
        let sp = Split::at_fraction(all.len(), 0.5);
        // Fenced prose is EXACTLY what a raw model emits — and it must score
        // zero, or the model never learns to stop.
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
        // stratlite compiles all of these to a lookback-0 no-op that gate-fails
        // at 2/3 — so without the guard, emitting nothing pays. Each must be a
        // compile-class zero instead.
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
        // The whole point: it keys a program that never even runs, where
        // equity_hash is 0 for everything and cannot tell templates apart.
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
        // Same inputs, same rewards — every time. This is what "the verifier
        // reward is reproducible even though the model is not" means.
        assert_eq!(a, b);
        assert_eq!(a.len(), 3);
        assert_eq!(a[1].class, Class::Compile);
    }
}

//! # stratlite — a total, fuel-bounded strategy language
//!
//! The litelite kit's M4 language: programs an LLM can generate, a machine
//! can verify, and a selection loop can keep or kill. This crate is the
//! LANGUAGE — lex → parse → per-bar fueled evaluation via [`Session`] — and
//! the trader agent's live dependency. Its verifier (the deterministic
//! backtest engine, `Report`, the activity gate, and paper §5's `verify`
//! predicate) is the sibling crate `backtestlite`; one evaluation path
//! serves both, so verified means deployable.
//!
//! ## The language
//!
//! [`REFERENCE`] is the compact, prompt-embeddable card. In short: three
//! stratified layers — `lookback N;` (0..=4096, the ONLY window on market
//! data), `var x = <literal>;` (the ONLY cross-bar state), then the body,
//! run once per closed bar (`let`/assignment, flat `if`/`else if`/`else`,
//! `repeat`, and `signal long|short|flat;` — the last signal sets the target
//! position; none means it carries). Expressions are prooflite's (i64 +
//! bool, checked arithmetic, short-circuit logic) plus builtins: series
//! reads `open/high/low/close/volume(k)` with DYNAMIC backward offsets
//! (run-checked, `E0207`), indicators `sma/ema/rsi/highest/lowest(n)` with
//! LITERAL windows (parse-checked against the lookback, `E0108` — their
//! fuel cost is static), and the probes `position()`/`entry_price()`.
//! Prices are plain i64 ticks; there is no f64 anywhere.
//!
//! ## The guarantees
//!
//! - **Each decision halts.** Every bar evaluates under a FRESH tank of
//!   [`Limits::fuel_per_bar`] (1 per statement / expression node / repeat
//!   iteration; +n per window-n indicator, charged up front) — adversarial
//!   programs included, live or backtested.
//! - **No look-ahead, by construction.** Future bars are UNREPRESENTABLE:
//!   data access is backward offsets from the just-closed bar. The
//!   prefix-invariance test pins it mechanically: mutating every bar after
//!   `k` cannot change any decision at or before `k`.
//! - **Bounded state and memory.** `lookback` caps every window and offset
//!   (and live memory: `lookback + 1` candles); `var` slots are scalar
//!   literals declared up front — state is bounded by program text.
//! - **Empty effect surface.** No host, no I/O, no clock, no randomness, no
//!   output channel: the candle ring is the program's complete world, and
//!   its entire observable behavior is its signal sequence.
//!
//! Codes: lex `E00xx`, parse `E01xx`, eval `E02xx` — all spanned; the data
//! band `E03xx` belongs to `backtestlite`.
//!
//! ```
//! use stratlite::{Candle, Limits, Session, Signal, compile};
//!
//! let s = compile(
//!     "lookback 4;
//!      if close(0) > sma(4) { signal long; } else { signal flat; }",
//! )
//! .unwrap();
//! let mut session = Session::new(&s, Limits::default());
//! let mut last = Signal::Flat;
//! for i in 0..12i64 {
//!     let p = 1_000 + i * (i % 3); // a small deterministic wobble
//!     let c = Candle { open: p, high: p + 5, low: p - 5, close: p + 2, volume: 1 };
//!     last = session.step(c).unwrap(); // the target to fill at the NEXT open
//! }
//! assert_eq!(last, Signal::Long); // rising closes end above their sma(4)
//! ```

mod eval;
mod lex;
mod parse;

pub use diaglite::{Diag, Span};
pub use eval::Session;
pub use parse::{Program as Strategy, parse};

/// The largest declarable `lookback` (window/offset bound, warmup length).
pub const MAX_LOOKBACK: i64 = 4096;

/// The compact language card — embed VERBATIM in a generation prompt so the
/// language the model sees and the language the verifier checks cannot
/// drift (the experiment's prompt is then a constant of the crate).
pub const REFERENCE: &str = "\
stratlite: one program = one trading strategy, run once per closed candle.
Structure (order is fixed):
  lookback N;            -- 0..=4096; max window/offset; warmup length
  var name = LITERAL;    -- persistent state (int, -int, true, false)
  <body statements>      -- run every bar after warmup
Statements:
  let x = EXPR;   x = EXPR;   signal long|short|flat;
  if EXPR { ... } else if EXPR { ... } else { ... }
  repeat EXPR { ... }    -- count evaluated once; every iteration costs fuel
Expressions: i64 and bool; + - * / % (checked; / truncates, % takes dividend
sign); == != < <= > >= && || (short-circuit) ! -; ( ); // and /* */ comments.
Market data (k = bars back, 0 = just-closed bar; k <= lookback):
  open(k) high(k) low(k) close(k) volume(k)
Indicators (window n must be an INTEGER LITERAL, 1 <= n <= lookback):
  sma(n)  ema(n)  rsi(n) -- rsi in hundredths: 3000 means 30.00
  highest(n)  lowest(n)  -- over highs/lows
State probes: position() -> -1|0|1;  entry_price() -> ticks, 0 when flat.
signal sets the TARGET position, filled at the NEXT bar's open; no signal
means the position carries. Prices are integer ticks. There is no print, no
string, no array, no function, no while, no f64, and no way to see the
future: offsets beyond lookback or below 0 are runtime errors.";

/// A runtime value — also the type of `var` initializers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Bool(bool),
}

/// One OHLCV bar, prices in integer ticks (the harness owns the scale; the
/// verifier validates coherence and range).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candle {
    pub open: i64,
    pub high: i64,
    pub low: i64,
    pub close: i64,
    pub volume: i64,
}

/// A target position — what `signal` sets and fills report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Long,
    Flat,
    Short,
}

/// Per-bar resource bound. A whole backtest halts within `bars × fuel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub fuel_per_bar: u64,
}

impl Default for Limits {
    /// 25,000 — a few full-lookback indicators fit; verifiers tighten at will.
    fn default() -> Self {
        Limits {
            fuel_per_bar: 25_000,
        }
    }
}

/// Stable diagnostic codes, banded by stage; every one is spanned.
pub mod codes {
    /// A character that starts no stratlite token.
    pub const UNEXPECTED_CHAR: u16 = 1;
    /// `/*` without its matching `*/`.
    pub const UNTERMINATED_COMMENT: u16 = 2;
    /// Malformed or out-of-range integer literal.
    pub const BAD_INT: u16 = 3;
    /// The parser needed a different token.
    pub const UNEXPECTED_TOKEN: u16 = 101;
    /// Source nests deeper than the parselite depth cap.
    pub const TOO_DEEP: u16 = 102;
    /// A call to a name that is not a builtin.
    pub const UNKNOWN_CALL: u16 = 103;
    /// A declaration out of place: `lookback` not first/once, `var` after the
    /// body or duplicated, or a `var` initializer that is not a literal.
    pub const BAD_DECL: u16 = 104;
    /// A builtin name declared or assigned.
    pub const RESERVED_NAME: u16 = 105;
    /// `lookback` outside 0..=[`crate::MAX_LOOKBACK`].
    pub const BAD_LOOKBACK: u16 = 106;
    /// A builtin called with the wrong number of arguments.
    pub const CALL_ARITY: u16 = 107;
    /// An indicator window that is not a literal in 1..=lookback.
    pub const BAD_WINDOW: u16 = 108;
    /// Read or assignment of an undeclared name.
    pub const UNDEFINED_VAR: u16 = 201;
    /// An operator or construct got the wrong type of value.
    pub const TYPE_MISMATCH: u16 = 202;
    /// `/` or `%` with a zero divisor.
    pub const DIV_BY_ZERO: u16 = 203;
    /// Arithmetic left the 64-bit integer range.
    pub const OVERFLOW: u16 = 204;
    /// `repeat` with a negative count.
    pub const NEGATIVE_REPEAT: u16 = 205;
    /// The bar's fuel tank ran dry — the decision was stopped, as promised.
    pub const FUEL_EXHAUSTED: u16 = 206;
    /// A series offset below 0 (the future) or beyond the lookback.
    pub const BAD_OFFSET: u16 = 207;
}

/// lex → parse → static checks: a [`Strategy`] or the first failure as a
/// coded, spanned [`Diag`].
pub fn compile(src: &str) -> Result<Strategy, Diag> {
    parse(src)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Step a compiled strategy over a series, collecting per-bar targets.
    fn drive(src: &str, candles: &[Candle]) -> Result<(Vec<Signal>, u64), Diag> {
        let s = compile(src).unwrap();
        let mut session = Session::new(&s, Limits::default());
        let mut out = Vec::with_capacity(candles.len());
        for c in candles {
            out.push(session.step(*c)?);
        }
        Ok((out, session.max_fuel_per_bar()))
    }

    #[test]
    fn the_reference_programs_run_or_die_as_designed() {
        let candles = series(64);
        // A crossover evaluates and signals.
        let (signals, _) = drive(
            "lookback 16;
             if sma(4) > sma(16) { signal long; }
             else if sma(4) < sma(16) { signal short; }",
            &candles,
        )
        .unwrap();
        assert!(signals.iter().any(|s| *s != Signal::Flat));
        // A mean-reverter with persistent state runs clean.
        let (_, fuel) = drive(
            "lookback 14;
             var cooldown = 0;
             if cooldown > 0 { cooldown = cooldown - 1; }
             let r = rsi(14);
             if position() == 0 && cooldown == 0 && r < 4500 { signal long; }
             if position() == 1 && r > 5500 { signal flat; cooldown = 5; }",
            &candles,
        )
        .unwrap();
        assert!(fuel > 0);
        // The adversarial program dies on its first evaluated bar: the
        // future has no name…
        let e = drive("var x = 0; repeat 10 { x = x + close(0 - 1); }", &candles).unwrap_err();
        assert_eq!(e.code, Some(codes::BAD_OFFSET));
        // …and an unbounded loop exhausts the bar's tank.
        let e = drive("var x = 0; repeat 100000000 { x = x + 1; }", &candles).unwrap_err();
        assert_eq!(e.code, Some(codes::FUEL_EXHAUSTED));
        // Offsets past the lookback are equally dead.
        let e = drive("lookback 4; let x = close(5);", &candles).unwrap_err();
        assert_eq!(e.code, Some(codes::BAD_OFFSET));
    }

    #[test]
    fn no_lookahead_prefix_invariance() {
        // Mutating every bar AFTER k cannot change any decision at or
        // before k — the headline guarantee, tested mechanically.
        let candles = series(96);
        let src = "lookback 16;
                   if sma(4) > sma(16) { signal long; }
                   else if sma(4) < sma(16) { signal short; }";
        let (baseline, _) = drive(src, &candles).unwrap();
        for k in [0, 17, 48, 94] {
            let mut mutated = candles.clone();
            for c in mutated.iter_mut().skip(k + 1) {
                // An adversarially different future (still valid candles).
                let p = 20_000 + (c.open % 100) * 31;
                *c = Candle {
                    open: p,
                    high: p + 500,
                    low: p - 500,
                    close: p - 123,
                    volume: 1,
                };
            }
            let (got, _) = drive(src, &mutated).unwrap();
            assert_eq!(got[..=k], baseline[..=k], "prefix diverged at k={k}");
        }
    }

    #[test]
    fn the_cost_model_is_exact_per_bar() {
        let candles = series(40);
        // signal(1) = 1 fuel; the session reports the worst bar.
        assert_eq!(drive("signal long;", &candles).unwrap().1, 1);
        // let(1) + call node(1) + window(4) = 6.
        assert_eq!(drive("lookback 4; let x = sma(4);", &candles).unwrap().1, 6);
        // let(1) + node(1) + arg literal(1) = 3 for a series read.
        assert_eq!(
            drive("lookback 1; let x = close(1);", &candles).unwrap().1,
            3
        );
    }

    #[test]
    fn warmup_state_and_signal_semantics() {
        let candles = series(20);
        // Warmup bars return the standing target without evaluating.
        let (signals, _) = drive("lookback 16; signal long;", &candles).unwrap();
        assert!(signals[..16].iter().all(|s| *s == Signal::Flat));
        assert!(signals[16..].iter().all(|s| *s == Signal::Long));
        // No signal at all: the position carries (all Flat), and vars persist.
        let (signals, _) = drive(
            "var n = 0; n = n + 1; if n == 5 { signal short; }",
            &candles,
        )
        .unwrap();
        assert!(signals[..4].iter().all(|s| *s == Signal::Flat));
        assert!(signals[4..].iter().all(|s| *s == Signal::Short));
        // The LAST signal in a bar wins.
        let (signals, _) = drive("signal long; signal flat; signal short;", &candles).unwrap();
        assert!(signals.iter().all(|s| *s == Signal::Short));
    }

    #[test]
    fn every_band_speaks_and_reference_matches_the_grammar() {
        assert_eq!(compile("let x = ⚡;").unwrap_err().code.unwrap() / 100, 0);
        assert_eq!(compile("signal long").unwrap_err().code.unwrap() / 100, 1);
        let e = drive("let x = 1 / 0;", &series(4)).unwrap_err();
        assert_eq!(e.code.unwrap() / 100, 2);
        // The prompt card mentions every statement form and builtin.
        for needle in [
            "lookback",
            "var",
            "signal",
            "repeat",
            "sma(",
            "ema(",
            "rsi(",
            "highest(",
            "lowest(",
            "position()",
            "entry_price()",
            "close(",
        ] {
            assert!(REFERENCE.contains(needle), "{needle}");
        }
    }
}

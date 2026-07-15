//! # prooflite — the litelite reference language
//!
//! The smallest TOTAL language that exercises every kit crate end-to-end:
//! lexlite lexing → parselite parsing (depth-guarded) → fuellite-fueled
//! tree-walk evaluation, with every failure a coded, spanned diaglite `Diag`.
//! Consumer: the paper's baseline (`paper/OUTLINE.md` §3) — the measured
//! answer to "what does a language on the kit cost, and what does it buy?".
//!
//! ## The language
//!
//! Values: 64-bit signed integers and booleans. Statements: `let x = e;`,
//! `x = e;`, `print e;`, `if e { … } else { … }` (else-if chains allowed),
//! `repeat e { … }` (count evaluated once, up front). Expressions: literals
//! (`42`, `1_000`, `0xff`, `true`), variables, unary `- !`, binary `* / %`,
//! `+ -`, `< <= > >=`, `== !=`, `&&`, `||` (both short-circuit), parentheses.
//! Comments: `// line` and nested `/* block */`.
//!
//! No functions, no recursion, no `while`: the only loop is `repeat` with an
//! up-front count. Arithmetic is CHECKED — overflow, division/remainder by
//! zero, and negation of `i64::MIN` are diagnostics, never wraparound (a
//! wrong-but-clean result is worse than an error). `i64::MIN` itself is not
//! writable as a literal (`-` is an operator, so the literal half overflows
//! first) — reach it arithmetically if you need it.
//!
//! ## The guarantees (what smallness buys)
//!
//! - **Termination, mechanically.** Every statement, expression node, and
//!   `repeat` iteration burns 1 fuel from ONE tank; a dry tank stops the
//!   program with `E0206`. "Halts within `limits.fuel` steps" holds for every
//!   program, adversarial ones included — by construction, not by review.
//! - **Bounded output.** `print` writes through a `ByteBudget`: past the cap
//!   the output is clipped (never mid-char), the run keeps going, and
//!   [`Outcome::output_clipped`] says so.
//! - **Bounded nesting.** The parser rides parselite's depth guard, so deeply
//!   nested source is an `E0102` diag — never a stack overflow.
//! - **No effects.** prooflite has no host calls; its complete effect surface
//!   is the returned `output` string. (M2 adds a capability table.)
//!
//! Diagnostics are code-banded per stage — lex `E00xx`, parse `E01xx`, eval
//! `E02xx` (see [`codes`]) — so tests and agents assert on codes, not on
//! message text.
//!
//! ```
//! use prooflite::{Limits, run};
//!
//! let out = run(
//!     "let acc = 1;
//!      repeat 10 { acc = acc * 2; }
//!      print acc;",
//!     Limits::default(),
//! )
//! .unwrap();
//! assert_eq!(out.output, "1024\n");
//!
//! // The headline guarantee: ANY prooflite program halts within its fuel.
//! let err = run(
//!     "repeat 1000000000 { }",
//!     Limits { fuel: 1_000, output_bytes: 0 },
//! )
//! .unwrap_err();
//! assert_eq!(err.code, Some(prooflite::codes::FUEL_EXHAUSTED));
//! ```

use diaglite::Diag;

mod eval;
mod lex;
mod parse;

pub use lex::{TokKind, Token, lex};
pub use parse::{Program, parse};

/// Stable diagnostic codes, banded by stage: lex `E00xx`, parse `E01xx`,
/// eval `E02xx`.
pub mod codes {
    /// A character that starts no prooflite token.
    pub const UNEXPECTED_CHAR: u16 = 1;
    /// `/*` without its matching `*/`.
    pub const UNTERMINATED_COMMENT: u16 = 2;
    /// Malformed or out-of-range integer literal.
    pub const BAD_INT: u16 = 3;
    /// The parser needed a different token (the message names both sides).
    pub const UNEXPECTED_TOKEN: u16 = 101;
    /// Source nests deeper than the parselite depth cap.
    pub const TOO_DEEP: u16 = 102;
    /// Read of, or assignment to, a name with no visible `let`.
    pub const UNDEFINED_VAR: u16 = 201;
    /// An operator or construct got the wrong type of value.
    pub const TYPE_MISMATCH: u16 = 202;
    /// `/` or `%` with a zero divisor.
    pub const DIV_BY_ZERO: u16 = 203;
    /// Arithmetic left the 64-bit integer range.
    pub const OVERFLOW: u16 = 204;
    /// `repeat` with a negative count.
    pub const NEGATIVE_REPEAT: u16 = 205;
    /// The fuel tank ran dry — the program was stopped, as promised.
    pub const FUEL_EXHAUSTED: u16 = 206;
}

/// Hard resource limits for one [`run`]. Both are guarantees, not hints: fuel
/// bounds total evaluation steps, `output_bytes` bounds what `print` can emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Evaluation steps available; see the cost model in the crate docs.
    pub fuel: u64,
    /// Byte cap on accumulated `print` output.
    pub output_bytes: usize,
}

impl Default for Limits {
    /// 100_000 fuel, 64 KiB of output — roomy for reference programs, tiny
    /// for a host.
    fn default() -> Self {
        Limits {
            fuel: 100_000,
            output_bytes: 64 * 1024,
        }
    }
}

/// What a completed run produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Everything `print` wrote (one `\n`-terminated line per print), up to
    /// the byte cap.
    pub output: String,
    /// True when output hit the cap and was clipped (execution continued).
    pub output_clipped: bool,
    /// Fuel actually burned — `≤ limits.fuel` by construction.
    pub fuel_used: u64,
}

/// Parse and evaluate `src` under `limits`.
///
/// `Err` is the FIRST failure at any stage, as a coded, spanned [`Diag`] —
/// prefer `err.render(src)` on any surface a human or agent reads.
pub fn run(src: &str, limits: Limits) -> Result<Outcome, Diag> {
    let program = parse(src)?;
    eval::eval(&program, &limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(src: &str) -> String {
        run(src, Limits::default()).unwrap().output
    }

    fn code(src: &str) -> u16 {
        run(src, Limits::default()).unwrap_err().code.unwrap()
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(out("print 1 + 2 * 3;"), "7\n");
        assert_eq!(out("print (1 + 2) * 3;"), "9\n");
        assert_eq!(out("print -2 * 3;"), "-6\n");
        assert_eq!(out("print 10 % 3;"), "1\n");
        assert_eq!(out("print 7 / 2;"), "3\n");
        assert_eq!(out("print 0xff - 0x0f;"), "240\n");
        assert_eq!(out("print 1_000_000;"), "1000000\n");
    }

    #[test]
    fn comparison_and_logic() {
        assert_eq!(out("print 1 < 2;"), "true\n");
        assert_eq!(out("print 2 <= 1;"), "false\n");
        assert_eq!(out("print 1 + 1 == 2;"), "true\n");
        assert_eq!(out("print true != false;"), "true\n");
        assert_eq!(out("print !true || 1 > 0 && 2 >= 2;"), "true\n");
    }

    #[test]
    fn short_circuit_skips_the_right_side() {
        // The unevaluated side would be a division by zero — it must not run.
        assert_eq!(out("print false && 1 / 0 == 0;"), "false\n");
        assert_eq!(out("print true || 1 / 0 == 0;"), "true\n");
        assert_eq!(code("print true && 1 / 0 == 0;"), codes::DIV_BY_ZERO);
    }

    #[test]
    fn checked_arithmetic_never_wraps() {
        assert_eq!(code("print 1 / 0;"), codes::DIV_BY_ZERO);
        assert_eq!(code("print 1 % 0;"), codes::DIV_BY_ZERO);
        assert_eq!(code("print 9223372036854775807 + 1;"), codes::OVERFLOW);
        assert_eq!(code("print 0 - 9223372036854775807 - 2;"), codes::OVERFLOW);
        assert_eq!(code("print 3037000500 * 3037000500;"), codes::OVERFLOW);
        // i64::MIN reached arithmetically; negating or dividing it overflows.
        let min = "let m = 0 - 9223372036854775807 - 1;";
        assert_eq!(code(&format!("{min} print -m;")), codes::OVERFLOW);
        assert_eq!(code(&format!("{min} print m / -1;")), codes::OVERFLOW);
        assert_eq!(code(&format!("{min} print m % -1;")), codes::OVERFLOW);
    }

    #[test]
    fn type_errors_are_coded_and_spanned() {
        for src in [
            "if 1 { }",
            "repeat true { }",
            "print -true;",
            "print !1;",
            "print 1 + true;",
            "print true * false;",
            "print 1 == true;",
            "print 1 && true;",
        ] {
            let e = run(src, Limits::default()).unwrap_err();
            assert_eq!(e.code, Some(codes::TYPE_MISMATCH), "{src}: {e}");
            assert!(e.span.is_some(), "{src}");
        }
    }

    #[test]
    fn scoping_shadows_and_pops() {
        // A block-local `let` disappears when the block ends…
        assert_eq!(
            code("if true { let inner = 1; } print inner;"),
            codes::UNDEFINED_VAR
        );
        // …while assignment reaches through blocks to the outer binding.
        assert_eq!(
            out("let x = 1; if true { x = 2; let x = 9; x = 8; } print x;"),
            "2\n"
        );
        assert_eq!(out("let x = 1; let x = x + 1; print x;"), "2\n");
        assert_eq!(code("print nope;"), codes::UNDEFINED_VAR);
        assert_eq!(code("nope = 1;"), codes::UNDEFINED_VAR);
    }

    #[test]
    fn if_else_chains() {
        let src = "let n = 2;
                   if n == 1 { print 10; }
                   else if n == 2 { print 20; }
                   else { print 30; }";
        assert_eq!(out(src), "20\n");
        assert_eq!(out("if false { print 1; }"), "");
    }

    #[test]
    fn repeat_semantics() {
        assert_eq!(out("let s = 0; repeat 5 { s = s + 1; } print s;"), "5\n");
        assert_eq!(out("repeat 0 { print 99; } print 1;"), "1\n");
        // The count is evaluated once — mutating it inside can't extend the loop.
        assert_eq!(out("let n = 3; repeat n { n = n + 10; } print n;"), "33\n");
        assert_eq!(code("repeat 0 - 1 { }"), codes::NEGATIVE_REPEAT);
    }

    #[test]
    fn the_cost_model_is_exact() {
        // 1 per statement + 1 per expression node, documented in the crate docs.
        let o = run("print 1;", Limits::default()).unwrap();
        assert_eq!(o.fuel_used, 2);
        // let(1) + binary(1) + int(1) + int(1) = 4
        let o = run("let x = 1 + 2;", Limits::default()).unwrap();
        assert_eq!(o.fuel_used, 4);
        // repeat(1) + count(1) + 3 × (iteration(1) + print(2)) = 11
        let o = run("repeat 3 { print 0; }", Limits::default()).unwrap();
        assert_eq!(o.fuel_used, 11);
        let o = run("", Limits::default()).unwrap();
        assert_eq!((o.fuel_used, o.output.as_str()), (0, ""));
    }

    #[test]
    fn fuel_bounds_any_composition() {
        // Nested repeats share the ONE tank — the fractal invariant.
        let e = run(
            "repeat 100000 { repeat 100000 { } }",
            Limits {
                fuel: 10_000,
                output_bytes: 0,
            },
        )
        .unwrap_err();
        assert_eq!(e.code, Some(codes::FUEL_EXHAUSTED));
        // Within budget, fuel_used never exceeds the limit.
        let o = run("repeat 10 { print 1; }", Limits::default()).unwrap();
        assert!(o.fuel_used <= Limits::default().fuel);
    }

    #[test]
    fn output_clips_at_the_cap_and_execution_continues() {
        let o = run(
            "print 12345; print 678; print 9;",
            Limits {
                fuel: 1_000,
                output_bytes: 6,
            },
        )
        .unwrap();
        assert_eq!(o.output, "12345\n");
        assert!(o.output_clipped);
        let o = run("print 1;", Limits::default()).unwrap();
        assert!(!o.output_clipped);
    }

    #[test]
    fn diags_render_with_carets() {
        let src = "let x = 1;\nprint x + true;";
        let r = run(src, Limits::default()).unwrap_err().render(src);
        assert!(r.contains("E0202"), "{r}");
        assert!(r.contains("line 2, col 11"), "{r}");
        assert!(r.contains("print x + true;"), "{r}");
        assert!(r.lines().last().unwrap().trim() == "^^^^", "{r}");
    }

    #[test]
    fn every_stage_speaks_in_its_band() {
        assert_eq!(code("print ⚡;") / 100, 0); // lex
        assert_eq!(code("print 1") / 100, 1); // parse
        assert_eq!(code("print x;") / 100, 2); // eval
    }
}

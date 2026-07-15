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
//! `x = e;`, `print e;`, `if e { … } else { … }` (else-if chains of any
//! length — they are flat, not nested), `repeat e { … }` (count evaluated
//! once, up front). Expressions: literals (`42`, `1_000`, `0xff`, `true`),
//! variables, host-capability calls `name(a, b)` (see [`Host`]), unary `- !`,
//! binary `* / %`, `+ -`, `< <= > >=`, `== !=`, `&&`, `||` (both
//! short-circuit), parentheses. Comments: `// line` and nested `/* block */`.
//!
//! No functions, no recursion, no `while`: the only loop is `repeat` with an
//! up-front count. Arithmetic is CHECKED — overflow, division/remainder by
//! zero, and negation of `i64::MIN` are diagnostics, never wraparound (a
//! wrong-but-clean result is worse than an error). `/` truncates toward zero
//! and `%` takes the dividend's sign: `(0-7)/2` is `-3`, `(0-7)%3` is `-1`.
//! `i64::MIN` itself is not writable as a literal (`-` is an operator, so the
//! literal half overflows first) — reach it arithmetically if you need it.
//!
//! ## The guarantees (what smallness buys)
//!
//! - **Termination, mechanically.** Every statement, expression node, and
//!   `repeat` iteration burns 1 fuel from ONE tank (a capability call burns
//!   1 plus its declared cost); a dry tank stops the program with `E0206`.
//!   "Halts within `limits.fuel` steps" holds for every program, adversarial
//!   ones included — by construction, not by review.
//! - **Bounded output.** `print` writes through a `ByteBudget`: past the cap
//!   the output is clipped (never mid-char), the run keeps going, and
//!   [`Outcome::output_clipped`] says so.
//! - **Bounded nesting.** The parser rides parselite's depth guard, and
//!   left-associative operator chains charge it one entry per fold (each fold
//!   deepens the AST spine the evaluator must later walk). Deep nesting AND
//!   arbitrarily long operator chains are an `E0102` diag — never a stack
//!   overflow, at parse, eval, or drop time. Statement sequences and else-if
//!   chains are flat and unbounded.
//! - **A complete effect bound.** The host's [`CapTable`] is the WHOLE world
//!   a program can touch: calls resolve, type-check, and cost fuel against
//!   it, and [`run`] (hostless) has an empty table — provably no effects
//!   beyond the output string. The same table renders the host's docs and a
//!   parity manifest for the far side of a boundary (`caplite`).
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

mod eval;
mod lex;
mod parse;

// Re-export the kit types every public signature speaks in, so a consumer
// can name them without separate diaglite/caplite dependencies.
pub use caplite::{Cap, CapTable, Ty};
pub use diaglite::{Diag, Span};
pub use eval::Value;
pub use lex::{TokKind, Token, lex};
pub use parse::{Program, parse};

/// prooflite's type vocabulary — the symbols capability tables speak in.
/// The `sym` strings (`i64`, `bool`) are ABI: they appear in signatures and
/// parity manifests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Int,
    Bool,
}

impl Ty for Type {
    fn sym(&self) -> &'static str {
        match self {
            Type::Int => "i64",
            Type::Bool => "bool",
        }
    }
}

/// The host seam: everything a prooflite program can reach beyond its own
/// state and output string. The capability table IS the complete effect
/// bound — a program provably cannot touch anything the table does not name.
///
/// Hosts must be TOTAL (the bashlite rule): report failure as `Err(message)`
/// (an `E0209` diag at the call site), never panic. Arguments arrive already
/// checked against the declared params; a result that contradicts the
/// declared type is also an `E0209`.
///
/// ```
/// use prooflite::{Cap, CapTable, Host, Limits, Type, Value, run_with_host};
///
/// struct Counter(i64);
/// static CAPS: CapTable<Type> = CapTable::new(&[Cap {
///     module: "counter",
///     name: "next",
///     params: &[],
///     result: Some(Type::Int),
///     cost: 1,
///     doc: "The next counter value.",
/// }]);
/// impl Host for Counter {
///     fn caps(&self) -> &CapTable<Type> {
///         &CAPS
///     }
///     fn call(&mut self, _idx: usize, _args: &[Value]) -> Result<Value, String> {
///         self.0 += 1;
///         Ok(Value::Int(self.0))
///     }
/// }
///
/// let mut host = Counter(0);
/// let out = run_with_host("repeat 3 { print next(); }", Limits::default(), &mut host).unwrap();
/// assert_eq!(out.output, "1\n2\n3\n");
/// ```
pub trait Host {
    /// The capability table this host implements. Drives name resolution,
    /// arg checking, per-call fuel costs, docs, and the parity manifest.
    fn caps(&self) -> &CapTable<Type>;
    /// Execute capability `idx` (an index into [`caps`](Self::caps)) with
    /// already-checked `args`.
    fn call(&mut self, idx: usize, args: &[Value]) -> Result<Value, String>;
}

/// The hostless host: an empty table, so every call site is an `E0207`.
struct NoHost;

static NO_CAPS: CapTable<Type> = CapTable::new(&[]);

impl Host for NoHost {
    fn caps(&self) -> &CapTable<Type> {
        &NO_CAPS
    }
    fn call(&mut self, _idx: usize, _args: &[Value]) -> Result<Value, String> {
        Err("hostless run".to_string())
    }
}

/// prooflite-specific table rules, checked once per run: caps must declare a
/// result (calls are expressions) and bare names must be unique across
/// modules (call sites have a flat namespace).
fn check_table(table: &CapTable<Type>) -> Result<(), Diag> {
    table
        .validate()
        .map_err(|e| Diag::new_code(codes::BAD_CAP_TABLE, e))?;
    for (i, cap) in table.iter() {
        if cap.result.is_none() {
            return Err(Diag::new_code(
                codes::BAD_CAP_TABLE,
                format!(
                    "capability `{}.{}` declares no result; prooflite calls are expressions",
                    cap.module, cap.name
                ),
            ));
        }
        for (j, other) in table.iter() {
            if j > i && other.name == cap.name {
                return Err(Diag::new_code(
                    codes::BAD_CAP_TABLE,
                    format!(
                        "capability name `{}` appears in both `{}` and `{}`; prooflite call sites use bare names",
                        cap.name, cap.module, other.module
                    ),
                ));
            }
        }
    }
    Ok(())
}

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
    /// Call of a name the host's capability table does not declare.
    pub const UNKNOWN_CAP: u16 = 207;
    /// Capability call with the wrong number or types of arguments.
    pub const CAP_ARGS: u16 = 208;
    /// The host failed, or returned a value contradicting its own table.
    pub const HOST_FAULT: u16 = 209;
    /// The host's capability table itself is unusable from prooflite.
    pub const BAD_CAP_TABLE: u16 = 210;
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

/// Parse and evaluate `src` under `limits`, hostless: the capability table
/// is empty, so the program provably has NO effects beyond its output.
///
/// `Err` is the FIRST failure at any stage, as a coded, spanned [`Diag`] —
/// prefer `err.render(src)` on any surface a human or agent reads.
pub fn run(src: &str, limits: Limits) -> Result<Outcome, Diag> {
    run_with_host(src, limits, &mut NoHost)
}

/// [`run`], with `host`'s capability table as the program's COMPLETE effect
/// surface. A call site burns 1 fuel for the node plus the capability's
/// declared `cost`. See [`Host`] for an end-to-end example.
pub fn run_with_host(src: &str, limits: Limits, host: &mut dyn Host) -> Result<Outcome, Diag> {
    check_table(host.caps())?;
    let program = parse(src)?;
    eval::eval(&program, &limits, host)
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
    fn division_truncates_toward_zero() {
        // Rust semantics, promised in the crate docs.
        assert_eq!(out("print (0 - 7) / 2;"), "-3\n");
        assert_eq!(out("print (0 - 7) % 3;"), "-1\n");
        assert_eq!(out("print 7 % (0 - 3);"), "1\n");
    }

    #[test]
    fn operator_chains_count_toward_the_depth_cap() {
        // Left-associative folds deepen the AST spine, so they charge the
        // same guard as parens: long flat chains are an E0102 diag — never a
        // stack overflow in eval or in drop glue.
        let ok = format!("print {}0;", "1+".repeat(50));
        assert_eq!(run(&ok, Limits::default()).unwrap().output, "50\n");
        let deep = format!("print {}0;", "1+".repeat(500));
        let e = run(&deep, Limits::default()).unwrap_err();
        assert_eq!(e.code, Some(codes::TOO_DEEP));
        // Parsing alone (then dropping the AST) is equally safe.
        let e = parse(&format!("print {}true;", "false||".repeat(5000))).unwrap_err();
        assert_eq!(e.code, Some(codes::TOO_DEEP));
    }

    #[test]
    fn flat_else_if_chains_are_unbounded() {
        // 300 links — flat in the source, flat in the AST, flat in guard depth.
        let mut src = String::from("let x = 250;\nif x == 0 { print 0; }\n");
        for i in 1..300 {
            src.push_str(&format!("else if x == {i} {{ print {i}; }}\n"));
        }
        src.push_str("else { print 999; }\n");
        let o = run(&src, Limits::default()).unwrap();
        assert_eq!(o.output, "250\n");
    }

    #[test]
    fn every_stage_speaks_in_its_band() {
        assert_eq!(code("print ⚡;") / 100, 0); // lex
        assert_eq!(code("print 1") / 100, 1); // parse
        assert_eq!(code("print x;") / 100, 2); // eval
    }

    static TEST_CAPS: CapTable<Type> = CapTable::new(&[
        Cap {
            module: "counter",
            name: "next",
            params: &[],
            result: Some(Type::Int),
            cost: 3,
            doc: "Monotonic counter.",
        },
        Cap {
            module: "math",
            name: "mix",
            params: &[Type::Int, Type::Int],
            result: Some(Type::Int),
            cost: 0,
            doc: "XOR of two integers.",
        },
        Cap {
            module: "flag",
            name: "both",
            params: &[Type::Bool, Type::Bool],
            result: Some(Type::Bool),
            cost: 1,
            doc: "Logical AND.",
        },
        Cap {
            module: "boom",
            name: "fail",
            params: &[],
            result: Some(Type::Int),
            cost: 0,
            doc: "Always errors.",
        },
    ]);

    struct TestHost {
        count: i64,
        lie: bool,
    }

    impl Host for TestHost {
        fn caps(&self) -> &CapTable<Type> {
            &TEST_CAPS
        }
        fn call(&mut self, idx: usize, args: &[Value]) -> Result<Value, String> {
            if self.lie {
                return Ok(Value::Bool(true)); // wrong type for `next`/`mix`/`fail`
            }
            match (idx, args) {
                (0, []) => {
                    self.count += 1;
                    Ok(Value::Int(self.count))
                }
                (1, [Value::Int(a), Value::Int(b)]) => Ok(Value::Int(a ^ b)),
                (2, [Value::Bool(a), Value::Bool(b)]) => Ok(Value::Bool(*a && *b)),
                (3, _) => Err("boom".to_string()),
                _ => Err(format!("bad dispatch: idx {idx}")),
            }
        }
    }

    fn host() -> TestHost {
        TestHost {
            count: 0,
            lie: false,
        }
    }

    fn host_code(src: &str) -> u16 {
        run_with_host(src, Limits::default(), &mut host())
            .unwrap_err()
            .code
            .unwrap()
    }

    #[test]
    fn capability_calls_flow_through_the_table() {
        let src = "print next();
                   print mix(6, next());
                   print both(true, 1 < 2);";
        let o = run_with_host(src, Limits::default(), &mut host()).unwrap();
        assert_eq!(o.output, "1\n4\ntrue\n"); // 6 ^ 2 = 4
    }

    #[test]
    fn the_table_is_the_whole_effect_surface() {
        // Hostless runs have an empty table: every call is unknown.
        assert_eq!(code("print next();"), codes::UNKNOWN_CAP);
        // With a host, only declared names resolve.
        assert_eq!(host_code("print nope(1);"), codes::UNKNOWN_CAP);
    }

    #[test]
    fn capability_args_are_checked_against_the_declaration() {
        let e = run_with_host("print mix(1);", Limits::default(), &mut host()).unwrap_err();
        assert_eq!(e.code, Some(codes::CAP_ARGS));
        assert!(e.message.contains("expects 2 argument(s), got 1"), "{e}");
        let e = run_with_host("print mix(1, true);", Limits::default(), &mut host()).unwrap_err();
        assert_eq!(e.code, Some(codes::CAP_ARGS));
        assert!(
            e.message.contains("argument 2 must be i64, got bool"),
            "{e}"
        );
    }

    #[test]
    fn a_misbehaving_host_is_a_coded_diag_not_trust() {
        assert_eq!(host_code("print fail();"), codes::HOST_FAULT);
        let e = run_with_host(
            "print next();",
            Limits::default(),
            &mut TestHost {
                count: 0,
                lie: true,
            },
        )
        .unwrap_err();
        assert_eq!(e.code, Some(codes::HOST_FAULT));
        assert!(e.message.contains("returned bool"), "{e}");
    }

    #[test]
    fn bad_tables_are_rejected_before_the_program_runs() {
        struct BadHost(CapTable<Type>);
        impl Host for BadHost {
            fn caps(&self) -> &CapTable<Type> {
                &self.0
            }
            fn call(&mut self, _: usize, _: &[Value]) -> Result<Value, String> {
                Err("unreachable".to_string())
            }
        }
        // Same bare name in two modules: prooflite call sites can't choose.
        static AMBIG: &[Cap<Type>] = &[
            Cap {
                module: "a",
                name: "x",
                params: &[],
                result: Some(Type::Int),
                cost: 0,
                doc: "",
            },
            Cap {
                module: "b",
                name: "x",
                params: &[],
                result: Some(Type::Int),
                cost: 0,
                doc: "",
            },
        ];
        let e = run_with_host("", Limits::default(), &mut BadHost(CapTable::new(AMBIG)));
        assert_eq!(e.unwrap_err().code, Some(codes::BAD_CAP_TABLE));
        // A result-less cap can't be an expression.
        static VOID: &[Cap<Type>] = &[Cap {
            module: "a",
            name: "x",
            params: &[],
            result: None,
            cost: 0,
            doc: "",
        }];
        let e = run_with_host("", Limits::default(), &mut BadHost(CapTable::new(VOID)));
        assert_eq!(e.unwrap_err().code, Some(codes::BAD_CAP_TABLE));
    }

    #[test]
    fn capability_costs_burn_from_the_one_tank() {
        // print(1 stmt) + call node(1) + declared cost(3) = 5.
        let o = run_with_host("print next();", Limits::default(), &mut host()).unwrap();
        assert_eq!(o.fuel_used, 5);
        // Declared costs can exhaust the tank mid-call chain.
        let e = run_with_host(
            "repeat 100 { print next(); }",
            Limits {
                fuel: 20,
                output_bytes: 64,
            },
            &mut host(),
        )
        .unwrap_err();
        assert_eq!(e.code, Some(codes::FUEL_EXHAUSTED));
    }

    #[test]
    fn the_manifest_pins_prooflite_type_symbols() {
        // `i64`/`bool` are Type's ABI symbols — a change here moves every
        // parity hash of every prooflite host, on purpose.
        assert_eq!(
            TEST_CAPS.manifest(),
            "caplite-manifest/1\n\
             0 counter.next()->i64 cost=3\n\
             1 math.mix(i64,i64)->i64 cost=0\n\
             2 flag.both(bool,bool)->bool cost=1\n\
             3 boom.fail()->i64 cost=0\n"
        );
    }
}

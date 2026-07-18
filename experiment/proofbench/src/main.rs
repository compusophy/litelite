//! p6 — the prooflite reward tool, the N=2 arm of the M6 experiment.
//!
//! It speaks the SAME CLI as `s5` (card / styles / reward / eval), so the
//! language-parametric trainer (experiment/train/) fine-tunes prooflite with
//! zero changes — just point it here. prooflite is a pure compute language
//! with no market data, so there is no train/held-out split: the reward is
//! intrinsic to a program's execution, and `eval` reports the validity-ladder
//! distribution, not a generalization gap. That is the point of N=2 — the same
//! method on a structurally different language.
//!
//! The reward is the validity ladder, parallel to stratlite's: a program that
//! does not PARSE (a data-independent failure) scores 0; one that parses but
//! FAULTS at eval scores 1/3; one that runs clean but computes nothing
//! non-trivial scores 2/3; one that runs clean and prints real, varied output
//! scores 1. As on stratlite, the value is validity only — the facts (fuel,
//! distinct output lines) are emitted so a trainer can reshape and own it.

use prooflite::{Limits, run};
use serde_json::Value;
use std::process::ExitCode;

/// A hand-written spec (prooflite ships no REFERENCE const — its verifier is
/// `run()`, which reads no card, so there is nothing for a card to drift from).
const CARD: &str = "\
prooflite: a tiny TOTAL language. One program computes and prints; it always halts.
Values: 64-bit signed integers and booleans. NO strings, floats, functions, or while.
Statements (each ends with ;):
  let x = EXPR;          -- introduce a variable
  x = EXPR;              -- reassign a variable already introduced with let
  print EXPR;            -- write one line of output
  if EXPR { ... } else if EXPR { ... } else { ... }   -- else-if chains are flat
  repeat EXPR { ... }    -- the count is evaluated ONCE, up front; then loops
Expressions: literals (42, 1_000, 0xff, true, false); variables; unary - and !;
  * / % (checked; / truncates toward zero, % takes the dividend's sign);
  + -; < <= > >=; == !=; && ||  (both short-circuit); parentheses.
Comments: // line  and  /* nested block */.
Arithmetic is CHECKED: overflow, divide-by-zero, and remainder-by-zero are ERRORS,
never wraparound. A variable must be introduced with let before it is read or assigned.
There is no way to loop unboundedly and no way to recurse: repeat with an up-front
count is the only loop. Reading an undefined variable is a runtime error.";

/// Diversity axes — computation FAMILIES, the prooflite analog of stratlite's
/// strategy families. They drive corpus variety and self-play exploration.
const STYLES: &[&str] = &[
    "a program that does nested integer arithmetic and prints the results",
    "a program that uses an if / else-if / else chain to branch on a computed value",
    "a program that uses a repeat loop with a var accumulator to build up a total",
    "a program that computes and prints boolean results using && || and comparisons",
    "a program that uses a repeat loop to print a running counter or a sequence",
    "a program that carries state in variables across a repeat loop and an if",
    "a program that guards against a divide-by-zero or overflow before computing",
    "a program with a repeat loop nested inside an if, printing several values",
];

/// The OK rung requires real, varied output. Thresholds are activity proxies
/// (documented, like stratlite's gate): a program that merely prints a few
/// constants burns almost no fuel, so the fuel floor forces actual evaluation.
const MIN_DISTINCT_LINES: usize = 3;
const MIN_FUEL: u64 = 30;

struct Reward {
    value: f64,
    class: &'static str,
    fuel: u64,
    distinct: usize,
    lines: usize,
}

fn reward(src: &str) -> Reward {
    // Empty rollout is the ABSENCE of a program — a compile-class zero, exactly
    // like s5. (Guards the "emit nothing" hack.)
    if src.trim().is_empty() {
        return Reward {
            value: 0.0,
            class: "compile",
            fuel: 0,
            distinct: 0,
            lines: 0,
        };
    }
    match run(src, Limits::default()) {
        // Lex (1-3) and parse (101,102) are the data-independent PARSE band;
        // eval/host codes (>=200) mean it parsed but faulted at run.
        Err(d) => {
            let parsed = d.code.is_some_and(|c| c >= 200);
            Reward {
                value: if parsed { 1.0 / 3.0 } else { 0.0 },
                class: if parsed { "run" } else { "compile" },
                fuel: 0,
                distinct: 0,
                lines: 0,
            }
        }
        Ok(o) => {
            let printed: Vec<&str> = o.output.lines().filter(|l| !l.is_empty()).collect();
            let lines = printed.len();
            let mut uniq = printed.clone();
            uniq.sort_unstable();
            uniq.dedup();
            let distinct = uniq.len();
            let rich = distinct >= MIN_DISTINCT_LINES && o.fuel_used >= MIN_FUEL;
            Reward {
                value: if rich { 1.0 } else { 2.0 / 3.0 },
                class: if rich { "ok" } else { "gate" },
                fuel: o.fuel_used,
                distinct,
                lines,
            }
        }
    }
}

/// Source-canonical dedup key — FNV-1a-64 over comment-stripped, whitespace-
/// collapsed source. Same rationale as s5's nkey: it works at every rung and
/// resists comment and whitespace-RUN clones — but NOT token-adjacency spacing
/// (`x=1` and `x = 1` hash differently), constant-perturbation, or semantic
/// clones. (prooflite has no string literals either.)
fn novelty_key(src: &str) -> u64 {
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
    let canon = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in canon.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn read_pool(path: &str) -> Result<Vec<(String, String)>, String> {
    let text = if path == "-" {
        std::io::read_to_string(std::io::stdin()).map_err(|e| format!("stdin: {e}"))?
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?
    };
    let mut rows = Vec::new();
    for (n, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let v: Value = serde_json::from_str(line).map_err(|e| format!("line {}: {e}", n + 1))?;
        rows.push((
            v["id"].as_str().unwrap_or("?").to_string(),
            v["source"].as_str().unwrap_or("").to_string(),
        ));
    }
    Ok(rows)
}

const USAGE: &str = "\
p6 — the prooflite reward tool (N=2)

  p6 card                        print the prooflite prompt card
  p6 styles                      print the diversity styles, one per line
  p6 reward   <pool.jsonl>       per-rollout validity reward (JSONL in, records out; - = stdin)
  p6 eval     <pool.jsonl>       the validity-ladder distribution over a pool
  p6 novelty  <pool> <corpus>    of a pool's RICH programs, the fraction absent from a corpus
";

fn cmd_reward(path: &str) -> Result<(), String> {
    let mut out = String::new();
    for (id, src) in read_pool(path)? {
        let r = reward(&src);
        out.push_str(&format!(
            "{{\"id\":{},\"value\":{:.6},\"class\":\"{}\",\"fuel\":{},\"distinct\":{},\"lines\":{},\"nkey\":\"0x{:016x}\"}}\n",
            serde_json::to_string(&id).unwrap(),
            r.value, r.class, r.fuel, r.distinct, r.lines, novelty_key(&src)
        ));
    }
    print!("{out}");
    Ok(())
}

fn cmd_eval(path: &str) -> Result<(), String> {
    let rows = read_pool(path)?;
    let (mut compile, mut run_f, mut gate, mut ok) = (0u32, 0u32, 0u32, 0u32);
    let mut keys = std::collections::BTreeSet::new();
    for (_, src) in &rows {
        let r = reward(src);
        keys.insert(novelty_key(src));
        match r.class {
            "compile" => compile += 1,
            "run" => run_f += 1,
            "gate" => gate += 1,
            "ok" => ok += 1,
            other => unreachable!("unknown rung class: {other}"),
        }
    }
    let n = rows.len().max(1) as f64;
    let parsed = (run_f + gate + ok) as f64;
    println!("=== prooflite validity eval ===");
    println!(
        "pool: {} programs | {} distinct source keys",
        rows.len(),
        keys.len()
    );
    println!(
        "parse rate {:.1}% | of parsers, run-clean {:.1}% | of all, RICH (ok) {:.1}%",
        100.0 * parsed / n,
        if parsed > 0.0 {
            100.0 * (gate + ok) as f64 / parsed
        } else {
            0.0
        },
        100.0 * ok as f64 / n
    );
    println!("ladder: compile {compile} | run {run_f} | gate {gate} | ok {ok}");
    Ok(())
}

/// Anti-memorization control. prooflite has no held-out DATA axis (it reads no
/// market data), so the generalization question is instead: are the model's
/// rich programs LEARNED or MEMORIZED? This reports, among a pool's ok-rung
/// programs, how many have a source-canonical key absent from `corpus` — the
/// human-authored cold-start set is the only external data the model saw, so
/// novelty against it is the honest "did it learn to write prooflite" signal.
fn cmd_novelty(pool_path: &str, corpus_path: &str) -> Result<(), String> {
    let corpus = read_pool(corpus_path)?;
    let corpus_keys: std::collections::BTreeSet<u64> =
        corpus.iter().map(|(_, s)| novelty_key(s)).collect();
    let rows = read_pool(pool_path)?;
    let (mut ok, mut novel_ok) = (0u32, 0u32);
    let mut ok_keys = std::collections::BTreeSet::new();
    let mut novel_keys = std::collections::BTreeSet::new();
    for (_, src) in &rows {
        if reward(src).class == "ok" {
            ok += 1;
            let k = novelty_key(src);
            ok_keys.insert(k);
            if !corpus_keys.contains(&k) {
                novel_ok += 1;
                novel_keys.insert(k);
            }
        }
    }
    println!(
        "=== prooflite novelty (rich programs vs the {}-program cold-start corpus) ===",
        corpus.len()
    );
    println!(
        "ok {ok} | novel ok {novel_ok} ({:.1}%) | distinct ok keys {} | distinct novel keys {}",
        100.0 * novel_ok as f64 / ok.max(1) as f64,
        ok_keys.len(),
        novel_keys.len()
    );
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("card") => {
            print!("{CARD}");
            Ok(())
        }
        Some("styles") => {
            for s in STYLES {
                println!("{s}");
            }
            Ok(())
        }
        Some("reward") => match args.get(1) {
            Some(p) => cmd_reward(p),
            None => Err("usage: p6 reward <pool.jsonl>".into()),
        },
        Some("eval") => match args.get(1) {
            Some(p) => cmd_eval(p),
            None => Err("usage: p6 eval <pool.jsonl>".into()),
        },
        Some("novelty") => match (args.get(1), args.get(2)) {
            (Some(p), Some(c)) => cmd_novelty(p, c),
            _ => Err("usage: p6 novelty <pool.jsonl> <corpus.jsonl>".into()),
        },
        _ => {
            eprint!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("p6: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cls(src: &str) -> &'static str {
        reward(src).class
    }

    #[test]
    fn the_ladder_separates_the_rungs() {
        // Does not parse -> compile rung.
        assert_eq!(cls("this is not prooflite"), "compile");
        assert_eq!(cls(""), "compile");
        // Parses, faults at eval (divide by zero is CHECKED) -> run rung.
        assert_eq!(cls("print 1 / 0;"), "run");
        // Reading an undefined variable is a runtime fault, not a parse error.
        assert_eq!(cls("print nope;"), "run");
        // Runs clean but trivial (one line, ~no fuel) -> gate rung.
        assert_eq!(cls("print 1;"), "gate");
        // Runs clean, varied output, real computation -> ok rung.
        let real = "let a = 0; repeat 5 { a = a + a + 1; print a; }";
        assert_eq!(cls(real), "ok");
    }

    #[test]
    fn a_rich_program_carries_its_facts() {
        let r = reward("let a = 1; repeat 4 { a = a * 2; print a; }");
        assert_eq!(r.class, "ok");
        assert!(r.fuel >= MIN_FUEL);
        assert!(r.distinct >= MIN_DISTINCT_LINES);
    }

    #[test]
    fn novelty_key_sees_through_comment_and_format_clones() {
        let a = "let x = 1; print x;";
        assert_eq!(
            novelty_key(a),
            novelty_key("let x = 1;   print x;  // note")
        );
        assert_ne!(novelty_key(a), novelty_key("let x = 2; print x;"));
    }

    #[test]
    fn novelty_key_agrees_with_the_s5_implementation() {
        // Pinned so p6's and s5's strip-and-collapse cannot silently diverge —
        // the N=2 "same method" claim requires byte-identical keys. s5 carries
        // the same test, same input and constant (experiment/src/reward.rs).
        assert_eq!(novelty_key("let a = 1; print a;"), 0x830a_f5b0_9ec6_541b);
    }

    #[test]
    fn novelty_counts_only_programs_absent_from_the_corpus() {
        // The exact membership decision cmd_novelty makes, at the key level.
        let corpus = "let a = 0; repeat 5 { a = a + a + 1; print a; }";
        let corpus_keys: std::collections::BTreeSet<u64> =
            [novelty_key(corpus)].into_iter().collect();
        // A reformatted/commented clone of a corpus program is NOT novel.
        assert!(corpus_keys.contains(&novelty_key(
            "let a = 0;  repeat 5 {  a = a + a + 1;  print a;  } // x"
        )));
        // A genuinely different rich program IS novel.
        assert!(!corpus_keys.contains(&novelty_key("let b = 1; repeat 4 { b = b * 3; print b; }")));
    }
}

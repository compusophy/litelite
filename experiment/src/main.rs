//! §5's harness: generate strategies with a model, verify them mechanically,
//! select, and score the picks on held-out candles.
//!
//! Scope, so the seams stay honest:
//!   * This tests verified SELECTION. The language-size claim is §4's.
//!   * The model is NOT reproducible — the Anthropic API has no seed, and
//!     temperature/top_p/top_k are REMOVED on Opus 4.8 (they 400). So the run
//!     RECORDS its generations rather than pretending to regenerate them, and
//!     reproducibility lives where it actually holds: `s5 score` is a pure
//!     function of committed artifacts + pinned candles, and every backtest
//!     collapses to one `Report::equity_hash`.
//!
//! Subcommands:
//!   s5 data    verify the pinned candles parse, print the split. No key.
//!   s5 score   re-derive every §5 number from committed artifacts. No key.

mod data;

use std::process::ExitCode;

const USAGE: &str = "\
s5 — the §5 experiment harness

  s5 data <csv>...   parse pinned klines, print the tick conversion + split
  s5 score           re-derive §5's tables from runs/ (pure; no network)
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("data") => match cmd_data(&args[1..]) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("s5 data: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprint!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn cmd_data(paths: &[String]) -> Result<(), String> {
    if paths.is_empty() {
        return Err("give me at least one klines CSV".into());
    }
    let mut all = Vec::new();
    for p in paths {
        let csv = std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?;
        let candles = data::parse(&csv).map_err(|e| format!("{p}: {e}"))?;
        println!("{p}: {} candles", candles.len());
        all.extend(candles);
    }

    // Prove the conversion satisfies the verifier's own contract before any
    // strategy runs — a bad candle is E0301, and finding that here beats
    // finding it inside a paid batch.
    for (i, c) in all.iter().enumerate() {
        let ok = 0 < c.low
            && c.low <= c.open.min(c.close)
            && c.open.max(c.close) <= c.high
            && c.high <= backtestlite::MAX_TICKS
            && (0..=backtestlite::MAX_TICKS).contains(&c.volume);
        if !ok {
            return Err(format!("candle {i} would fail the verifier: {c:?}"));
        }
    }

    let lo = all.iter().map(|c| c.low).min().unwrap_or(0);
    let hi = all.iter().map(|c| c.high).max().unwrap_or(0);
    println!(
        "total {} candles | price {}..{} ticks (cents) | headroom to 2^53: {:.0}x",
        all.len(),
        lo,
        hi,
        backtestlite::MAX_TICKS as f64 / hi.max(1) as f64
    );
    println!("all candles satisfy the verifier's coherence + range contract");
    Ok(())
}

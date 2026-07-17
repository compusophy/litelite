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
//! Intended order: `data` (no key) -> `pilot` (cents) -> `submit` -> `poll`
//! -> `score` (no key). The pilot exists because the step after it is the one
//! that costs money: it surfaces a rejected schema, an unexpected stop_reason,
//! or a refusal rate in seconds instead of after a paid batch.

mod api;
mod data;
mod reward;
mod score;

use score::Split;
use std::process::ExitCode;

const USAGE: &str = "\
s5 — the §5 experiment harness + M6 reward oracle

  s5 data   <csv>...              parse pinned klines, check the verifier's contract
  s5 pilot                        a few SYNC requests before any batch is billed
  s5 submit <n> <out.json>        submit a batch of n candidates; records the id
  s5 poll   <batch_id> <out>      poll until ended, write results verbatim
  s5 score  <raw.jsonl> <csv>...  re-derive §5's numbers (pure; no network)
  s5 reward <pool.jsonl> <csv>... M6: verifier reward per rollout (pure; no network)
  s5 card                         print stratlite::REFERENCE — the ONE prompt card
  s5 styles                       print the diversity styles, one per line
";

/// The recorded diversity axis. With no temperature and no seed, THIS is what
/// makes candidates differ — so it is data, it is committed, and it is
/// identical across replicates (which is what makes them replicates).
const STYLES: &[&str] = &[
    "a trend-following strategy using a fast/slow sma crossover",
    "a mean-reversion strategy using rsi extremes",
    "a breakout strategy using highest() and lowest() channels",
    "a momentum strategy using ema and a position() check",
    "a strategy that uses var state to avoid flipping position every bar",
    "a strategy that goes flat when the recent range is narrow",
    "a strategy combining an rsi filter with an sma trend check",
    "a conservative strategy that trades rarely",
];

/// Chronological 60/40 — never shuffled. Shuffling bars would destroy the
/// only structure a market series has.
const TRAIN_FRACTION: f64 = 0.6;
/// Binance spot taker fee, plus conservative slippage. Adverse by
/// construction: backtestlite rejects negative costs (E0304) precisely
/// because negative slippage would silently invert the guarantee.
const FEE_BPS: i64 = 5;
const SLIP_BPS: i64 = 1;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("data") => cmd_data(&args[1..]),
        Some("pilot") => cmd_pilot(),
        Some("submit") => cmd_submit(&args[1..]),
        Some("poll") => cmd_poll(&args[1..]),
        Some("score") => cmd_score(&args[1..]),
        Some("reward") => cmd_reward(&args[1..]),
        // The trainer reads the card and styles from HERE rather than copying
        // them, so the prompt the model learns and the language the verifier
        // enforces stay ONE artifact across the Rust/Python boundary.
        Some("card") => {
            print!("{}", stratlite::REFERENCE);
            Ok(())
        }
        Some("styles") => {
            for s in STYLES {
                println!("{s}");
            }
            Ok(())
        }
        _ => {
            eprint!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("s5: {e}");
            ExitCode::FAILURE
        }
    }
}

fn load(paths: &[String]) -> Result<Vec<stratlite::Candle>, String> {
    if paths.is_empty() {
        return Err("give me at least one klines CSV".into());
    }
    let mut all = Vec::new();
    for p in paths {
        let csv = std::fs::read_to_string(p).map_err(|e| format!("{p}: {e}"))?;
        all.extend(data::parse(&csv).map_err(|e| format!("{p}: {e}"))?);
    }
    Ok(all)
}

fn cmd_data(paths: &[String]) -> Result<(), String> {
    let all = load(paths)?;
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
    let split = Split::at_fraction(all.len(), TRAIN_FRACTION);
    let costs = score::costs_from_train(split.train(&all), FEE_BPS, SLIP_BPS);
    let hi = all.iter().map(|c| c.high).max().unwrap_or(1);
    println!("{} candles | price max {hi} ticks (cents)", all.len());
    println!(
        "  headroom to 2^53: {:.0}x | all satisfy the verifier's contract",
        backtestlite::MAX_TICKS as f64 / hi.max(1) as f64
    );
    println!(
        "split: train 0..{} | held-out {}..{}",
        split.train_end,
        split.train_end,
        all.len()
    );
    println!(
        "costs from TRAIN only: fee {} slip {} ticks | drift held-out/train {:.3}x",
        costs.fee_ticks,
        costs.slippage_ticks,
        score::price_drift(&all, &split)
    );
    Ok(())
}

fn cmd_pilot() -> Result<(), String> {
    let styles: Vec<String> = STYLES.iter().take(3).map(|s| s.to_string()).collect();
    println!(
        "pilot: {} SYNC requests to {} — no batch is billed",
        styles.len(),
        api::MODEL
    );
    let out = api::pilot(&styles)?;
    let mut usable = 0;
    for (style, r) in &out {
        match r {
            Ok(src) => {
                // Compile immediately: the point of a pilot is to learn NOW
                // whether the card actually yields the language.
                match stratlite::compile(src) {
                    Ok(s) => {
                        usable += 1;
                        println!("  OK   lookback {} | {style}", s.lookback());
                    }
                    Err(d) => println!("  BAD  does not compile: {d} | {style}"),
                }
            }
            Err(e) => println!("  FAIL {e} | {style}"),
        }
    }
    println!("pilot: {usable}/{} compile", out.len());
    Ok(())
}

fn cmd_submit(args: &[String]) -> Result<(), String> {
    let n: usize = args
        .first()
        .ok_or("usage: s5 submit <n> <out.json>")?
        .parse()
        .map_err(|e| format!("n: {e}"))?;
    let out = args.get(1).ok_or("usage: s5 submit <n> <out.json>")?;
    let reqs: Vec<(String, serde_json::Value)> = (0..n)
        .map(|i| {
            (
                format!("c{i:03}"),
                api::params(i as u32, STYLES[i % STYLES.len()]),
            )
        })
        .collect();
    let id = api::submit(&reqs)?;
    // Record the id BEFORE anything interprets the batch: if this process dies
    // now, the batch is still findable and the spend is not orphaned.
    std::fs::write(
        out,
        format!(
            "{{\"batch_id\":\"{id}\",\"n\":{n},\"model\":\"{}\"}}\n",
            api::MODEL
        ),
    )
    .map_err(|e| format!("{out}: {e}"))?;
    println!("submitted {n} candidates as {id} -> {out}");
    Ok(())
}

fn cmd_poll(args: &[String]) -> Result<(), String> {
    let id = args
        .first()
        .ok_or("usage: s5 poll <batch_id> <out.jsonl>")?;
    let out = args.get(1).ok_or("usage: s5 poll <batch_id> <out.jsonl>")?;
    while !api::ended(id)? {
        println!("  still processing; sleeping 60s");
        std::thread::sleep(std::time::Duration::from_secs(60));
    }
    let raw = api::results(id)?;
    // Verbatim, before interpretation. This file IS the run's evidence: the
    // generation can never be repeated, so unrecorded is gone.
    std::fs::write(out, &raw).map_err(|e| format!("{out}: {e}"))?;
    println!("{id}: ended | {} bytes -> {out}", raw.len());
    Ok(())
}

fn cmd_score(args: &[String]) -> Result<(), String> {
    let raw_path = args.first().ok_or("usage: s5 score <raw.jsonl> <csv>...")?;
    let all = load(&args[1..])?;
    let raw = std::fs::read_to_string(raw_path).map_err(|e| format!("{raw_path}: {e}"))?;

    // Batch results arrive in ANY order — key by custom_id, never by position.
    let mut pool: Vec<api::Attempt> = Vec::new();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let v: serde_json::Value = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let id = v["custom_id"].as_str().unwrap_or("?").to_string();
        let got = match v["result"]["type"].as_str() {
            Some("succeeded") => api::extract(&v["result"]["message"]),
            Some(other) => Err(other.to_string()),
            None => Err("no result type".into()),
        };
        pool.push((id, got));
    }
    pool.sort_by(|a, b| a.0.cmp(&b.0));

    let generated: Vec<String> = pool.iter().filter_map(|(_, r)| r.clone().ok()).collect();
    let refused = pool.len() - generated.len();

    let split = Split::at_fraction(all.len(), TRAIN_FRACTION);
    let costs = score::costs_from_train(split.train(&all), FEE_BPS, SLIP_BPS);
    let limits = stratlite::Limits::default();
    let gate = backtestlite::Gate::default();

    let results = score::verify_pool(&generated, split.train(&all), limits, costs, gate);
    let hist = score::Histogram::tally(&results);
    let fuel = score::fuel_distribution(&results);

    println!("=== §5 ===");
    println!(
        "pool: {} requested | {} generated | {refused} refused/failed",
        pool.len(),
        generated.len()
    );
    println!(
        "Reject histogram (train): survived {} | compile {} | run {} | gate {}",
        hist.survived, hist.compile, hist.run, hist.gate
    );

    // The kit's central thesis, MEASURED rather than counted. A distribution
    // hugging the low end means the termination bound never bound on this task
    // and §6 must say so; a tail pressing the cap is the evidence that
    // smallness bought something real.
    if let (Some(&lo), Some(&hi)) = (fuel.first(), fuel.last()) {
        println!(
            "fuel/bar over survivors: min {lo} | median {} | max {hi} | cap {} ({:.1}% of cap at max)",
            fuel[fuel.len() / 2],
            limits.fuel_per_bar,
            100.0 * hi as f64 / limits.fuel_per_bar as f64
        );
    }

    match score::pick_verified(&results) {
        Some(i) => {
            let (pnl, rep) = score::score_heldout(&generated[i], &all, &split, limits, costs);
            println!("arm V  (verified pick): candidate {i} | held-out net {pnl} ticks");
            if let Some(r) = rep {
                println!(
                    "  trades {} | equity_hash 0x{:016x}",
                    r.trades, r.equity_hash
                );
            }
        }
        None => println!("arm V: no survivors — the pool produced nothing deployable"),
    }
    // Arm U1: the naive user. Ask once, ship it. Free — no extra API call.
    let u1_pick = (!generated.is_empty()).then_some(0usize);
    let (u1, _) = generated
        .first()
        .map(|s| score::score_heldout(s, &all, &split, limits, costs))
        .unwrap_or((0, None));
    println!("arm U1 (candidate 0, unverified): held-out net {u1} ticks");

    // How often physics and testimony land on the same program. Free, and the
    // repo's central metaphor as a number — and it has to be reported, because
    // an exact sign test drops ties and an unreported tie rate silently
    // inflates the power calculation over replicates.
    let (same, n) = score::agreement(&[score::pick_verified(&results)], &[u1_pick]);
    println!("arm agreement V vs U1: {same}/{n} comparable pairs");
    println!(
        "price drift held-out/train {:.3}x — the absolute fee is fit to train, \
         so this quantifies the cost-model bias against arm V",
        score::price_drift(&all, &split)
    );
    Ok(())
}

/// M6: score a batch of model rollouts with the verifier reward. Input is
/// JSONL, one `{"id","source"}` per line (what a trainer emits); output is
/// JSONL, one reward record per input line, IN THE SAME ORDER and carrying the
/// id — so an async/batched trainer can map rewards back to rollouts. Pure and
/// deterministic: no network, no key, and the same rollouts always score the
/// same. This is the boundary the Python trainer shells out to.
fn cmd_reward(args: &[String]) -> Result<(), String> {
    let pool_path = args
        .first()
        .ok_or("usage: s5 reward <pool.jsonl> <csv>...")?;
    let all = load(&args[1..])?;
    let pool = std::fs::read_to_string(pool_path).map_err(|e| format!("{pool_path}: {e}"))?;

    let split = Split::at_fraction(all.len(), TRAIN_FRACTION);
    let costs = score::costs_from_train(split.train(&all), FEE_BPS, SLIP_BPS);
    let limits = stratlite::Limits::default();
    let gate = backtestlite::Gate::default();

    // Parse rollouts, preserving id and order. A missing/non-string source is
    // not an error — it is an empty rollout, a compile-class zero, exactly like
    // the model emitting junk.
    let mut ids: Vec<String> = Vec::new();
    let mut sources: Vec<String> = Vec::new();
    for (n, line) in pool.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let v: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", n + 1))?;
        ids.push(v["id"].as_str().unwrap_or("?").to_string());
        sources.push(v["source"].as_str().unwrap_or("").to_string());
    }

    let rewards = reward::reward_pool(&sources, &all, &split, limits, costs, gate);
    let mut out = String::new();
    for ((id, src), r) in ids.iter().zip(&sources).zip(&rewards) {
        // `nkey` is the source-canonical dedup key — emitted so the trainer can
        // resist mode collapse at every rung, including below survivor where
        // `hash` is 0 for everything.
        out.push_str(&format!(
            "{{\"id\":{},\"value\":{:.6},\"class\":\"{}\",\"fuel\":{},\"trades\":{},\"train_pnl\":{},\"hash\":\"0x{:016x}\",\"nkey\":\"0x{:016x}\"}}\n",
            serde_json::to_string(id).unwrap(),
            r.value,
            r.class.as_str(),
            r.fuel,
            r.trades,
            r.train_pnl,
            r.hash,
            reward::novelty_key(src)
        ));
    }
    print!("{out}");
    Ok(())
}

//! s5: the §5 selection harness + the M6 reward/eval tooling (design:
//! `../M6.md`). Verified SELECTION, not the language-size claim (that is §4's).
//! The generator is NOT reproducible (no seed; temperature/top_p/top_k 400 on
//! Opus 4.8) so the run RECORDS generations; reproducibility lives in the
//! deterministic verifier — `score`/`reward`/`eval` are pure functions of
//! committed artifacts + pinned candles. §5 order: data -> pilot -> submit ->
//! poll -> score. M6: reward (per-rollout) and eval (the conditional metric).

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
  s5 eval   <pool.jsonl> <csv>... the CONDITIONAL held-out metric + the gap
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
        Some("eval") => cmd_eval(&args[1..]),
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

/// A rollout pool line: `{id, source, style}`, each field optional. A missing
/// source is not an error — it is an empty rollout, a compile-class zero,
/// exactly like the model emitting junk. Used by `reward` and `eval`.
fn read_pool(path: &str) -> Result<Vec<(String, String, String)>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut rows = Vec::new();
    for (n, line) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let v: serde_json::Value =
            serde_json::from_str(line).map_err(|e| format!("line {}: {e}", n + 1))?;
        rows.push((
            v["id"].as_str().unwrap_or("?").to_string(),
            v["source"].as_str().unwrap_or("").to_string(),
            v["style"].as_str().unwrap_or("").to_string(),
        ));
    }
    Ok(rows)
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
    // Record the id BEFORE interpreting the batch — else a crash orphans spend.
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
    // Verbatim, before interpretation — the generation cannot be repeated.
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

    // Fuel MEASURED not counted — the fuel-bound evidence (see score.rs).
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

    // Physics-vs-testimony agreement — reported because a sign test drops ties.
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
    let rows = read_pool(pool_path)?;

    let split = Split::at_fraction(all.len(), TRAIN_FRACTION);
    let costs = score::costs_from_train(split.train(&all), FEE_BPS, SLIP_BPS);
    let limits = stratlite::Limits::default();
    let gate = backtestlite::Gate::default();

    let sources: Vec<String> = rows.iter().map(|(_, s, _)| s.clone()).collect();
    let rewards = reward::reward_pool(&sources, &all, &split, limits, costs, gate);
    let mut out = String::new();
    for ((id, src, _), r) in rows.iter().zip(&rewards) {
        // `nkey` is the source-canonical dedup key for the trainer (see reward.rs).
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

/// The CONDITIONAL held-out metric — the honest answer to the trap that raw
/// survivor rate hides (the compile rung is data-independent, so a raw lift can
/// be grammar-learning). Input is JSONL {source, optional style}; stratifies by
/// style when present, because aggregate rate can rise by collapsing to the
/// single easiest style.
fn cmd_eval(args: &[String]) -> Result<(), String> {
    let pool_path = args.first().ok_or("usage: s5 eval <pool.jsonl> <csv>...")?;
    let all = load(&args[1..])?;
    let pool = read_pool(pool_path)?;

    let split = Split::at_fraction(all.len(), TRAIN_FRACTION);
    let costs = score::costs_from_train(split.train(&all), FEE_BPS, SLIP_BPS);
    let limits = stratlite::Limits::default();
    let gate = backtestlite::Gate::default();

    let sources: Vec<String> = pool.iter().map(|(_, s, _)| s.clone()).collect();
    let styles: Vec<String> = pool.iter().map(|(_, _, st)| st.clone()).collect();
    let rows = score::eval_pool(&sources, &all, &split, limits, costs, gate);
    let (compile_rate, train, held) = score::conditional_rates(&rows);
    let gap = train - held;

    println!("=== conditional held-out eval ===");
    println!(
        "pool: {} programs | compile rate {:.1}%",
        rows.len(),
        100.0 * compile_rate
    );
    println!(
        "among compilers: gate-clear TRAIN {:.1}% | HELD-OUT {:.1}%",
        100.0 * train,
        100.0 * held
    );
    println!(
        "GAP (train - heldout) = {:.1} points — {}",
        100.0 * gap,
        if gap.abs() < 0.05 {
            "NEAR ZERO: held-out is no harder than train, the benchmark has no out-of-sample teeth"
        } else {
            "positive: held-out discriminates, so raising held-out clear is a real lift"
        }
    );

    // Per-style — an aggregate rate can rise by collapsing to the easiest style.
    if styles.iter().any(|s| !s.is_empty()) {
        println!("per-style held-out gate-clear (among that style's compilers):");
        let mut seen: Vec<&str> = styles
            .iter()
            .map(String::as_str)
            .filter(|s| !s.is_empty())
            .collect();
        seen.sort_unstable();
        seen.dedup();
        for st in seen {
            let comp: Vec<usize> = (0..rows.len())
                .filter(|&i| styles[i] == st && rows[i].compiles)
                .collect();
            let clear = comp.iter().filter(|&&i| rows[i].heldout_clear).count();
            let rate = if comp.is_empty() {
                0.0
            } else {
                clear as f64 / comp.len() as f64
            };
            println!(
                "  {st:10} {:.0}% ({}/{} compilers)",
                100.0 * rate,
                clear,
                comp.len()
            );
        }
    }
    Ok(())
}

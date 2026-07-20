//! a8 — the applite reward tool: teach a small local model to write apps.
//!
//! Same CLI protocol as s5/p6, so the language-parametric trainer runs
//! unchanged. What is NEW is the top rung: BEHAVIORAL correctness. A task is
//! a spec plus an event SCRIPT — clicks by button label, typing into inputs,
//! and assertions over the rendered labels. The reward ladder:
//!
//!   compile (0)   does not compile/check (applite's parse or static check)
//!   run     (1/3) compiles, but a script event or render FAULTS
//!   gate    (2/3) runs clean, but a script target is missing or an
//!                 assertion fails — a valid app that is NOT the asked app
//!   ok      (1)   every script step passes: the app DOES what was asked
//!
//! This is §5.8's lesson built in from day one: the reward is the spec,
//! never an output shape — so self-play cannot imprint a shape that
//! overrides the user's ask.

use applite::{App, Event, Limits, Node, Program, REFERENCE, compile};
use serde_json::Value;
use std::process::ExitCode;

struct Reward {
    value: f64,
    class: &'static str,
}

/// One script step, parsed from a task's `script` array.
#[derive(Debug)]
enum Step {
    /// `{"click": "text"}` — click the first button whose label is `text`.
    Click(String),
    /// `{"type": ["text"]}` / `{"type": [i, "text"]}` — type into the i-th
    /// input field (0-based, render order).
    Type(usize, String),
    /// `{"assert_shows": "text"}` — some label CONTAINS `text`.
    Shows(String),
    /// `{"assert_exact": "text"}` — some label EQUALS `text` exactly. Use
    /// this for numbers: "shows 1" by substring would also accept "-1".
    Exact(String),
    /// `{"assert_hides": "text"}` — no label contains `text`.
    Hides(String),
}

fn parse_step(v: &Value) -> Result<Step, String> {
    if let Some(t) = v["click"].as_str() {
        return Ok(Step::Click(t.to_string()));
    }
    if let Some(arr) = v["type"].as_array() {
        return match arr.as_slice() {
            [Value::String(t)] => Ok(Step::Type(0, t.clone())),
            [Value::Number(i), Value::String(t)] => {
                Ok(Step::Type(i.as_u64().unwrap_or(0) as usize, t.clone()))
            }
            _ => Err("bad `type` step".to_string()),
        };
    }
    if let Some(t) = v["assert_shows"].as_str() {
        return Ok(Step::Shows(t.to_string()));
    }
    if let Some(t) = v["assert_exact"].as_str() {
        return Ok(Step::Exact(t.to_string()));
    }
    if let Some(t) = v["assert_hides"].as_str() {
        return Ok(Step::Hides(t.to_string()));
    }
    Err(format!("unknown script step: {v}"))
}

/// Walk a render tree collecting (labels, buttons, input state names).
fn flatten(
    nodes: &[Node],
    labels: &mut Vec<String>,
    buttons: &mut Vec<(String, u32)>,
    inputs: &mut Vec<String>,
) {
    for n in nodes {
        match n {
            Node::Label { text } => labels.push(text.clone()),
            Node::Button { text, id } => buttons.push((text.clone(), *id)),
            Node::Input { state, .. } => inputs.push(state.clone()),
            Node::Row { children } | Node::Col { children } => {
                flatten(children, labels, buttons, inputs)
            }
        }
    }
}

enum ScriptOut {
    Ok,
    /// A target was missing or an assertion failed (with a message).
    Gate(String),
    /// An event or render faulted.
    Fault(String),
}

/// Run `script` against a fresh instance of `program`.
fn run_script(program: Program, script: &[Step]) -> ScriptOut {
    let mut app = App::new(program, Limits::default());
    for step in script {
        let nodes = match app.render() {
            Ok(n) => n,
            Err(d) => return ScriptOut::Fault(format!("render fault: {d}")),
        };
        let (mut labels, mut buttons, mut inputs) = (Vec::new(), Vec::new(), Vec::new());
        flatten(&nodes, &mut labels, &mut buttons, &mut inputs);
        match step {
            Step::Click(text) => {
                let Some((_, id)) = buttons.iter().find(|(t, _)| t == text) else {
                    return ScriptOut::Gate(format!("no button labeled `{text}`"));
                };
                if let Err(d) = app.handle(&Event::Click { id: *id }) {
                    return ScriptOut::Fault(format!("click `{text}` faulted: {d}"));
                }
            }
            Step::Type(i, text) => {
                let Some(state) = inputs.get(*i) else {
                    return ScriptOut::Gate(format!("no input field #{i}"));
                };
                let ev = Event::Input {
                    state: state.clone(),
                    text: text.clone(),
                };
                if let Err(d) = app.handle(&ev) {
                    return ScriptOut::Fault(format!("typing into #{i} faulted: {d}"));
                }
            }
            Step::Shows(text) => {
                if !labels.iter().any(|l| l.contains(text.as_str())) {
                    return ScriptOut::Gate(format!("nothing shows `{text}`"));
                }
            }
            Step::Exact(text) => {
                if !labels.iter().any(|l| l == text) {
                    return ScriptOut::Gate(format!("no label equals `{text}`"));
                }
            }
            Step::Hides(text) => {
                if labels.iter().any(|l| l.contains(text.as_str())) {
                    return ScriptOut::Gate(format!("`{text}` is shown but must not be"));
                }
            }
        }
    }
    ScriptOut::Ok
}

fn reward(src: &str, script: &[Step]) -> (Reward, String) {
    if src.trim().is_empty() {
        return (
            Reward {
                value: 0.0,
                class: "compile",
            },
            "empty".to_string(),
        );
    }
    let program = match compile(src) {
        Ok(p) => p,
        Err(d) => {
            return (
                Reward {
                    value: 0.0,
                    class: "compile",
                },
                d.to_string(),
            );
        }
    };
    match run_script(program, script) {
        ScriptOut::Ok => (
            Reward {
                value: 1.0,
                class: "ok",
            },
            String::new(),
        ),
        ScriptOut::Gate(m) => (
            Reward {
                value: 2.0 / 3.0,
                class: "gate",
            },
            m,
        ),
        ScriptOut::Fault(m) => (
            Reward {
                value: 1.0 / 3.0,
                class: "run",
            },
            m,
        ),
    }
}

/// Source-canonical dedup key — FNV-1a-64 over comment-stripped, whitespace-
/// collapsed source (byte-identical to s5/p6's; whitespace inside string
/// literals collapses too, which conflates only near-clones).
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

fn read_text(path: &str) -> Result<String, String> {
    if path == "-" {
        std::io::read_to_string(std::io::stdin()).map_err(|e| format!("stdin: {e}"))
    } else {
        std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))
    }
}

/// Tasks: (id, spec, script, reference source). Every reference must itself
/// pass its script — checked on load, so a broken task cannot mis-teach.
/// (id, spec, script, reference source).
type Task = (String, String, Vec<Step>, String);

fn read_tasks(path: &str) -> Result<Vec<Task>, String> {
    let mut out = Vec::new();
    for (n, line) in read_text(path)?
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        let v: Value =
            serde_json::from_str(line).map_err(|e| format!("task line {}: {e}", n + 1))?;
        let id = v["id"].as_str().unwrap_or("?").to_string();
        let spec = v["spec"].as_str().unwrap_or("").to_string();
        let script: Vec<Step> = v["script"]
            .as_array()
            .ok_or_else(|| format!("task {id}: no script"))?
            .iter()
            .map(parse_step)
            .collect::<Result<_, _>>()
            .map_err(|e| format!("task {id}: {e}"))?;
        let reference = v["ref"].as_str().unwrap_or("").to_string();
        let (r, why) = reward(&reference, &script);
        if r.class != "ok" {
            return Err(format!(
                "task {id}: its own reference fails ({}: {why})",
                r.class
            ));
        }
        out.push((id, spec, script, reference));
    }
    Ok(out)
}

fn read_pool(path: &str) -> Result<Vec<(String, String)>, String> {
    let text = read_text(path)?;
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

/// The style index a trainer rollout id carries (`r<round>s<idx>n<j>`).
fn style_index(id: &str) -> Result<usize, String> {
    let s = id
        .find('s')
        .ok_or_else(|| format!("id {id}: no s<idx> segment"))?;
    let digits: String = id[s + 1..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse()
        .map_err(|_| format!("id {id}: no digits after s"))
}

fn cmd_trainstyles(tasks_path: &str) -> Result<(), String> {
    for (_, spec, _, _) in read_tasks(tasks_path)? {
        println!("an app that {spec}");
    }
    Ok(())
}

/// Per-rollout spec-conditioned reward records, `s5 reward`-compatible.
fn cmd_solvereward(tasks_path: &str, pool_path: &str) -> Result<(), String> {
    let tasks = read_tasks(tasks_path)?;
    let mut out = String::new();
    for (id, src) in read_pool(pool_path)? {
        let si = style_index(&id)?;
        let (_, _, script, _) = tasks
            .get(si)
            .ok_or_else(|| format!("id {id}: style index {si} >= {} tasks", tasks.len()))?;
        let (r, _) = reward(&src, script);
        out.push_str(&format!(
            "{{\"id\":{},\"value\":{:.6},\"class\":\"{}\",\"fuel\":0,\"distinct\":0,\"lines\":0,\"nkey\":\"0x{:016x}\"}}\n",
            serde_json::to_string(&id).unwrap(),
            r.value, r.class, novelty_key(&src)
        ));
    }
    print!("{out}");
    Ok(())
}

/// pass@k per task over a solutions pool (`{id, source}`, many per id) —
/// the held-out benchmark. Also prints the per-attempt ladder histogram.
fn cmd_eval(tasks_path: &str, pool_path: &str) -> Result<(), String> {
    use std::collections::BTreeMap;
    let tasks = read_tasks(tasks_path)?;
    let mut sols: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (id, src) in read_pool(pool_path)? {
        sols.entry(id).or_default().push(src);
    }
    let (mut passed, mut hist) = (0u32, BTreeMap::<&str, u32>::new());
    let mut lines = String::new();
    for (id, _, script, _) in &tasks {
        let attempts = sols.get(id).map(Vec::as_slice).unwrap_or(&[]);
        let mut ok = false;
        for src in attempts {
            let (r, _) = reward(src, script);
            *hist.entry(r.class).or_default() += 1;
            ok |= r.class == "ok";
        }
        passed += ok as u32;
        lines.push_str(&format!(
            "  {} {id} ({} tries)\n",
            if ok { "PASS" } else { "fail" },
            attempts.len()
        ));
    }
    println!("=== applite behavioral eval (event scripts + assertions) ===");
    println!(
        "tasks {} | attempts {} | PASSED (pass@k) {}/{} = {:.1}%",
        tasks.len(),
        sols.values().map(Vec::len).sum::<usize>(),
        passed,
        tasks.len(),
        100.0 * passed as f64 / tasks.len().max(1) as f64
    );
    println!("attempt ladder: {hist:?}");
    print!("{lines}");
    Ok(())
}

/// Compile + first render of a raw program, as text — a debug aid.
fn cmd_run(path: &str) -> Result<(), String> {
    let src = read_text(path)?;
    let program = compile(&src).map_err(|d| d.render(&src))?;
    let app = App::new(program, Limits::default());
    let nodes = app.render().map_err(|d| d.to_string())?;
    let (mut labels, mut buttons, mut inputs) = (Vec::new(), Vec::new(), Vec::new());
    flatten(&nodes, &mut labels, &mut buttons, &mut inputs);
    for l in labels {
        println!("label: {l}");
    }
    for (t, id) in buttons {
        println!("button[{id}]: {t}");
    }
    for s in inputs {
        println!("input: {s}");
    }
    Ok(())
}

const USAGE: &str = "\
a8 — the applite reward tool (M7's generator arm)

  a8 card                          print the applite prompt card (REFERENCE)
  a8 trainstyles <tasks.jsonl>     task specs as sampling prompts, one per line
  a8 solvereward <tasks> <pool|->  spec-conditioned reward: top rung = the SCRIPT passes
  a8 eval        <tasks> <pool>    behavioral pass@k over a solutions pool
  a8 run         <program | ->     compile + print the first render
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let r = match args.first().map(String::as_str) {
        Some("card") => {
            print!("{REFERENCE}");
            Ok(())
        }
        Some("trainstyles") => match args.get(1) {
            Some(p) => cmd_trainstyles(p),
            None => Err("usage: a8 trainstyles <tasks.jsonl>".into()),
        },
        Some("solvereward") => match (args.get(1), args.get(2)) {
            (Some(t), Some(p)) => cmd_solvereward(t, p),
            _ => Err("usage: a8 solvereward <tasks.jsonl> <pool.jsonl | ->".into()),
        },
        Some("eval") => match (args.get(1), args.get(2)) {
            (Some(t), Some(p)) => cmd_eval(t, p),
            _ => Err("usage: a8 eval <tasks.jsonl> <pool.jsonl>".into()),
        },
        Some("run") => match args.get(1) {
            Some(p) => cmd_run(p),
            None => Err("usage: a8 run <program.txt | ->".into()),
        },
        _ => {
            eprint!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("a8: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps(json: &str) -> Vec<Step> {
        serde_json::from_str::<Value>(json)
            .unwrap()
            .as_array()
            .unwrap()
            .iter()
            .map(parse_step)
            .map(Result::unwrap)
            .collect()
    }

    #[test]
    fn the_ladder_is_behavioral() {
        // assert_exact for the number: "shows 1" by substring would also
        // accept the decrementing app's "-1" — the bug this test now pins.
        let script = steps(
            r#"[{"assert_exact":"0"},{"click":"+"},{"assert_exact":"1"},{"assert_hides":"boom"}]"#,
        );
        // ok: does what was asked.
        let good = "state n = 0; label n; button \"+\" { n = n + 1; }";
        assert_eq!(reward(good, &script).0.class, "ok");
        // gate: VALID app, wrong behavior (decrements) — §5.8's whole point.
        let wrong = "state n = 0; label n; button \"+\" { n = n - 1; }";
        assert_eq!(reward(wrong, &script).0.class, "gate");
        // gate: no such button.
        let unlabeled = "state n = 0; label n; button \"inc\" { n = n + 1; }";
        assert_eq!(reward(unlabeled, &script).0.class, "gate");
        // run: the click faults (div by zero) — rolled back AND penalized.
        let faulty = "state n = 0; label n; button \"+\" { n = 1 / n; }";
        assert_eq!(reward(faulty, &script).0.class, "run");
        // compile: not applite.
        assert_eq!(reward("not a program", &script).0.class, "compile");
        assert_eq!(reward("", &script).0.class, "compile");
    }

    #[test]
    fn typing_targets_inputs_by_position() {
        let script = steps(r#"[{"type":["Ada"]},{"assert_shows":"hi Ada"}]"#);
        let app = "state s = \"\"; input s; label \"hi \" + s;";
        assert_eq!(reward(app, &script).0.class, "ok");
        let no_input = "state s = \"\"; label \"hi \" + s;";
        assert_eq!(reward(no_input, &script).0.class, "gate");
    }

    #[test]
    fn tasks_verify_their_own_references_on_load() {
        let dir = std::env::temp_dir().join("a8_task_test.jsonl");
        std::fs::write(
            &dir,
            r#"{"id":"t","spec":"x","script":[{"assert_shows":"7"}],"ref":"label 3 + 4;"}"#,
        )
        .unwrap();
        assert_eq!(read_tasks(dir.to_str().unwrap()).unwrap().len(), 1);
        std::fs::write(
            &dir,
            r#"{"id":"t","spec":"x","script":[{"assert_shows":"8"}],"ref":"label 3 + 4;"}"#,
        )
        .unwrap();
        assert!(
            read_tasks(dir.to_str().unwrap())
                .unwrap_err()
                .contains("reference fails")
        );
        let _ = std::fs::remove_file(dir);
    }

    #[test]
    fn nkey_agrees_with_the_s5_implementation() {
        // Pinned: the N-language "same method" claim needs byte-identical keys.
        assert_eq!(novelty_key("let a = 1; print a;"), 0x830a_f5b0_9ec6_541b);
    }
}

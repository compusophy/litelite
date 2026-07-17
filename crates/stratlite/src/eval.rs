//! stratlite evaluator: one fueled tree-walk per CLOSED bar, driven by a
//! [`Session`] — the same mechanism serves the in-crate backtester and a
//! live trader. Each bar gets a FRESH tank of `fuel_per_bar`; within the bar
//! that one tank feeds every sub-evaluation (the fuellite rule), so every
//! DECISION halts within the bound, independent of series length.
//!
//! The world a bar sees, exhaustively: the ring of the last `lookback + 1`
//! candles (backward offsets only — the future has no name), the `var`
//! slots, and the engine-reported position. Nothing else exists: no clock,
//! no randomness, no I/O, no output channel — a strategy's entire observable
//! behavior is its signal sequence.

use std::collections::VecDeque;

use diaglite::{Diag, Span};
use fuellite::Fuel;

use crate::parse::{BinOp, Builtin, Expr, Program, Stmt, UnOp};
use crate::{Candle, Limits, Signal, Value, codes};

fn kind_name(v: Value) -> &'static str {
    match v {
        Value::Int(_) => "an integer",
        Value::Bool(_) => "a boolean",
    }
}

fn mismatch(msg: String, sp: Span) -> Diag {
    Diag::at_code(codes::TYPE_MISMATCH, msg, sp)
}

fn overflow(op: &str, sp: Span) -> Diag {
    Diag::at_code(
        codes::OVERFLOW,
        format!("`{op}` overflowed the 64-bit integer range"),
        sp,
    )
}

/// Lexical scopes: a flat binding stack plus frame marks. The bottom frame is
/// the `var` slots (persistent across bars); everything above dies with the bar.
struct Scopes {
    vars: Vec<(String, Value)>,
    frames: Vec<usize>,
}

impl Scopes {
    fn push(&mut self) {
        self.frames.push(self.vars.len());
    }
    fn pop(&mut self) {
        let mark = self.frames.pop().unwrap_or(0);
        self.vars.truncate(mark);
    }
    fn define(&mut self, name: &str, v: Value) {
        self.vars.push((name.to_string(), v));
    }
    fn get(&self, name: &str) -> Option<Value> {
        self.vars
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| *v)
    }
    fn assign(&mut self, name: &str, v: Value) -> bool {
        match self.vars.iter_mut().rev().find(|(n, _)| n == name) {
            Some(slot) => {
                slot.1 = v;
                true
            }
            None => false,
        }
    }
}

/// Per-bar stepping over a compiled strategy — the backtester's core and the
/// trader agent's live seam. Feed CLOSED candles with [`step`](Self::step);
/// report actual fills with [`set_position`](Self::set_position).
pub struct Session<'s> {
    program: &'s Program,
    limits: Limits,
    /// The last `lookback + 1` candles; index `len-1-k` is `k` bars ago.
    ring: VecDeque<Candle>,
    bars_seen: usize,
    /// Persistent `var` slot values, in declaration order.
    slots: Vec<Value>,
    /// The standing target: position carries unless a `signal` changes it.
    standing: Signal,
    /// Engine-reported position (-1/0/1) and entry fill price in ticks.
    position: i64,
    entry: i64,
    /// The worst observed bar — the strategy's real per-decision cost.
    max_fuel: u64,
}

impl<'s> Session<'s> {
    pub fn new(program: &'s Program, limits: Limits) -> Session<'s> {
        Session {
            program,
            limits,
            ring: VecDeque::with_capacity(program.lookback as usize + 1),
            bars_seen: 0,
            slots: program.vars.iter().map(|(_, v)| *v).collect(),
            standing: Signal::Flat,
            position: 0,
            entry: 0,
            max_fuel: 0,
        }
    }

    /// Bars whose body evaluation BEGAN (bars seen minus warmup) — a bar
    /// that faulted mid-body counts here, though its effects were discarded.
    pub fn bars_evaluated(&self) -> usize {
        self.bars_seen
            .saturating_sub(self.program.lookback as usize)
    }

    /// The worst per-bar fuel burn observed so far.
    pub fn max_fuel_per_bar(&self) -> u64 {
        self.max_fuel
    }

    /// Report what actually got filled (the engine after a simulated fill, or
    /// the live agent after a real one) — feeds `position()`/`entry_price()`.
    pub fn set_position(&mut self, pos: Signal, entry_price_ticks: i64) {
        self.position = match pos {
            Signal::Long => 1,
            Signal::Flat => 0,
            Signal::Short => -1,
        };
        self.entry = if self.position == 0 {
            0
        } else {
            entry_price_ticks
        };
    }

    /// Feed the next CLOSED candle. Returns the target to fill at the NEXT
    /// bar's open: the last `signal` executed this bar, or the standing
    /// target if none ran (position carries by default). Warmup bars (the
    /// first `lookback`) buffer the candle and return the standing target
    /// without evaluating.
    ///
    /// BARS ARE ATOMIC: on `Err`, the candle stays consumed (it entered the
    /// ring) but the bar's effects are discarded — `var` slots, the standing
    /// target, position, and entry are exactly as before the call, and
    /// `max_fuel_per_bar` is not updated. Stepping the next candle is valid;
    /// a fault caused by persistent `var` state will therefore recur.
    pub fn step(&mut self, candle: Candle) -> Result<Signal, Diag> {
        self.ring.push_back(candle);
        if self.ring.len() > self.program.lookback as usize + 1 {
            self.ring.pop_front();
        }
        self.bars_seen += 1;
        if self.bars_seen <= self.program.lookback as usize {
            return Ok(self.standing);
        }
        let mut scopes = Scopes {
            vars: Vec::new(),
            frames: Vec::new(),
        };
        for ((name, _), value) in self.program.vars.iter().zip(&self.slots) {
            scopes.define(name, *value);
        }
        let mut ev = Evaluator {
            fuel: Fuel::new(self.limits.fuel_per_bar),
            scopes,
            ring: &self.ring,
            lookback: self.program.lookback,
            position: self.position,
            entry: self.entry,
            pending: None,
        };
        for s in &self.program.body {
            ev.stmt(s)?;
        }
        let used = self.limits.fuel_per_bar - ev.fuel.remaining();
        self.max_fuel = self.max_fuel.max(used);
        // Read the (possibly reassigned) var slots back for the next bar:
        // they are the bottom `slots.len()` bindings, in declaration order.
        // Rebuild-then-readback (rather than a persistent scope stack) is
        // what makes bars atomic: a faulted bar never reaches this line, so
        // its slot writes vanish. The per-bar name clones are that price.
        for (slot, (_, v)) in self.slots.iter_mut().zip(&ev.scopes.vars) {
            *slot = *v;
        }
        if let Some(target) = ev.pending {
            self.standing = target;
        }
        Ok(self.standing)
    }
}

struct Evaluator<'e> {
    fuel: Fuel,
    scopes: Scopes,
    ring: &'e VecDeque<Candle>,
    lookback: u32,
    position: i64,
    entry: i64,
    /// The last `signal` executed this bar, if any.
    pending: Option<Signal>,
}

impl Evaluator<'_> {
    fn burn(&mut self, cost: u64, sp: Span) -> Result<(), Diag> {
        self.fuel
            .burn(cost)
            .map_err(|_| Diag::at_code(codes::FUEL_EXHAUSTED, "fuel exhausted", sp))
    }

    fn stmt(&mut self, s: &Stmt) -> Result<(), Diag> {
        self.burn(1, s.span())?;
        match s {
            Stmt::Let { name, value, .. } => {
                let v = self.expr(value)?;
                self.scopes.define(name, v);
                Ok(())
            }
            Stmt::Assign {
                name,
                name_span,
                value,
                ..
            } => {
                let v = self.expr(value)?;
                if self.scopes.assign(name, v) {
                    Ok(())
                } else {
                    Err(Diag::at_code(
                        codes::UNDEFINED_VAR,
                        format!("`{name}` is not declared (use `var` or `let`)"),
                        *name_span,
                    ))
                }
            }
            Stmt::Signal { target, .. } => {
                self.pending = Some(*target);
                Ok(())
            }
            Stmt::If { arms, els, .. } => {
                for (cond, body) in arms {
                    if self.bool_expr(cond, "`if` condition")? {
                        return self.block(body);
                    }
                }
                self.block(els)
            }
            Stmt::Repeat { count, body, span } => {
                let n = self.int_expr(count, "`repeat` count")?;
                if n < 0 {
                    return Err(Diag::at_code(
                        codes::NEGATIVE_REPEAT,
                        format!("`repeat` count is negative ({n})"),
                        count.span(),
                    ));
                }
                for _ in 0..n {
                    // Loop-head burn: an empty body still spends.
                    self.burn(1, *span)?;
                    self.block(body)?;
                }
                Ok(())
            }
        }
    }

    fn block(&mut self, stmts: &[Stmt]) -> Result<(), Diag> {
        self.scopes.push();
        let r = stmts.iter().try_for_each(|s| self.stmt(s));
        self.scopes.pop();
        r
    }

    /// `k` bars ago, validated against the lookback. The ring holds exactly
    /// `lookback + 1` candles whenever the body runs, so a valid `k` always
    /// indexes it.
    fn candle(&self, k: i64, sp: Span) -> Result<Candle, Diag> {
        if !(0..=i64::from(self.lookback)).contains(&k) {
            return Err(Diag::at_code(
                codes::BAD_OFFSET,
                format!(
                    "offset {k} is outside 0..=lookback ({}); the future has no name",
                    self.lookback
                ),
                sp,
            ));
        }
        match self.ring.get(self.ring.len() - 1 - k as usize) {
            Some(c) => Ok(*c),
            // Unreachable: the body only runs with a full ring. Diag, not panic.
            None => Err(Diag::at_code(codes::BAD_OFFSET, "ring underflow", sp)),
        }
    }

    fn expr(&mut self, e: &Expr) -> Result<Value, Diag> {
        self.burn(1, e.span())?;
        match e {
            Expr::Int(v, _) => Ok(Value::Int(*v)),
            Expr::Bool(b, _) => Ok(Value::Bool(*b)),
            Expr::Ident(name, sp) => self.scopes.get(name).ok_or_else(|| {
                Diag::at_code(
                    codes::UNDEFINED_VAR,
                    format!("undefined variable `{name}`"),
                    *sp,
                )
            }),
            Expr::Series(b, arg, _) => {
                let k = self.int_expr(arg, "a series offset")?;
                let c = self.candle(k, arg.span())?;
                Ok(Value::Int(match b {
                    Builtin::Open => c.open,
                    Builtin::High => c.high,
                    Builtin::Low => c.low,
                    Builtin::Close => c.close,
                    Builtin::Volume => c.volume,
                    _ => unreachable!("parse resolves series builtins only"),
                }))
            }
            Expr::Indicator(b, n, sp) => self.indicator(*b, *n, *sp),
            Expr::Probe(b, _) => Ok(Value::Int(match b {
                Builtin::Position => self.position,
                Builtin::EntryPrice => self.entry,
                _ => unreachable!("parse resolves probe builtins only"),
            })),
            Expr::Unary(op, inner, sp) => {
                let v = self.expr(inner)?;
                match (op, v) {
                    (UnOp::Neg, Value::Int(n)) => n
                        .checked_neg()
                        .map(Value::Int)
                        .ok_or_else(|| overflow("-", *sp)),
                    (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (UnOp::Neg, v) => Err(mismatch(
                        format!("`-` needs an integer, got {}", kind_name(v)),
                        inner.span(),
                    )),
                    (UnOp::Not, v) => Err(mismatch(
                        format!("`!` needs a boolean, got {}", kind_name(v)),
                        inner.span(),
                    )),
                }
            }
            Expr::Binary(op, lhs, rhs, sp) => match op {
                BinOp::And => {
                    if !self.bool_expr(lhs, "`&&` operand")? {
                        return Ok(Value::Bool(false));
                    }
                    Ok(Value::Bool(self.bool_expr(rhs, "`&&` operand")?))
                }
                BinOp::Or => {
                    if self.bool_expr(lhs, "`||` operand")? {
                        return Ok(Value::Bool(true));
                    }
                    Ok(Value::Bool(self.bool_expr(rhs, "`||` operand")?))
                }
                _ => {
                    let l = self.expr(lhs)?;
                    let r = self.expr(rhs)?;
                    binop(*op, l, r, lhs.span(), rhs.span(), *sp)
                }
            },
        }
    }

    /// An indicator over the last `n` closed bars. `n` is parse-validated
    /// (1..=lookback); the window walk costs `n` fuel, charged up front.
    /// Accumulation is i128, so i64 candles cannot overflow intermediates.
    /// NORMATIVE rounding: every indicator division truncates toward zero
    /// (`i128` `/`) — sma's mean, each ema step, rsi's ratio — matching the
    /// language's own `/`. (Backtested candles are positive, where truncation
    /// and floor agree; a live feed is whatever the caller validates.)
    fn indicator(&mut self, b: Builtin, n: u32, sp: Span) -> Result<Value, Diag> {
        self.burn(u64::from(n), sp)?;
        let bar = |k: u32| self.ring[self.ring.len() - 1 - k as usize];
        let closes = |k: u32| i128::from(bar(k).close);
        let v: i128 = match b {
            Builtin::Sma => {
                let sum: i128 = (0..n).map(closes).sum();
                sum / i128::from(n)
            }
            Builtin::Ema => {
                // Normative recurrence: seed at the window's oldest close,
                // then e = (2·close + (n-1)·e) / (n+1), truncating each step.
                let mut e = closes(n - 1);
                for k in (0..n - 1).rev() {
                    e = (2 * closes(k) + i128::from(n - 1) * e) / i128::from(n + 1);
                }
                e
            }
            Builtin::Rsi => {
                // Cutler RSI in hundredths (0..=10000), 5000 when flat.
                let (mut gain, mut loss) = (0i128, 0i128);
                for k in 0..n {
                    let change = closes(k) - closes(k + 1);
                    if change > 0 {
                        gain += change;
                    } else {
                        loss -= change;
                    }
                }
                if gain + loss == 0 {
                    5000
                } else {
                    10000 * gain / (gain + loss)
                }
            }
            Builtin::Highest => (0..n).map(|k| bar(k).high).max().unwrap_or(0).into(),
            Builtin::Lowest => (0..n).map(|k| bar(k).low).min().unwrap_or(0).into(),
            _ => unreachable!("parse resolves indicator builtins only"),
        };
        // Means/extremes/ratios of i64 inputs always fit back into i64, but
        // stay a diag rather than a debug-only assumption.
        i64::try_from(v)
            .map(Value::Int)
            .map_err(|_| overflow("indicator", sp))
    }

    fn bool_expr(&mut self, e: &Expr, what: &str) -> Result<bool, Diag> {
        match self.expr(e)? {
            Value::Bool(b) => Ok(b),
            v => Err(mismatch(
                format!("{what} must be a boolean, got {}", kind_name(v)),
                e.span(),
            )),
        }
    }

    fn int_expr(&mut self, e: &Expr, what: &str) -> Result<i64, Diag> {
        match self.expr(e)? {
            Value::Int(n) => Ok(n),
            v => Err(mismatch(
                format!("{what} must be an integer, got {}", kind_name(v)),
                e.span(),
            )),
        }
    }
}

fn binop(op: BinOp, l: Value, r: Value, lsp: Span, rsp: Span, sp: Span) -> Result<Value, Diag> {
    if matches!(op, BinOp::Eq | BinOp::Ne) {
        let same = match (l, r) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            _ => {
                return Err(mismatch(
                    format!(
                        "`{}` cannot compare {} with {}",
                        op.sym(),
                        kind_name(l),
                        kind_name(r)
                    ),
                    sp,
                ));
            }
        };
        return Ok(Value::Bool(if op == BinOp::Eq { same } else { !same }));
    }
    let (Value::Int(a), Value::Int(b)) = (l, r) else {
        let (bad, bad_sp) = if matches!(l, Value::Int(_)) {
            (r, rsp)
        } else {
            (l, lsp)
        };
        return Err(mismatch(
            format!(
                "`{}` needs integer operands, got {}",
                op.sym(),
                kind_name(bad)
            ),
            bad_sp,
        ));
    };
    match op {
        BinOp::Lt => Ok(Value::Bool(a < b)),
        BinOp::Le => Ok(Value::Bool(a <= b)),
        BinOp::Gt => Ok(Value::Bool(a > b)),
        BinOp::Ge => Ok(Value::Bool(a >= b)),
        BinOp::Add => a
            .checked_add(b)
            .map(Value::Int)
            .ok_or_else(|| overflow("+", sp)),
        BinOp::Sub => a
            .checked_sub(b)
            .map(Value::Int)
            .ok_or_else(|| overflow("-", sp)),
        BinOp::Mul => a
            .checked_mul(b)
            .map(Value::Int)
            .ok_or_else(|| overflow("*", sp)),
        BinOp::Div | BinOp::Rem => {
            if b == 0 {
                let what = if op == BinOp::Div {
                    "division"
                } else {
                    "remainder"
                };
                return Err(Diag::at_code(
                    codes::DIV_BY_ZERO,
                    format!("{what} by zero"),
                    sp,
                ));
            }
            let checked = if op == BinOp::Div {
                i64::checked_div
            } else {
                i64::checked_rem
            };
            checked(a, b)
                .map(Value::Int)
                .ok_or_else(|| overflow(op.sym(), sp))
        }
        BinOp::Or | BinOp::And | BinOp::Eq | BinOp::Ne => {
            unreachable!("handled before the integer path")
        }
    }
}

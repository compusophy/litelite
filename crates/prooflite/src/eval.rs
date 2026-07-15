//! prooflite evaluator: a fueled tree-walk. One `Fuel` tank and one
//! `ByteBudget` serve the whole program — every statement, expression node,
//! and `repeat` iteration burns 1 unit, so "halts within `limits.fuel` steps"
//! is mechanical. Evaluation recursion needs no guard of its own — but only
//! because the parser bounds AST DEPTH, not just its own recursion: nesting
//! is guarded, binary folds charge the guard per spine node, and if/else-if
//! chains are flat vectors. Whatever the parser accepts, eval (and drop glue)
//! can walk within a bounded stack.

use caplite::Ty;
use diaglite::{Diag, Span};
use fuellite::{ByteBudget, Fuel};

use crate::parse::{BinOp, Expr, Program, Stmt, UnOp};
use crate::{Host, Limits, Outcome, Type, codes};

/// A prooflite runtime value — what programs compute and hosts receive and
/// return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Int(i64),
    Bool(bool),
}

impl Value {
    /// The [`Type`] this value inhabits (what capability params check against).
    pub fn type_of(&self) -> Type {
        match self {
            Value::Int(_) => Type::Int,
            Value::Bool(_) => Type::Bool,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Bool(b) => write!(f, "{b}"),
        }
    }
}

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

/// Lexical scopes: a flat binding stack plus frame marks. Lookup walks
/// innermost-out, so `let` shadows outward bindings until its frame pops.
struct Scopes {
    vars: Vec<(String, Value)>,
    frames: Vec<usize>,
}

impl Scopes {
    fn new() -> Self {
        Scopes {
            vars: Vec::new(),
            frames: Vec::new(),
        }
    }
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

pub(crate) fn eval(
    program: &Program,
    limits: &Limits,
    host: &mut dyn Host,
) -> Result<Outcome, Diag> {
    let mut ev = Evaluator {
        fuel: Fuel::new(limits.fuel),
        budget: ByteBudget::new(limits.output_bytes),
        out: String::new(),
        clipped: false,
        scopes: Scopes::new(),
        host,
    };
    for s in &program.stmts {
        ev.stmt(s)?;
    }
    Ok(Outcome {
        output: ev.out,
        output_clipped: ev.clipped,
        fuel_used: limits.fuel - ev.fuel.remaining(),
    })
}

struct Evaluator<'h> {
    fuel: Fuel,
    budget: ByteBudget,
    out: String,
    clipped: bool,
    scopes: Scopes,
    host: &'h mut dyn Host,
}

impl Evaluator<'_> {
    fn burn(&mut self, sp: Span) -> Result<(), Diag> {
        self.fuel
            .burn(1)
            .map_err(|_| Diag::at_code(codes::FUEL_EXHAUSTED, "fuel exhausted", sp))
    }

    fn stmt(&mut self, s: &Stmt) -> Result<(), Diag> {
        self.burn(s.span())?;
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
                        format!(
                            "cannot assign to undefined variable `{name}` (declare it with `let`)"
                        ),
                        *name_span,
                    ))
                }
            }
            Stmt::Print { value, .. } => {
                let v = self.expr(value)?;
                let line = format!("{v}\n");
                if !self.budget.push_str(&mut self.out, &line) {
                    self.clipped = true; // clip and keep going — the cap is on bytes, not on running
                }
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
                // The count is evaluated ONCE, up front — the loop bound cannot
                // move while the loop runs.
                let n = self.int_expr(count, "`repeat` count")?;
                if n < 0 {
                    return Err(Diag::at_code(
                        codes::NEGATIVE_REPEAT,
                        format!("`repeat` count is negative ({n})"),
                        count.span(),
                    ));
                }
                for _ in 0..n {
                    // Loop-head burn: an empty body still spends, so a huge
                    // count exhausts the tank instead of spinning free.
                    self.burn(*span)?;
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

    fn expr(&mut self, e: &Expr) -> Result<Value, Diag> {
        self.burn(e.span())?;
        match e {
            Expr::Int(v, _) => Ok(Value::Int(*v)),
            Expr::Bool(b, _) => Ok(Value::Bool(*b)),
            Expr::Var(name, sp) => self.scopes.get(name).ok_or_else(|| {
                Diag::at_code(
                    codes::UNDEFINED_VAR,
                    format!("undefined variable `{name}`"),
                    *sp,
                )
            }),
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
            Expr::Call {
                name,
                name_span,
                args,
                span,
            } => self.call(name, *name_span, args, *span),
            Expr::Binary(op, lhs, rhs, sp) => match op {
                // Short-circuit: an unevaluated right side burns no fuel.
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

    /// A host-capability call: resolve against the table, evaluate and check
    /// the arguments, burn the declared cost, dispatch, and verify the host's
    /// answer against its own declaration.
    fn call(
        &mut self,
        name: &str,
        name_span: Span,
        args: &[Expr],
        span: Span,
    ) -> Result<Value, Diag> {
        // Cap fields are `'static`, so copy them out — the table borrow must
        // end before the `&mut` dispatch below.
        let Some((idx, cap)) = self.host.caps().find(name) else {
            return Err(Diag::at_code(
                codes::UNKNOWN_CAP,
                format!("unknown capability `{name}` (the host table is the whole effect surface)"),
                name_span,
            ));
        };
        let (params, result, cost) = (cap.params, cap.result, cap.cost);
        let mut vals = Vec::with_capacity(args.len());
        for a in args {
            vals.push(self.expr(a)?);
        }
        let tys: Vec<Type> = vals.iter().map(Value::type_of).collect();
        caplite::check_args(params, &tys).map_err(|e| {
            Diag::at_code(codes::CAP_ARGS, format!("capability `{name}` {e}"), span)
        })?;
        self.fuel
            .burn(cost)
            .map_err(|_| Diag::at_code(codes::FUEL_EXHAUSTED, "fuel exhausted", span))?;
        let v = self.host.call(idx, &vals).map_err(|msg| {
            Diag::at_code(
                codes::HOST_FAULT,
                format!("capability `{name}` failed: {msg}"),
                span,
            )
        })?;
        // `check_table` guarantees a declared result; stay diag, not panic.
        let Some(want) = result else {
            return Err(Diag::at_code(
                codes::BAD_CAP_TABLE,
                format!("capability `{name}` declares no result"),
                span,
            ));
        };
        if v.type_of() != want {
            return Err(Diag::at_code(
                codes::HOST_FAULT,
                format!(
                    "capability `{name}` returned {}, but its table declares ->{}",
                    v.type_of().sym(),
                    want.sym()
                ),
                span,
            ));
        }
        Ok(v)
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
            // The one non-zero failing case is i64::MIN / -1 (and % -1): overflow.
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

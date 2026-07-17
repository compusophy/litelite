//! stratlite parser: tokens → a checked [`Program`] on the parselite harness.
//! prooflite's discipline ported whole: recursion enters only through the
//! guard, binary folds charge it per spine node, else-if chains are flat.
//! Strategy-specific statics land here too: the `lookback` pragma bounds
//! every indicator WINDOW at parse time (windows are literals — E0108), var
//! declarations precede the body, and builtin names are reserved.

use diaglite::{Diag, Span};
use parselite::{DEFAULT_MAX_DEPTH, TokCursor};

use crate::lex::{TokKind, Token, lex};
use crate::{MAX_LOOKBACK, Signal, Value, codes};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinOp {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl BinOp {
    pub(crate) fn sym(self) -> &'static str {
        match self {
            BinOp::Or => "||",
            BinOp::And => "&&",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
        }
    }
}

/// The fixed builtin table — the language's COMPLETE window on the world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Builtin {
    Open,
    High,
    Low,
    Close,
    Volume,
    Sma,
    Ema,
    Rsi,
    Highest,
    Lowest,
    Position,
    EntryPrice,
}

impl Builtin {
    pub(crate) fn from_name(name: &str) -> Option<Builtin> {
        Some(match name {
            "open" => Builtin::Open,
            "high" => Builtin::High,
            "low" => Builtin::Low,
            "close" => Builtin::Close,
            "volume" => Builtin::Volume,
            "sma" => Builtin::Sma,
            "ema" => Builtin::Ema,
            "rsi" => Builtin::Rsi,
            "highest" => Builtin::Highest,
            "lowest" => Builtin::Lowest,
            "position" => Builtin::Position,
            "entry_price" => Builtin::EntryPrice,
            _ => return None,
        })
    }

    /// Series accessors take one DYNAMIC offset expression (checked at run
    /// time, E0207); indicators take one LITERAL window (checked here,
    /// E0108); probes take none.
    fn kind(self) -> Kind {
        match self {
            Builtin::Open | Builtin::High | Builtin::Low | Builtin::Close | Builtin::Volume => {
                Kind::Series
            }
            Builtin::Sma | Builtin::Ema | Builtin::Rsi | Builtin::Highest | Builtin::Lowest => {
                Kind::Indicator
            }
            Builtin::Position | Builtin::EntryPrice => Kind::Probe,
        }
    }
}

enum Kind {
    Series,
    Indicator,
    Probe,
}

#[derive(Debug)]
pub(crate) enum Expr {
    Int(i64, Span),
    Bool(bool, Span),
    Ident(String, Span),
    Unary(UnOp, Box<Expr>, Span),
    Binary(BinOp, Box<Expr>, Box<Expr>, Span),
    /// `close(k)` etc. — offset checked at run time against the lookback.
    Series(Builtin, Box<Expr>, Span),
    /// `sma(n)` etc. — the window is a LITERAL, validated at parse time, so
    /// its fuel cost is static and its bound is a guarantee, not a path.
    Indicator(Builtin, u32, Span),
    /// `position()` / `entry_price()`.
    Probe(Builtin, Span),
}

impl Expr {
    pub(crate) fn span(&self) -> Span {
        match self {
            Expr::Int(_, sp)
            | Expr::Bool(_, sp)
            | Expr::Ident(_, sp)
            | Expr::Unary(_, _, sp)
            | Expr::Binary(_, _, _, sp)
            | Expr::Series(_, _, sp)
            | Expr::Indicator(_, _, sp)
            | Expr::Probe(_, sp) => *sp,
        }
    }

    /// Widen a parenthesized expression to its parens (spans stay balanced).
    fn with_span(self, sp: Span) -> Expr {
        match self {
            Expr::Int(v, _) => Expr::Int(v, sp),
            Expr::Bool(b, _) => Expr::Bool(b, sp),
            Expr::Ident(n, _) => Expr::Ident(n, sp),
            Expr::Unary(op, e, _) => Expr::Unary(op, e, sp),
            Expr::Binary(op, l, r, _) => Expr::Binary(op, l, r, sp),
            Expr::Series(b, e, _) => Expr::Series(b, e, sp),
            Expr::Indicator(b, n, _) => Expr::Indicator(b, n, sp),
            Expr::Probe(b, _) => Expr::Probe(b, sp),
        }
    }
}

#[derive(Debug)]
pub(crate) enum Stmt {
    Let {
        name: String,
        value: Expr,
        span: Span,
    },
    Assign {
        name: String,
        name_span: Span,
        value: Expr,
        span: Span,
    },
    Signal {
        target: Signal,
        span: Span,
    },
    If {
        /// Flat on purpose — a chain must not deepen the AST (the M1 lesson).
        arms: Vec<(Expr, Vec<Stmt>)>,
        els: Vec<Stmt>,
        span: Span,
    },
    Repeat {
        count: Expr,
        body: Vec<Stmt>,
        span: Span,
    },
}

impl Stmt {
    pub(crate) fn span(&self) -> Span {
        match self {
            Stmt::Let { span, .. }
            | Stmt::Assign { span, .. }
            | Stmt::Signal { span, .. }
            | Stmt::If { span, .. }
            | Stmt::Repeat { span, .. } => *span,
        }
    }
}

/// A compiled strategy: pragma, persistent state, per-bar body. Proof the
/// source is well-formed and every window fits the lookback.
#[derive(Debug)]
pub struct Program {
    pub(crate) lookback: u32,
    /// `var` slots in declaration order — the ONLY cross-bar strategy state.
    pub(crate) vars: Vec<(String, Value)>,
    pub(crate) body: Vec<Stmt>,
}

impl Program {
    /// The declared lookback: warmup length, max offset, max window.
    pub fn lookback(&self) -> u32 {
        self.lookback
    }
}

/// Parse `src` (lexing first) into a [`Program`], or the first failure as a
/// coded, spanned `Diag`.
pub fn parse(src: &str) -> Result<Program, Diag> {
    let toks = lex(src)?;
    let mut cur = TokCursor::new(&toks);
    let lookback = parse_lookback(src, &mut cur).map_err(|e| e.0)?;
    let vars = parse_vars(src, &mut cur).map_err(|e| e.0)?;
    let mut body = Vec::new();
    while !cur.at_last() {
        body.push(stmt(src, &mut cur, lookback).map_err(|e| e.0)?);
    }
    Ok(Program {
        lookback,
        vars,
        body,
    })
}

fn parse_lookback(src: &str, t: &mut Toks<'_>) -> PResult<u32> {
    if t.eat(|x| x.kind == TokKind::Lookback).is_none() {
        return Ok(0);
    }
    let tok = *t.peek();
    let TokKind::Int(n) = tok.kind else {
        return Err(unexpected(src, t, "a lookback length"));
    };
    t.advance();
    expect(src, t, TokKind::Semi, "`;`")?;
    if !(0..=MAX_LOOKBACK).contains(&n) {
        return Err(PErr(Diag::at_code(
            codes::BAD_LOOKBACK,
            format!("lookback must be 0..={MAX_LOOKBACK}, got {n}"),
            tok.span,
        )));
    }
    Ok(n as u32)
}

fn parse_vars(src: &str, t: &mut Toks<'_>) -> PResult<Vec<(String, Value)>> {
    let mut vars: Vec<(String, Value)> = Vec::new();
    while t.eat(|x| x.kind == TokKind::Var).is_some() {
        let (name, name_span) = ident(src, t, "a variable name")?;
        reserved(&name, name_span)?;
        if vars.iter().any(|(n, _)| *n == name) {
            return Err(PErr(Diag::at_code(
                codes::BAD_DECL,
                format!("`{name}` is declared twice"),
                name_span,
            )));
        }
        expect(src, t, TokKind::Assign, "`=`")?;
        // Literal initializers only: persistent state is visible at the top
        // of the program, bounded by its text.
        let tok = *t.peek();
        let value = match tok.kind {
            TokKind::Int(v) => {
                t.advance();
                Value::Int(v)
            }
            TokKind::Minus => {
                t.advance();
                let inner = *t.peek();
                let TokKind::Int(v) = inner.kind else {
                    return Err(unexpected(src, t, "an integer literal"));
                };
                t.advance();
                Value::Int(-v) // literals are ≤ i64::MAX, so negation is safe
            }
            TokKind::True => {
                t.advance();
                Value::Bool(true)
            }
            TokKind::False => {
                t.advance();
                Value::Bool(false)
            }
            _ => {
                return Err(PErr(Diag::at_code(
                    codes::BAD_DECL,
                    "`var` initializers are literals only (int, -int, true, false)",
                    tok.span,
                )));
            }
        };
        expect(src, t, TokKind::Semi, "`;`")?;
        vars.push((name, value));
    }
    Ok(vars)
}

/// Parse-internal error: a `Diag` plus the `From<Span>` hook through which
/// parselite's `guarded` reports a depth-cap trip.
struct PErr(Diag);

impl From<Span> for PErr {
    fn from(sp: Span) -> Self {
        PErr(Diag::at_code(
            codes::TOO_DEEP,
            format!("source nests deeper than the parser allows (depth cap {DEFAULT_MAX_DEPTH})"),
            sp,
        ))
    }
}

type PResult<T> = Result<T, PErr>;
type Toks<'t> = TokCursor<'t, Token>;

/// Names that may not be declared or assigned: the builtins.
fn reserved(name: &str, sp: Span) -> PResult<()> {
    if Builtin::from_name(name).is_some() {
        return Err(PErr(Diag::at_code(
            codes::RESERVED_NAME,
            format!("`{name}` is a builtin and cannot be declared or assigned"),
            sp,
        )));
    }
    Ok(())
}

fn stmt(src: &str, t: &mut Toks<'_>, lookback: u32) -> PResult<Stmt> {
    t.guarded(|t| {
        let tok = *t.peek();
        match tok.kind {
            TokKind::Let => {
                t.advance();
                let (name, name_span) = ident(src, t, "a variable name")?;
                reserved(&name, name_span)?;
                expect(src, t, TokKind::Assign, "`=`")?;
                let value = expr(src, t, lookback)?;
                let end = expect(src, t, TokKind::Semi, "`;`")?;
                Ok(Stmt::Let {
                    name,
                    value,
                    span: Span::new(tok.span.start, end.end),
                })
            }
            TokKind::Ident => {
                let name = text(src, tok.span).to_string();
                t.advance();
                // `=` first, THEN the reserved check: `sma(4);` is a missing
                // `=` (there are no expression statements), not an assignment
                // to a builtin — the diag must describe what the source did.
                expect(src, t, TokKind::Assign, "`=`")?;
                reserved(&name, tok.span)?;
                let value = expr(src, t, lookback)?;
                let end = expect(src, t, TokKind::Semi, "`;`")?;
                Ok(Stmt::Assign {
                    name,
                    name_span: tok.span,
                    value,
                    span: Span::new(tok.span.start, end.end),
                })
            }
            TokKind::Signal => {
                t.advance();
                let target = *t.peek();
                let target_sig = match target.kind {
                    TokKind::Long => Signal::Long,
                    TokKind::Short => Signal::Short,
                    TokKind::Flat => Signal::Flat,
                    _ => return Err(unexpected(src, t, "`long`, `short`, or `flat`")),
                };
                t.advance();
                let end = expect(src, t, TokKind::Semi, "`;`")?;
                Ok(Stmt::Signal {
                    target: target_sig,
                    span: Span::new(tok.span.start, end.end),
                })
            }
            TokKind::If => {
                // The whole chain parses ITERATIVELY inside one guard entry:
                // flat in the source, flat in the AST, flat in guard depth.
                let mut arms = Vec::new();
                let mut els = Vec::new();
                let mut end;
                loop {
                    t.advance(); // the `if`
                    let cond = expr(src, t, lookback)?;
                    let (body, bend) = block(src, t, lookback)?;
                    end = bend.end;
                    arms.push((cond, body));
                    if t.eat(|x| x.kind == TokKind::Else).is_none() {
                        break;
                    }
                    if t.peek().kind == TokKind::If {
                        continue;
                    }
                    let (body, bend) = block(src, t, lookback)?;
                    els = body;
                    end = bend.end;
                    break;
                }
                Ok(Stmt::If {
                    arms,
                    els,
                    span: Span::new(tok.span.start, end),
                })
            }
            TokKind::Repeat => {
                t.advance();
                let count = expr(src, t, lookback)?;
                let (body, end) = block(src, t, lookback)?;
                Ok(Stmt::Repeat {
                    count,
                    body,
                    span: Span::new(tok.span.start, end.end),
                })
            }
            TokKind::Var => Err(PErr(Diag::at_code(
                codes::BAD_DECL,
                "`var` declarations must precede the body",
                tok.span,
            ))),
            TokKind::Lookback => Err(PErr(Diag::at_code(
                codes::BAD_DECL,
                "`lookback` must be the first declaration, once",
                tok.span,
            ))),
            _ => Err(unexpected(src, t, "a statement")),
        }
    })
}

fn block(src: &str, t: &mut Toks<'_>, lookback: u32) -> PResult<(Vec<Stmt>, Span)> {
    expect(src, t, TokKind::LBrace, "`{`")?;
    let mut stmts = Vec::new();
    while t.peek().kind != TokKind::RBrace {
        if t.at_last() {
            return Err(unexpected(src, t, "`}`"));
        }
        stmts.push(stmt(src, t, lookback)?);
    }
    let end = expect(src, t, TokKind::RBrace, "`}`")?;
    Ok((stmts, end))
}

/// Binary precedence ladder, loosest-binding row first; left-associative.
const LADDER: &[&[(TokKind, BinOp)]] = &[
    &[(TokKind::OrOr, BinOp::Or)],
    &[(TokKind::AndAnd, BinOp::And)],
    &[(TokKind::EqEq, BinOp::Eq), (TokKind::BangEq, BinOp::Ne)],
    &[
        (TokKind::Lt, BinOp::Lt),
        (TokKind::LtEq, BinOp::Le),
        (TokKind::Gt, BinOp::Gt),
        (TokKind::GtEq, BinOp::Ge),
    ],
    &[(TokKind::Plus, BinOp::Add), (TokKind::Minus, BinOp::Sub)],
    &[
        (TokKind::Star, BinOp::Mul),
        (TokKind::Slash, BinOp::Div),
        (TokKind::Percent, BinOp::Rem),
    ],
];

fn expr(src: &str, t: &mut Toks<'_>, lookback: u32) -> PResult<Expr> {
    t.guarded(|t| binary(src, t, lookback, 0))
}

fn binary(src: &str, t: &mut Toks<'_>, lookback: u32, level: usize) -> PResult<Expr> {
    if level == LADDER.len() {
        return unary(src, t, lookback);
    }
    // Each fold deepens the AST's left spine one node, so it charges one kit
    // guard entry (the M1 lesson: the guard bounds parser recursion, not AST
    // depth, unless folds are charged too). Entries release together here.
    let mut entered = 0usize;
    let r = fold_level(src, t, lookback, level, &mut entered);
    for _ in 0..entered {
        t.leave();
    }
    r
}

fn fold_level(
    src: &str,
    t: &mut Toks<'_>,
    lookback: u32,
    level: usize,
    entered: &mut usize,
) -> PResult<Expr> {
    let mut lhs = binary(src, t, lookback, level + 1)?;
    'level: loop {
        for &(kind, op) in LADDER[level] {
            if t.peek().kind == kind {
                *entered += 1; // enter() counts even when it trips; binary() leaves
                t.enter()?;
                t.advance();
                let rhs = binary(src, t, lookback, level + 1)?;
                let span = Span::new(lhs.span().start, rhs.span().end);
                lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs), span);
                continue 'level;
            }
        }
        return Ok(lhs);
    }
}

fn unary(src: &str, t: &mut Toks<'_>, lookback: u32) -> PResult<Expr> {
    t.guarded(|t| {
        let tok = *t.peek();
        let op = match tok.kind {
            TokKind::Minus => UnOp::Neg,
            TokKind::Bang => UnOp::Not,
            _ => return primary(src, t, lookback),
        };
        t.advance();
        let operand = unary(src, t, lookback)?;
        let span = Span::new(tok.span.start, operand.span().end);
        Ok(Expr::Unary(op, Box::new(operand), span))
    })
}

fn primary(src: &str, t: &mut Toks<'_>, lookback: u32) -> PResult<Expr> {
    let tok = *t.peek();
    match tok.kind {
        TokKind::Int(v) => {
            t.advance();
            Ok(Expr::Int(v, tok.span))
        }
        TokKind::True => {
            t.advance();
            Ok(Expr::Bool(true, tok.span))
        }
        TokKind::False => {
            t.advance();
            Ok(Expr::Bool(false, tok.span))
        }
        TokKind::Ident => {
            t.advance();
            let name = text(src, tok.span);
            if t.peek().kind != TokKind::LParen {
                // A bare builtin name can never be bound (E0105), so reading
                // one is statically dead — reject now, not at bar 1.
                if Builtin::from_name(name).is_some() {
                    return Err(PErr(Diag::at_code(
                        codes::UNKNOWN_CALL,
                        format!("`{name}` is a builtin — call it: `{name}(…)`"),
                        tok.span,
                    )));
                }
                return Ok(Expr::Ident(name.to_string(), tok.span));
            }
            call(src, t, tok, lookback)
        }
        TokKind::LParen => {
            t.advance();
            let inner = expr(src, t, lookback)?;
            let rparen = expect(src, t, TokKind::RParen, "`)`")?;
            Ok(inner.with_span(Span::new(tok.span.start, rparen.end)))
        }
        _ => Err(unexpected(src, t, "an expression")),
    }
}

/// A call site: the name must be a builtin (E0103), the arity is static
/// (E0107), and indicator windows are RAW literal tokens within the lookback
/// (E0108 — `sma((4))` is not a literal; the REFERENCE card means it).
fn call(src: &str, t: &mut Toks<'_>, name_tok: Token, lookback: u32) -> PResult<Expr> {
    let name = text(src, name_tok.span);
    t.advance(); // the `(`
    let Some(builtin) = Builtin::from_name(name) else {
        return Err(PErr(Diag::at_code(
            codes::UNKNOWN_CALL,
            format!("`{name}` is not a builtin (there are no user functions)"),
            name_tok.span,
        )));
    };
    if matches!(builtin.kind(), Kind::Indicator) && t.peek().kind != TokKind::RParen {
        let w = *t.peek();
        let TokKind::Int(n) = w.kind else {
            return Err(PErr(Diag::at_code(
                codes::BAD_WINDOW,
                format!("`{name}` windows are integer literals (their fuel cost is static)"),
                w.span,
            )));
        };
        t.advance();
        if t.peek().kind == TokKind::Comma {
            return arity(src, t, name_tok, lookback);
        }
        let rparen = expect(src, t, TokKind::RParen, "`)`")?;
        if !(1..=i64::from(lookback)).contains(&n) {
            return Err(PErr(Diag::at_code(
                codes::BAD_WINDOW,
                format!("window {n} is outside 1..=lookback ({lookback})"),
                w.span,
            )));
        }
        return Ok(Expr::Indicator(
            builtin,
            n as u32,
            Span::new(name_tok.span.start, rparen.end),
        ));
    }
    let mut args = Vec::new();
    if t.peek().kind != TokKind::RParen {
        loop {
            args.push(expr(src, t, lookback)?);
            if t.eat(|x| x.kind == TokKind::Comma).is_none() {
                break;
            }
        }
    }
    let rparen = expect(src, t, TokKind::RParen, "`)` or `,`")?;
    let span = Span::new(name_tok.span.start, rparen.end);
    match (builtin.kind(), args.len()) {
        (Kind::Probe, 0) => Ok(Expr::Probe(builtin, span)),
        (Kind::Series, 1) => Ok(Expr::Series(builtin, Box::new(args.swap_remove(0)), span)),
        (kind, got) => {
            let want = if matches!(kind, Kind::Probe) { 0 } else { 1 };
            Err(PErr(Diag::at_code(
                codes::CALL_ARITY,
                format!("`{name}` takes {want} argument(s), got {got}"),
                span,
            )))
        }
    }
}

/// Consume the rest of an over-long indicator argument list (the window seen,
/// a comma pending) and fail with the E0107 diag.
fn arity(src: &str, t: &mut Toks<'_>, name_tok: Token, lookback: u32) -> PResult<Expr> {
    let mut got = 1;
    while t.eat(|x| x.kind == TokKind::Comma).is_some() {
        expr(src, t, lookback)?;
        got += 1;
    }
    let rparen = expect(src, t, TokKind::RParen, "`)`")?;
    Err(PErr(Diag::at_code(
        codes::CALL_ARITY,
        format!(
            "`{}` takes 1 argument(s), got {got}",
            text(src, name_tok.span)
        ),
        Span::new(name_tok.span.start, rparen.end),
    )))
}

fn ident(src: &str, t: &mut Toks<'_>, what: &str) -> PResult<(String, Span)> {
    match t.eat(|x| x.kind == TokKind::Ident) {
        Some(tok) => Ok((text(src, tok.span).to_string(), tok.span)),
        None => Err(unexpected(src, t, what)),
    }
}

fn expect(src: &str, t: &mut Toks<'_>, kind: TokKind, what: &str) -> PResult<Span> {
    match t.eat(|x| x.kind == kind) {
        Some(tok) => Ok(tok.span),
        None => Err(unexpected(src, t, what)),
    }
}

fn unexpected(src: &str, t: &Toks<'_>, what: &str) -> PErr {
    let tok = t.peek();
    PErr(Diag::at_code(
        codes::UNEXPECTED_TOKEN,
        format!("expected {what}, found {}", describe(src, tok)),
        tok.span,
    ))
}

/// Every token's lexeme IS its span text (the lexer cuts exactly around it),
/// so quoting the source covers keywords, operators, idents, and literals —
/// `0xff` stays `0xff`, never a decoded `255`.
fn describe(src: &str, tok: &Token) -> String {
    match tok.kind {
        TokKind::Eof => "end of input".to_string(),
        _ => format!("`{}`", text(src, tok.span)),
    }
}

/// Token spans come from the lexer, which only ever cuts on char boundaries.
fn text(src: &str, sp: Span) -> &str {
    &src[sp.start..sp.end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_structure_parses() {
        let p = parse(
            "lookback 40;
             var cooldown = 0;
             var armed = true;
             let fast = sma(10);
             if fast > sma(40) { signal long; } else { signal flat; }",
        )
        .unwrap();
        assert_eq!(p.lookback(), 40);
        assert_eq!(p.vars.len(), 2);
        assert_eq!(p.vars[1], ("armed".to_string(), Value::Bool(true)));
        assert_eq!(p.body.len(), 2);
        // Absent pragma means lookback 0.
        assert_eq!(parse("signal flat;").unwrap().lookback(), 0);
    }

    #[test]
    fn static_rules_are_coded() {
        for (src, code) in [
            ("lookback 5000;", codes::BAD_LOOKBACK),
            ("lookback 5; lookback 5;", codes::BAD_DECL), // once, first
            ("signal long; var x = 0;", codes::BAD_DECL), // vars precede body
            ("var x = 0; var x = 1;", codes::BAD_DECL),   // dup var
            ("var x = close(0);", codes::BAD_DECL),       // literal init only
            ("let sma = 1;", codes::RESERVED_NAME),
            ("var close = 0;", codes::RESERVED_NAME),
            ("rsi = 1;", codes::RESERVED_NAME),
            ("let x = foo(1);", codes::UNKNOWN_CALL),
            ("let x = sma(1, 2);", codes::CALL_ARITY),
            ("let x = position(1);", codes::CALL_ARITY),
            ("lookback 5; let x = sma(6);", codes::BAD_WINDOW), // > lookback
            ("let x = sma(1);", codes::BAD_WINDOW),             // lookback 0
            ("lookback 5; let x = sma(0);", codes::BAD_WINDOW),
            ("lookback 5; let n = 5; let x = sma(n);", codes::BAD_WINDOW), // literal only
            ("lookback 5; let x = sma((5));", codes::BAD_WINDOW),          // RAW literal only
            ("lookback 5; let x = sma();", codes::CALL_ARITY),
            ("lookback 5; let x = sma(5, 2);", codes::CALL_ARITY),
            ("position();", codes::UNEXPECTED_TOKEN), // missing `=`, not E0105
            ("lookback 4; let x = sma;", codes::UNKNOWN_CALL), // statically dead read
            ("signal maybe;", codes::UNEXPECTED_TOKEN),
        ] {
            let e = parse(src).unwrap_err();
            assert_eq!(e.code, Some(code), "{src}: {e}");
        }
    }

    #[test]
    fn offsets_stay_dynamic_and_folds_charge_the_guard() {
        // A computed offset is legal at parse time (checked when it runs).
        let p = parse("lookback 8; var i = 0; let x = close(i + 1);").unwrap();
        assert_eq!(p.lookback(), 8);
        // The ported spine rule: long flat chains trip E0102, never the stack.
        let deep = format!("let x = {}0;", "1+".repeat(500));
        assert_eq!(parse(&deep).unwrap_err().code, Some(codes::TOO_DEEP));
        let ok = format!("let x = {}0;", "1+".repeat(50));
        assert!(parse(&ok).is_ok());
    }
}

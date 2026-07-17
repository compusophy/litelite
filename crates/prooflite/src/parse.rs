//! prooflite parser: tokens → AST on the parselite harness. Recursion enters
//! only through `guarded`, so nesting depth is capped by the kit; every parse
//! failure is a coded, spanned `Diag`.

use diaglite::{Diag, Span};
use parselite::{DEFAULT_MAX_DEPTH, TokCursor};

use crate::codes;
use crate::lex::{TokKind, Token, lex};

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

#[derive(Debug)]
pub(crate) enum Expr {
    Int(i64, Span),
    Bool(bool, Span),
    Var(String, Span),
    Unary(UnOp, Box<Expr>, Span),
    Binary(BinOp, Box<Expr>, Box<Expr>, Span),
    /// A host-capability call — the only thing that can reach past the
    /// program's own state (and only what the host's table declares).
    Call {
        name: String,
        name_span: Span,
        args: Vec<Expr>,
        span: Span,
    },
}

impl Expr {
    pub(crate) fn span(&self) -> Span {
        match self {
            Expr::Int(_, sp)
            | Expr::Bool(_, sp)
            | Expr::Var(_, sp)
            | Expr::Unary(_, _, sp)
            | Expr::Binary(_, _, _, sp) => *sp,
            Expr::Call { span, .. } => *span,
        }
    }

    /// Replace the node's span — used to widen a parenthesized expression to
    /// its parens, so spans joined from it always cover balanced source text.
    fn set_span(&mut self, sp: Span) {
        match self {
            Expr::Int(_, s)
            | Expr::Bool(_, s)
            | Expr::Var(_, s)
            | Expr::Unary(_, _, s)
            | Expr::Binary(_, _, _, s) => *s = sp,
            Expr::Call { span, .. } => *span = sp,
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
    Print {
        value: Expr,
        span: Span,
    },
    If {
        /// The `if` and every `else if` link, in source order: (condition,
        /// block). Flat on purpose — a chain must not deepen the AST.
        arms: Vec<(Expr, Vec<Stmt>)>,
        /// The final `else` block; empty when absent.
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
            | Stmt::Print { span, .. }
            | Stmt::If { span, .. }
            | Stmt::Repeat { span, .. } => *span,
        }
    }
}

/// A parsed prooflite program — proof the source is well-formed. Run it with
/// [`crate::run`].
#[derive(Debug)]
pub struct Program {
    pub(crate) stmts: Vec<Stmt>,
}

/// Parse `src` (lexing first) into a [`Program`], or the first error as a
/// coded, spanned `Diag`.
pub fn parse(src: &str) -> Result<Program, Diag> {
    let toks = lex(src)?;
    let mut cur = TokCursor::new(&toks);
    let mut stmts = Vec::new();
    while !cur.at_last() {
        stmts.push(stmt(src, &mut cur).map_err(|e| e.0)?);
    }
    Ok(Program { stmts })
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

fn stmt(src: &str, t: &mut Toks<'_>) -> PResult<Stmt> {
    t.guarded(|t| {
        let tok = *t.peek();
        match tok.kind {
            TokKind::Let => {
                t.advance();
                let name = ident(src, t, "a variable name")?;
                expect(src, t, TokKind::Assign, "`=`")?;
                let value = expr(src, t)?;
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
                expect(src, t, TokKind::Assign, "`=`")?;
                let value = expr(src, t)?;
                let end = expect(src, t, TokKind::Semi, "`;`")?;
                Ok(Stmt::Assign {
                    name,
                    name_span: tok.span,
                    value,
                    span: Span::new(tok.span.start, end.end),
                })
            }
            TokKind::Print => {
                t.advance();
                let value = expr(src, t)?;
                let end = expect(src, t, TokKind::Semi, "`;`")?;
                Ok(Stmt::Print {
                    value,
                    span: Span::new(tok.span.start, end.end),
                })
            }
            TokKind::If => {
                // The whole `if / else if / … / else` chain parses ITERATIVELY
                // inside this one statement's guard entry: the chain is flat in
                // the source, flat in the AST, and must cost flat guard depth.
                let mut arms = Vec::new();
                let mut els = Vec::new();
                let mut end;
                loop {
                    t.advance(); // the `if`
                    let cond = expr(src, t)?;
                    let (body, bend) = block(src, t)?;
                    end = bend.end;
                    arms.push((cond, body));
                    if t.eat(|x| x.kind == TokKind::Else).is_none() {
                        break;
                    }
                    if t.peek().kind == TokKind::If {
                        continue;
                    }
                    let (body, bend) = block(src, t)?;
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
                let count = expr(src, t)?;
                let (body, end) = block(src, t)?;
                Ok(Stmt::Repeat {
                    count,
                    body,
                    span: Span::new(tok.span.start, end.end),
                })
            }
            _ => Err(unexpected(src, t, "a statement")),
        }
    })
}

fn block(src: &str, t: &mut Toks<'_>) -> PResult<(Vec<Stmt>, Span)> {
    expect(src, t, TokKind::LBrace, "`{`")?;
    let mut stmts = Vec::new();
    while t.peek().kind != TokKind::RBrace {
        if t.at_last() {
            return Err(unexpected(src, t, "`}`"));
        }
        stmts.push(stmt(src, t)?);
    }
    let end = expect(src, t, TokKind::RBrace, "`}`")?;
    Ok((stmts, end))
}

/// Binary precedence ladder, loosest-binding row first; every row is
/// left-associative.
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

fn expr(src: &str, t: &mut Toks<'_>) -> PResult<Expr> {
    t.guarded(|t| binary(src, t, 0))
}

fn binary(src: &str, t: &mut Toks<'_>, level: usize) -> PResult<Expr> {
    if level == LADDER.len() {
        return unary(src, t);
    }
    // The fold below is iterative, but each fold deepens the AST's left spine
    // by one Binary node — and the evaluator (and drop glue) later recurse one
    // frame per node. So every fold charges one kit guard entry, exactly like
    // a paren level; the entries release together when this level completes.
    // Without this, a flat `1+1+…+1` is O(1) guard depth at parse time yet
    // overflows the stack at eval/drop time — the guard bounds parser
    // recursion, not AST depth, unless folds are charged too.
    let mut entered = 0usize;
    let r = fold_level(src, t, level, &mut entered);
    for _ in 0..entered {
        t.leave();
    }
    r
}

fn fold_level(src: &str, t: &mut Toks<'_>, level: usize, entered: &mut usize) -> PResult<Expr> {
    let mut lhs = binary(src, t, level + 1)?;
    'level: loop {
        for &(kind, op) in LADDER[level] {
            if t.peek().kind == kind {
                *entered += 1; // enter() counts even when it trips; binary() leaves
                t.enter()?;
                t.advance();
                let rhs = binary(src, t, level + 1)?;
                let span = Span::new(lhs.span().start, rhs.span().end);
                lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs), span);
                continue 'level;
            }
        }
        return Ok(lhs);
    }
}

fn unary(src: &str, t: &mut Toks<'_>) -> PResult<Expr> {
    t.guarded(|t| {
        let tok = *t.peek();
        let op = match tok.kind {
            TokKind::Minus => UnOp::Neg,
            TokKind::Bang => UnOp::Not,
            _ => return primary(src, t),
        };
        t.advance();
        let operand = unary(src, t)?;
        let span = Span::new(tok.span.start, operand.span().end);
        Ok(Expr::Unary(op, Box::new(operand), span))
    })
}

fn primary(src: &str, t: &mut Toks<'_>) -> PResult<Expr> {
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
            if t.peek().kind != TokKind::LParen {
                return Ok(Expr::Var(text(src, tok.span).to_string(), tok.span));
            }
            t.advance(); // the `(`
            let mut args = Vec::new();
            if t.peek().kind != TokKind::RParen {
                loop {
                    args.push(expr(src, t)?);
                    if t.eat(|x| x.kind == TokKind::Comma).is_none() {
                        break;
                    }
                }
            }
            let rparen = expect(src, t, TokKind::RParen, "`)` or `,`")?;
            Ok(Expr::Call {
                name: text(src, tok.span).to_string(),
                name_span: tok.span,
                args,
                span: Span::new(tok.span.start, rparen.end),
            })
        }
        TokKind::LParen => {
            t.advance();
            let mut inner = expr(src, t)?;
            let rparen = expect(src, t, TokKind::RParen, "`)`")?;
            inner.set_span(Span::new(tok.span.start, rparen.end));
            Ok(inner)
        }
        _ => Err(unexpected(src, t, "an expression")),
    }
}

fn ident(src: &str, t: &mut Toks<'_>, what: &str) -> PResult<String> {
    match t.eat(|x| x.kind == TokKind::Ident) {
        Some(tok) => Ok(text(src, tok.span).to_string()),
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

/// Every token except `Eof` spans exactly the source text the lexer consumed
/// for it, so quote that — never a decoded value (`0xff` is not `255`).
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
    fn precedence_shapes_the_tree() {
        let p = parse("let x = 1 + 2 * 3;").unwrap();
        let Stmt::Let { value, .. } = &p.stmts[0] else {
            panic!("expected let");
        };
        // `+` at the root proves `*` bound tighter.
        let Expr::Binary(BinOp::Add, lhs, rhs, sp) = value else {
            panic!("expected `+` at the root, got {value:?}");
        };
        assert!(matches!(**lhs, Expr::Int(1, _)));
        assert!(matches!(**rhs, Expr::Binary(BinOp::Mul, _, _, _)));
        assert_eq!((sp.start, sp.end), (8, 17)); // spans join the operands
    }

    #[test]
    fn else_if_chains_are_flat_in_the_ast() {
        let p = parse("if a { } else if b { } else { print 1; }").unwrap();
        let Stmt::If { arms, els, .. } = &p.stmts[0] else {
            panic!()
        };
        assert_eq!(arms.len(), 2);
        assert_eq!(els.len(), 1);
        let p = parse("if a { }").unwrap();
        let Stmt::If { arms, els, .. } = &p.stmts[0] else {
            panic!()
        };
        assert_eq!((arms.len(), els.len()), (1, 0));
    }

    #[test]
    fn parenthesized_spans_include_the_parens() {
        let p = parse("print (1) == (true);").unwrap();
        let Stmt::Print { value, .. } = &p.stmts[0] else {
            panic!()
        };
        let sp = value.span();
        assert_eq!((sp.start, sp.end), (6, 19)); // covers `(1) == (true)`
        let Expr::Binary(_, lhs, _, _) = value else {
            panic!()
        };
        let l = lhs.span();
        assert_eq!((l.start, l.end), (6, 9)); // covers `(1)`, both parens
    }

    #[test]
    fn parse_errors_are_coded_and_descriptive() {
        for (src, needle) in [
            ("let x = 1", "expected `;`, found end of input"),
            ("let 5 = 1;", "expected a variable name, found `5`"),
            ("x == 1;", "expected `=`, found `==`"),
            ("if true { print 1;", "expected `}`, found end of input"),
            ("1 + 2;", "expected a statement, found `1`"),
            ("print (1;", "expected `)`, found `;`"),
            ("print ;", "expected an expression, found `;`"),
        ] {
            let e = parse(src).unwrap_err();
            assert_eq!(e.code, Some(codes::UNEXPECTED_TOKEN), "{src}: {e}");
            assert!(e.message.contains(needle), "{src}: {e}");
            assert!(e.span.is_some(), "{src}");
        }
    }

    #[test]
    fn deep_nesting_trips_the_kit_guard_not_the_stack() {
        // Parens: ~2 guard entries per level (expr + unary).
        let deep = format!("print {}1{};", "(".repeat(60), ")".repeat(60));
        let e = parse(&deep).unwrap_err();
        assert_eq!(e.code, Some(codes::TOO_DEEP), "{e}");
        // Unary chains: 1 entry each.
        let bangs = format!("print {}true;", "!".repeat(200));
        assert_eq!(parse(&bangs).unwrap_err().code, Some(codes::TOO_DEEP));
        // Statement nesting counts too.
        let ifs = format!("{}print 1;{}", "if true { ".repeat(120), "}".repeat(120));
        assert_eq!(parse(&ifs).unwrap_err().code, Some(codes::TOO_DEEP));
        // Reasonable nesting stays well inside the cap.
        let ok = format!("print {}1{};", "(".repeat(40), ")".repeat(40));
        assert!(parse(&ok).is_ok());
    }

    #[test]
    fn empty_program_parses() {
        assert_eq!(parse("").unwrap().stmts.len(), 0);
        assert_eq!(parse("  // just a comment\n").unwrap().stmts.len(), 0);
    }
}

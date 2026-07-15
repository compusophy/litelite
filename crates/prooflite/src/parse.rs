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
}

impl Expr {
    pub(crate) fn span(&self) -> Span {
        match self {
            Expr::Int(_, sp)
            | Expr::Bool(_, sp)
            | Expr::Var(_, sp)
            | Expr::Unary(_, _, sp)
            | Expr::Binary(_, _, _, sp) => *sp,
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
        cond: Expr,
        then: Vec<Stmt>,
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
                let (name, _) = ident(src, t, "a variable name")?;
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
                t.advance();
                let cond = expr(src, t)?;
                let (then, then_end) = block(src, t)?;
                let (els, end) = if t.eat(|x| x.kind == TokKind::Else).is_some() {
                    if t.peek().kind == TokKind::If {
                        // `else if …` is sugar: an else-branch of one statement.
                        let chained = stmt(src, t)?;
                        let end = chained.span().end;
                        (vec![chained], end)
                    } else {
                        let (body, end) = block(src, t)?;
                        (body, end.end)
                    }
                } else {
                    (Vec::new(), then_end.end)
                };
                Ok(Stmt::If {
                    cond,
                    then,
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
    let mut lhs = binary(src, t, level + 1)?;
    'level: loop {
        for &(kind, op) in LADDER[level] {
            if t.peek().kind == kind {
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
            Ok(Expr::Var(text(src, tok.span).to_string(), tok.span))
        }
        TokKind::LParen => {
            t.advance();
            let inner = expr(src, t)?;
            expect(src, t, TokKind::RParen, "`)`")?;
            Ok(inner)
        }
        _ => Err(unexpected(src, t, "an expression")),
    }
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

fn describe(src: &str, tok: &Token) -> String {
    let sym = match tok.kind {
        TokKind::Eof => return "end of input".to_string(),
        TokKind::Int(v) => return format!("`{v}`"),
        TokKind::Ident => return format!("`{}`", text(src, tok.span)),
        TokKind::True => "true",
        TokKind::False => "false",
        TokKind::Let => "let",
        TokKind::If => "if",
        TokKind::Else => "else",
        TokKind::Repeat => "repeat",
        TokKind::Print => "print",
        TokKind::Plus => "+",
        TokKind::Minus => "-",
        TokKind::Star => "*",
        TokKind::Slash => "/",
        TokKind::Percent => "%",
        TokKind::Bang => "!",
        TokKind::BangEq => "!=",
        TokKind::Assign => "=",
        TokKind::EqEq => "==",
        TokKind::Lt => "<",
        TokKind::LtEq => "<=",
        TokKind::Gt => ">",
        TokKind::GtEq => ">=",
        TokKind::AndAnd => "&&",
        TokKind::OrOr => "||",
        TokKind::LParen => "(",
        TokKind::RParen => ")",
        TokKind::LBrace => "{",
        TokKind::RBrace => "}",
        TokKind::Semi => ";",
    };
    format!("`{sym}`")
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
    fn else_if_desugars_to_a_nested_if() {
        let p = parse("if a { } else if b { } else { }").unwrap();
        let Stmt::If { els, .. } = &p.stmts[0] else {
            panic!()
        };
        assert_eq!(els.len(), 1);
        assert!(matches!(els[0], Stmt::If { .. }));
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

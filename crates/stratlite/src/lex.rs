//! stratlite lexer: source text → spanned tokens, on the lexlite cursor.

use diaglite::{Diag, Span};
use lexlite::{Cursor, ident_cont, ident_start};

use crate::codes;

/// A token kind. `Int` carries its parsed value; `Ident` text is recovered by
/// slicing the source with the token's span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokKind {
    Int(i64),
    Ident,
    True,
    False,
    Let,
    If,
    Else,
    Repeat,
    Lookback,
    Var,
    Signal,
    Long,
    Short,
    Flat,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    BangEq,
    Assign,
    EqEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    AndAnd,
    OrOr,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Eof,
}

/// A spanned token — the slice a [`parselite::TokCursor`] rides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokKind,
    pub span: Span,
}

impl parselite::Tok for Token {
    fn span(&self) -> Span {
        self.span
    }
}

/// Lex `src` into tokens, ending with a [`TokKind::Eof`] sentinel.
///
/// Trivia: whitespace, `//` line comments, and NESTED `/* */` block comments
/// (the flag lexlite forces every language to pick; prooflite picks nested).
pub fn lex(src: &str) -> Result<Vec<Token>, Diag> {
    let mut cur = Cursor::new(src);
    let mut toks = Vec::new();
    loop {
        skip_trivia(&mut cur)?;
        if cur.at_eof() {
            toks.push(Token {
                kind: TokKind::Eof,
                span: Span::new(src.len(), src.len()),
            });
            return Ok(toks);
        }
        let start = cur.pos();
        let kind = next_kind(&mut cur)?;
        toks.push(Token {
            kind,
            span: cur.span_from(start),
        });
    }
}

fn skip_trivia(cur: &mut Cursor<'_>) -> Result<(), Diag> {
    loop {
        cur.skip_ws();
        if cur.skip_line_comment("//") {
            continue;
        }
        match cur.skip_block_comment("/*", "*/", true) {
            Ok(true) => continue,
            Ok(false) => return Ok(()),
            Err(sp) => {
                return Err(Diag::at_code(
                    codes::UNTERMINATED_COMMENT,
                    "unterminated block comment",
                    sp,
                ));
            }
        }
    }
}

/// Keyword lookup. Builtin names (`close`, `sma`, …) are NOT keywords — they
/// stay `Ident` and resolve at parse time, where declaring one is E0105.
pub(crate) fn keyword(text: &str) -> Option<TokKind> {
    Some(match text {
        "let" => TokKind::Let,
        "if" => TokKind::If,
        "else" => TokKind::Else,
        "repeat" => TokKind::Repeat,
        "lookback" => TokKind::Lookback,
        "var" => TokKind::Var,
        "signal" => TokKind::Signal,
        "long" => TokKind::Long,
        "short" => TokKind::Short,
        "flat" => TokKind::Flat,
        "true" => TokKind::True,
        "false" => TokKind::False,
        _ => return None,
    })
}

fn next_kind(cur: &mut Cursor<'_>) -> Result<TokKind, Diag> {
    if let Some(sp) = cur.eat_ident(ident_start, ident_cont) {
        return Ok(keyword(cur.text(sp)).unwrap_or(TokKind::Ident));
    }
    if cur.peek().is_some_and(|b| b.is_ascii_digit()) {
        return int_literal(cur);
    }
    for (text, kind) in [
        ("&&", TokKind::AndAnd),
        ("||", TokKind::OrOr),
        ("==", TokKind::EqEq),
        ("!=", TokKind::BangEq),
        ("<=", TokKind::LtEq),
        (">=", TokKind::GtEq),
    ] {
        if cur.eat_str(text) {
            return Ok(kind);
        }
    }
    let single = match cur.peek() {
        Some(b'+') => Some(TokKind::Plus),
        Some(b'-') => Some(TokKind::Minus),
        Some(b'*') => Some(TokKind::Star),
        Some(b'/') => Some(TokKind::Slash),
        Some(b'%') => Some(TokKind::Percent),
        Some(b'!') => Some(TokKind::Bang),
        Some(b'=') => Some(TokKind::Assign),
        Some(b'<') => Some(TokKind::Lt),
        Some(b'>') => Some(TokKind::Gt),
        Some(b'(') => Some(TokKind::LParen),
        Some(b')') => Some(TokKind::RParen),
        Some(b'{') => Some(TokKind::LBrace),
        Some(b'}') => Some(TokKind::RBrace),
        Some(b',') => Some(TokKind::Comma),
        Some(b';') => Some(TokKind::Semi),
        _ => None,
    };
    if let Some(kind) = single {
        cur.bump();
        return Ok(kind);
    }
    // Not a prooflite byte: consume ONE FULL CHAR so the diag spans it whole
    // (a byte-wide span inside a multi-byte char would render as mojibake).
    let start = cur.pos();
    match cur.next_char() {
        Ok(Some(c)) => Err(Diag::at_code(
            codes::UNEXPECTED_CHAR,
            format!("unexpected character `{c}`"),
            cur.span_from(start),
        )),
        // Unreachable: not at EOF, and byte ops only ever consume ASCII, so
        // the cursor sits on a char boundary. Kept as a diag, not a panic.
        _ => Err(Diag::at_code(
            codes::UNEXPECTED_CHAR,
            "unexpected byte",
            Span::new(start, start + 1),
        )),
    }
}

fn int_literal(cur: &mut Cursor<'_>) -> Result<TokKind, Diag> {
    let start = cur.pos();
    let (digits, radix) = if cur.peek() == Some(b'0') && cur.peek_at(1) == Some(b'x') {
        cur.bump();
        cur.bump();
        let Some(sp) = cur.eat_hex(true) else {
            // Cover whatever follows (`0x_f`, `0xg`, bare `0x`) so the caret
            // spans the whole malformed literal, not just the prefix.
            cur.eat_while(ident_cont);
            return Err(Diag::at_code(
                codes::BAD_INT,
                "`0x` must be immediately followed by a hex digit",
                cur.span_from(start),
            ));
        };
        (sp, 16)
    } else {
        let sp = cur.eat_decimal(true).expect("caller saw a digit");
        (sp, 10)
    };
    // A digit run flowing straight into ident chars (`123abc`, `0x12g`) is one
    // malformed literal, not two tokens — never silently misparse.
    if cur.peek().is_some_and(ident_start) {
        cur.eat_while(ident_cont);
        return Err(Diag::at_code(
            codes::BAD_INT,
            "malformed integer literal",
            cur.span_from(start),
        ));
    }
    let text: String = cur.text(digits).chars().filter(|&c| c != '_').collect();
    match i64::from_str_radix(&text, radix) {
        Ok(v) => Ok(TokKind::Int(v)),
        Err(_) => Err(Diag::at_code(
            codes::BAD_INT,
            format!("integer literal out of range (max {})", i64::MAX),
            cur.span_from(start),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<TokKind> {
        lex(src).unwrap().into_iter().map(|t| t.kind).collect()
    }

    #[test]
    fn strategy_keywords_lex_and_builtins_stay_idents() {
        assert_eq!(
            kinds("lookback var signal long short flat close sma"),
            [
                TokKind::Lookback,
                TokKind::Var,
                TokKind::Signal,
                TokKind::Long,
                TokKind::Short,
                TokKind::Flat,
                TokKind::Ident,
                TokKind::Ident,
                TokKind::Eof
            ]
        );
    }

    #[test]
    fn the_ported_lexer_invariants_hold() {
        // Spot checks of the prooflite port (full coverage lives there).
        assert_eq!(kinds("1_000")[0], TokKind::Int(1000));
        assert_eq!(kinds("0xff")[0], TokKind::Int(255));
        for src in ["123abc", "0x_f", "9223372036854775808"] {
            let e = lex(src).unwrap_err();
            assert_eq!(e.code, Some(codes::BAD_INT), "{src}");
            let sp = e.span.unwrap();
            assert_eq!((sp.start, sp.end), (0, src.len()), "{src}");
        }
        assert_eq!(
            kinds("1 // line\n/* a /* nested */ b */ 2"),
            [TokKind::Int(1), TokKind::Int(2), TokKind::Eof]
        );
        assert_eq!(lex("é").unwrap_err().code, Some(codes::UNEXPECTED_CHAR));
        assert_eq!(
            lex("1 /* nope").unwrap_err().code,
            Some(codes::UNTERMINATED_COMMENT)
        );
        assert_eq!(lex("").unwrap()[0].kind, TokKind::Eof);
    }
}

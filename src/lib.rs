//! # litelite — a kit for purpose-sized languages
//!
//! Thesis: a language's smallness is not a cost you pay for embeddability —
//! it is what BUYS guarantees big languages cannot give. Fuel-bounded
//! evaluation is a termination proof. A host-capability table is a complete
//! effect bound. Pick the guarantees you need; the kit gives you the largest
//! language for which they stay mechanical.
//!
//! The kit is the shared kernel of the lite family (rustlite, soliditylite,
//! bashlite — ~19K LOC of purpose-sized languages that each hand-rolled these
//! pieces, with divergent bugs to show for it):
//!
//! - [`diag`] — spans, coded diagnostics, caret snippets
//! - [`lex`] — the byte-cursor lexer kit (UTF-8-safe by construction)
//! - [`parse`] — the recursive-descent harness with the depth guard baked in
//! - [`fuel`] — fuel + byte budgets: mechanical termination and output bounds
//! - [`cap`] — host-capability tables as data: one declaration drives
//!   checking, import emission, docs, and cross-boundary parity manifests
//!
//! Zero external dependencies. Native + wasm32.

pub use caplite as cap;
pub use diaglite as diag;
pub use fuellite as fuel;
pub use lexlite as lex;
pub use parselite as parse;

#[cfg(test)]
mod tests {
    #[test]
    fn facade_reexports_the_kernel() {
        let d = crate::diag::Diag::at_code(1, "x", crate::diag::Span::new(0, 1));
        assert!(d.to_string().starts_with("E0001"));
        assert_eq!(crate::fuel::Fuel::new(1).remaining(), 1);
        assert!(crate::lex::Cursor::new("a").peek().is_some());
        assert_eq!(crate::parse::DEFAULT_MAX_DEPTH, 96);
        assert_eq!(crate::cap::fnv1a_64(b""), 0xcbf2_9ce4_8422_2325);
    }
}

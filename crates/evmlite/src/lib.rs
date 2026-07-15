//! # evmlite — EVM bytecode assembler + diff-oracle interpreter
//!
//! Ported at M3 from soliditylite (localharness), where both halves ran in
//! production. Two independent pieces, deliberately NOT unified with any wasm
//! emitter (constitution rule 4: absolute-jump machines and structured
//! control flow are semantically irreconcilable):
//!
//! - [`asm`] — the assembler: named opcode consts (the opcode SSOT),
//!   minimal-width `PUSH`, two-pass `PUSH2` label back-patching (fixed-width
//!   placeholders, so no width fixpoint), and the `CODECOPY`/`RETURN`
//!   creation-time [`asm::init_wrapper`]. Failures are deferred, sticky, and
//!   surfaced by [`asm::Asm::finish`] as [`asm::AsmError`] — never a panic,
//!   never silently truncated bytecode.
//! - [`interp`] — a minimal, dependency-free EVM-subset executor used as the
//!   assembler's DIFF-ORACLE: deploy real init code, call it, assert on
//!   returned bytes / storage / logs. Hard-bounded (step budget, memory cap,
//!   stack cap) so hostile bytecode is an error, not an OOM. `KECCAK256` is
//!   [`interp::ExecError::Unsupported`]: hashing needs a dependency and the
//!   kit takes none — consumers that need it bring their own interpreter.
//!
//! Zero dependencies. Native + wasm32.

pub mod asm;
pub mod interp;

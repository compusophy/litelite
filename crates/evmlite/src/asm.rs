//! EVM bytecode assembler — for an ABSOLUTE-jump machine (vs wasm's
//! structured/relative control flow; the two emitters stay independent).
//!
//! A single struct accumulates bytes; opcodes are named consts; a final pass
//! produces the program. Errors are STICKY: the first construction fault
//! (over-wide push, double-placed label) is recorded and surfaced by
//! [`Asm::finish`], so emit methods stay chainable and a broken build can
//! never yield wrong-but-clean bytecode.
//!
//! ## Two-pass label resolution
//!
//! EVM jumps take an ABSOLUTE program-counter operand, so a forward jump's
//! target is unknown when the jump is emitted. Resolution is two passes with
//! NO width fixpoint:
//!
//! - **Pass 1** ([`Asm::push_label`]) emits a FIXED-WIDTH `PUSH2 0x0000`
//!   placeholder per reference and records its operand offset;
//!   [`Asm::jumpdest`] emits `0x5B` and records the label's byte offset.
//!   Every reference is exactly 3 bytes, so placement never shifts offsets.
//! - **Pass 2** ([`Asm::finish`]) back-patches each reference's 2 big-endian
//!   operand bytes with its label's resolved offset.

/// EVM opcodes used by the assembler and the [`crate::interp`] oracle — the
/// opcode SSOT; no stray byte literals elsewhere.
pub mod op {
    /// Halt + revert, returning `mem[offset..offset+len]`.
    pub const REVERT: u8 = 0xFD;
    /// Halt + return `mem[offset..offset+len]` as the call's output.
    pub const RETURN: u8 = 0xF3;
    /// Copy `len` bytes of THIS contract's code into memory.
    pub const CODECOPY: u8 = 0x39;
    /// Load a 32-byte word from memory.
    pub const MLOAD: u8 = 0x51;
    /// Store a 32-byte word into memory (`MSTORE(off, word)`).
    pub const MSTORE: u8 = 0x52;
    /// Load a 32-byte word from storage (`SLOAD(slot)`).
    pub const SLOAD: u8 = 0x54;
    /// Store a 32-byte word into storage (`SSTORE(slot, word)`).
    pub const SSTORE: u8 = 0x55;
    /// Size of the call's calldata in bytes.
    pub const CALLDATASIZE: u8 = 0x36;
    /// Load a 32-byte word from calldata at `off` (zero-extended past the end).
    pub const CALLDATALOAD: u8 = 0x35;
    /// Copy `len` calldata bytes into memory, zero-extending (the EVM rule).
    pub const CALLDATACOPY: u8 = 0x37;
    /// Unsigned less-than (`μs[0] < μs[1]`, top vs next).
    pub const LT: u8 = 0x10;
    /// Unsigned greater-than.
    pub const GT: u8 = 0x11;
    /// Equality.
    pub const EQ: u8 = 0x14;
    /// `1` if the top item is `0`, else `0` — logical NOT / branch inversion.
    pub const ISZERO: u8 = 0x15;
    /// Logical right shift (`SHR(shift, value)`).
    pub const SHR: u8 = 0x1C;
    /// Bitwise AND.
    pub const AND: u8 = 0x16;
    /// Addition (wrapping mod 2^256).
    pub const ADD: u8 = 0x01;
    /// Subtraction — `μs[0] - μs[1]` (top minus next), wrapping mod 2^256.
    pub const SUB: u8 = 0x03;
    /// Multiplication, wrapping mod 2^256.
    pub const MUL: u8 = 0x02;
    /// Integer division — yields 0 when the divisor is 0 (EVM, no revert).
    pub const DIV: u8 = 0x04;
    /// Modulo — yields 0 when the divisor is 0 (EVM, no revert).
    pub const MOD: u8 = 0x06;
    /// Keccak-256 of `mem[offset..offset+len]`. The [`crate::interp`] oracle
    /// reports this as `Unsupported` (hashing needs a dep; the kit takes none).
    pub const KECCAK256: u8 = 0x20;
    /// The 20-byte caller address, left-padded to a word (`msg.sender`).
    pub const CALLER: u8 = 0x33;
    /// The current block's unix timestamp (`block.timestamp`).
    pub const TIMESTAMP: u8 = 0x42;
    /// The current block height (`block.number`).
    pub const NUMBER: u8 = 0x43;
    /// Duplicate the top stack item.
    pub const DUP1: u8 = 0x80;
    /// Duplicate the 2nd-from-top stack item.
    pub const DUP2: u8 = 0x81;
    /// Duplicate the 3rd-from-top stack item.
    pub const DUP3: u8 = 0x82;
    /// Swap the top two stack items.
    pub const SWAP1: u8 = 0x90;
    /// Pop the top stack item.
    pub const POP: u8 = 0x50;
    /// Unconditional absolute jump (`JUMP(dest)`).
    pub const JUMP: u8 = 0x56;
    /// Conditional absolute jump (`JUMPI(dest, cond)`).
    pub const JUMPI: u8 = 0x57;
    /// Valid jump target marker.
    pub const JUMPDEST: u8 = 0x5B;
    /// `LOG0(offset, len)` — a log with 0 topics over `mem[offset..offset+len]`.
    pub const LOG0: u8 = 0xA0;
    /// `LOG1(offset, len, topic0)`.
    pub const LOG1: u8 = 0xA1;
    /// `LOG2(offset, len, topic0, topic1)`.
    pub const LOG2: u8 = 0xA2;
    /// `LOG3(offset, len, topic0..topic2)`.
    pub const LOG3: u8 = 0xA3;
    /// `LOG4(offset, len, topic0..topic3)` — the EVM max.
    pub const LOG4: u8 = 0xA4;
    /// `PUSH1` base; `PUSH<n>` = `PUSH1 + (n - 1)`.
    pub const PUSH1: u8 = 0x60;
    /// `PUSH2` — the fixed width used for every label reference.
    pub const PUSH2: u8 = 0x61;
}

/// Why a build cannot produce bytecode. Recorded sticky at the faulting call,
/// surfaced by [`Asm::finish`] — a broken build never yields bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsmError {
    /// A [`Asm::push`] operand wider than 32 significant bytes (an EVM word).
    PushTooWide(usize),
    /// A label was [`Asm::jumpdest`]-placed more than once.
    LabelPlacedTwice(usize),
    /// A referenced label was never placed (a jump with no `JUMPDEST`).
    UnplacedLabel(usize),
    /// A resolved jump target exceeds `u16::MAX` (a >64KB program — far past
    /// the EIP-170 code-size limit).
    JumpTargetTooFar(usize),
    /// [`init_wrapper`]'s runtime blob exceeds `u16::MAX` bytes.
    RuntimeTooLarge(usize),
    /// A [`Label`] from a DIFFERENT `Asm` instance (labels are per-assembler).
    UnknownLabel(usize),
}

impl std::fmt::Display for AsmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AsmError::PushTooWide(n) => write!(f, "PUSH operand is {n} bytes; an EVM word is 32"),
            AsmError::LabelPlacedTwice(l) => write!(f, "label {l} placed twice"),
            AsmError::UnplacedLabel(l) => write!(f, "label {l} referenced but never placed"),
            AsmError::JumpTargetTooFar(o) => {
                write!(f, "jump target {o} exceeds 64KB (past EIP-170)")
            }
            AsmError::RuntimeTooLarge(n) => write!(f, "runtime is {n} bytes; the cap is 64KB"),
            AsmError::UnknownLabel(l) => {
                write!(f, "label {l} belongs to a different assembler")
            }
        }
    }
}

impl std::error::Error for AsmError {}

/// A label: a named jump target whose absolute PC resolves in pass 2.
/// Obtained from [`Asm::new_label`], placed with [`Asm::jumpdest`], referenced
/// with [`Asm::push_label`] — before OR after placement, any number of times.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Label(usize);

/// The EVM bytecode assembler. Build with the chainable `emit*`/`push*`/
/// `jumpdest` methods, then [`Asm::finish`] back-patches labels and returns
/// the bytes (or the first construction fault).
#[derive(Debug, Default)]
pub struct Asm {
    /// Accumulating bytecode (with `PUSH2 0x0000` placeholders for labels).
    code: Vec<u8>,
    /// `dests[label]` = the byte offset of that label's `JUMPDEST`, once placed.
    dests: Vec<Option<usize>>,
    /// Pending back-patches: `(operand_byte_offset, label)`.
    refs: Vec<(usize, Label)>,
    /// The first construction fault, surfaced by [`Asm::finish`].
    err: Option<AsmError>,
}

impl Asm {
    /// A fresh, empty assembler.
    pub fn new() -> Self {
        Self::default()
    }

    /// The current byte offset (the PC of the next emitted byte).
    pub fn here(&self) -> usize {
        self.code.len()
    }

    fn fault(&mut self, e: AsmError) {
        if self.err.is_none() {
            self.err = Some(e);
        }
    }

    /// Emit a single raw opcode byte (no operand). Use the [`op`] consts.
    pub fn emit(&mut self, opcode: u8) -> &mut Self {
        self.code.push(opcode);
        self
    }

    /// Emit several raw opcode bytes in order.
    pub fn emit_all(&mut self, opcodes: &[u8]) -> &mut Self {
        self.code.extend_from_slice(opcodes);
        self
    }

    /// Push a big-endian integer using the MINIMAL `PUSH<n>` that fits.
    ///
    /// Leading zero bytes are stripped before selecting the width; the
    /// all-zero value emits `PUSH1 0x00` (NOT `PUSH0`/EIP-3855 — availability
    /// treated conservatively). More than 32 significant bytes is a sticky
    /// [`AsmError::PushTooWide`] — never a truncation.
    pub fn push(&mut self, bytes: &[u8]) -> &mut Self {
        let first = bytes.iter().position(|&b| b != 0);
        let sig: &[u8] = match first {
            None => &[0u8], // all-zero (or empty) → PUSH1 0x00
            Some(i) => &bytes[i..],
        };
        if sig.len() > 32 {
            self.fault(AsmError::PushTooWide(sig.len()));
            return self;
        }
        let n = sig.len() as u8; // 1..=32
        self.code.push(op::PUSH1 + (n - 1));
        self.code.extend_from_slice(sig);
        self
    }

    /// Push a `u64` constant (minimal width, big-endian).
    pub fn push_u64(&mut self, value: u64) -> &mut Self {
        self.push(&value.to_be_bytes())
    }

    /// Push a full 32-byte word with `PUSH32`, NO leading-zero stripping —
    /// for values whose full width is semantically meaningful (keccak-derived
    /// slots, 32-byte return words).
    pub fn push32(&mut self, word: &[u8; 32]) -> &mut Self {
        self.code.push(op::PUSH1 + 31); // PUSH32
        self.code.extend_from_slice(word);
        self
    }

    /// Allocate a fresh, unplaced label.
    pub fn new_label(&mut self) -> Label {
        let id = self.dests.len();
        self.dests.push(None);
        Label(id)
    }

    /// Place `label` at the current offset: emits `0x5B JUMPDEST` and records
    /// this offset as the label's PC. A second placement is a sticky
    /// [`AsmError::LabelPlacedTwice`]; a label from another `Asm` is a sticky
    /// [`AsmError::UnknownLabel`] — never a panic.
    pub fn jumpdest(&mut self, label: Label) -> &mut Self {
        match self.dests.get(label.0) {
            None => {
                self.fault(AsmError::UnknownLabel(label.0));
                return self;
            }
            Some(Some(_)) => {
                self.fault(AsmError::LabelPlacedTwice(label.0));
                return self;
            }
            Some(None) => {}
        }
        self.dests[label.0] = Some(self.code.len());
        self.code.push(op::JUMPDEST);
        self
    }

    /// Emit a FIXED-WIDTH `PUSH2 0x0000` placeholder referencing `label`, to
    /// be back-patched in pass 2. Pushes the label's absolute PC; follow with
    /// `JUMP`/`JUMPI`.
    pub fn push_label(&mut self, label: Label) -> &mut Self {
        self.code.push(op::PUSH2);
        self.refs.push((self.code.len(), label));
        self.code.push(0x00);
        self.code.push(0x00);
        self
    }

    /// Pass 2: back-patch every label reference and return the bytecode, or
    /// the FIRST fault — a sticky construction error, an unplaced label, or a
    /// jump target past `u16::MAX`.
    pub fn finish(mut self) -> Result<Vec<u8>, AsmError> {
        if let Some(e) = self.err {
            return Err(e);
        }
        for (operand_off, label) in &self.refs {
            let Some(slot) = self.dests.get(label.0) else {
                return Err(AsmError::UnknownLabel(label.0));
            };
            let Some(dest) = *slot else {
                return Err(AsmError::UnplacedLabel(label.0));
            };
            let Ok(dest16) = u16::try_from(dest) else {
                return Err(AsmError::JumpTargetTooFar(dest));
            };
            let be = dest16.to_be_bytes();
            self.code[*operand_off] = be[0];
            self.code[*operand_off + 1] = be[1];
        }
        Ok(self.code)
    }
}

/// Prepend the constant init wrapper (the contract-creation constructor) to a
/// runtime blob, yielding the full INIT code for a CREATE transaction.
///
/// The wrapper `CODECOPY`s the trailing runtime into memory and `RETURN`s it
/// as the deployed code. The prelude is a fixed 13 bytes, so the runtime
/// offset is the constant `0x000D`:
///
/// ```text
/// PUSH2 <rt_len>  DUP1  PUSH2 <rt_off>  PUSH1 0x00  CODECOPY  PUSH1 0x00  RETURN
/// ```
pub fn init_wrapper(runtime: &[u8]) -> Result<Vec<u8>, AsmError> {
    let Ok(rt_len) = u16::try_from(runtime.len()) else {
        return Err(AsmError::RuntimeTooLarge(runtime.len()));
    };
    const PRELUDE_LEN: u16 = 13; // 3+1+3+2+1+2+1
    let len_be = rt_len.to_be_bytes();
    let off_be = PRELUDE_LEN.to_be_bytes();
    let mut out = Vec::with_capacity(PRELUDE_LEN as usize + runtime.len());
    out.extend_from_slice(&[op::PUSH2, len_be[0], len_be[1]]); // [len]
    out.push(op::DUP1); // [len, len]
    out.extend_from_slice(&[op::PUSH2, off_be[0], off_be[1]]); // [len, len, off]
    out.extend_from_slice(&[op::PUSH1, 0x00]); // [len, len, off, 0]
    out.push(op::CODECOPY); // mem[0..len] = code[off..]; [len]
    out.extend_from_slice(&[op::PUSH1, 0x00]); // [len, 0]
    out.push(op::RETURN);
    debug_assert_eq!(out.len(), PRELUDE_LEN as usize);
    out.extend_from_slice(runtime);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{Asm, AsmError, init_wrapper, op};

    #[test]
    fn push_then_add_emits_exact_bytes() {
        let mut asm = Asm::new();
        asm.push(&[1]).push(&[2]).emit(op::ADD);
        assert_eq!(asm.finish().unwrap(), vec![0x60, 0x01, 0x60, 0x02, 0x01]);
    }

    #[test]
    fn push_size_boundaries() {
        for (input, expect) in [
            (vec![0u8], vec![op::PUSH1, 0x00]), // zero → PUSH1 0x00, never PUSH0
            (vec![], vec![op::PUSH1, 0x00]),
            (vec![0, 0, 0, 0], vec![op::PUSH1, 0x00]),
            (vec![0xff], vec![op::PUSH1, 0xff]),
            (
                0x0100u16.to_be_bytes().to_vec(),
                vec![op::PUSH2, 0x01, 0x00],
            ),
            // leading-zero stripping
            (0x0000_00ffu32.to_be_bytes().to_vec(), vec![op::PUSH1, 0xff]),
        ] {
            let mut a = Asm::new();
            a.push(&input);
            assert_eq!(a.finish().unwrap(), expect, "{input:x?}");
        }
        // A full 32-byte value → PUSH32 + 32 bytes, no stripping.
        let mut val = [0u8; 32];
        val[0] = 0x12;
        val[31] = 0x34;
        let mut a = Asm::new();
        a.push(&val);
        let out = a.finish().unwrap();
        assert_eq!((out[0], out.len()), (op::PUSH1 + 31, 33));
        assert_eq!(&out[1..], &val);
    }

    #[test]
    fn push32_never_strips() {
        let mut word = [0u8; 32];
        word[31] = 0x2a;
        let mut a = Asm::new();
        a.push32(&word);
        let out = a.finish().unwrap();
        assert_eq!((out[0], out.len()), (op::PUSH1 + 31, 33));
        assert_eq!(&out[1..], &word);
    }

    #[test]
    fn forward_jump_resolves() {
        // PUSH2 <L> JUMPI POP ; L: JUMPDEST — L resolves to offset 5.
        let mut a = Asm::new();
        let l = a.new_label();
        a.push_label(l).emit(op::JUMPI).emit(op::POP);
        a.jumpdest(l);
        let out = a.finish().unwrap();
        assert_eq!(
            out,
            vec![op::PUSH2, 0x00, 0x05, op::JUMPI, op::POP, op::JUMPDEST]
        );
        let operand = u16::from_be_bytes([out[1], out[2]]) as usize;
        assert_eq!(out[operand], op::JUMPDEST);
    }

    #[test]
    fn back_jump_and_multiple_refs_resolve() {
        let mut a = Asm::new();
        let l = a.new_label();
        a.jumpdest(l); // L = offset 0
        a.push_label(l).emit(op::JUMP);
        let out = a.finish().unwrap();
        assert_eq!(out, vec![op::JUMPDEST, op::PUSH2, 0x00, 0x00, op::JUMP]);
        // Multiple references all patch.
        let mut a = Asm::new();
        let l = a.new_label();
        a.push_label(l).emit(op::POP);
        a.push_label(l).emit(op::POP);
        a.jumpdest(l); // L = offset 8
        let out = a.finish().unwrap();
        assert_eq!(u16::from_be_bytes([out[1], out[2]]), 8);
        assert_eq!(u16::from_be_bytes([out[5], out[6]]), 8);
        assert_eq!(out[8], op::JUMPDEST);
    }

    #[test]
    fn construction_faults_are_sticky_never_silent() {
        // Over-wide push: the build fails at finish, no truncated bytes.
        let mut a = Asm::new();
        a.push(&[1u8; 33]).emit(op::POP);
        assert_eq!(a.finish(), Err(AsmError::PushTooWide(33)));
        // Double placement.
        let mut a = Asm::new();
        let l = a.new_label();
        a.jumpdest(l);
        a.jumpdest(l);
        assert_eq!(a.finish(), Err(AsmError::LabelPlacedTwice(0)));
        // A referenced label never placed.
        let mut a = Asm::new();
        let l = a.new_label();
        a.push_label(l).emit(op::JUMP);
        assert_eq!(a.finish(), Err(AsmError::UnplacedLabel(0)));
        // Oversized runtime for the init wrapper.
        let big = vec![0u8; 70_000];
        assert_eq!(init_wrapper(&big), Err(AsmError::RuntimeTooLarge(70_000)));
        // A label from ANOTHER assembler: sticky error, never a panic.
        let mut a = Asm::new();
        let foreign = a.new_label();
        let mut b = Asm::new();
        b.jumpdest(foreign);
        assert_eq!(b.finish(), Err(AsmError::UnknownLabel(0)));
        let mut b = Asm::new();
        b.push_label(foreign).emit(op::JUMP);
        assert_eq!(b.finish(), Err(AsmError::UnknownLabel(0)));
    }

    #[test]
    fn init_wrapper_prelude_is_exact() {
        let runtime = [0xAA, 0xBB, 0xCC];
        let init = init_wrapper(&runtime).unwrap();
        #[rustfmt::skip]
        let expected_prelude = [
            op::PUSH2, 0x00, 0x03, // PUSH2 rt_len (3)
            op::DUP1,
            op::PUSH2, 0x00, 0x0D, // PUSH2 rt_off (13)
            op::PUSH1, 0x00,
            op::CODECOPY,
            op::PUSH1, 0x00,
            op::RETURN,
        ];
        assert_eq!(&init[..13], &expected_prelude);
        assert_eq!(&init[13..], &runtime);
        // A 300-byte runtime → rt_len 0x012C, rt_off still 0x000D.
        let init = init_wrapper(&[0x5Bu8; 300]).unwrap();
        assert_eq!(&init[..3], &[op::PUSH2, 0x01, 0x2C]);
        assert_eq!(&init[4..7], &[op::PUSH2, 0x00, 0x0D]);
        assert_eq!(init.len(), 13 + 300);
    }
}

//! A MINIMAL, dependency-free EVM-subset executor — the diff-oracle for
//! [`crate::asm`]. It executes exactly the assembler's opcode set (a stack
//! machine + memory + storage), so assembled programs can be deployed and
//! called entirely in-process and their results asserted. Anything outside
//! the set is [`ExecError::UnknownOpcode`] — the oracle never silently no-ops
//! an unhandled instruction (which would mask an emitter bug).
//!
//! Hard bounds make hostile bytecode an error instead of an OOM/hang: a step
//! budget ([`STEP_BUDGET`]), a memory cap ([`MAX_MEMORY`] — untrusted code
//! can ask for `off = 0xFFFF_FFFF`), and the EVM's 1024-item stack limit.
//!
//! `KECCAK256` is [`ExecError::Unsupported`]: hashing needs a dependency and
//! the kit takes none (constitution rule 1) — a consumer needing keccak
//! brings its own interpreter or wraps this one.
//!
//! Arithmetic note: `ADD`/`SUB` are full 256-bit (wrapping); `MUL`/`DIV`/
//! `MOD` compute in `u128` — operands are read as their LOW 128 BITS and a
//! `MUL` product truncates mod 2^128 — sufficient for an oracle over bounded
//! test values, NOT a full 256-bit ALU. Documented so the limit is a known
//! edge, not a surprise.
//!
//! State semantics match the chain: a call that does not complete with
//! `RETURN`/stop rolls its storage writes back (execution runs against a
//! staged copy, committed only on success), and jump targets are validated
//! against a JUMPDEST analysis that EXCLUDES `PUSH` immediate bytes.

use std::collections::HashMap;

use crate::asm::op;

/// A 256-bit EVM word, big-endian.
pub type Word = [u8; 32];

/// Why execution halted abnormally (anything but a clean `RETURN`/stop).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecError {
    /// The program executed `REVERT(off, len)` — the returned data.
    Revert(Vec<u8>),
    /// An opcode outside the evmlite subset — an emitter bug, never ignored.
    UnknownOpcode(u8),
    /// An opcode in the set but not executable without a dependency
    /// (`KECCAK256`).
    Unsupported(u8),
    /// Popped more than was pushed — an emitter bug.
    StackUnderflow,
    /// Stack exceeded the EVM 1024-item limit.
    StackOverflow,
    /// A `JUMP`/`JUMPI` target that is not a `JUMPDEST` (real EVM rule).
    BadJumpDest(usize),
    /// The step budget or memory cap was exhausted (runaway guard).
    OutOfGas,
}

impl std::fmt::Display for ExecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecError::Revert(d) => write!(f, "REVERT with {} bytes of data", d.len()),
            ExecError::UnknownOpcode(b) => write!(f, "opcode 0x{b:02x} is outside the subset"),
            ExecError::Unsupported(b) => {
                write!(
                    f,
                    "opcode 0x{b:02x} needs a dependency the kit does not take"
                )
            }
            ExecError::StackUnderflow => write!(f, "stack underflow"),
            ExecError::StackOverflow => write!(f, "stack exceeded the EVM 1024-item limit"),
            ExecError::BadJumpDest(pc) => write!(f, "jump target {pc} is not a JUMPDEST"),
            ExecError::OutOfGas => write!(f, "the step budget or memory cap was exhausted"),
        }
    }
}

impl std::error::Error for ExecError {}

/// The result of a successful call: the `RETURN`ed data (possibly empty).
pub type ExecResult = Result<Vec<u8>, ExecError>;

/// A persistent contract account: deployed runtime bytecode + word→word
/// storage. Construct via [`Contract::deploy`] so the oracle exercises the
/// same `CODECOPY`/`RETURN` constructor path the chain runs.
#[derive(Debug, Clone, Default)]
pub struct Contract {
    /// The deployed runtime bytecode.
    pub code: Vec<u8>,
    /// Persistent storage: slot → word. Missing slots read as zero.
    pub storage: HashMap<Word, Word>,
}

/// The transaction-like context for one call — the only environment reads
/// the subset has (`CALLER`/`TIMESTAMP`/`NUMBER`).
#[derive(Debug, Clone, Default)]
pub struct CallEnv {
    /// `msg.sender` — a 20-byte address, left-padded to a word on read.
    pub caller: [u8; 20],
    /// `block.timestamp`.
    pub timestamp: u64,
    /// `block.number`.
    pub number: u64,
}

/// An emitted log (`LOGn`): topics (`topic0` first) + the data region bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub topics: Vec<Word>,
    pub data: Vec<u8>,
}

// The oracle's three hard bounds are public: they are what makes hostile
// bytecode an error instead of a hang, so a consumer is entitled to know
// them (and the module docs above link to them).

/// Hard cap on executed instructions — a runaway/loop guard. Past it,
/// execution stops with [`ExecError::OutOfGas`].
pub const STEP_BUDGET: usize = 1_000_000;

/// Hard cap on memory growth (16 MiB) — a huge required end offset is a
/// clean [`ExecError::OutOfGas`] instead of a giant allocation.
pub const MAX_MEMORY: usize = 16 * 1024 * 1024;

/// The real EVM's stack-depth limit; past it, [`ExecError::StackOverflow`].
pub const MAX_STACK: usize = 1024;

impl Contract {
    /// "Deploy" INIT code EVM-style: run it with empty calldata, and the
    /// bytes it `RETURN`s become the deployed `code` (covers
    /// [`crate::asm::init_wrapper`]'s constructor). Storage starts empty.
    pub fn deploy(init_code: &[u8], env: &CallEnv) -> Result<Contract, ExecError> {
        let mut c = Contract::default();
        let runtime = run(init_code, &[], env, &mut c.storage, &mut Vec::new())?;
        c.code = runtime;
        Ok(c)
    }

    /// Execute a call against the deployed `code` with `calldata`. Storage
    /// writes COMMIT only when the call completes with `RETURN`/stop — any
    /// error (a `REVERT` included) rolls them back, as on the real chain.
    /// Logs are discarded (use [`Contract::call_logs`]).
    pub fn call(&mut self, calldata: &[u8], env: &CallEnv) -> ExecResult {
        self.call_logs(calldata, env).map(|(ret, _)| ret)
    }

    /// Like [`Contract::call`], also returning the logs emitted (logs, like
    /// storage, only survive a successful call).
    pub fn call_logs(
        &mut self,
        calldata: &[u8],
        env: &CallEnv,
    ) -> Result<(Vec<u8>, Vec<LogEntry>), ExecError> {
        let mut staged = self.storage.clone();
        let mut logs = Vec::new();
        let ret = run(&self.code, calldata, env, &mut staged, &mut logs)?;
        self.storage = staged;
        Ok((ret, logs))
    }

    /// Read storage slot `slot` (zero if never written) — post-state
    /// assertions without going through a getter.
    pub fn sload(&self, slot: &Word) -> Word {
        self.storage.get(slot).copied().unwrap_or([0u8; 32])
    }
}

/// Build a `selector ++ args` calldata blob (each arg a 32-byte word).
pub fn calldata(selector: [u8; 4], args: &[Word]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + 32 * args.len());
    out.extend_from_slice(&selector);
    for a in args {
        out.extend_from_slice(a);
    }
    out
}

/// A `u64` as a big-endian 32-byte word.
pub fn word(v: u64) -> Word {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&v.to_be_bytes());
    w
}

/// A 20-byte address as a left-padded 32-byte word.
pub fn addr_word(a: &[u8; 20]) -> Word {
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(a);
    w
}

/// The low 8 bytes of a word as a `u64` — for small return values.
pub fn word_to_u64(w: &Word) -> u64 {
    let mut b = [0u8; 8];
    b.copy_from_slice(&w[24..]);
    u64::from_be_bytes(b)
}

/// Full 256-bit wrapping add (matches the EVM).
fn add256(a: &Word, b: &Word) -> Word {
    let mut out = [0u8; 32];
    let mut carry = 0u16;
    for i in (0..32).rev() {
        let v = a[i] as u16 + b[i] as u16 + carry;
        out[i] = (v & 0xFF) as u8;
        carry = v >> 8;
    }
    out
}

/// Full 256-bit wrapping subtract `a - b`.
fn sub256(a: &Word, b: &Word) -> Word {
    let mut out = [0u8; 32];
    let mut borrow = 0i16;
    for i in (0..32).rev() {
        let v = a[i] as i16 - b[i] as i16 - borrow;
        if v < 0 {
            out[i] = (v + 256) as u8;
            borrow = 1;
        } else {
            out[i] = v as u8;
            borrow = 0;
        }
    }
    out
}

struct Vm<'a> {
    code: &'a [u8],
    calldata: &'a [u8],
    env: &'a CallEnv,
    storage: &'a mut HashMap<Word, Word>,
    logs: &'a mut Vec<LogEntry>,
    stack: Vec<Word>,
    /// Byte-addressed memory, grown on demand (zero-filled).
    memory: Vec<u8>,
    pc: usize,
    /// Byte offsets that are REAL `JUMPDEST`s — a `0x5B` inside a `PUSH`
    /// immediate is not one (the chain's JUMPDEST analysis).
    jumpdests: Vec<usize>,
}

/// One linear scan collecting valid jump targets, skipping `PUSH` immediates.
fn jumpdest_analysis(code: &[u8]) -> Vec<usize> {
    let mut dests = Vec::new();
    let mut pc = 0;
    while pc < code.len() {
        let b = code[pc];
        if b == op::JUMPDEST {
            dests.push(pc);
        }
        pc += if (op::PUSH1..=op::PUSH1 + 31).contains(&b) {
            1 + (b - op::PUSH1) as usize + 1
        } else {
            1
        };
    }
    dests
}

/// Execute `code` with `calldata`; `storage` is read+written in place and
/// `logs` accumulates any `LOGn`.
fn run(
    code: &[u8],
    calldata: &[u8],
    env: &CallEnv,
    storage: &mut HashMap<Word, Word>,
    logs: &mut Vec<LogEntry>,
) -> ExecResult {
    Vm {
        code,
        calldata,
        env,
        storage,
        logs,
        stack: Vec::new(),
        memory: Vec::new(),
        pc: 0,
        jumpdests: jumpdest_analysis(code),
    }
    .exec()
}

impl Vm<'_> {
    fn pop(&mut self) -> Result<Word, ExecError> {
        self.stack.pop().ok_or(ExecError::StackUnderflow)
    }

    /// Ensure memory covers `[off, off+len)`, zero-extending; a required end
    /// past [`MAX_MEMORY`] is a clean [`ExecError::OutOfGas`]. A zero-length
    /// span touches nothing regardless of offset (the EVM's zero-size rule).
    fn ensure_mem(&mut self, off: usize, len: usize) -> Result<(), ExecError> {
        if len == 0 {
            return Ok(());
        }
        let end = off.saturating_add(len);
        if end > MAX_MEMORY {
            return Err(ExecError::OutOfGas);
        }
        if end > self.memory.len() {
            self.memory.resize(end, 0);
        }
        Ok(())
    }

    fn mstore(&mut self, off: usize, w: &Word) -> Result<(), ExecError> {
        self.ensure_mem(off, 32)?;
        self.memory[off..off + 32].copy_from_slice(w);
        Ok(())
    }

    fn mload(&mut self, off: usize) -> Result<Word, ExecError> {
        self.ensure_mem(off, 32)?;
        let mut w = [0u8; 32];
        w.copy_from_slice(&self.memory[off..off + 32]);
        Ok(w)
    }

    /// Copy `mem[off..off+len]` out (for `RETURN`/`REVERT`/`LOGn`). A
    /// zero-length span is empty regardless of offset — never a slice panic.
    fn mem_slice(&mut self, off: usize, len: usize) -> Result<Vec<u8>, ExecError> {
        if len == 0 {
            return Ok(Vec::new());
        }
        self.ensure_mem(off, len)?;
        Ok(self.memory[off..off + len].to_vec())
    }

    /// The `CALLDATALOAD` word: 32 bytes at `off`, zero-extended past the end
    /// (checked adds — a huge offset reads zeros, never wraps to the start).
    fn calldataword(&self, off: usize) -> Word {
        let mut w = [0u8; 32];
        for (i, byte) in w.iter_mut().enumerate() {
            *byte = off
                .checked_add(i)
                .and_then(|src| self.calldata.get(src))
                .copied()
                .unwrap_or(0);
        }
        w
    }

    fn exec(&mut self) -> ExecResult {
        let mut steps = 0usize;
        loop {
            steps += 1;
            if steps > STEP_BUDGET {
                return Err(ExecError::OutOfGas);
            }
            if self.stack.len() > MAX_STACK {
                return Err(ExecError::StackOverflow);
            }
            if self.pc >= self.code.len() {
                return Ok(Vec::new()); // running off the end = implicit STOP
            }
            let opc = self.code[self.pc];
            match opc {
                // PUSH1..PUSH32: the immediate, right-aligned into a word.
                o if (op::PUSH1..=op::PUSH1 + 31).contains(&o) => {
                    let n = (o - op::PUSH1) as usize + 1;
                    let start = self.pc + 1;
                    let mut w = [0u8; 32];
                    for i in 0..n {
                        w[32 - n + i] = self.code.get(start + i).copied().unwrap_or(0);
                    }
                    self.stack.push(w);
                    self.pc += 1 + n;
                }
                op::POP => {
                    self.pop()?;
                    self.pc += 1;
                }
                o if (op::DUP1..=op::DUP3).contains(&o) => {
                    let depth = (o - op::DUP1) as usize;
                    let v = *self
                        .stack
                        .iter()
                        .rev()
                        .nth(depth)
                        .ok_or(ExecError::StackUnderflow)?;
                    self.stack.push(v);
                    self.pc += 1;
                }
                op::SWAP1 => {
                    let n = self.stack.len();
                    if n < 2 {
                        return Err(ExecError::StackUnderflow);
                    }
                    self.stack.swap(n - 1, n - 2);
                    self.pc += 1;
                }
                op::ADD => {
                    let a = self.pop()?;
                    let b = self.pop()?;
                    self.stack.push(add256(&a, &b));
                    self.pc += 1;
                }
                op::SUB => {
                    // μs[0] - μs[1] (top minus next).
                    let a = self.pop()?;
                    let b = self.pop()?;
                    self.stack.push(sub256(&a, &b));
                    self.pc += 1;
                }
                op::MUL => {
                    let a = word_to_u128(&self.pop()?);
                    let b = word_to_u128(&self.pop()?);
                    self.stack.push(u128_to_word(a.wrapping_mul(b)));
                    self.pc += 1;
                }
                op::DIV => {
                    let a = word_to_u128(&self.pop()?);
                    let b = word_to_u128(&self.pop()?);
                    // EVM DIV-by-zero yields 0.
                    self.stack.push(u128_to_word(a.checked_div(b).unwrap_or(0)));
                    self.pc += 1;
                }
                op::MOD => {
                    let a = word_to_u128(&self.pop()?);
                    let b = word_to_u128(&self.pop()?);
                    self.stack.push(u128_to_word(a.checked_rem(b).unwrap_or(0)));
                    self.pc += 1;
                }
                op::LT => {
                    let a = self.pop()?;
                    let b = self.pop()?;
                    self.stack.push(bool_word(a < b)); // unsigned BE compare
                    self.pc += 1;
                }
                op::GT => {
                    let a = self.pop()?;
                    let b = self.pop()?;
                    self.stack.push(bool_word(a > b));
                    self.pc += 1;
                }
                op::EQ => {
                    let a = self.pop()?;
                    let b = self.pop()?;
                    self.stack.push(bool_word(a == b));
                    self.pc += 1;
                }
                op::ISZERO => {
                    let a = self.pop()?;
                    self.stack.push(bool_word(a == [0u8; 32]));
                    self.pc += 1;
                }
                op::SHR => {
                    // SHR(shift, value): top = shift, next = value.
                    let shift = word_to_u128(&self.pop()?);
                    let value = self.pop()?;
                    self.stack.push(shr256(&value, shift));
                    self.pc += 1;
                }
                op::AND => {
                    let a = self.pop()?;
                    let b = self.pop()?;
                    let mut out = [0u8; 32];
                    for i in 0..32 {
                        out[i] = a[i] & b[i];
                    }
                    self.stack.push(out);
                    self.pc += 1;
                }
                op::KECCAK256 => return Err(ExecError::Unsupported(opc)),
                op::MSTORE => {
                    let off = word_offset(&self.pop()?);
                    let val = self.pop()?;
                    self.mstore(off, &val)?;
                    self.pc += 1;
                }
                op::MLOAD => {
                    let off = word_offset(&self.pop()?);
                    let w = self.mload(off)?;
                    self.stack.push(w);
                    self.pc += 1;
                }
                op::SLOAD => {
                    let slot = self.pop()?;
                    let v = self.storage.get(&slot).copied().unwrap_or([0u8; 32]);
                    self.stack.push(v);
                    self.pc += 1;
                }
                op::SSTORE => {
                    let slot = self.pop()?;
                    let val = self.pop()?;
                    if val == [0u8; 32] {
                        self.storage.remove(&slot);
                    } else {
                        self.storage.insert(slot, val);
                    }
                    self.pc += 1;
                }
                op::CALLDATASIZE => {
                    self.stack.push(word(self.calldata.len() as u64));
                    self.pc += 1;
                }
                op::CALLDATALOAD => {
                    let off = word_offset(&self.pop()?);
                    let w = self.calldataword(off);
                    self.stack.push(w);
                    self.pc += 1;
                }
                op::CALLER => {
                    self.stack.push(addr_word(&self.env.caller));
                    self.pc += 1;
                }
                op::TIMESTAMP => {
                    self.stack.push(word(self.env.timestamp));
                    self.pc += 1;
                }
                op::NUMBER => {
                    self.stack.push(word(self.env.number));
                    self.pc += 1;
                }
                op::CODECOPY => {
                    // CODECOPY(destOff, codeOff, len), zero-extending past code.
                    let dest = word_offset(&self.pop()?);
                    let src = word_offset(&self.pop()?);
                    let len = word_offset(&self.pop()?);
                    self.ensure_mem(dest, len)?;
                    for i in 0..len {
                        self.memory[dest + i] = src
                            .checked_add(i)
                            .and_then(|s| self.code.get(s))
                            .copied()
                            .unwrap_or(0);
                    }
                    self.pc += 1;
                }
                op::CALLDATACOPY => {
                    // CALLDATACOPY(destOff, srcOff, len), zero-extending.
                    let dest = word_offset(&self.pop()?);
                    let src = word_offset(&self.pop()?);
                    let len = word_offset(&self.pop()?);
                    self.ensure_mem(dest, len)?;
                    for i in 0..len {
                        self.memory[dest + i] = src
                            .checked_add(i)
                            .and_then(|s| self.calldata.get(s))
                            .copied()
                            .unwrap_or(0);
                    }
                    self.pc += 1;
                }
                op::JUMP => {
                    let dest = word_offset(&self.pop()?);
                    self.jump(dest)?;
                }
                op::JUMPI => {
                    let dest = word_offset(&self.pop()?);
                    let cond = self.pop()?;
                    if cond != [0u8; 32] {
                        self.jump(dest)?;
                    } else {
                        self.pc += 1;
                    }
                }
                op::JUMPDEST => {
                    self.pc += 1;
                }
                op::RETURN => {
                    let off = word_offset(&self.pop()?);
                    let len = word_offset(&self.pop()?);
                    return self.mem_slice(off, len);
                }
                op::REVERT => {
                    let off = word_offset(&self.pop()?);
                    let len = word_offset(&self.pop()?);
                    return Err(ExecError::Revert(self.mem_slice(off, len)?));
                }
                o if (op::LOG0..=op::LOG4).contains(&o) => {
                    let ntopics = (o - op::LOG0) as usize;
                    let off = word_offset(&self.pop()?);
                    let len = word_offset(&self.pop()?);
                    let mut topics = Vec::with_capacity(ntopics);
                    for _ in 0..ntopics {
                        topics.push(self.pop()?);
                    }
                    let data = self.mem_slice(off, len)?;
                    self.logs.push(LogEntry { topics, data });
                    self.pc += 1;
                }
                other => return Err(ExecError::UnknownOpcode(other)),
            }
        }
    }

    /// Jump to `dest`, requiring a REAL `JUMPDEST` there — validated against
    /// the [`jumpdest_analysis`] set, so a `0x5B` byte inside a `PUSH`
    /// immediate, an offset off the end, or a target past the address space
    /// is [`ExecError::BadJumpDest`] (the chain's rule).
    fn jump(&mut self, dest: usize) -> Result<(), ExecError> {
        if self.jumpdests.binary_search(&dest).is_err() {
            return Err(ExecError::BadJumpDest(dest));
        }
        self.pc = dest;
        Ok(())
    }
}

/// `1`/`0` as a word (the EVM boolean encoding).
fn bool_word(b: bool) -> Word {
    let mut w = [0u8; 32];
    if b {
        w[31] = 1;
    }
    w
}

/// Logical right shift of a 256-bit word by `shift` bits.
fn shr256(value: &Word, shift: u128) -> Word {
    if shift >= 256 {
        return [0u8; 32];
    }
    let shift = shift as usize;
    let (byte_shift, bit_shift) = (shift / 8, shift % 8);
    let mut out = [0u8; 32];
    for (i, byte) in out.iter_mut().enumerate() {
        if i >= byte_shift {
            *byte = value[i - byte_shift];
        }
    }
    if bit_shift > 0 {
        let mut carry = 0u8;
        for byte in out.iter_mut() {
            let new_carry = *byte << (8 - bit_shift);
            *byte = (*byte >> bit_shift) | carry;
            carry = new_carry;
        }
    }
    out
}

/// The LOW 128 bits of a word (see the module docs' arithmetic note).
fn word_to_u128(w: &Word) -> u128 {
    let mut b = [0u8; 16];
    b.copy_from_slice(&w[16..]);
    u128::from_be_bytes(b)
}

fn u128_to_word(v: u128) -> Word {
    let mut w = [0u8; 32];
    w[16..].copy_from_slice(&v.to_be_bytes());
    w
}

/// A word as a memory/calldata/code offset, SATURATING to `usize::MAX` when
/// it exceeds the address space. Downstream guards then reject or zero-read
/// it (memory cap → `OutOfGas`, jump set → `BadJumpDest`, calldata reads →
/// zeros) — never a silent truncation to an aliasing small offset, and no
/// native-vs-wasm32 divergence.
fn word_offset(w: &Word) -> usize {
    if w[..24].iter().any(|&b| b != 0) {
        return usize::MAX;
    }
    usize::try_from(word_to_u64(w)).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asm::{Asm, init_wrapper, op};

    fn env() -> CallEnv {
        CallEnv::default()
    }

    /// The canonical roundtrip: assemble a counter, deploy it through the
    /// real init wrapper, call it twice, read its storage directly.
    #[test]
    fn counter_roundtrips_through_deploy_and_calls() {
        let mut a = Asm::new();
        a.push(&[0]).emit(op::SLOAD); // [v]
        a.push(&[1]).emit(op::ADD); // [v+1]
        a.emit(op::DUP1); // [v+1, v+1]
        a.push(&[0]).emit(op::SSTORE); // slot 0 = v+1; [v+1]
        a.push(&[0]).emit(op::MSTORE); // mem[0..32] = v+1
        a.push(&[32]).push(&[0]).emit(op::RETURN);
        let runtime = a.finish().unwrap();
        let init = init_wrapper(&runtime).unwrap();

        let mut c = Contract::deploy(&init, &env()).unwrap();
        assert_eq!(c.code, runtime); // the constructor returned the runtime
        assert_eq!(
            word_to_u64(&c.call(&[], &env()).unwrap().try_into().unwrap()),
            1
        );
        assert_eq!(
            word_to_u64(&c.call(&[], &env()).unwrap().try_into().unwrap()),
            2
        );
        assert_eq!(word_to_u64(&c.sload(&word(0))), 2);
    }

    /// Forward branch: return 1 if calldata word0 < 10, else 2.
    #[test]
    fn forward_branch_takes_the_right_arm() {
        let mut a = Asm::new();
        let small = a.new_label();
        a.push(&[10]); // [10]
        a.push(&[0]).emit(op::CALLDATALOAD); // [10, x] — LT: x < 10
        a.emit(op::LT);
        a.push_label(small).emit(op::JUMPI);
        a.push(&[2]).push(&[0]).emit(op::MSTORE);
        a.push(&[32]).push(&[0]).emit(op::RETURN);
        a.jumpdest(small);
        a.push(&[1]).push(&[0]).emit(op::MSTORE);
        a.push(&[32]).push(&[0]).emit(op::RETURN);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        let ret = c.call(&word(3), &env()).unwrap();
        assert_eq!(word_to_u64(&ret.try_into().unwrap()), 1);
        let ret = c.call(&word(99), &env()).unwrap();
        assert_eq!(word_to_u64(&ret.try_into().unwrap()), 2);
    }

    /// Backward branch: a real loop computing 2n via repeated ADD/SUB.
    #[test]
    fn loop_with_back_jump_terminates_and_computes() {
        let mut a = Asm::new();
        let (top, end) = (a.new_label(), a.new_label());
        a.push(&[0]).emit(op::CALLDATALOAD); // [n]
        a.push(&[0]); // [n, acc]
        a.jumpdest(top); // loop: [n, acc]
        a.emit(op::DUP2).emit(op::ISZERO); // [n, acc, n==0]
        a.push_label(end).emit(op::JUMPI); // [n, acc]
        a.push(&[2]).emit(op::ADD); // [n, acc+2]
        a.emit(op::SWAP1); // [acc+2, n]
        a.push(&[1]).emit(op::SWAP1).emit(op::SUB); // [acc+2, n-1]
        a.emit(op::SWAP1); // [n-1, acc+2]
        a.push_label(top).emit(op::JUMP);
        a.jumpdest(end); // [0, acc]
        a.push(&[0]).emit(op::MSTORE); // mem[0] = acc
        a.push(&[32]).push(&[0]).emit(op::RETURN);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        let ret = c.call(&word(5), &env()).unwrap();
        assert_eq!(word_to_u64(&ret.try_into().unwrap()), 10);
        let ret = c.call(&word(0), &env()).unwrap();
        assert_eq!(word_to_u64(&ret.try_into().unwrap()), 0);
    }

    #[test]
    fn arithmetic_matches_the_evm_rules() {
        // 256-bit ADD wraps: (2^256 - 1) + 1 = 0.
        let mut a = Asm::new();
        a.push32(&[0xFF; 32]).push(&[1]).emit(op::ADD);
        a.push(&[0]).emit(op::MSTORE);
        a.push(&[32]).push(&[0]).emit(op::RETURN);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        assert_eq!(c.call(&[], &env()).unwrap(), vec![0u8; 32]);
        // DIV/MOD by zero yield 0, not an error (EVM rule).
        let mut a = Asm::new();
        a.push(&[0]).push(&[7]).emit(op::DIV); // 7 / 0
        a.push(&[0]).emit(op::MSTORE);
        a.push(&[32]).push(&[0]).emit(op::RETURN);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        assert_eq!(c.call(&[], &env()).unwrap(), vec![0u8; 32]);
    }

    #[test]
    fn env_reads_and_logs_flow_through() {
        let mut a = Asm::new();
        a.emit(op::CALLER).push(&[0]).emit(op::MSTORE); // mem[0] = caller
        a.emit(op::TIMESTAMP); // topic0 = timestamp
        a.push(&[32]).push(&[0]); // [t, len, off]... LOG1 pops off,len,topic
        a.emit(op::LOG1);
        a.push(&[0]).push(&[0]).emit(op::RETURN);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        let e = CallEnv {
            caller: [0xAB; 20],
            timestamp: 1234,
            number: 7,
        };
        let (ret, logs) = c.call_logs(&[], &e).unwrap();
        assert!(ret.is_empty());
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].topics, vec![word(1234)]);
        assert_eq!(logs[0].data, addr_word(&[0xAB; 20]).to_vec());
    }

    #[test]
    fn hostile_and_broken_bytecode_is_an_error_never_a_hang() {
        let e = env();
        // REVERT carries its data.
        let mut a = Asm::new();
        a.push(&[0xEE]).push(&[0]).emit(op::MSTORE);
        a.push(&[32]).push(&[0]).emit(op::REVERT);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        let Err(ExecError::Revert(data)) = c.call(&[], &e) else {
            panic!("expected revert");
        };
        assert_eq!(data[31], 0xEE);
        // An opcode outside the subset (0xFE = INVALID).
        let mut c = Contract {
            code: vec![0xFE],
            ..Default::default()
        };
        assert_eq!(c.call(&[], &e), Err(ExecError::UnknownOpcode(0xFE)));
        // KECCAK256 is present-but-unsupported (no hashing dep in the kit).
        let mut c = Contract {
            code: vec![op::PUSH1, 0, op::PUSH1, 0, op::KECCAK256],
            ..Default::default()
        };
        assert_eq!(c.call(&[], &e), Err(ExecError::Unsupported(op::KECCAK256)));
        // An infinite loop exhausts the step budget.
        let mut a = Asm::new();
        let l = a.new_label();
        a.jumpdest(l);
        a.push_label(l).emit(op::JUMP);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        assert_eq!(c.call(&[], &e), Err(ExecError::OutOfGas));
        // A jump into a PUSH immediate is a bad dest.
        let mut c = Contract {
            code: vec![op::PUSH1, 0x01, op::JUMP],
            ..Default::default()
        };
        assert_eq!(c.call(&[], &e), Err(ExecError::BadJumpDest(1)));
        // Popping an empty stack underflows cleanly.
        let mut c = Contract {
            code: vec![op::ADD],
            ..Default::default()
        };
        assert_eq!(c.call(&[], &e), Err(ExecError::StackUnderflow));
        // A giant memory offset is OutOfGas, not an allocation.
        let mut a = Asm::new();
        a.push(&u64::MAX.to_be_bytes()).emit(op::MLOAD);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        assert_eq!(c.call(&[], &e), Err(ExecError::OutOfGas));
    }

    #[test]
    fn jumpdest_analysis_excludes_push_immediates() {
        // PUSH1 0x5B; PUSH1 0x01; JUMP — offset 1 is a 0x5B byte, but it is
        // PUSH data, not a JUMPDEST. Real EVM rejects the jump; so do we.
        let code = vec![op::PUSH1, 0x5B, op::PUSH1, 0x01, op::JUMP];
        let mut c = Contract {
            code,
            ..Default::default()
        };
        assert_eq!(c.call(&[], &env()), Err(ExecError::BadJumpDest(1)));
    }

    #[test]
    fn reverted_calls_roll_back_storage_and_logs() {
        // SSTORE(0, 1) then REVERT: the write must not survive the call.
        let mut a = Asm::new();
        a.push(&[1]).push(&[0]).emit(op::SSTORE);
        a.push(&[7]).push(&[0]).emit(op::MSTORE);
        a.push(&[32]).push(&[0]).emit(op::LOG0);
        a.push(&[0]).push(&[0]).emit(op::REVERT);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        assert!(matches!(c.call(&[], &env()), Err(ExecError::Revert(_))));
        assert_eq!(c.sload(&word(0)), [0u8; 32]); // rolled back
        // A successful call still commits.
        let mut a = Asm::new();
        a.push(&[9]).push(&[0]).emit(op::SSTORE);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        c.call(&[], &env()).unwrap();
        assert_eq!(word_to_u64(&c.sload(&word(0))), 9);
    }

    #[test]
    fn offsets_past_the_address_space_never_alias_small_ones() {
        let e = env();
        // A 2^64-and-up offset must not wrap to offset 0: MLOAD errs...
        let mut huge = [0u8; 32];
        huge[23] = 1; // 2^64
        let mut a = Asm::new();
        a.push32(&huge).emit(op::MLOAD);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        assert_eq!(c.call(&[], &e), Err(ExecError::OutOfGas));
        // ...a JUMP there is a bad dest, not a jump to offset 0...
        let mut a = Asm::new();
        let l = a.new_label();
        a.jumpdest(l);
        a.push32(&huge).emit(op::JUMP);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        assert!(matches!(c.call(&[], &e), Err(ExecError::BadJumpDest(_))));
        // ...and CALLDATALOAD there reads zeros (no expansion involved).
        let mut a = Asm::new();
        a.push32(&huge).emit(op::CALLDATALOAD);
        a.push(&[0]).emit(op::MSTORE);
        a.push(&[32]).push(&[0]).emit(op::RETURN);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        assert_eq!(c.call(&[0xFF; 64], &e).unwrap(), vec![0u8; 32]);
        // Zero-length memory ops at huge offsets are fine (EVM zero-size rule).
        let mut a = Asm::new();
        a.push(&[0]).push(&u64::MAX.to_be_bytes()).emit(op::RETURN);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        assert_eq!(c.call(&[], &e).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn calldata_reads_zero_extend() {
        // CALLDATALOAD far past the end reads zeros; CALLDATASIZE is exact.
        let mut a = Asm::new();
        a.push(&[200]).emit(op::CALLDATALOAD); // [0]
        a.emit(op::CALLDATASIZE).emit(op::ADD); // [0 + size]
        a.push(&[0]).emit(op::MSTORE);
        a.push(&[32]).push(&[0]).emit(op::RETURN);
        let mut c = Contract {
            code: a.finish().unwrap(),
            ..Default::default()
        };
        let ret = c.call(&calldata([1, 2, 3, 4], &[word(9)]), &env()).unwrap();
        assert_eq!(word_to_u64(&ret.try_into().unwrap()), 36); // 4 + 32
    }
}

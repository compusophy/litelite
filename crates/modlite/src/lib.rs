//! # modlite — a wasm binary MODULE builder
//!
//! The mechanical layer every wasm backend needs and rustlite hand-rolled:
//! LEB128, section framing, functype interning, locals run-length encoding,
//! and the index bookkeeping the binary format imposes (imported functions
//! occupy indices `0..imports`, so every local function index is offset — the
//! classic drift bug when an import is added late). modlite makes the
//! ordering STRUCTURAL: importing after a function exists is an error, and
//! [`Module::finish`] hands back either a well-framed module or the first
//! construction fault — never silently malformed bytes.
//!
//! Instruction bytes are the CONSUMER's job: build each body as a `Vec<u8>`
//! with the [`op`] consts and the [`leb128_u32`]/[`leb128_i32`]/[`leb128_i64`]
//! helpers, end it with [`op::END`], and hand it to [`Module::func`]. There
//! is deliberately no shared codegen abstraction with the EVM emitter
//! (constitution rule 4): wasm is structured/relative, the EVM is
//! absolute-jump; only the byte-plumbing lives here.
//!
//! Scope: type/import/function/memory/export/code/data sections — what a
//! purpose-sized language needs. No tables, globals, element or start
//! sections (add them when a consumer needs them, not before).
//!
//! ```
//! use modlite::{Module, op, val};
//!
//! let mut m = Module::new();
//! let sig = m.functype(&[val::I64, val::I64], &[val::I64]);
//! let body = vec![op::LOCAL_GET, 0, op::LOCAL_GET, 1, op::I64_ADD, op::END];
//! let f = m.func(sig, &[], body);
//! m.export_func("add", f);
//! let wasm = m.finish().unwrap();
//! assert_eq!(&wasm[..8], b"\0asm\x01\0\0\0");
//! ```

/// Wasm value types (the binary-format encodings).
pub mod val {
    pub const I32: u8 = 0x7F;
    pub const I64: u8 = 0x7E;
    pub const F32: u8 = 0x7D;
    pub const F64: u8 = 0x7C;
}

/// The empty block type for `block`/`loop`/`if` with no result.
pub const BLOCK_VOID: u8 = 0x40;

/// Wasm opcodes (core, single-byte). Control-flow ops take a block type
/// (`BLOCK_VOID` or a `val::*`); `br`/`br_if`/`call`/`local_*` take a LEB
/// index; memory ops take TWO LEB immediates (align, offset); `*_const`
/// take a signed LEB value.
pub mod op {
    pub const UNREACHABLE: u8 = 0x00;
    pub const NOP: u8 = 0x01;
    pub const BLOCK: u8 = 0x02;
    pub const LOOP: u8 = 0x03;
    pub const IF: u8 = 0x04;
    pub const ELSE: u8 = 0x05;
    pub const END: u8 = 0x0B;
    pub const BR: u8 = 0x0C;
    pub const BR_IF: u8 = 0x0D;
    pub const RETURN: u8 = 0x0F;
    pub const CALL: u8 = 0x10;
    pub const DROP: u8 = 0x1A;
    pub const LOCAL_GET: u8 = 0x20;
    pub const LOCAL_SET: u8 = 0x21;
    pub const LOCAL_TEE: u8 = 0x22;
    pub const I32_LOAD: u8 = 0x28;
    pub const I64_LOAD: u8 = 0x29;
    pub const F32_LOAD: u8 = 0x2A;
    pub const F64_LOAD: u8 = 0x2B;
    pub const I32_STORE: u8 = 0x36;
    pub const I64_STORE: u8 = 0x37;
    pub const F32_STORE: u8 = 0x38;
    pub const F64_STORE: u8 = 0x39;
    pub const I32_CONST: u8 = 0x41;
    pub const I64_CONST: u8 = 0x42;
    pub const F32_CONST: u8 = 0x43;
    pub const F64_CONST: u8 = 0x44;
    pub const I32_EQZ: u8 = 0x45;
    pub const I32_EQ: u8 = 0x46;
    pub const I32_NE: u8 = 0x47;
    pub const I32_LT_S: u8 = 0x48;
    pub const I32_GT_S: u8 = 0x4A;
    pub const I32_LE_S: u8 = 0x4C;
    pub const I32_GE_S: u8 = 0x4E;
    pub const I64_EQ: u8 = 0x51;
    pub const I64_NE: u8 = 0x52;
    pub const I64_LT_S: u8 = 0x53;
    pub const I64_GT_S: u8 = 0x55;
    pub const I64_LE_S: u8 = 0x57;
    pub const I64_GE_S: u8 = 0x59;
    pub const F64_EQ: u8 = 0x61;
    pub const F64_NE: u8 = 0x62;
    pub const F64_LT: u8 = 0x63;
    pub const F64_GT: u8 = 0x64;
    pub const F64_LE: u8 = 0x65;
    pub const F64_GE: u8 = 0x66;
    pub const I32_ADD: u8 = 0x6A;
    pub const I32_SUB: u8 = 0x6B;
    pub const I32_MUL: u8 = 0x6C;
    pub const I32_DIV_S: u8 = 0x6D;
    pub const I32_REM_S: u8 = 0x6F;
    pub const I32_AND: u8 = 0x71;
    pub const I32_OR: u8 = 0x72;
    pub const I32_XOR: u8 = 0x73;
    pub const I32_SHL: u8 = 0x74;
    pub const I32_SHR_S: u8 = 0x75;
    pub const I64_ADD: u8 = 0x7C;
    pub const I64_SUB: u8 = 0x7D;
    pub const I64_MUL: u8 = 0x7E;
    pub const I64_DIV_S: u8 = 0x7F;
    pub const I64_REM_S: u8 = 0x81;
    pub const I64_AND: u8 = 0x83;
    pub const I64_OR: u8 = 0x84;
    pub const I64_XOR: u8 = 0x85;
    pub const I64_SHL: u8 = 0x86;
    pub const I64_SHR_S: u8 = 0x87;
    pub const F64_NEG: u8 = 0x9A;
    pub const F64_ADD: u8 = 0xA0;
    pub const F64_SUB: u8 = 0xA1;
    pub const F64_MUL: u8 = 0xA2;
    pub const F64_DIV: u8 = 0xA3;
}

const WASM_MAGIC: &[u8] = b"\0asm";
const WASM_VERSION: &[u8] = &[1, 0, 0, 0];

const SEC_TYPE: u8 = 1;
const SEC_IMPORT: u8 = 2;
const SEC_FUNCTION: u8 = 3;
const SEC_MEMORY: u8 = 5;
const SEC_EXPORT: u8 = 7;
const SEC_CODE: u8 = 10;
const SEC_DATA: u8 = 11;

/// Unsigned LEB128 (the wasm index/size encoding).
pub fn leb128_u32(mut val: u32, out: &mut Vec<u8>) {
    loop {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        if val != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if val == 0 {
            break;
        }
    }
}

/// Signed LEB128 for `i32.const` operands.
pub fn leb128_i32(mut val: i32, out: &mut Vec<u8>) {
    loop {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        let more = !((val == 0 && byte & 0x40 == 0) || (val == -1 && byte & 0x40 != 0));
        if more {
            byte |= 0x80;
        }
        out.push(byte);
        if !more {
            break;
        }
    }
}

/// Signed LEB128 for `i64.const` operands.
pub fn leb128_i64(mut val: i64, out: &mut Vec<u8>) {
    loop {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        let more = !((val == 0 && byte & 0x40 == 0) || (val == -1 && byte & 0x40 != 0));
        if more {
            byte |= 0x80;
        }
        out.push(byte);
        if !more {
            break;
        }
    }
}

/// Why a build cannot produce a module. The first fault is recorded sticky
/// and surfaced by [`Module::finish`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildError {
    /// [`Module::import_func`] after a local function exists — that would
    /// shift every already-assigned function index (the classic wasm bug).
    ImportAfterFunc,
    /// A byte that is not a `val::*` value type.
    BadValType(u8),
    /// A type index no [`Module::functype`] call returned.
    BadTypeIndex(u32),
    /// An exported function index that no function occupies.
    BadFuncIndex(u32),
    /// Two exports share a name (the wasm spec requires uniqueness).
    DuplicateExport(String),
    /// [`Module::export_memory`] or [`Module::data`] without a
    /// [`Module::memory`] declaration.
    MissingMemory(&'static str),
    /// [`Module::memory`] called twice (core wasm allows one memory).
    SecondMemory,
    /// A function body that does not end with [`op::END`].
    BodyMissingEnd,
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildError::ImportAfterFunc => {
                write!(
                    f,
                    "import added after a local function (indices would shift)"
                )
            }
            BuildError::BadValType(b) => write!(f, "0x{b:02x} is not a wasm value type"),
            BuildError::BadTypeIndex(i) => write!(f, "type index {i} was never declared"),
            BuildError::BadFuncIndex(i) => write!(f, "function index {i} does not exist"),
            BuildError::DuplicateExport(n) => write!(f, "duplicate export name `{n}`"),
            BuildError::MissingMemory(what) => write!(f, "{what} requires a declared memory"),
            BuildError::SecondMemory => write!(f, "core wasm allows exactly one memory"),
            BuildError::BodyMissingEnd => {
                write!(f, "function body must end with op::END (0x0B)")
            }
        }
    }
}

impl std::error::Error for BuildError {}

/// Run-length-encode a flat list of local value types into the code-section
/// locals vector (`count` runs of `(count, type)`).
pub fn encode_locals(types: &[u8]) -> Vec<u8> {
    let mut runs: Vec<(u32, u8)> = Vec::new();
    for &t in types {
        match runs.last_mut() {
            Some((n, rt)) if *rt == t => *n += 1,
            _ => runs.push((1, t)),
        }
    }
    let mut out = Vec::new();
    leb128_u32(runs.len() as u32, &mut out);
    for (n, t) in runs {
        leb128_u32(n, &mut out);
        out.push(t);
    }
    out
}

/// A wasm module under construction. Declare imports FIRST (the binary format
/// gives them the low function indices), then functions; [`finish`](Self::finish)
/// frames the sections and returns the bytes or the first fault.
#[derive(Debug, Default)]
pub struct Module {
    /// Interned functype encodings (deduped).
    types: Vec<Vec<u8>>,
    /// `(module, name, type_idx)` function imports, in index order.
    imports: Vec<(String, String, u32)>,
    /// `(type_idx, encoded_locals, code)` local functions, in index order.
    functions: Vec<(u32, Vec<u8>, Vec<u8>)>,
    /// `(name, kind_byte, index)` exports.
    exports: Vec<(String, u8, u32)>,
    /// `(min_pages, max_pages)` — at most one.
    memory: Option<(u32, Option<u32>)>,
    /// Active data segments: `(offset, bytes)`.
    data: Vec<(u32, Vec<u8>)>,
    err: Option<BuildError>,
}

impl Module {
    pub fn new() -> Self {
        Self::default()
    }

    fn fault(&mut self, e: BuildError) {
        if self.err.is_none() {
            self.err = Some(e);
        }
    }

    fn check_valtypes(&mut self, types: &[u8]) {
        for &t in types {
            if !matches!(t, val::I32 | val::I64 | val::F32 | val::F64) {
                self.fault(BuildError::BadValType(t));
            }
        }
    }

    /// Intern the function type `(params) -> (results)`, returning its type
    /// index (deduped — declaring the same signature twice is one entry).
    pub fn functype(&mut self, params: &[u8], results: &[u8]) -> u32 {
        self.check_valtypes(params);
        self.check_valtypes(results);
        let mut enc = vec![0x60];
        leb128_u32(params.len() as u32, &mut enc);
        enc.extend_from_slice(params);
        leb128_u32(results.len() as u32, &mut enc);
        enc.extend_from_slice(results);
        if let Some(i) = self.types.iter().position(|t| *t == enc) {
            return i as u32;
        }
        self.types.push(enc);
        (self.types.len() - 1) as u32
    }

    /// Import function `module.name` with signature `type_idx`, returning its
    /// FUNCTION INDEX. All imports must precede the first [`func`](Self::func)
    /// — a late import would shift every existing index (sticky
    /// [`BuildError::ImportAfterFunc`]).
    pub fn import_func(&mut self, module: &str, name: &str, type_idx: u32) -> u32 {
        if !self.functions.is_empty() {
            self.fault(BuildError::ImportAfterFunc);
        }
        if type_idx as usize >= self.types.len() {
            self.fault(BuildError::BadTypeIndex(type_idx));
        }
        self.imports
            .push((module.to_string(), name.to_string(), type_idx));
        (self.imports.len() - 1) as u32
    }

    /// Add a local function, returning its FUNCTION INDEX (offset past the
    /// imports). `locals` lists the declared locals' value types in order
    /// (params are implicit in the signature); `code` is the complete body,
    /// ending with [`op::END`].
    pub fn func(&mut self, type_idx: u32, locals: &[u8], code: Vec<u8>) -> u32 {
        if type_idx as usize >= self.types.len() {
            self.fault(BuildError::BadTypeIndex(type_idx));
        }
        self.check_valtypes(locals);
        if code.last() != Some(&op::END) {
            self.fault(BuildError::BodyMissingEnd);
        }
        self.functions.push((type_idx, encode_locals(locals), code));
        (self.imports.len() + self.functions.len() - 1) as u32
    }

    /// Export function index `func_idx` (imported or local) as `name`.
    pub fn export_func(&mut self, name: &str, func_idx: u32) {
        if func_idx as usize >= self.imports.len() + self.functions.len() {
            self.fault(BuildError::BadFuncIndex(func_idx));
        }
        self.exports.push((name.to_string(), 0x00, func_idx));
    }

    /// Declare THE memory: `min_pages` (64 KiB each), optional `max_pages`.
    pub fn memory(&mut self, min_pages: u32, max_pages: Option<u32>) {
        if self.memory.is_some() {
            self.fault(BuildError::SecondMemory);
        }
        self.memory = Some((min_pages, max_pages));
    }

    /// Export the memory as `name` (requires [`memory`](Self::memory)).
    pub fn export_memory(&mut self, name: &str) {
        if self.memory.is_none() {
            self.fault(BuildError::MissingMemory("export_memory"));
        }
        self.exports.push((name.to_string(), 0x02, 0));
    }

    /// Add an active data segment at byte `offset` in the memory.
    pub fn data(&mut self, offset: u32, bytes: &[u8]) {
        if self.memory.is_none() {
            self.fault(BuildError::MissingMemory("data"));
        }
        self.data.push((offset, bytes.to_vec()));
    }

    /// Frame the sections (in the id order the spec requires) and return the
    /// module bytes, or the FIRST construction fault. Empty sections are
    /// omitted.
    pub fn finish(self) -> Result<Vec<u8>, BuildError> {
        if let Some(e) = self.err {
            return Err(e);
        }
        for (name, _, _) in &self.exports {
            let dups = self.exports.iter().filter(|(n, _, _)| n == name).count();
            if dups > 1 {
                return Err(BuildError::DuplicateExport(name.clone()));
            }
        }
        let mut out = Vec::new();
        out.extend_from_slice(WASM_MAGIC);
        out.extend_from_slice(WASM_VERSION);

        if !self.types.is_empty() {
            let mut sec = Vec::new();
            leb128_u32(self.types.len() as u32, &mut sec);
            for ty in &self.types {
                sec.extend_from_slice(ty);
            }
            write_section(SEC_TYPE, &sec, &mut out);
        }
        if !self.imports.is_empty() {
            let mut sec = Vec::new();
            leb128_u32(self.imports.len() as u32, &mut sec);
            for (module, name, type_idx) in &self.imports {
                write_name(module, &mut sec);
                write_name(name, &mut sec);
                sec.push(0x00); // import kind: func
                leb128_u32(*type_idx, &mut sec);
            }
            write_section(SEC_IMPORT, &sec, &mut out);
        }
        if !self.functions.is_empty() {
            let mut sec = Vec::new();
            leb128_u32(self.functions.len() as u32, &mut sec);
            for (type_idx, _, _) in &self.functions {
                leb128_u32(*type_idx, &mut sec);
            }
            write_section(SEC_FUNCTION, &sec, &mut out);
        }
        if let Some((min, max)) = self.memory {
            let mut sec = Vec::new();
            leb128_u32(1, &mut sec); // one memory
            match max {
                None => {
                    sec.push(0x00);
                    leb128_u32(min, &mut sec);
                }
                Some(max) => {
                    sec.push(0x01);
                    leb128_u32(min, &mut sec);
                    leb128_u32(max, &mut sec);
                }
            }
            write_section(SEC_MEMORY, &sec, &mut out);
        }
        if !self.exports.is_empty() {
            let mut sec = Vec::new();
            leb128_u32(self.exports.len() as u32, &mut sec);
            for (name, kind, idx) in &self.exports {
                write_name(name, &mut sec);
                sec.push(*kind);
                leb128_u32(*idx, &mut sec);
            }
            write_section(SEC_EXPORT, &sec, &mut out);
        }
        if !self.functions.is_empty() {
            let mut sec = Vec::new();
            leb128_u32(self.functions.len() as u32, &mut sec);
            for (_, locals, code) in &self.functions {
                leb128_u32((locals.len() + code.len()) as u32, &mut sec);
                sec.extend_from_slice(locals);
                sec.extend_from_slice(code);
            }
            write_section(SEC_CODE, &sec, &mut out);
        }
        if !self.data.is_empty() {
            let mut sec = Vec::new();
            leb128_u32(self.data.len() as u32, &mut sec);
            for (offset, bytes) in &self.data {
                sec.push(0x00); // active, memory 0
                sec.push(op::I32_CONST);
                leb128_i32(*offset as i32, &mut sec);
                sec.push(op::END);
                leb128_u32(bytes.len() as u32, &mut sec);
                sec.extend_from_slice(bytes);
            }
            write_section(SEC_DATA, &sec, &mut out);
        }
        Ok(out)
    }
}

fn write_name(name: &str, out: &mut Vec<u8>) {
    leb128_u32(name.len() as u32, out);
    out.extend_from_slice(name.as_bytes());
}

fn write_section(id: u8, data: &[u8], out: &mut Vec<u8>) {
    out.push(id);
    leb128_u32(data.len() as u32, out);
    out.extend_from_slice(data);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(v: u32) -> Vec<u8> {
        let mut out = Vec::new();
        leb128_u32(v, &mut out);
        out
    }

    #[test]
    fn leb128_matches_the_published_vectors() {
        assert_eq!(u(0), [0x00]);
        assert_eq!(u(127), [0x7F]);
        assert_eq!(u(128), [0x80, 0x01]);
        assert_eq!(u(624485), [0xE5, 0x8E, 0x26]);
        let mut s = Vec::new();
        leb128_i32(-1, &mut s);
        assert_eq!(s, [0x7F]);
        let mut s = Vec::new();
        leb128_i32(63, &mut s);
        assert_eq!(s, [0x3F]);
        let mut s = Vec::new();
        leb128_i32(-64, &mut s);
        assert_eq!(s, [0x40]);
        let mut s = Vec::new();
        leb128_i64(-123456, &mut s);
        assert_eq!(s, [0xC0, 0xBB, 0x78]);
    }

    #[test]
    fn empty_module_is_just_magic_and_version() {
        assert_eq!(Module::new().finish().unwrap(), b"\0asm\x01\0\0\0");
    }

    #[test]
    fn add_function_module_is_byte_exact() {
        let mut m = Module::new();
        let sig = m.functype(&[val::I64, val::I64], &[val::I64]);
        let f = m.func(
            sig,
            &[],
            vec![op::LOCAL_GET, 0, op::LOCAL_GET, 1, op::I64_ADD, op::END],
        );
        m.export_func("add", f);
        #[rustfmt::skip]
        let expected = [
            0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00, // magic + version
            0x01, 0x07, 0x01, 0x60, 0x02, 0x7E, 0x7E, 0x01, 0x7E, // type
            0x03, 0x02, 0x01, 0x00, // function
            0x07, 0x07, 0x01, 0x03, b'a', b'd', b'd', 0x00, 0x00, // export
            0x0A, 0x09, 0x01, 0x07, 0x00, 0x20, 0x00, 0x20, 0x01, 0x7C, 0x0B, // code
        ];
        assert_eq!(m.finish().unwrap(), expected);
    }

    #[test]
    fn imports_take_the_low_indices_and_locals_offset_past_them() {
        let mut m = Module::new();
        let log_sig = m.functype(&[val::I32], &[]);
        let add_sig = m.functype(&[val::I64, val::I64], &[val::I64]);
        let log_idx = m.import_func("env", "log", log_sig);
        let f = m.func(
            add_sig,
            &[],
            vec![op::LOCAL_GET, 0, op::LOCAL_GET, 1, op::I64_ADD, op::END],
        );
        assert_eq!((log_idx, f), (0, 1)); // import first, local offset past it
        m.export_func("add", f);
        let wasm = m.finish().unwrap();
        // The import section names env.log with kind func and the export
        // points at function index 1.
        let s = section(&wasm, SEC_IMPORT).unwrap();
        assert_eq!(
            s,
            [
                0x01, 0x03, b'e', b'n', b'v', 0x03, b'l', b'o', b'g', 0x00, 0x00
            ]
        );
        let s = section(&wasm, SEC_EXPORT).unwrap();
        assert_eq!(s[s.len() - 1], 0x01); // exported func index 1
    }

    #[test]
    fn functypes_are_deduped_and_locals_run_length_encode() {
        let mut m = Module::new();
        let a = m.functype(&[val::I32], &[val::I32]);
        let b = m.functype(&[val::I32], &[val::I32]);
        assert_eq!(a, b);
        assert_eq!(
            encode_locals(&[val::I32, val::I32, val::I64, val::I32]),
            // 3 runs: 2×i32, 1×i64, 1×i32
            vec![3, 2, val::I32, 1, val::I64, 1, val::I32]
        );
        assert_eq!(encode_locals(&[]), vec![0]);
    }

    #[test]
    fn memory_export_and_data_sections_frame_correctly() {
        let mut m = Module::new();
        m.memory(1, Some(4));
        m.export_memory("memory");
        m.data(1024, b"hi");
        let wasm = m.finish().unwrap();
        assert_eq!(section(&wasm, SEC_MEMORY).unwrap(), [1, 0x01, 1, 4]);
        let data = section(&wasm, SEC_DATA).unwrap();
        // 1 segment: active mem0, i32.const 1024, end, len 2, "hi"
        assert_eq!(
            data,
            [
                0x01,
                0x00,
                op::I32_CONST,
                0x80,
                0x08,
                op::END,
                0x02,
                b'h',
                b'i'
            ]
        );
        // Section ids appear in strictly increasing order.
        let ids = section_ids(&wasm);
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn construction_faults_are_sticky_never_malformed_bytes() {
        // Import after a local function: the index-shift footgun.
        let mut m = Module::new();
        let sig = m.functype(&[], &[]);
        m.func(sig, &[], vec![op::END]);
        m.import_func("env", "late", sig);
        assert_eq!(m.finish(), Err(BuildError::ImportAfterFunc));
        // A body without END.
        let mut m = Module::new();
        let sig = m.functype(&[], &[]);
        m.func(sig, &[], vec![op::NOP]);
        assert_eq!(m.finish(), Err(BuildError::BodyMissingEnd));
        // A bad value type.
        let mut m = Module::new();
        m.functype(&[0x99], &[]);
        assert_eq!(m.finish(), Err(BuildError::BadValType(0x99)));
        // An undeclared type index.
        let mut m = Module::new();
        m.func(7, &[], vec![op::END]);
        assert_eq!(m.finish(), Err(BuildError::BadTypeIndex(7)));
        // An export of a function that does not exist.
        let mut m = Module::new();
        m.export_func("ghost", 3);
        assert_eq!(m.finish(), Err(BuildError::BadFuncIndex(3)));
        // Data without a memory.
        let mut m = Module::new();
        m.data(0, b"x");
        assert_eq!(m.finish(), Err(BuildError::MissingMemory("data")));
        // Two memories.
        let mut m = Module::new();
        m.memory(1, None);
        m.memory(1, None);
        assert_eq!(m.finish(), Err(BuildError::SecondMemory));
        // Duplicate export names.
        let mut m = Module::new();
        let sig = m.functype(&[], &[]);
        let f = m.func(sig, &[], vec![op::END]);
        m.export_func("x", f);
        m.export_func("x", f);
        assert_eq!(m.finish(), Err(BuildError::DuplicateExport("x".into())));
    }

    /// Minimal section walker for assertions: the content of section `id`.
    fn section(wasm: &[u8], id: u8) -> Option<Vec<u8>> {
        let mut i = 8;
        while i < wasm.len() {
            let sid = wasm[i];
            i += 1;
            let (len, adv) = read_leb(wasm, i);
            i += adv;
            if sid == id {
                return Some(wasm[i..i + len as usize].to_vec());
            }
            i += len as usize;
        }
        None
    }

    fn section_ids(wasm: &[u8]) -> Vec<u8> {
        let mut ids = Vec::new();
        let mut i = 8;
        while i < wasm.len() {
            ids.push(wasm[i]);
            i += 1;
            let (len, adv) = read_leb(wasm, i);
            i += adv + len as usize;
        }
        ids
    }

    fn read_leb(bytes: &[u8], at: usize) -> (u32, usize) {
        let (mut val, mut shift, mut n) = (0u32, 0u32, 0usize);
        loop {
            let b = bytes[at + n];
            val |= u32::from(b & 0x7F) << shift;
            n += 1;
            if b & 0x80 == 0 {
                return (val, n);
            }
            shift += 7;
        }
    }
}

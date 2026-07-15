# port/ — frozen parent snapshots (reference only, NOT part of the kit)

Verbatim copies from `localharness` @ `f52d048c` (2026-07-14), staged so the
port milestones never need the 131K-LOC parent repo in context. These files do
NOT compile here (they reference `crate::` paths that don't exist) — they are
reading material. Delete each one when its port lands. Excluded from
`scripts/caps.sh` on purpose.

| file | for | what to extract |
|---|---|---|
| `rustlite_codegen.rs` | M3 wasm emitter | the mechanical layer: section writers, LEB128, opcode consts, function/type/import/export/code section assembly (~500-600 LOC of the 1,777). Leave the rustlite-specific lowering behind. |
| `soliditylite_asm.rs` | M3 `evmlite` | nearly whole-file: named opcodes, minimal-width push, two-pass PUSH2 label back-patch, `init_wrapper` (CODECOPY/RETURN constructor). |
| `soliditylite_interp.rs` | M3 `evmlite` tests | the diff-oracle: minimal EVM interpreter (STEP_BUDGET, 16 MiB mem cap, CallEnv). Keccak came from an external dep in the parent — the kit must NOT take that dep; stub or vendor a tiny keccak, or leave keccak-dependent ops to the consumer. |
| `rustlite_loader.rs` | M2 CapTable | `build_host_imports` — one half of the hand-synced Rust↔JS host table (the other half is `web/cartridge-worker.js` in the parent). M2's CapTable-as-data must make this class of drift impossible. |
| `bashlite_host.rs` | M2 CapTable | the `BashHost` trait — the cleanest host seam of the three parents; the shape CapTable should generalize. |

The typed host-fn signature table (the third leg of M2 evidence) lives in the
parent at `src/rustlite/typecheck.rs::resolve_host_fn` — not snapshotted (the
file is 1.5K LOC of type checker); the loader + BashHost snapshots carry the
pattern. If the fresh session truly needs it, ask the operator to paste that
one function rather than opening the parent repo.

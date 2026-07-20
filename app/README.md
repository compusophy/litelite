# app/ — the applite vibe-coding shell

A one-page app-builder on the kit: write (or paste) an applite program on
the left, hit **verify + run**, and the app is live on the right — because
the verifier proved it total first. No framework, no npm, no wasm-bindgen,
no network calls: one Rust cdylib with a hand-rolled extern "C" ABI
(`src/lib.rs`), one HTML page (`index.html`), and the `applite` crate.

## Run it

```sh
./build.sh                          # cargo build + copy appshell.wasm here
python -m http.server -d . 8080     # wasm needs http, not file://
# -> http://localhost:8080
```

## The vibe-coding loop (keyless by design)

1. **copy prompt** — puts the applite language card (`applite::REFERENCE`,
   served verbatim from the wasm, so prompt and verifier cannot drift) plus
   your app idea on the clipboard.
2. Paste into ANY LLM — a frontier chat or the local fine-tune pipeline.
3. Paste the program it writes back into the editor. **verify + run.**
4. If the verifier rejects it, the caret-rendered error is right there —
   paste it back to the model, or fix it yourself. Rejection is the product
   working: nothing unverified ever runs.

What "verified" buys, mechanically (see the crate docs): every event
handler and every render halts within its fuel; faults roll back atomically
(a buggy handler can never corrupt state); strings are bounded per value
and per app (no OOM by concat bomb); a generated app cannot touch anything
outside its own widgets.

## Status codes on the ABI

`compile_src`/`click`/`input` return 0 = ok (output buffer holds the render
tree JSON), 1 = compile rejection (output holds the rendered diag), 2 =
event fault, rolled back (output holds the rendered diag; the last tree is
still the true UI). `card` serves the language card; `out_ptr`/`out_len`
locate the output; `alloc`/`dealloc` manage argument buffers.

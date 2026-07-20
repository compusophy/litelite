# app/ — the applite vibe-coding shell

A one-page app-builder on the kit: write (or paste) an applite program on
the left, hit **verify + run**, and the app is live on the right — because
the verifier proved it total first. No framework, no npm, no wasm-bindgen,
no network calls: one Rust cdylib with a hand-rolled extern "C" ABI
(`src/lib.rs`), one HTML page (`index.html`), and the `applite` crate.

## Run it

**Live: <https://litelite.vercel.app>** — the single-file build, deployed.

```sh
./build.sh                          # -> appshell.wasm (dev) + dist/index.html
python -m http.server -d . 8080     # dev loop (fetches the wasm over http)
# or just open dist/index.html — the single file works from file:// too
vercel deploy dist --prod           # ship it (project: litelite)
```

`dist/index.html` is the whole product in one ~154 KB file: the applite
compiler/verifier/runtime as wasm, base64-inlined. No assets, no requests.

## The vibe-coding loop (keyless by design)

**Fully local (the generate button):** start the generator —

```sh
../experiment/train/.venv/Scripts/python.exe serve.py   # loads the C5 fine-tune, port 8765
```

— then type an idea in the header box and hit **generate**. The fine-tune
(trained against the a8 behavioral reward, `experiment/appbench`) samples 4
candidates; THIS PAGE's wasm verifier is the selector — the first candidate
that compiles and checks becomes the live app, and the status bar reports
how many were rejected. Generate → verify → keep, visible in the UI. No
cloud, no key, ~1.4 GB of local model. (The model satisfies specs
literally — name your button labels and displayed strings; nuance like
"only after typing" can be missed while still verifying valid.)

**Via any LLM (copy prompt):**

1. **copy prompt** — puts the applite language card (`applite::REFERENCE`,
   served verbatim from the wasm, so prompt and verifier cannot drift) plus
   your app idea on the clipboard.
2. Paste into ANY LLM.
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

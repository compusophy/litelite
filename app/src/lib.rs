//! The wasm boundary of the vibe-coding shell: a hand-rolled extern "C" ABI
//! over `applite` — no wasm-bindgen, no JS framework, no deps. JS writes
//! UTF-8 into linear memory (`alloc`), calls an entry point, and reads the
//! result from the output buffer (`out_ptr`/`out_len`). Every entry point
//! returns a status: 0 = ok (output is the render-tree JSON), 1 = compile
//! error (output is the caret-rendered diag), 2 = event fault (output is the
//! diag; state rolled back, the last tree is still true).
//!
//! wasm32-unknown-unknown is single-threaded; the one live app and its
//! source live in thread_locals.

use std::cell::RefCell;

use applite::{App, Event, Limits, Node, REFERENCE, compile};

thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    static SRC: RefCell<String> = const { RefCell::new(String::new()) };
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Give JS `len` bytes to write into; freed by `dealloc` (or reused).
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut v: Vec<u8> = Vec::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr`/`len` must be exactly what a prior [`alloc`] returned and was given.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dealloc(ptr: *mut u8, len: usize) {
    unsafe { drop(Vec::from_raw_parts(ptr, 0, len.max(1))) }
}

#[unsafe(no_mangle)]
pub extern "C" fn out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}

#[unsafe(no_mangle)]
pub extern "C" fn out_len() -> usize {
    OUT.with(|o| o.borrow().len())
}

fn set_out(s: String) {
    OUT.with(|o| *o.borrow_mut() = s.into_bytes());
}

/// The language card — the shell's "copy prompt" serves the crate's
/// REFERENCE verbatim (prompt and verifier are one artifact).
#[unsafe(no_mangle)]
pub extern "C" fn card() -> i32 {
    set_out(REFERENCE.to_string());
    0
}

fn read_str(ptr: *const u8, len: usize) -> String {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    String::from_utf8_lossy(bytes).into_owned()
}

/// Compile `src`; on success the app starts at its initial state and the
/// output is its first render.
///
/// # Safety
/// `ptr..ptr+len` must be readable (an [`alloc`] buffer JS filled).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn compile_src(ptr: *const u8, len: usize) -> i32 {
    let src = read_str(ptr, len);
    match compile(&src) {
        Ok(program) => {
            let app = App::new(program, Limits::default());
            let r = render_json(&app);
            APP.with(|a| *a.borrow_mut() = Some(app));
            SRC.with(|s| *s.borrow_mut() = src);
            set_out(r);
            0
        }
        Err(d) => {
            set_out(d.render(&src));
            1
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn click(id: u32) -> i32 {
    event(&Event::Click { id })
}

/// # Safety
/// Both ranges must be readable (see [`compile_src`]).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn input(sptr: *const u8, slen: usize, tptr: *const u8, tlen: usize) -> i32 {
    event(&Event::Input {
        state: read_str(sptr, slen),
        text: read_str(tptr, tlen),
    })
}

fn event(ev: &Event) -> i32 {
    APP.with(|a| match a.borrow_mut().as_mut() {
        None => {
            set_out("no app is compiled".to_string());
            2
        }
        Some(app) => match app.handle(ev) {
            Ok(()) => {
                set_out(render_json(app));
                0
            }
            // Atomic rollback happened; render the diag against the source.
            Err(d) => {
                set_out(SRC.with(|s| d.render(&s.borrow())));
                2
            }
        },
    })
}

/// The render tree as JSON — hand-rolled, since the tree is tiny and the
/// kit takes no deps. A render fault (fuel) is also possible and reported
/// as status 2 by the caller path that hits it.
fn render_json(app: &App) -> String {
    let mut s = String::from("[");
    match app.render() {
        Ok(nodes) => {
            for (i, n) in nodes.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                node(n, &mut s);
            }
        }
        Err(d) => {
            // Surface a render fault as a label so the page shows SOMETHING.
            s.push_str("{\"t\":\"label\",\"text\":");
            esc(&format!("render fault: {d}"), &mut s);
            s.push('}');
        }
    }
    s.push(']');
    s
}

fn node(n: &Node, s: &mut String) {
    match n {
        Node::Label { text } => {
            s.push_str("{\"t\":\"label\",\"text\":");
            esc(text, s);
            s.push('}');
        }
        Node::Button { text, id } => {
            s.push_str(&format!("{{\"t\":\"button\",\"id\":{id},\"text\":"));
            esc(text, s);
            s.push('}');
        }
        Node::Input { state, value } => {
            s.push_str("{\"t\":\"input\",\"state\":");
            esc(state, s);
            s.push_str(",\"value\":");
            esc(value, s);
            s.push('}');
        }
        Node::Row { children } | Node::Col { children } => {
            let t = if matches!(n, Node::Row { .. }) {
                "row"
            } else {
                "col"
            };
            s.push_str(&format!("{{\"t\":\"{t}\",\"children\":["));
            for (i, c) in children.iter().enumerate() {
                if i > 0 {
                    s.push(',');
                }
                node(c, s);
            }
            s.push_str("]}");
        }
    }
}

/// Minimal JSON string escape: quotes, backslash, control chars.
fn esc(text: &str, s: &mut String) {
    s.push('"');
    for c in text.chars() {
        match c {
            '"' => s.push_str("\\\""),
            '\\' => s.push_str("\\\\"),
            '\n' => s.push_str("\\n"),
            '\r' => s.push_str("\\r"),
            '\t' => s.push_str("\\t"),
            c if (c as u32) < 0x20 => s.push_str(&format!("\\u{:04x}", c as u32)),
            c => s.push(c),
        }
    }
    s.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(src: &str) -> (i32, String) {
        let code = unsafe { compile_src(src.as_ptr(), src.len()) };
        (
            code,
            OUT.with(|o| String::from_utf8_lossy(&o.borrow()).into_owned()),
        )
    }

    #[test]
    fn compile_click_and_json_shape() {
        let (code, tree) = call("state n = 0; button \"+\" { n = n + 1; } label \"n: \" + n;");
        assert_eq!(code, 0, "{tree}");
        assert!(
            tree.contains("{\"t\":\"button\",\"id\":0,\"text\":\"+\"}"),
            "{tree}"
        );
        assert!(tree.contains("\"n: 0\""), "{tree}");
        assert_eq!(click(0), 0);
        let tree = OUT.with(|o| String::from_utf8_lossy(&o.borrow()).into_owned());
        assert!(tree.contains("\"n: 1\""), "{tree}");
    }

    #[test]
    fn errors_come_back_rendered_and_faults_roll_back() {
        let (code, err) = call("label nope;");
        assert_eq!(code, 1);
        assert!(err.contains("E0302"), "{err}");
        assert!(err.contains('^'), "{err}"); // caret snippet, verbatim to the page
        // Fault path: divide by zero rolls back and reports.
        let (code, _) = call("state x = 1; button \"b\" { x = x / (x - x); }");
        assert_eq!(code, 0);
        assert_eq!(click(0), 2);
        let err = OUT.with(|o| String::from_utf8_lossy(&o.borrow()).into_owned());
        assert!(err.contains("E0203"), "{err}");
    }

    #[test]
    fn input_events_and_string_escaping() {
        let (code, _) = call("state s = \"\"; input s; label s;");
        assert_eq!(code, 0);
        let (st, tx) = ("s", "a\"b\\c\nd");
        assert_eq!(
            unsafe { input(st.as_ptr(), st.len(), tx.as_ptr(), tx.len()) },
            0
        );
        let tree = OUT.with(|o| String::from_utf8_lossy(&o.borrow()).into_owned());
        assert!(tree.contains("a\\\"b\\\\c\\nd"), "{tree}");
        assert_eq!(card(), 0);
        assert!(OUT.with(|o| o.borrow().starts_with(b"applite: a tiny TOTAL")));
    }
}

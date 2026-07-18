//! `flow-rt` — the shared runtime seam (backend-llvm DESIGN §1, ADR-0020 §2).
//!
//! The emitted native program calls these `extern "C"` symbols for every
//! `Print` and every trap, so float formatting and trap/exit behaviour are the
//! interpreter oracle's *by construction*: each print renders via Rust
//! shortest-round-trip `Display`, exactly as `flow_interp::render` does (both
//! call the same formatter). `flow_trap` writes the defined message to stderr
//! and exits 101 (ADR-0013 / ADR-0020 §3).
//!
//! Stdout is flushed on every call — the differential harness reads pipes.

use std::io::{self, Write};

/// Render `v` via `Display` (== interp `render`) and write it to stdout,
/// appending a newline when `newline`; flush before returning.
fn emit(v: &dyn std::fmt::Display, newline: bool) {
    let mut out = io::stdout();
    // Ignore write errors: a broken pipe is the harness's business, not ours.
    if newline {
        let _ = writeln!(out, "{v}");
    } else {
        let _ = write!(out, "{v}");
    }
    let _ = out.flush();
}

/// Define one `flow_print_<ty>(v, newline)` extern. `Display` for every scalar
/// type is the shortest round-trip, matching interp `render` verbatim.
macro_rules! print_fn {
    ($name:ident, $ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(v: $ty, newline: bool) {
            emit(&v, newline);
        }
    };
}

print_fn!(flow_print_i32, i32);
print_fn!(flow_print_i64, i64);
print_fn!(flow_print_u8, u8);
print_fn!(flow_print_bool, bool);
print_fn!(flow_print_f32, f32);
print_fn!(flow_print_f64, f64);

/// Print a UTF-8 string given as `(ptr, len)` (interp `render(Str) == s`).
///
/// # Safety
/// `ptr` must point to `len` initialised bytes of valid UTF-8 (the emitter only
/// ever passes a private `Str` global constant — never data, since lower
/// rejects strings-as-data, DESIGN §2).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn flow_print_str(ptr: *const u8, len: usize, newline: bool) {
    let s = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len)) };
    emit(&s, newline);
}

/// A defined trap: write the message to stderr and exit 101 (ADR-0013).
/// `0 = div_zero`, `1 = index_oob`; the emitter only ever passes these two.
#[unsafe(no_mangle)]
pub extern "C" fn flow_trap(kind: u32) -> ! {
    let msg = match kind {
        0 => "div_zero",
        1 => "index_oob",
        _ => "unknown", // ponytail: emitter passes only 0/1; total match anyway
    };
    // Flush any buffered stdout first (belt-and-braces; prints already flush).
    let _ = io::stdout().flush();
    eprintln!("flow trap: {msg}");
    std::process::exit(101);
}

#[cfg(test)]
mod tests {
    use flow_interp::render;
    use flow_ir::Value;

    /// Render-parity table (DESIGN §1): every scalar flow-rt prints must be
    /// byte-identical to `flow_interp::render`. flow-rt prints `v` via
    /// `Display`; assert that string equals the oracle's render for the same
    /// value. Covers the pinned tricky cases: `4080.0 → "4080"`, `5.375`,
    /// `-0.0`, `NaN`, `inf`, u8 `255`, bools, i64 extremes.
    #[test]
    fn render_parity() {
        macro_rules! check {
            ($disp:expr, $val:expr) => {{
                let rt = format!("{}", $disp); // what flow-rt's emit() writes
                let oracle = render(&$val);
                assert_eq!(rt, oracle, "parity mismatch for {:?}", $val);
            }};
        }

        // f64 — the formatting-sensitive cases.
        check!(4080.0_f64, Value::F64(4080.0));
        check!(5.375_f64, Value::F64(5.375));
        check!(-0.0_f64, Value::F64(-0.0));
        check!(0.0_f64, Value::F64(0.0));
        check!(f64::NAN, Value::F64(f64::NAN));
        check!(f64::INFINITY, Value::F64(f64::INFINITY));
        check!(f64::NEG_INFINITY, Value::F64(f64::NEG_INFINITY));
        check!(f64::MAX, Value::F64(f64::MAX));

        // f32.
        check!(4080.0_f32, Value::F32(4080.0));
        check!(5.375_f32, Value::F32(5.375));
        check!(-0.0_f32, Value::F32(-0.0));
        check!(f32::NAN, Value::F32(f32::NAN));
        check!(f32::INFINITY, Value::F32(f32::INFINITY));

        // u8 — incl. the high half (255, > 127).
        check!(0_u8, Value::U8(0));
        check!(127_u8, Value::U8(127));
        check!(128_u8, Value::U8(128));
        check!(255_u8, Value::U8(255));

        // bools.
        check!(true, Value::Bool(true));
        check!(false, Value::Bool(false));

        // Str — flow_print_str renders bytes via Display (== render(Str) == s).
        check!("hello", Value::Str("hello".into()));
        check!("", Value::Str("".into()));
        check!("4080", Value::Str("4080".into()));

        // i32 / i64 extremes.
        check!(0_i32, Value::I32(0));
        check!(-1_i32, Value::I32(-1));
        check!(i32::MIN, Value::I32(i32::MIN));
        check!(i32::MAX, Value::I32(i32::MAX));
        check!(i64::MIN, Value::I64(i64::MIN));
        check!(i64::MAX, Value::I64(i64::MAX));
    }
}

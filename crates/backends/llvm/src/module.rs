//! Module skeleton (DESIGN §2/§4): the `flow-rt` extern declarations (with the
//! S13 `zeroext` ABI rule on every `i8`/`i1` parameter), `Str` private globals,
//! and the public `@main` wrapper.

use flow_ir::{CategoryIr, ObjectId, ObjectKind, Ty, Value};
use slotmap::SecondaryMap;

use crate::ty::{lower_named_input_ty, lower_ty};

/// A private `Str` constant global: its symbol name and byte length. `Print` of a
/// `Str` passes `getelementptr(@name)` + `len` to `flow_print_str` (DESIGN §1).
pub(crate) struct StrGlobal {
    pub name: String,
    pub bytes: Vec<u8>,
}

impl StrGlobal {
    /// The array LLVM type holding the bytes (no NUL — `flow_print_str` reads
    /// exactly `len` bytes via `from_raw_parts`).
    pub fn arr_ty(&self) -> String {
        format!("[{} x i8]", self.bytes.len())
    }
}

/// The `flow-rt` extern block + `llvm.memcpy` intrinsic (DESIGN §1). Every
/// `i8`/`i1` parameter carries `zeroext` — the S13 ABI rule, load-bearing for u8
/// values > 127 on arm64 (sepia's channels) and the trailing-newline `i1`.
/// `flow_trap` alone carries `noreturn` (flow-rt defines it `-> !`, exit 101);
/// the print externs stay attribute-free.
pub(crate) const RT_DECLS: &str = "\
declare void @flow_print_i32(i32, i1 zeroext)\n\
declare void @flow_print_i64(i64, i1 zeroext)\n\
declare void @flow_print_u8(i8 zeroext, i1 zeroext)\n\
declare void @flow_print_bool(i1 zeroext, i1 zeroext)\n\
declare void @flow_print_f32(float, i1 zeroext)\n\
declare void @flow_print_f64(double, i1 zeroext)\n\
declare void @flow_print_str(ptr, i64, i1 zeroext)\n\
declare void @flow_trap(i32) noreturn\n\
declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)\n";

/// The parallel scheduler ABI, emitted only for a parallel `flow_main`.
pub(crate) const PAR_DECLS: &str = "\
declare ptr @flow_par_begin(i32)\n\
declare void @flow_par_task(ptr, i32, i32, ptr, i64, i32)\n\
declare void @flow_par_pin(ptr, i32)\n\
declare void @flow_par_dep(ptr, i32, i32)\n\
declare void @flow_par_launch(ptr, ptr)\n\
declare void @flow_par_wait(ptr, ptr, i32)\n\
declare void @flow_par_check(ptr, i64)\n\
declare void @flow_par_trap(i64, i32)\n\
declare void @flow_par_watermark(i64)\n\
declare void @flow_par_run_pinned(ptr, i32)\n\
declare void @flow_par_finish(ptr)\n";

/// Collect every `Str` constant object → a private global (DESIGN §2). One
/// global per object, named `@.strN` by deterministic object order.
pub(crate) fn collect_str_globals(ir: &CategoryIr) -> SecondaryMap<ObjectId, StrGlobal> {
    let mut out: SecondaryMap<ObjectId, StrGlobal> = SecondaryMap::new();
    let mut n = 0usize;
    for (id, obj) in ir.objects() {
        if obj.kind == ObjectKind::Constant
            && obj.ty == Ty::Str
            && let Some(Value::Str(s)) = &obj.value
        {
            out.insert(
                id,
                StrGlobal {
                    name: format!("@.str{n}"),
                    bytes: s.clone().into_bytes(),
                },
            );
            n += 1;
        }
    }
    out
}

/// Emit the `Str` globals block (deterministic object order via the SecondaryMap).
pub(crate) fn emit_str_globals(globals: &SecondaryMap<ObjectId, StrGlobal>) -> String {
    let mut out = String::new();
    for (_, g) in globals.iter() {
        out.push_str(&format!(
            "{} = private unnamed_addr constant {} c\"{}\"\n",
            g.name,
            g.arr_ty(),
            escape_bytes(&g.bytes),
        ));
    }
    out
}

/// LLVM string-constant escaping: printable ASCII (except `"` and `\`) verbatim,
/// everything else as `\HH`.
fn escape_bytes(bytes: &[u8]) -> String {
    let mut s = String::new();
    for &b in bytes {
        if b == b'"' || b == b'\\' || !(0x20..0x7f).contains(&b) {
            s.push_str(&format!("\\{b:02X}"));
        } else {
            s.push(b as char);
        }
    }
    s
}

/// The public `@main` wrapper (DESIGN §4, BL8). Calls the entry body `@flow_main`
/// and returns exit 0; a non-erased return is printed through `flow-rt` so the
/// differential observes it (the `Unit → i32` closed shape).
pub(crate) fn emit_main_wrapper(ir: &CategoryIr) -> String {
    let entry = ir.entry();
    let fd = ir.func(entry).expect("sealed graph: entry resolves");
    let input_ty = &ir.object(fd.input).expect("input resolves").ty;
    let output_ty = &ir.object(fd.output).expect("output resolves").ty;

    let mut out = String::from("define i32 @main() {\nentry:\n");

    // Build the argument list. Closed entries (Unit / IoToken input) pass none;
    // any other shape is not a native-observable closed program (BL8) but still
    // gets a valid call with a zeroinitializer so emission is total. The type
    // is the entry fn's by-ref signature (suggestions #8: array components
    // arrive as `ptr`; `zeroinitializer` nulls them).
    let arg = match lower_named_input_ty(input_ty) {
        None => String::new(),
        Some(t) => format!("{t} zeroinitializer"),
    };

    match lower_ty(output_ty) {
        None => {
            out.push_str(&format!("  call void @flow_main({arg})\n"));
        }
        Some(rty) => {
            out.push_str(&format!("  %r = call {rty} @flow_main({arg})\n"));
            if let Some(call) = print_call(output_ty, "%r") {
                out.push_str(&format!("  {call}\n"));
            }
        }
    }
    out.push_str("  ret i32 0\n}\n");
    out
}

/// The `flow-rt` print call for a scalar return value operand (BL8 result print).
/// `None` for a type flow-rt cannot print through this path (e.g. an aggregate).
fn print_call(ty: &Ty, operand: &str) -> Option<String> {
    let (func, tystr, ze) = match ty {
        Ty::Int { bits: 32, .. } => ("flow_print_i32", "i32", false),
        Ty::Int { bits: 64, .. } => ("flow_print_i64", "i64", false),
        Ty::Int { bits: 8, .. } => ("flow_print_u8", "i8", true),
        Ty::Bool => ("flow_print_bool", "i1", true),
        Ty::Float { bits: 32 } => ("flow_print_f32", "float", false),
        Ty::Float { bits: 64 } => ("flow_print_f64", "double", false),
        _ => return None,
    };
    // Param attr goes *after* the type in a call arg (`i8 zeroext %r`) — the
    // attr-before-type form is invalid LLVM (matches the after-type declare).
    let ze = if ze { "zeroext " } else { "" };
    Some(format!(
        "call void @{func}({tystr} {ze}{operand}, i1 zeroext true)"
    ))
}

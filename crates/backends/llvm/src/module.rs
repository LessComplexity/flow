//! Module skeleton (DESIGN §2/§4): the `mapal-rt` extern declarations (with the
//! S13 `zeroext` ABI rule on every `i8`/`i1` parameter), `Str` private globals,
//! and the public `@main` wrapper.

use mapal_ir::{CategoryIr, ObjectId, ObjectKind, Ty, Value};
use slotmap::SecondaryMap;

use crate::ty::{lower_named_input_ty, lower_ty};

/// A private `Str` constant global: its symbol name and byte length. `Print` of a
/// `Str` passes `getelementptr(@name)` + `len` to `mapal_print_str` (DESIGN §1).
pub(crate) struct StrGlobal {
    pub name: String,
    pub bytes: Vec<u8>,
}

impl StrGlobal {
    /// The array LLVM type holding the bytes (no NUL — `mapal_print_str` reads
    /// exactly `len` bytes via `from_raw_parts`).
    pub fn arr_ty(&self) -> String {
        format!("[{} x i8]", self.bytes.len())
    }
}

/// The `mapal-rt` extern block + `llvm.memcpy` intrinsic (DESIGN §1). Every
/// `i8`/`i1` parameter carries `zeroext` — the S13 ABI rule, load-bearing for u8
/// values > 127 on arm64 (sepia's channels) and the trailing-newline `i1`.
/// `mapal_trap` alone carries `noreturn` (mapal-rt defines it `-> !`, exit 101);
/// the print externs stay attribute-free, and so does `mapal_time_ms` (the
/// `time` builtin's clock read — declared unconditionally, NOT gated on
/// `EmitOpts::perf_timing` like `PERF_DECLS`).
pub(crate) const RT_DECLS: &str = "\
declare void @mapal_print_i32(i32, i1 zeroext)\n\
declare void @mapal_print_i64(i64, i1 zeroext)\n\
declare void @mapal_print_u8(i8 zeroext, i1 zeroext)\n\
declare void @mapal_print_bool(i1 zeroext, i1 zeroext)\n\
declare void @mapal_print_f32(float, i1 zeroext)\n\
declare void @mapal_print_f64(double, i1 zeroext)\n\
declare void @mapal_print_str(ptr, i64, i1 zeroext)\n\
declare double @mapal_time_ms()\n\
declare void @mapal_trap(i32) noreturn\n\
declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)\n";

/// Opt-in compute timer ABI, emitted only with `EmitOpts::perf_timing`.
pub(crate) const PERF_DECLS: &str = "\
declare void @mapal_perf_begin()\n\
declare void @mapal_perf_end()\n";

/// The heap-lowering arena ABI (plan-s29 emission item 4), emitted only for a
/// module that actually lowers a block to it (`profile.rs:heap_min_bytes`) — a
/// program whose every array still fits an `alloca` keeps today's declaration
/// block byte-for-byte.
pub(crate) const HEAP_DECLS: &str = "\
declare ptr @mapal_rt_alloc(i64, i64)\n\
declare void @mapal_rt_free_all()\n";

/// Packed tiled kernels prefetch their next panel line.
pub(crate) const PREFETCH_DECL: &str =
    "declare void @llvm.prefetch.p0(ptr, i32 immarg, i32 immarg, i32 immarg)\n";

/// The three ARM SME intrinsics the streaming panel kernel calls, emitted only
/// for a module that actually contains a call to it. Verified set — nothing
/// else is needed: predicates are literal `splat (i1 true)` (no `ptrue` call)
/// and operand loads are plain `load <vscale x 4 x float>` (no SVE load
/// intrinsic). See `benches/sme/README.md`.
pub(crate) const SME_DECLS: &str = "\
declare void @llvm.aarch64.sme.zero(i32 immarg)\n\
declare void @llvm.aarch64.sme.mopa.nxv4f32(i32 immarg, <vscale x 4 x i1>, <vscale x 4 x i1>, <vscale x 4 x float>, <vscale x 4 x float>)\n\
declare <vscale x 4 x float> @llvm.aarch64.sme.read.horiz.nxv4f32(<vscale x 4 x float>, <vscale x 4 x i1>, i32 immarg, i32)\n";

/// The streaming SME panel kernel: `C[0..t][0..t] = Σ_k ap[k][0..t] ⊗ b[k][0..t]`,
/// accumulated in ZA tile 0 and stored once. `t` is the SVL-derived tile side
/// (`TargetProfile::sme_tile_side` — 16 for f32 at SVL 512), the only machine
/// constant in the text.
///
/// This is `benches/sme/spec-verified.ll` — hand-written, lowered, linked and
/// **run** (0/256 cells differ against a fused reference, 0 spill) — with one
/// change: the single `%N` stride is split into `%bn` (the b row stride) and
/// `%cn` (the c row stride), because a packed b panel strides by `t` while `c`
/// strides by the output width. Everything else is byte-for-byte the verified
/// text.
///
/// The attribute set is the load-bearing part and every token in it cost a
/// SIGILL to learn:
///
/// - **`aarch64_pstate_sm_body`, NOT `aarch64_pstate_sm_enabled`.** `_enabled`
///   means "my caller is already in streaming mode" and pushes the transition
///   onto every call site; the emitted body then runs before `smstart sm` and
///   the process dies with `EXC_BAD_INSTRUCTION`. `_body` emits `smstart za` +
///   `smstart sm` at entry and both `smstop`s at exit, so the kernel is
///   self-contained and **nothing else in the emitted module needs to know
///   streaming mode exists**. That is what keeps this a leaf swap rather than
///   an ABI change.
/// - `aarch64_new_za` gives the kernel its own ZA state (zeroed at entry,
///   restored at exit) — the reason a caller on any thread is unaffected.
/// - `+sme,+sme2` and NOT `+sve`: this part has SME without full SVE, so a
///   target that implies `+sve` (`-march=armv9-a`) makes LLVM emit
///   non-streaming SVE in the prologue and the program SIGILLs before
///   `smstart`. Build the emitted module with `-march=armv8-a+sme2`.
///
/// One kernel per module, not one per site: the geometry that varies between
/// sites (`bn`, `cn`, `K`) is passed, exactly as the verified spec passes it.
pub(crate) fn sme_panel(t: u64) -> String {
    format!(
        "\
define internal void @mapal_sme_panel(ptr %ap, ptr %b, ptr %c, i64 %bn, i64 %cn, i64 %K) \
\"aarch64_new_za\" \"aarch64_pstate_sm_body\" vscale_range(1,16) \
\"target-features\"=\"+sme,+sme2,+neon,+fp-armv8,+v8a\" {{
entry:
  call void @llvm.aarch64.sme.zero(i32 255)
  br label %kloop

kloop:
  %k = phi i64 [ 0, %entry ], [ %knext, %kloop ]
  %aoff = mul nuw nsw i64 %k, {t}
  %apk = getelementptr inbounds float, ptr %ap, i64 %aoff
  %zn = load <vscale x 4 x float>, ptr %apk, align 4
  %boff = mul nuw nsw i64 %k, %bn
  %bk = getelementptr inbounds float, ptr %b, i64 %boff
  %zm = load <vscale x 4 x float>, ptr %bk, align 4
  call void @llvm.aarch64.sme.mopa.nxv4f32(i32 0, <vscale x 4 x i1> splat (i1 true), <vscale x 4 x i1> splat (i1 true), <vscale x 4 x float> %zn, <vscale x 4 x float> %zm)
  %knext = add nuw nsw i64 %k, 1
  %done = icmp eq i64 %knext, %K
  br i1 %done, label %store, label %kloop

store:
  br label %rows

rows:
  %r = phi i64 [ 0, %store ], [ %rnext, %rows ]
  %r32 = trunc i64 %r to i32
  %row = call <vscale x 4 x float> @llvm.aarch64.sme.read.horiz.nxv4f32(<vscale x 4 x float> undef, <vscale x 4 x i1> splat (i1 true), i32 0, i32 %r32)
  %coff = mul nuw nsw i64 %r, %cn
  %crow = getelementptr inbounds float, ptr %c, i64 %coff
  store <vscale x 4 x float> %row, ptr %crow, align 4
  %rnext = add nuw nsw i64 %r, 1
  %rdone = icmp eq i64 %rnext, {t}
  br i1 %rdone, label %exit, label %rows

exit:
  ret void
}}
"
    )
}

/// The parallel scheduler ABI, emitted only for a parallel `mapal_main`.
pub(crate) const PAR_DECLS: &str = "\
declare ptr @mapal_par_begin(i32)\n\
declare void @mapal_par_task(ptr, i32, i32, ptr, i64, i32, i64, i32, i32)\n\
declare void @mapal_par_pin(ptr, i32)\n\
declare void @mapal_par_dep(ptr, i32, i32)\n\
declare void @mapal_par_launch(ptr, ptr)\n\
declare void @mapal_par_wait(ptr, ptr, i32)\n\
declare void @mapal_par_check(ptr, i64)\n\
declare void @mapal_par_trap(i64, i32)\n\
declare void @mapal_par_watermark(i64)\n\
declare void @mapal_par_run_pinned(ptr, i32)\n\
declare void @mapal_par_finish(ptr)\n";

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

/// The public `@main` wrapper (DESIGN §4, BL8). Calls the entry body `@mapal_main`
/// and returns exit 0; a non-erased return is printed through `mapal-rt` so the
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
            out.push_str(&format!("  call void @mapal_main({arg})\n"));
        }
        Some(rty) => {
            out.push_str(&format!("  %r = call {rty} @mapal_main({arg})\n"));
            if let Some(call) = print_call(output_ty, "%r") {
                out.push_str(&format!("  {call}\n"));
            }
        }
    }
    out.push_str("  ret i32 0\n}\n");
    out
}

/// The `mapal-rt` print call for a scalar return value operand (BL8 result print).
/// `None` for a type mapal-rt cannot print through this path (e.g. an aggregate).
fn print_call(ty: &Ty, operand: &str) -> Option<String> {
    let (func, tystr, ze) = match ty {
        Ty::Int { bits: 32, .. } => ("mapal_print_i32", "i32", false),
        Ty::Int { bits: 64, .. } => ("mapal_print_i64", "i64", false),
        Ty::Int { bits: 8, .. } => ("mapal_print_u8", "i8", true),
        Ty::Bool => ("mapal_print_bool", "i1", true),
        Ty::Float { bits: 32 } => ("mapal_print_f32", "float", false),
        Ty::Float { bits: 64 } => ("mapal_print_f64", "double", false),
        _ => return None,
    };
    // Param attr goes *after* the type in a call arg (`i8 zeroext %r`) — the
    // attr-before-type form is invalid LLVM (matches the after-type declare).
    let ze = if ze { "zeroext " } else { "" };
    Some(format!(
        "call void @{func}({tystr} {ze}{operand}, i1 zeroext true)"
    ))
}

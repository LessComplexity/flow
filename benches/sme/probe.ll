; SME probe: does LLVM 22.1.8 lower an outer-product-accumulate into ZA?
target triple = "aarch64-apple-darwin"

declare void @llvm.aarch64.sme.mopa.nxv4f32(i32 immarg, <vscale x 4 x i1>, <vscale x 4 x i1>, <vscale x 4 x float>, <vscale x 4 x float>)

define void @fmopa_probe(<vscale x 4 x i1> %pn, <vscale x 4 x i1> %pm,
                         <vscale x 4 x float> %zn, <vscale x 4 x float> %zm)
    #0
{
  call void @llvm.aarch64.sme.mopa.nxv4f32(i32 0, <vscale x 4 x i1> %pn, <vscale x 4 x i1> %pm,
                                           <vscale x 4 x float> %zn, <vscale x 4 x float> %zm)
  ret void
}

attributes #0 = { "aarch64_pstate_sm_enabled" "aarch64_new_za" "target-features"="+sme,+sme2,+sve" }

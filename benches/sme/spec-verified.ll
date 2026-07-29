; The exact shape a Mapal SME realization must emit.
; C[i0..i0+16][j0..j0+16] += sum_k A_packed[k][0..16] (x) B[k][j0..j0+16]
; ap: A packed so ap[k*16 + i] = A[i0+i][k]  (contiguous per k)
; b : row-major B, row stride N
; c : row-major C, row stride N

declare void @llvm.aarch64.sme.zero(i32 immarg)
declare void @llvm.aarch64.sme.mopa.nxv4f32(i32 immarg, <vscale x 4 x i1>, <vscale x 4 x i1>, <vscale x 4 x float>, <vscale x 4 x float>)
declare <vscale x 4 x float> @llvm.aarch64.sme.read.horiz.nxv4f32(<vscale x 4 x float>, <vscale x 4 x i1>, i32 immarg, i32)

define void @mapal_sme_panel(ptr %ap, ptr %b, ptr %c, i64 %N, i64 %K) #0 {
entry:
  call void @llvm.aarch64.sme.zero(i32 255)
  br label %kloop

kloop:
  %k = phi i64 [ 0, %entry ], [ %knext, %kloop ]
  %aoff = shl nuw nsw i64 %k, 4
  %apk  = getelementptr inbounds float, ptr %ap, i64 %aoff
  %zn   = load <vscale x 4 x float>, ptr %apk, align 4
  %boff = mul nuw nsw i64 %k, %N
  %bk   = getelementptr inbounds float, ptr %b, i64 %boff
  %zm   = load <vscale x 4 x float>, ptr %bk, align 4
  call void @llvm.aarch64.sme.mopa.nxv4f32(i32 0, <vscale x 4 x i1> splat (i1 true), <vscale x 4 x i1> splat (i1 true), <vscale x 4 x float> %zn, <vscale x 4 x float> %zm)
  %knext = add nuw nsw i64 %k, 1
  %done  = icmp eq i64 %knext, %K
  br i1 %done, label %store, label %kloop

store:
  br label %rows

rows:
  %r = phi i64 [ 0, %store ], [ %rnext, %rows ]
  %r32 = trunc i64 %r to i32
  %row = call <vscale x 4 x float> @llvm.aarch64.sme.read.horiz.nxv4f32(<vscale x 4 x float> undef, <vscale x 4 x i1> splat (i1 true), i32 0, i32 %r32)
  %coff = mul nuw nsw i64 %r, %N
  %crow = getelementptr inbounds float, ptr %c, i64 %coff
  store <vscale x 4 x float> %row, ptr %crow, align 4
  %rnext = add nuw nsw i64 %r, 1
  %rdone = icmp eq i64 %rnext, 16
  br i1 %rdone, label %exit, label %rows

exit:
  ret void
}

attributes #0 = { "aarch64_new_za" "aarch64_pstate_sm_body" vscale_range(1,16) "target-features"="+sme,+sme2,+neon,+fp-armv8,+v8a" }

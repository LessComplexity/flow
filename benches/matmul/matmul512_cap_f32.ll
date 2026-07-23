; flow-backend-llvm emitted module
declare void @flow_print_i32(i32, i1 zeroext)
declare void @flow_print_i64(i64, i1 zeroext)
declare void @flow_print_u8(i8 zeroext, i1 zeroext)
declare void @flow_print_bool(i1 zeroext, i1 zeroext)
declare void @flow_print_f32(float, i1 zeroext)
declare void @flow_print_f64(double, i1 zeroext)
declare void @flow_print_str(ptr, i64, i1 zeroext)
declare void @flow_trap(i32) noreturn
declare void @llvm.memcpy.p0.p0.i64(ptr, ptr, i64, i1)
declare ptr @flow_par_begin(i32)
declare void @flow_par_task(ptr, i32, i32, ptr, i64, i32)
declare void @flow_par_pin(ptr, i32)
declare void @flow_par_dep(ptr, i32, i32)
declare void @flow_par_launch(ptr, ptr)
declare void @flow_par_wait(ptr, ptr, i32)
declare void @flow_par_check(ptr, i64)
declare void @flow_par_trap(i64, i32)
declare void @flow_par_watermark(i64)
declare void @flow_par_run_pinned(ptr, i32)
declare void @flow_par_finish(ptr)

%Frame = type { [262144 x i32], [512 x i32], [262144 x float], [262144 x float], { ptr, ptr, ptr, ptr }, [262144 x float], { ptr, i32 }, float, { ptr, i32 }, float, float, float }

@ckpt0_entries = private unnamed_addr constant [4 x i64] [i64 12884901887, i64 12884901893, i64 17179869190, i64 25769803787]
@ckpt1_entries = private unnamed_addr constant [4 x i64] [i64 12884901887, i64 12884901893, i64 17179869190, i64 25769803787]

define internal void @task0(i64 %lo, i64 %hi, ptr %frame) {
entry:
  %s0 = alloca i64
  %o2 = getelementptr %Frame, ptr %frame, i32 0, i32 0
  store i64 %lo, ptr %s0
  br label %bb1
bb1:
  %t4 = load i64, ptr %s0
  %t5 = icmp uge i64 %t4, %hi
  br i1 %t5, label %bb3, label %bb2
bb2:
  %t6 = trunc i64 %t4 to i32
  %t7 = getelementptr [262144 x i32], ptr %o2, i64 0, i64 %t4
  store i32 %t6, ptr %t7
  %t8 = add i64 %t4, 1
  store i64 %t8, ptr %s0
  br label %bb1
bb3:
  ret void
}

define internal void @task1(i64 %lo, i64 %hi, ptr %frame) {
entry:
  %s0 = alloca i64
  %o3 = getelementptr %Frame, ptr %frame, i32 0, i32 1
  store i64 %lo, ptr %s0
  br label %bb1
bb1:
  %t4 = load i64, ptr %s0
  %t5 = icmp uge i64 %t4, %hi
  br i1 %t5, label %bb3, label %bb2
bb2:
  %t6 = trunc i64 %t4 to i32
  %t7 = getelementptr [512 x i32], ptr %o3, i64 0, i64 %t4
  store i32 %t6, ptr %t7
  %t8 = add i64 %t4, 1
  store i64 %t8, ptr %s0
  br label %bb1
bb3:
  ret void
}

define internal void @task2(i64 %lo, i64 %hi, ptr %frame) {
entry:
  %o8 = getelementptr %Frame, ptr %frame, i32 0, i32 6
  %o10 = getelementptr %Frame, ptr %frame, i32 0, i32 8
  %o7 = getelementptr %Frame, ptr %frame, i32 0, i32 5
  %o9 = getelementptr %Frame, ptr %frame, i32 0, i32 7
  %o11 = getelementptr %Frame, ptr %frame, i32 0, i32 9
  %t0 = getelementptr { ptr, i32 }, ptr %o8, i32 0, i32 1
  store i32 0, ptr %t0
  %t1 = getelementptr { ptr, i32 }, ptr %o10, i32 0, i32 1
  store i32 262143, ptr %t1
  %t2 = getelementptr { ptr, i32 }, ptr %o8, i32 0, i32 0
  store ptr %o7, ptr %t2
  %t3 = getelementptr { ptr, i32 }, ptr %o10, i32 0, i32 0
  store ptr %o7, ptr %t3
  %t4 = getelementptr { ptr, i32 }, ptr %o8, i32 0, i32 1
  %t5 = load i32, ptr %t4
  %t6 = sext i32 %t5 to i64
  %t7 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t6
  %t8 = load float, ptr %t7
  store float %t8, ptr %o9
  %t9 = getelementptr { ptr, i32 }, ptr %o10, i32 0, i32 1
  %t10 = load i32, ptr %t9
  %t11 = sext i32 %t10 to i64
  %t12 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t11
  %t13 = load float, ptr %t12
  store float %t13, ptr %o11
  ret void
}

define internal void @task3(i64 %lo, i64 %hi, ptr %frame) {
entry:
  %s0 = alloca i64
  %o2 = getelementptr %Frame, ptr %frame, i32 0, i32 0
  %o4 = getelementptr %Frame, ptr %frame, i32 0, i32 2
  store i64 %lo, ptr %s0
  br label %bb1
bb1:
  %t4 = load i64, ptr %s0
  %t5 = icmp uge i64 %t4, %hi
  br i1 %t5, label %bb3, label %bb2
bb2:
  %t6 = getelementptr [262144 x i32], ptr %o2, i64 0, i64 %t4
  %t7 = load i32, ptr %t6
  %t8 = call float @fn1(i32 %t7)
  %t9 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t4
  store float %t8, ptr %t9
  %t10 = add i64 %t4, 1
  store i64 %t10, ptr %s0
  br label %bb1
bb3:
  ret void
}

define internal void @task4(i64 %lo, i64 %hi, ptr %frame) {
entry:
  %s0 = alloca i64
  %o2 = getelementptr %Frame, ptr %frame, i32 0, i32 0
  %o5 = getelementptr %Frame, ptr %frame, i32 0, i32 3
  store i64 %lo, ptr %s0
  br label %bb1
bb1:
  %t4 = load i64, ptr %s0
  %t5 = icmp uge i64 %t4, %hi
  br i1 %t5, label %bb3, label %bb2
bb2:
  %t6 = getelementptr [262144 x i32], ptr %o2, i64 0, i64 %t4
  %t7 = load i32, ptr %t6
  %t8 = call float @fn2(i32 %t7)
  %t9 = getelementptr [262144 x float], ptr %o5, i64 0, i64 %t4
  store float %t8, ptr %t9
  %t10 = add i64 %t4, 1
  store i64 %t10, ptr %s0
  br label %bb1
bb3:
  ret void
}

define internal void @task5(i64 %lo, i64 %hi, ptr %frame) {
entry:
  %o2 = getelementptr %Frame, ptr %frame, i32 0, i32 0
  %o6 = getelementptr %Frame, ptr %frame, i32 0, i32 4
  %o3 = getelementptr %Frame, ptr %frame, i32 0, i32 1
  %o4 = getelementptr %Frame, ptr %frame, i32 0, i32 2
  %o5 = getelementptr %Frame, ptr %frame, i32 0, i32 3
  %t0 = getelementptr { ptr, ptr, ptr, ptr }, ptr %o6, i32 0, i32 3
  store ptr %o2, ptr %t0
  %t1 = getelementptr { ptr, ptr, ptr, ptr }, ptr %o6, i32 0, i32 0
  store ptr %o3, ptr %t1
  %t2 = getelementptr { ptr, ptr, ptr, ptr }, ptr %o6, i32 0, i32 1
  store ptr %o4, ptr %t2
  %t3 = getelementptr { ptr, ptr, ptr, ptr }, ptr %o6, i32 0, i32 2
  store ptr %o5, ptr %t3
  ret void
}

define internal void @task6(i64 %lo, i64 %hi, ptr %frame) {
entry:
  %s0 = alloca [64 x float]
  %s1 = alloca i64
  %s2 = alloca i64
  %s3 = alloca i64
  %s4 = alloca i64
  %o4 = getelementptr %Frame, ptr %frame, i32 0, i32 2
  %o5 = getelementptr %Frame, ptr %frame, i32 0, i32 3
  %o7 = getelementptr %Frame, ptr %frame, i32 0, i32 5
  %t5 = udiv i64 %lo, 512
  %t6 = add i64 %hi, 511
  %t7 = udiv i64 %t6, 512
  %t8 = add i64 %lo, 511
  %t9 = udiv i64 %t8, 512
  %t10 = udiv i64 %hi, 512
  store i64 %t5, ptr %s1
  br label %bb11
bb11:
  %t14 = load i64, ptr %s1
  %t15 = icmp uge i64 %t14, %t9
  br i1 %t15, label %bb13, label %bb12
bb12:
  %t16 = mul i64 %t14, 512
  %t17 = sub i64 %lo, %t16
  %t18 = icmp slt i64 %t17, 0
  %t19 = select i1 %t18, i64 0, i64 %t17
  %t20 = sub i64 %hi, %t16
  %t21 = icmp sgt i64 %t20, 512
  %t22 = select i1 %t21, i64 512, i64 %t20
  %t23 = mul i64 %t14, 512
  store i64 %t19, ptr %s2
  br label %bb24
bb24:
  %t29 = load i64, ptr %s2
  %t30 = add i64 %t29, 16
  %t31 = icmp ule i64 %t30, %t22
  br i1 %t31, label %bb25, label %bb26
bb25:
  store i64 0, ptr %s4
  br label %bb32
bb32:
  %t35 = load i64, ptr %s4
  %t36 = icmp uge i64 %t35, 16
  br i1 %t36, label %bb34, label %bb33
bb33:
  %t37 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t35
  store float 0x0000000000000000, ptr %t37
  %t38 = add i64 %t35, 1
  store i64 %t38, ptr %s4
  br label %bb32
bb34:
  store i64 0, ptr %s3
  br label %bb39
bb39:
  %t45 = load i64, ptr %s3
  %t46 = icmp uge i64 %t45, 512
  br i1 %t46, label %bb41, label %bb40
bb40:
  %t47 = add i64 %t23, %t45
  %t48 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t47
  %t49 = load float, ptr %t48
  %t50 = mul i64 %t45, 512
  %t51 = add i64 %t50, %t29
  store i64 0, ptr %s4
  br label %bb42
bb42:
  %t52 = load i64, ptr %s4
  %t53 = icmp uge i64 %t52, 16
  br i1 %t53, label %bb44, label %bb43
bb43:
  %t54 = add i64 %t51, %t52
  %t55 = getelementptr [262144 x float], ptr %o5, i64 0, i64 %t54
  %t56 = load float, ptr %t55
  %t57 = fmul float %t49, %t56
  %t58 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t52
  %t59 = load float, ptr %t58
  %t60 = fadd float %t59, %t57
  store float %t60, ptr %t58
  %t61 = add i64 %t52, 1
  store i64 %t61, ptr %s4
  br label %bb42
bb44:
  %t62 = add i64 %t45, 1
  store i64 %t62, ptr %s3
  br label %bb39
bb41:
  %t63 = add i64 %t16, %t29
  store i64 0, ptr %s4
  br label %bb64
bb64:
  %t67 = load i64, ptr %s4
  %t68 = icmp uge i64 %t67, 16
  br i1 %t68, label %bb66, label %bb65
bb65:
  %t69 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t67
  %t70 = load float, ptr %t69
  %t71 = add i64 %t63, %t67
  %t72 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t71
  store float %t70, ptr %t72
  %t73 = add i64 %t67, 1
  store i64 %t73, ptr %s4
  br label %bb64
bb66:
  %t74 = add i64 %t29, 16
  store i64 %t74, ptr %s2
  br label %bb24
bb26:
  %t75 = icmp ult i64 %t29, %t22
  br i1 %t75, label %bb27, label %bb28
bb27:
  %t76 = sub i64 %t22, %t29
  %t77 = icmp ult i64 %t76, 16
  %t78 = select i1 %t77, i64 %t76, i64 16
  store i64 0, ptr %s4
  br label %bb79
bb79:
  %t82 = load i64, ptr %s4
  %t83 = icmp uge i64 %t82, %t78
  br i1 %t83, label %bb81, label %bb80
bb80:
  %t84 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t82
  store float 0x0000000000000000, ptr %t84
  %t85 = add i64 %t82, 1
  store i64 %t85, ptr %s4
  br label %bb79
bb81:
  store i64 0, ptr %s3
  br label %bb86
bb86:
  %t92 = load i64, ptr %s3
  %t93 = icmp uge i64 %t92, 512
  br i1 %t93, label %bb88, label %bb87
bb87:
  %t94 = add i64 %t23, %t92
  %t95 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t94
  %t96 = load float, ptr %t95
  %t97 = mul i64 %t92, 512
  %t98 = add i64 %t97, %t29
  store i64 0, ptr %s4
  br label %bb89
bb89:
  %t99 = load i64, ptr %s4
  %t100 = icmp uge i64 %t99, %t78
  br i1 %t100, label %bb91, label %bb90
bb90:
  %t101 = add i64 %t98, %t99
  %t102 = getelementptr [262144 x float], ptr %o5, i64 0, i64 %t101
  %t103 = load float, ptr %t102
  %t104 = fmul float %t96, %t103
  %t105 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t99
  %t106 = load float, ptr %t105
  %t107 = fadd float %t106, %t104
  store float %t107, ptr %t105
  %t108 = add i64 %t99, 1
  store i64 %t108, ptr %s4
  br label %bb89
bb91:
  %t109 = add i64 %t92, 1
  store i64 %t109, ptr %s3
  br label %bb86
bb88:
  %t110 = add i64 %t16, %t29
  store i64 0, ptr %s4
  br label %bb111
bb111:
  %t114 = load i64, ptr %s4
  %t115 = icmp uge i64 %t114, %t78
  br i1 %t115, label %bb113, label %bb112
bb112:
  %t116 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t114
  %t117 = load float, ptr %t116
  %t118 = add i64 %t110, %t114
  %t119 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t118
  store float %t117, ptr %t119
  %t120 = add i64 %t114, 1
  store i64 %t120, ptr %s4
  br label %bb111
bb113:
  br label %bb28
bb28:
  %t121 = add i64 %t14, 1
  store i64 %t121, ptr %s1
  br label %bb11
bb13:
  store i64 %t9, ptr %s1
  br label %bb122
bb122:
  %t125 = load i64, ptr %s1
  %t126 = add i64 %t125, 4
  %t127 = icmp ule i64 %t126, %t10
  br i1 %t127, label %bb123, label %bb124
bb123:
  %t128 = mul i64 %t125, 512
  %t129 = mul i64 %t125, 512
  %t130 = mul i64 %t125, 512
  %t131 = add i64 512, %t130
  %t132 = mul i64 %t125, 512
  %t133 = add i64 1024, %t132
  %t134 = mul i64 %t125, 512
  %t135 = add i64 1536, %t134
  store i64 0, ptr %s2
  br label %bb136
bb136:
  %t141 = load i64, ptr %s2
  %t142 = add i64 %t141, 16
  %t143 = icmp ule i64 %t142, 512
  br i1 %t143, label %bb137, label %bb138
bb137:
  store i64 0, ptr %s4
  br label %bb144
bb144:
  %t147 = load i64, ptr %s4
  %t148 = icmp uge i64 %t147, 16
  br i1 %t148, label %bb146, label %bb145
bb145:
  %t149 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t147
  store float 0x0000000000000000, ptr %t149
  %t150 = add i64 %t147, 1
  store i64 %t150, ptr %s4
  br label %bb144
bb146:
  store i64 0, ptr %s4
  br label %bb151
bb151:
  %t154 = load i64, ptr %s4
  %t155 = icmp uge i64 %t154, 16
  br i1 %t155, label %bb153, label %bb152
bb152:
  %t156 = add i64 %t154, 16
  %t157 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t156
  store float 0x0000000000000000, ptr %t157
  %t158 = add i64 %t154, 1
  store i64 %t158, ptr %s4
  br label %bb151
bb153:
  store i64 0, ptr %s4
  br label %bb159
bb159:
  %t162 = load i64, ptr %s4
  %t163 = icmp uge i64 %t162, 16
  br i1 %t163, label %bb161, label %bb160
bb160:
  %t164 = add i64 %t162, 32
  %t165 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t164
  store float 0x0000000000000000, ptr %t165
  %t166 = add i64 %t162, 1
  store i64 %t166, ptr %s4
  br label %bb159
bb161:
  store i64 0, ptr %s4
  br label %bb167
bb167:
  %t170 = load i64, ptr %s4
  %t171 = icmp uge i64 %t170, 16
  br i1 %t171, label %bb169, label %bb168
bb168:
  %t172 = add i64 %t170, 48
  %t173 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t172
  store float 0x0000000000000000, ptr %t173
  %t174 = add i64 %t170, 1
  store i64 %t174, ptr %s4
  br label %bb167
bb169:
  store i64 0, ptr %s3
  br label %bb175
bb175:
  %t181 = load i64, ptr %s3
  %t182 = icmp uge i64 %t181, 512
  br i1 %t182, label %bb177, label %bb176
bb176:
  %t183 = add i64 %t129, %t181
  %t184 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t183
  %t185 = load float, ptr %t184
  %t186 = add i64 %t131, %t181
  %t187 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t186
  %t188 = load float, ptr %t187
  %t189 = add i64 %t133, %t181
  %t190 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t189
  %t191 = load float, ptr %t190
  %t192 = add i64 %t135, %t181
  %t193 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t192
  %t194 = load float, ptr %t193
  %t195 = mul i64 %t181, 512
  %t196 = add i64 %t195, %t141
  store i64 0, ptr %s4
  br label %bb178
bb178:
  %t197 = load i64, ptr %s4
  %t198 = icmp uge i64 %t197, 16
  br i1 %t198, label %bb180, label %bb179
bb179:
  %t199 = add i64 %t196, %t197
  %t200 = getelementptr [262144 x float], ptr %o5, i64 0, i64 %t199
  %t201 = load float, ptr %t200
  %t202 = fmul float %t185, %t201
  %t203 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t197
  %t204 = load float, ptr %t203
  %t205 = fadd float %t204, %t202
  store float %t205, ptr %t203
  %t206 = fmul float %t188, %t201
  %t207 = add i64 %t197, 16
  %t208 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t207
  %t209 = load float, ptr %t208
  %t210 = fadd float %t209, %t206
  store float %t210, ptr %t208
  %t211 = fmul float %t191, %t201
  %t212 = add i64 %t197, 32
  %t213 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t212
  %t214 = load float, ptr %t213
  %t215 = fadd float %t214, %t211
  store float %t215, ptr %t213
  %t216 = fmul float %t194, %t201
  %t217 = add i64 %t197, 48
  %t218 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t217
  %t219 = load float, ptr %t218
  %t220 = fadd float %t219, %t216
  store float %t220, ptr %t218
  %t221 = add i64 %t197, 1
  store i64 %t221, ptr %s4
  br label %bb178
bb180:
  %t222 = add i64 %t181, 1
  store i64 %t222, ptr %s3
  br label %bb175
bb177:
  %t223 = add i64 %t128, %t141
  store i64 0, ptr %s4
  br label %bb224
bb224:
  %t227 = load i64, ptr %s4
  %t228 = icmp uge i64 %t227, 16
  br i1 %t228, label %bb226, label %bb225
bb225:
  %t229 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t227
  %t230 = load float, ptr %t229
  %t231 = add i64 %t223, %t227
  %t232 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t231
  store float %t230, ptr %t232
  %t233 = add i64 %t227, 1
  store i64 %t233, ptr %s4
  br label %bb224
bb226:
  %t234 = add i64 %t223, 512
  store i64 0, ptr %s4
  br label %bb235
bb235:
  %t238 = load i64, ptr %s4
  %t239 = icmp uge i64 %t238, 16
  br i1 %t239, label %bb237, label %bb236
bb236:
  %t240 = add i64 %t238, 16
  %t241 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t240
  %t242 = load float, ptr %t241
  %t243 = add i64 %t234, %t238
  %t244 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t243
  store float %t242, ptr %t244
  %t245 = add i64 %t238, 1
  store i64 %t245, ptr %s4
  br label %bb235
bb237:
  %t246 = add i64 %t223, 1024
  store i64 0, ptr %s4
  br label %bb247
bb247:
  %t250 = load i64, ptr %s4
  %t251 = icmp uge i64 %t250, 16
  br i1 %t251, label %bb249, label %bb248
bb248:
  %t252 = add i64 %t250, 32
  %t253 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t252
  %t254 = load float, ptr %t253
  %t255 = add i64 %t246, %t250
  %t256 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t255
  store float %t254, ptr %t256
  %t257 = add i64 %t250, 1
  store i64 %t257, ptr %s4
  br label %bb247
bb249:
  %t258 = add i64 %t223, 1536
  store i64 0, ptr %s4
  br label %bb259
bb259:
  %t262 = load i64, ptr %s4
  %t263 = icmp uge i64 %t262, 16
  br i1 %t263, label %bb261, label %bb260
bb260:
  %t264 = add i64 %t262, 48
  %t265 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t264
  %t266 = load float, ptr %t265
  %t267 = add i64 %t258, %t262
  %t268 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t267
  store float %t266, ptr %t268
  %t269 = add i64 %t262, 1
  store i64 %t269, ptr %s4
  br label %bb259
bb261:
  %t270 = add i64 %t141, 16
  store i64 %t270, ptr %s2
  br label %bb136
bb138:
  %t271 = icmp ult i64 %t141, 512
  br i1 %t271, label %bb139, label %bb140
bb139:
  %t272 = sub i64 512, %t141
  %t273 = icmp ult i64 %t272, 16
  %t274 = select i1 %t273, i64 %t272, i64 16
  store i64 0, ptr %s4
  br label %bb275
bb275:
  %t278 = load i64, ptr %s4
  %t279 = icmp uge i64 %t278, %t274
  br i1 %t279, label %bb277, label %bb276
bb276:
  %t280 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t278
  store float 0x0000000000000000, ptr %t280
  %t281 = add i64 %t278, 1
  store i64 %t281, ptr %s4
  br label %bb275
bb277:
  store i64 0, ptr %s4
  br label %bb282
bb282:
  %t285 = load i64, ptr %s4
  %t286 = icmp uge i64 %t285, %t274
  br i1 %t286, label %bb284, label %bb283
bb283:
  %t287 = add i64 %t285, 16
  %t288 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t287
  store float 0x0000000000000000, ptr %t288
  %t289 = add i64 %t285, 1
  store i64 %t289, ptr %s4
  br label %bb282
bb284:
  store i64 0, ptr %s4
  br label %bb290
bb290:
  %t293 = load i64, ptr %s4
  %t294 = icmp uge i64 %t293, %t274
  br i1 %t294, label %bb292, label %bb291
bb291:
  %t295 = add i64 %t293, 32
  %t296 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t295
  store float 0x0000000000000000, ptr %t296
  %t297 = add i64 %t293, 1
  store i64 %t297, ptr %s4
  br label %bb290
bb292:
  store i64 0, ptr %s4
  br label %bb298
bb298:
  %t301 = load i64, ptr %s4
  %t302 = icmp uge i64 %t301, %t274
  br i1 %t302, label %bb300, label %bb299
bb299:
  %t303 = add i64 %t301, 48
  %t304 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t303
  store float 0x0000000000000000, ptr %t304
  %t305 = add i64 %t301, 1
  store i64 %t305, ptr %s4
  br label %bb298
bb300:
  store i64 0, ptr %s3
  br label %bb306
bb306:
  %t312 = load i64, ptr %s3
  %t313 = icmp uge i64 %t312, 512
  br i1 %t313, label %bb308, label %bb307
bb307:
  %t314 = add i64 %t129, %t312
  %t315 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t314
  %t316 = load float, ptr %t315
  %t317 = add i64 %t131, %t312
  %t318 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t317
  %t319 = load float, ptr %t318
  %t320 = add i64 %t133, %t312
  %t321 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t320
  %t322 = load float, ptr %t321
  %t323 = add i64 %t135, %t312
  %t324 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t323
  %t325 = load float, ptr %t324
  %t326 = mul i64 %t312, 512
  %t327 = add i64 %t326, %t141
  store i64 0, ptr %s4
  br label %bb309
bb309:
  %t328 = load i64, ptr %s4
  %t329 = icmp uge i64 %t328, %t274
  br i1 %t329, label %bb311, label %bb310
bb310:
  %t330 = add i64 %t327, %t328
  %t331 = getelementptr [262144 x float], ptr %o5, i64 0, i64 %t330
  %t332 = load float, ptr %t331
  %t333 = fmul float %t316, %t332
  %t334 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t328
  %t335 = load float, ptr %t334
  %t336 = fadd float %t335, %t333
  store float %t336, ptr %t334
  %t337 = fmul float %t319, %t332
  %t338 = add i64 %t328, 16
  %t339 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t338
  %t340 = load float, ptr %t339
  %t341 = fadd float %t340, %t337
  store float %t341, ptr %t339
  %t342 = fmul float %t322, %t332
  %t343 = add i64 %t328, 32
  %t344 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t343
  %t345 = load float, ptr %t344
  %t346 = fadd float %t345, %t342
  store float %t346, ptr %t344
  %t347 = fmul float %t325, %t332
  %t348 = add i64 %t328, 48
  %t349 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t348
  %t350 = load float, ptr %t349
  %t351 = fadd float %t350, %t347
  store float %t351, ptr %t349
  %t352 = add i64 %t328, 1
  store i64 %t352, ptr %s4
  br label %bb309
bb311:
  %t353 = add i64 %t312, 1
  store i64 %t353, ptr %s3
  br label %bb306
bb308:
  %t354 = add i64 %t128, %t141
  store i64 0, ptr %s4
  br label %bb355
bb355:
  %t358 = load i64, ptr %s4
  %t359 = icmp uge i64 %t358, %t274
  br i1 %t359, label %bb357, label %bb356
bb356:
  %t360 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t358
  %t361 = load float, ptr %t360
  %t362 = add i64 %t354, %t358
  %t363 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t362
  store float %t361, ptr %t363
  %t364 = add i64 %t358, 1
  store i64 %t364, ptr %s4
  br label %bb355
bb357:
  %t365 = add i64 %t354, 512
  store i64 0, ptr %s4
  br label %bb366
bb366:
  %t369 = load i64, ptr %s4
  %t370 = icmp uge i64 %t369, %t274
  br i1 %t370, label %bb368, label %bb367
bb367:
  %t371 = add i64 %t369, 16
  %t372 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t371
  %t373 = load float, ptr %t372
  %t374 = add i64 %t365, %t369
  %t375 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t374
  store float %t373, ptr %t375
  %t376 = add i64 %t369, 1
  store i64 %t376, ptr %s4
  br label %bb366
bb368:
  %t377 = add i64 %t354, 1024
  store i64 0, ptr %s4
  br label %bb378
bb378:
  %t381 = load i64, ptr %s4
  %t382 = icmp uge i64 %t381, %t274
  br i1 %t382, label %bb380, label %bb379
bb379:
  %t383 = add i64 %t381, 32
  %t384 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t383
  %t385 = load float, ptr %t384
  %t386 = add i64 %t377, %t381
  %t387 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t386
  store float %t385, ptr %t387
  %t388 = add i64 %t381, 1
  store i64 %t388, ptr %s4
  br label %bb378
bb380:
  %t389 = add i64 %t354, 1536
  store i64 0, ptr %s4
  br label %bb390
bb390:
  %t393 = load i64, ptr %s4
  %t394 = icmp uge i64 %t393, %t274
  br i1 %t394, label %bb392, label %bb391
bb391:
  %t395 = add i64 %t393, 48
  %t396 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t395
  %t397 = load float, ptr %t396
  %t398 = add i64 %t389, %t393
  %t399 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t398
  store float %t397, ptr %t399
  %t400 = add i64 %t393, 1
  store i64 %t400, ptr %s4
  br label %bb390
bb392:
  br label %bb140
bb140:
  store i64 %t126, ptr %s1
  br label %bb122
bb124:
  br label %bb401
bb401:
  %t404 = load i64, ptr %s1
  %t405 = icmp uge i64 %t404, %t7
  br i1 %t405, label %bb403, label %bb402
bb402:
  %t406 = mul i64 %t404, 512
  %t407 = sub i64 %lo, %t406
  %t408 = icmp slt i64 %t407, 0
  %t409 = select i1 %t408, i64 0, i64 %t407
  %t410 = sub i64 %hi, %t406
  %t411 = icmp sgt i64 %t410, 512
  %t412 = select i1 %t411, i64 512, i64 %t410
  %t413 = mul i64 %t404, 512
  store i64 %t409, ptr %s2
  br label %bb414
bb414:
  %t419 = load i64, ptr %s2
  %t420 = add i64 %t419, 16
  %t421 = icmp ule i64 %t420, %t412
  br i1 %t421, label %bb415, label %bb416
bb415:
  store i64 0, ptr %s4
  br label %bb422
bb422:
  %t425 = load i64, ptr %s4
  %t426 = icmp uge i64 %t425, 16
  br i1 %t426, label %bb424, label %bb423
bb423:
  %t427 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t425
  store float 0x0000000000000000, ptr %t427
  %t428 = add i64 %t425, 1
  store i64 %t428, ptr %s4
  br label %bb422
bb424:
  store i64 0, ptr %s3
  br label %bb429
bb429:
  %t435 = load i64, ptr %s3
  %t436 = icmp uge i64 %t435, 512
  br i1 %t436, label %bb431, label %bb430
bb430:
  %t437 = add i64 %t413, %t435
  %t438 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t437
  %t439 = load float, ptr %t438
  %t440 = mul i64 %t435, 512
  %t441 = add i64 %t440, %t419
  store i64 0, ptr %s4
  br label %bb432
bb432:
  %t442 = load i64, ptr %s4
  %t443 = icmp uge i64 %t442, 16
  br i1 %t443, label %bb434, label %bb433
bb433:
  %t444 = add i64 %t441, %t442
  %t445 = getelementptr [262144 x float], ptr %o5, i64 0, i64 %t444
  %t446 = load float, ptr %t445
  %t447 = fmul float %t439, %t446
  %t448 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t442
  %t449 = load float, ptr %t448
  %t450 = fadd float %t449, %t447
  store float %t450, ptr %t448
  %t451 = add i64 %t442, 1
  store i64 %t451, ptr %s4
  br label %bb432
bb434:
  %t452 = add i64 %t435, 1
  store i64 %t452, ptr %s3
  br label %bb429
bb431:
  %t453 = add i64 %t406, %t419
  store i64 0, ptr %s4
  br label %bb454
bb454:
  %t457 = load i64, ptr %s4
  %t458 = icmp uge i64 %t457, 16
  br i1 %t458, label %bb456, label %bb455
bb455:
  %t459 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t457
  %t460 = load float, ptr %t459
  %t461 = add i64 %t453, %t457
  %t462 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t461
  store float %t460, ptr %t462
  %t463 = add i64 %t457, 1
  store i64 %t463, ptr %s4
  br label %bb454
bb456:
  %t464 = add i64 %t419, 16
  store i64 %t464, ptr %s2
  br label %bb414
bb416:
  %t465 = icmp ult i64 %t419, %t412
  br i1 %t465, label %bb417, label %bb418
bb417:
  %t466 = sub i64 %t412, %t419
  %t467 = icmp ult i64 %t466, 16
  %t468 = select i1 %t467, i64 %t466, i64 16
  store i64 0, ptr %s4
  br label %bb469
bb469:
  %t472 = load i64, ptr %s4
  %t473 = icmp uge i64 %t472, %t468
  br i1 %t473, label %bb471, label %bb470
bb470:
  %t474 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t472
  store float 0x0000000000000000, ptr %t474
  %t475 = add i64 %t472, 1
  store i64 %t475, ptr %s4
  br label %bb469
bb471:
  store i64 0, ptr %s3
  br label %bb476
bb476:
  %t482 = load i64, ptr %s3
  %t483 = icmp uge i64 %t482, 512
  br i1 %t483, label %bb478, label %bb477
bb477:
  %t484 = add i64 %t413, %t482
  %t485 = getelementptr [262144 x float], ptr %o4, i64 0, i64 %t484
  %t486 = load float, ptr %t485
  %t487 = mul i64 %t482, 512
  %t488 = add i64 %t487, %t419
  store i64 0, ptr %s4
  br label %bb479
bb479:
  %t489 = load i64, ptr %s4
  %t490 = icmp uge i64 %t489, %t468
  br i1 %t490, label %bb481, label %bb480
bb480:
  %t491 = add i64 %t488, %t489
  %t492 = getelementptr [262144 x float], ptr %o5, i64 0, i64 %t491
  %t493 = load float, ptr %t492
  %t494 = fmul float %t486, %t493
  %t495 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t489
  %t496 = load float, ptr %t495
  %t497 = fadd float %t496, %t494
  store float %t497, ptr %t495
  %t498 = add i64 %t489, 1
  store i64 %t498, ptr %s4
  br label %bb479
bb481:
  %t499 = add i64 %t482, 1
  store i64 %t499, ptr %s3
  br label %bb476
bb478:
  %t500 = add i64 %t406, %t419
  store i64 0, ptr %s4
  br label %bb501
bb501:
  %t504 = load i64, ptr %s4
  %t505 = icmp uge i64 %t504, %t468
  br i1 %t505, label %bb503, label %bb502
bb502:
  %t506 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t504
  %t507 = load float, ptr %t506
  %t508 = add i64 %t500, %t504
  %t509 = getelementptr [262144 x float], ptr %o7, i64 0, i64 %t508
  store float %t507, ptr %t509
  %t510 = add i64 %t504, 1
  store i64 %t510, ptr %s4
  br label %bb501
bb503:
  br label %bb418
bb418:
  %t511 = add i64 %t404, 1
  store i64 %t511, ptr %s1
  br label %bb401
bb403:
  ret void
}

define internal void @flow_main() {
entry:
  %frame = alloca %Frame
  %o2 = getelementptr %Frame, ptr %frame, i32 0, i32 0
  %o3 = getelementptr %Frame, ptr %frame, i32 0, i32 1
  %o4 = getelementptr %Frame, ptr %frame, i32 0, i32 2
  %o5 = getelementptr %Frame, ptr %frame, i32 0, i32 3
  %o6 = getelementptr %Frame, ptr %frame, i32 0, i32 4
  %o7 = getelementptr %Frame, ptr %frame, i32 0, i32 5
  %o8 = getelementptr %Frame, ptr %frame, i32 0, i32 6
  %o9 = getelementptr %Frame, ptr %frame, i32 0, i32 7
  %o10 = getelementptr %Frame, ptr %frame, i32 0, i32 8
  %o11 = getelementptr %Frame, ptr %frame, i32 0, i32 9
  %o12 = getelementptr %Frame, ptr %frame, i32 0, i32 10
  %o14 = getelementptr %Frame, ptr %frame, i32 0, i32 11
  %h = call ptr @flow_par_begin(i32 7)
  call void @flow_par_task(ptr %h, i32 0, i32 1, ptr @task0, i64 262144, i32 786434)
  call void @flow_par_task(ptr %h, i32 1, i32 1, ptr @task1, i64 512, i32 262658)
  call void @flow_par_task(ptr %h, i32 2, i32 0, ptr @task2, i64 6, i32 1)
  call void @flow_par_task(ptr %h, i32 3, i32 1, ptr @task3, i64 262144, i32 524290)
  call void @flow_par_task(ptr %h, i32 4, i32 1, ptr @task4, i64 262144, i32 524290)
  call void @flow_par_task(ptr %h, i32 5, i32 0, ptr @task5, i64 4, i32 262146)
  call void @flow_par_task(ptr %h, i32 6, i32 1, ptr @task6, i64 262144, i32 262145)
  call void @flow_par_dep(ptr %h, i32 6, i32 2)
  call void @flow_par_dep(ptr %h, i32 0, i32 3)
  call void @flow_par_dep(ptr %h, i32 0, i32 4)
  call void @flow_par_dep(ptr %h, i32 0, i32 5)
  call void @flow_par_dep(ptr %h, i32 1, i32 5)
  call void @flow_par_dep(ptr %h, i32 3, i32 5)
  call void @flow_par_dep(ptr %h, i32 4, i32 5)
  call void @flow_par_dep(ptr %h, i32 5, i32 6)
  call void @flow_par_launch(ptr %h, ptr %frame)
  call void @flow_par_wait(ptr %h, ptr @ckpt0_entries, i32 4)
  call void @flow_par_check(ptr %h, i64 18)
  %t0 = load float, ptr %o9
  store float %t0, ptr %o12
  call void @flow_par_wait(ptr %h, ptr @ckpt1_entries, i32 4)
  call void @flow_par_check(ptr %h, i64 20)
  %t1 = load float, ptr %o11
  store float %t1, ptr %o14
  %t2 = load float, ptr %o12
  call void @flow_print_f32(float %t2, i1 zeroext true)
  %t3 = load float, ptr %o14
  call void @flow_print_f32(float %t3, i1 zeroext true)
  call void @flow_par_finish(ptr %h)
  ret void
}

define internal float @fn1(i32 %arg) readonly nounwind willreturn {
entry:
  %o0 = alloca i32
  %o1 = alloca float
  %o2 = alloca { i32, i32 }
  %o3 = alloca i32
  %o4 = alloca { i32, i32 }
  %o5 = alloca i32
  %o6 = alloca { i32, i32 }
  %o7 = alloca i32
  %o8 = alloca { i32, i32 }
  %o9 = alloca i32
  store i32 %arg, ptr %o0
  %t0 = load i32, ptr %o0
  %t1 = getelementptr { i32, i32 }, ptr %o2, i32 0, i32 0
  store i32 %t0, ptr %t1
  %t2 = getelementptr { i32, i32 }, ptr %o2, i32 0, i32 1
  store i32 7, ptr %t2
  %t3 = getelementptr { i32, i32 }, ptr %o4, i32 0, i32 1
  store i32 13, ptr %t3
  %t4 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 1
  store i32 101, ptr %t4
  %t5 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  store i32 50, ptr %t5
  %t6 = getelementptr { i32, i32 }, ptr %o2, i32 0, i32 0
  %t7 = load i32, ptr %t6
  %t8 = getelementptr { i32, i32 }, ptr %o2, i32 0, i32 1
  %t9 = load i32, ptr %t8
  %t10 = mul i32 %t7, %t9
  store i32 %t10, ptr %o3
  %t11 = load i32, ptr %o3
  %t12 = getelementptr { i32, i32 }, ptr %o4, i32 0, i32 0
  store i32 %t11, ptr %t12
  %t13 = getelementptr { i32, i32 }, ptr %o4, i32 0, i32 0
  %t14 = load i32, ptr %t13
  %t15 = getelementptr { i32, i32 }, ptr %o4, i32 0, i32 1
  %t16 = load i32, ptr %t15
  %t17 = add i32 %t14, %t16
  store i32 %t17, ptr %o5
  %t18 = load i32, ptr %o5
  %t19 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 0
  store i32 %t18, ptr %t19
  %t20 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 0
  %t21 = load i32, ptr %t20
  %t22 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 1
  %t23 = load i32, ptr %t22
  %t24 = srem i32 %t21, %t23
  store i32 %t24, ptr %o7
  %t25 = load i32, ptr %o7
  %t26 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  store i32 %t25, ptr %t26
  %t27 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  %t28 = load i32, ptr %t27
  %t29 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  %t30 = load i32, ptr %t29
  %t31 = sub i32 %t28, %t30
  store i32 %t31, ptr %o9
  %t32 = load i32, ptr %o9
  %t33 = sitofp i32 %t32 to float
  store float %t33, ptr %o1
  %t34 = load float, ptr %o1
  ret float %t34
}

define internal float @fn2(i32 %arg) readonly nounwind willreturn {
entry:
  %o0 = alloca i32
  %o1 = alloca float
  %o2 = alloca { i32, i32 }
  %o3 = alloca i32
  %o4 = alloca { i32, i32 }
  %o5 = alloca i32
  %o6 = alloca { i32, i32 }
  %o7 = alloca i32
  %o8 = alloca { i32, i32 }
  %o9 = alloca i32
  store i32 %arg, ptr %o0
  %t0 = load i32, ptr %o0
  %t1 = getelementptr { i32, i32 }, ptr %o2, i32 0, i32 0
  store i32 %t0, ptr %t1
  %t2 = getelementptr { i32, i32 }, ptr %o2, i32 0, i32 1
  store i32 7, ptr %t2
  %t3 = getelementptr { i32, i32 }, ptr %o4, i32 0, i32 1
  store i32 57, ptr %t3
  %t4 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 1
  store i32 101, ptr %t4
  %t5 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  store i32 50, ptr %t5
  %t6 = getelementptr { i32, i32 }, ptr %o2, i32 0, i32 0
  %t7 = load i32, ptr %t6
  %t8 = getelementptr { i32, i32 }, ptr %o2, i32 0, i32 1
  %t9 = load i32, ptr %t8
  %t10 = mul i32 %t7, %t9
  store i32 %t10, ptr %o3
  %t11 = load i32, ptr %o3
  %t12 = getelementptr { i32, i32 }, ptr %o4, i32 0, i32 0
  store i32 %t11, ptr %t12
  %t13 = getelementptr { i32, i32 }, ptr %o4, i32 0, i32 0
  %t14 = load i32, ptr %t13
  %t15 = getelementptr { i32, i32 }, ptr %o4, i32 0, i32 1
  %t16 = load i32, ptr %t15
  %t17 = add i32 %t14, %t16
  store i32 %t17, ptr %o5
  %t18 = load i32, ptr %o5
  %t19 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 0
  store i32 %t18, ptr %t19
  %t20 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 0
  %t21 = load i32, ptr %t20
  %t22 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 1
  %t23 = load i32, ptr %t22
  %t24 = srem i32 %t21, %t23
  store i32 %t24, ptr %o7
  %t25 = load i32, ptr %o7
  %t26 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  store i32 %t25, ptr %t26
  %t27 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  %t28 = load i32, ptr %t27
  %t29 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  %t30 = load i32, ptr %t29
  %t31 = sub i32 %t28, %t30
  store i32 %t31, ptr %o9
  %t32 = load i32, ptr %o9
  %t33 = sitofp i32 %t32 to float
  store float %t33, ptr %o1
  %t34 = load float, ptr %o1
  ret float %t34
}

define internal float @fn3({ ptr, i32, ptr, i32, float, i32 } %arg) readonly nounwind willreturn {
entry:
  %o0 = alloca { ptr, i32, ptr, i32, float, i32 }
  %o1 = alloca float
  %o2 = alloca ptr
  %o3 = alloca i32
  %o4 = alloca ptr
  %o5 = alloca i32
  %o6 = alloca float
  %o7 = alloca i32
  %o8 = alloca { i32, i32 }
  %o9 = alloca i32
  %o10 = alloca { i32, i32 }
  %o11 = alloca i32
  %o12 = alloca { i32, i32 }
  %o13 = alloca i32
  %o14 = alloca { i32, i32 }
  %o15 = alloca i32
  %o16 = alloca { ptr, i32 }
  %o17 = alloca float
  %o18 = alloca { ptr, i32 }
  %o19 = alloca float
  %o20 = alloca { float, float }
  %o21 = alloca float
  %o22 = alloca { float, float }
  store { ptr, i32, ptr, i32, float, i32 } %arg, ptr %o0
  %t0 = getelementptr { ptr, i32, ptr, i32, float, i32 }, ptr %o0, i32 0, i32 0
  %t1 = load ptr, ptr %t0
  store ptr %t1, ptr %o2
  %t2 = getelementptr { ptr, i32, ptr, i32, float, i32 }, ptr %o0, i32 0, i32 1
  %t3 = load i32, ptr %t2
  store i32 %t3, ptr %o3
  %t4 = getelementptr { ptr, i32, ptr, i32, float, i32 }, ptr %o0, i32 0, i32 2
  %t5 = load ptr, ptr %t4
  store ptr %t5, ptr %o4
  %t6 = getelementptr { ptr, i32, ptr, i32, float, i32 }, ptr %o0, i32 0, i32 3
  %t7 = load i32, ptr %t6
  store i32 %t7, ptr %o5
  %t8 = getelementptr { ptr, i32, ptr, i32, float, i32 }, ptr %o0, i32 0, i32 4
  %t9 = load float, ptr %t8
  store float %t9, ptr %o6
  %t10 = getelementptr { ptr, i32, ptr, i32, float, i32 }, ptr %o0, i32 0, i32 5
  %t11 = load i32, ptr %t10
  store i32 %t11, ptr %o7
  %t12 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  store i32 512, ptr %t12
  %t13 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  store i32 512, ptr %t13
  %t14 = load ptr, ptr %o2
  %t15 = getelementptr { ptr, i32 }, ptr %o16, i32 0, i32 0
  store ptr %t14, ptr %t15
  %t16 = load i32, ptr %o3
  %t17 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  store i32 %t16, ptr %t17
  %t18 = load ptr, ptr %o4
  %t19 = getelementptr { ptr, i32 }, ptr %o18, i32 0, i32 0
  store ptr %t18, ptr %t19
  %t20 = load i32, ptr %o5
  %t21 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 1
  store i32 %t20, ptr %t21
  %t22 = load float, ptr %o6
  %t23 = getelementptr { float, float }, ptr %o22, i32 0, i32 0
  store float %t22, ptr %t23
  %t24 = load i32, ptr %o7
  %t25 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 0
  store i32 %t24, ptr %t25
  %t26 = load i32, ptr %o7
  %t27 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 1
  store i32 %t26, ptr %t27
  %t28 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  %t29 = load i32, ptr %t28
  %t30 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  %t31 = load i32, ptr %t30
  %t32 = mul i32 %t29, %t31
  store i32 %t32, ptr %o9
  %t33 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 0
  %t34 = load i32, ptr %t33
  %t35 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  %t36 = load i32, ptr %t35
  %t37 = mul i32 %t34, %t36
  store i32 %t37, ptr %o11
  %t38 = load i32, ptr %o9
  %t39 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 0
  store i32 %t38, ptr %t39
  %t40 = load i32, ptr %o11
  %t41 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 0
  store i32 %t40, ptr %t41
  %t42 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 0
  %t43 = load i32, ptr %t42
  %t44 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 1
  %t45 = load i32, ptr %t44
  %t46 = add i32 %t43, %t45
  store i32 %t46, ptr %o13
  %t47 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 0
  %t48 = load i32, ptr %t47
  %t49 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 1
  %t50 = load i32, ptr %t49
  %t51 = add i32 %t48, %t50
  store i32 %t51, ptr %o15
  %t52 = load i32, ptr %o13
  %t53 = getelementptr { ptr, i32 }, ptr %o16, i32 0, i32 1
  store i32 %t52, ptr %t53
  %t54 = load i32, ptr %o15
  %t55 = getelementptr { ptr, i32 }, ptr %o18, i32 0, i32 1
  store i32 %t54, ptr %t55
  %t56 = load ptr, ptr %o2
  %t57 = getelementptr { ptr, i32 }, ptr %o16, i32 0, i32 1
  %t58 = load i32, ptr %t57
  %t59 = sext i32 %t58 to i64
  %t60 = getelementptr [262144 x float], ptr %t56, i64 0, i64 %t59
  %t61 = load float, ptr %t60
  store float %t61, ptr %o17
  %t62 = load ptr, ptr %o4
  %t63 = getelementptr { ptr, i32 }, ptr %o18, i32 0, i32 1
  %t64 = load i32, ptr %t63
  %t65 = sext i32 %t64 to i64
  %t66 = getelementptr [262144 x float], ptr %t62, i64 0, i64 %t65
  %t67 = load float, ptr %t66
  store float %t67, ptr %o19
  %t68 = load float, ptr %o17
  %t69 = getelementptr { float, float }, ptr %o20, i32 0, i32 0
  store float %t68, ptr %t69
  %t70 = load float, ptr %o19
  %t71 = getelementptr { float, float }, ptr %o20, i32 0, i32 1
  store float %t70, ptr %t71
  %t72 = getelementptr { float, float }, ptr %o20, i32 0, i32 0
  %t73 = load float, ptr %t72
  %t74 = getelementptr { float, float }, ptr %o20, i32 0, i32 1
  %t75 = load float, ptr %t74
  %t76 = fmul float %t73, %t75
  store float %t76, ptr %o21
  %t77 = load float, ptr %o21
  %t78 = getelementptr { float, float }, ptr %o22, i32 0, i32 1
  store float %t77, ptr %t78
  %t79 = getelementptr { float, float }, ptr %o22, i32 0, i32 0
  %t80 = load float, ptr %t79
  %t81 = getelementptr { float, float }, ptr %o22, i32 0, i32 1
  %t82 = load float, ptr %t81
  %t83 = fadd float %t80, %t82
  store float %t83, ptr %o1
  %t84 = load float, ptr %o1
  ret float %t84
}

define internal float @fn4({ ptr, ptr, ptr, i32 } %arg) readonly nounwind willreturn {
entry:
  %o0 = alloca { ptr, ptr, ptr, i32 }
  %o1 = alloca float
  %o2 = alloca ptr
  %o3 = alloca ptr
  %o4 = alloca ptr
  %o5 = alloca i32
  %o6 = alloca { i32, i32 }
  %o7 = alloca i32
  %o8 = alloca { i32, i32 }
  %o9 = alloca i32
  %o10 = alloca { ptr, i32, ptr, i32, float, ptr }
  %s38 = alloca float
  %s39 = alloca i64
  %s48 = alloca { ptr, i32, ptr, i32, float, i32 }
  store { ptr, ptr, ptr, i32 } %arg, ptr %o0
  %t0 = getelementptr { ptr, ptr, ptr, i32 }, ptr %o0, i32 0, i32 0
  %t1 = load ptr, ptr %t0
  store ptr %t1, ptr %o2
  %t2 = getelementptr { ptr, ptr, ptr, i32 }, ptr %o0, i32 0, i32 1
  %t3 = load ptr, ptr %t2
  store ptr %t3, ptr %o3
  %t4 = getelementptr { ptr, ptr, ptr, i32 }, ptr %o0, i32 0, i32 2
  %t5 = load ptr, ptr %t4
  store ptr %t5, ptr %o4
  %t6 = getelementptr { ptr, ptr, ptr, i32 }, ptr %o0, i32 0, i32 3
  %t7 = load i32, ptr %t6
  store i32 %t7, ptr %o5
  %t8 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 1
  store i32 512, ptr %t8
  %t9 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  store i32 512, ptr %t9
  %t10 = getelementptr { ptr, i32, ptr, i32, float, ptr }, ptr %o10, i32 0, i32 4
  store float 0x0000000000000000, ptr %t10
  %t11 = load ptr, ptr %o2
  %t12 = getelementptr { ptr, i32, ptr, i32, float, ptr }, ptr %o10, i32 0, i32 5
  store ptr %t11, ptr %t12
  %t13 = load ptr, ptr %o3
  %t14 = getelementptr { ptr, i32, ptr, i32, float, ptr }, ptr %o10, i32 0, i32 0
  store ptr %t13, ptr %t14
  %t15 = load ptr, ptr %o4
  %t16 = getelementptr { ptr, i32, ptr, i32, float, ptr }, ptr %o10, i32 0, i32 2
  store ptr %t15, ptr %t16
  %t17 = load i32, ptr %o5
  %t18 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 0
  store i32 %t17, ptr %t18
  %t19 = load i32, ptr %o5
  %t20 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  store i32 %t19, ptr %t20
  %t21 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 0
  %t22 = load i32, ptr %t21
  %t23 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 1
  %t24 = load i32, ptr %t23
  %t25 = sdiv i32 %t22, %t24
  store i32 %t25, ptr %o7
  %t26 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  %t27 = load i32, ptr %t26
  %t28 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  %t29 = load i32, ptr %t28
  %t30 = srem i32 %t27, %t29
  store i32 %t30, ptr %o9
  %t31 = load i32, ptr %o7
  %t32 = getelementptr { ptr, i32, ptr, i32, float, ptr }, ptr %o10, i32 0, i32 1
  store i32 %t31, ptr %t32
  %t33 = load i32, ptr %o9
  %t34 = getelementptr { ptr, i32, ptr, i32, float, ptr }, ptr %o10, i32 0, i32 3
  store i32 %t33, ptr %t34
  %t35 = getelementptr { ptr, i32, ptr, i32, float, ptr }, ptr %o10, i32 0, i32 4
  %t36 = load float, ptr %t35
  %t37 = load ptr, ptr %o2
  store float %t36, ptr %s38
  store i64 0, ptr %s39
  br label %bb40
bb40:
  %t43 = load i64, ptr %s39
  %t44 = icmp uge i64 %t43, 512
  br i1 %t44, label %bb42, label %bb41
bb41:
  %t45 = getelementptr [512 x i32], ptr %t37, i64 0, i64 %t43
  %t46 = load i32, ptr %t45
  %t47 = load float, ptr %s38
  %t49 = load ptr, ptr %o3
  %t50 = getelementptr { ptr, i32, ptr, i32, float, i32 }, ptr %s48, i32 0, i32 0
  store ptr %t49, ptr %t50
  %t51 = getelementptr { ptr, i32, ptr, i32, float, ptr }, ptr %o10, i32 0, i32 1
  %t52 = load i32, ptr %t51
  %t53 = getelementptr { ptr, i32, ptr, i32, float, i32 }, ptr %s48, i32 0, i32 1
  store i32 %t52, ptr %t53
  %t54 = load ptr, ptr %o4
  %t55 = getelementptr { ptr, i32, ptr, i32, float, i32 }, ptr %s48, i32 0, i32 2
  store ptr %t54, ptr %t55
  %t56 = getelementptr { ptr, i32, ptr, i32, float, ptr }, ptr %o10, i32 0, i32 3
  %t57 = load i32, ptr %t56
  %t58 = getelementptr { ptr, i32, ptr, i32, float, i32 }, ptr %s48, i32 0, i32 3
  store i32 %t57, ptr %t58
  %t59 = getelementptr { ptr, i32, ptr, i32, float, i32 }, ptr %s48, i32 0, i32 4
  store float %t47, ptr %t59
  %t60 = getelementptr { ptr, i32, ptr, i32, float, i32 }, ptr %s48, i32 0, i32 5
  store i32 %t46, ptr %t60
  %t61 = load { ptr, i32, ptr, i32, float, i32 }, ptr %s48
  %t62 = call float @fn3({ ptr, i32, ptr, i32, float, i32 } %t61)
  store float %t62, ptr %s38
  %t63 = add i64 %t43, 1
  store i64 %t63, ptr %s39
  br label %bb40
bb42:
  %t64 = load float, ptr %s38
  store float %t64, ptr %o1
  %t65 = load float, ptr %o1
  ret float %t65
}

define i32 @main() {
entry:
  call void @flow_main()
  ret i32 0
}

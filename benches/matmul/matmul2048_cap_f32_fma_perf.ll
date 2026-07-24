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
declare void @llvm.prefetch.p0(ptr, i32 immarg, i32 immarg, i32 immarg)
declare void @flow_perf_begin()
declare void @flow_perf_end()
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

%Frame = type { [4194304 x i32], [2048 x i32], [4194304 x float], [4194304 x float], { ptr, ptr, ptr, ptr }, [4194304 x float], { ptr, i32 }, float, { ptr, i32 }, float, float, float, ptr }

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
  %t7 = getelementptr [4194304 x i32], ptr %o2, i64 0, i64 %t4
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
  %t7 = getelementptr [2048 x i32], ptr %o3, i64 0, i64 %t4
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
  store i32 4194303, ptr %t1
  %t2 = getelementptr { ptr, i32 }, ptr %o8, i32 0, i32 0
  store ptr %o7, ptr %t2
  %t3 = getelementptr { ptr, i32 }, ptr %o10, i32 0, i32 0
  store ptr %o7, ptr %t3
  %t4 = getelementptr { ptr, i32 }, ptr %o8, i32 0, i32 1
  %t5 = load i32, ptr %t4
  %t6 = sext i32 %t5 to i64
  %t7 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t6
  %t8 = load float, ptr %t7
  store float %t8, ptr %o9
  %t9 = getelementptr { ptr, i32 }, ptr %o10, i32 0, i32 1
  %t10 = load i32, ptr %t9
  %t11 = sext i32 %t10 to i64
  %t12 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t11
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
  %t6 = getelementptr [4194304 x i32], ptr %o2, i64 0, i64 %t4
  %t7 = load i32, ptr %t6
  %t8 = call float @fn1(i32 %t7)
  %t9 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t4
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
  %t6 = getelementptr [4194304 x i32], ptr %o2, i64 0, i64 %t4
  %t7 = load i32, ptr %t6
  %t8 = call float @fn2(i32 %t7)
  %t9 = getelementptr [4194304 x float], ptr %o5, i64 0, i64 %t4
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

define internal void @task6_slice(i64 %lo, i64 %hi, ptr %frame) {
entry:
  %s0 = alloca [64 x float]
  %s1 = alloca i64
  %s2 = alloca i64
  %s3 = alloca i64
  %s4 = alloca i64
  %pack_field0 = getelementptr %Frame, ptr %frame, i32 0, i32 12
  %packed0 = load ptr, ptr %pack_field0
  %o4 = getelementptr %Frame, ptr %frame, i32 0, i32 2
  %o5 = getelementptr %Frame, ptr %frame, i32 0, i32 3
  %o7 = getelementptr %Frame, ptr %frame, i32 0, i32 5
  %t5 = udiv i64 %lo, 2048
  %t6 = add i64 %hi, 2047
  %t7 = udiv i64 %t6, 2048
  %t8 = add i64 %lo, 2047
  %t9 = udiv i64 %t8, 2048
  %t10 = udiv i64 %hi, 2048
  store i64 0, ptr %s2
  br label %bb11
bb11:
  %t16 = load i64, ptr %s2
  %t17 = add i64 %t16, 16
  %t18 = icmp ule i64 %t17, 2048
  br i1 %t18, label %bb12, label %bb13
bb12:
  %t19 = udiv i64 %t16, 16
  %t20 = mul i64 %t19, 32768
  store i64 %t5, ptr %s1
  br label %bb21
bb21:
  %t24 = load i64, ptr %s1
  %t25 = icmp uge i64 %t24, %t9
  br i1 %t25, label %bb23, label %bb22
bb22:
  %t26 = mul i64 %t24, 2048
  %t27 = sub i64 %lo, %t26
  %t28 = icmp slt i64 %t27, 0
  %t29 = select i1 %t28, i64 0, i64 %t27
  %t30 = sub i64 %hi, %t26
  %t31 = icmp sgt i64 %t30, 2048
  %t32 = select i1 %t31, i64 2048, i64 %t30
  %t33 = add i64 %t16, 16
  %t34 = icmp ult i64 %t29, %t16
  %t35 = select i1 %t34, i64 %t16, i64 %t29
  %t36 = icmp ugt i64 %t32, %t33
  %t37 = select i1 %t36, i64 %t33, i64 %t32
  %t38 = icmp ult i64 %t35, %t37
  br i1 %t38, label %bb39, label %bb40
bb39:
  %t41 = sub i64 %t37, %t35
  %t42 = sub i64 %t35, %t16
  %t43 = mul i64 %t24, 2048
  store i64 0, ptr %s4
  br label %bb44
bb44:
  %t47 = load i64, ptr %s4
  %t48 = icmp uge i64 %t47, %t41
  br i1 %t48, label %bb46, label %bb45
bb45:
  %t49 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t47
  store float 0x0000000000000000, ptr %t49
  %t50 = add i64 %t47, 1
  store i64 %t50, ptr %s4
  br label %bb44
bb46:
  store i64 0, ptr %s3
  br label %bb51
bb51:
  %t54 = load i64, ptr %s3
  %t55 = icmp uge i64 %t54, 2048
  br i1 %t55, label %bb53, label %bb52
bb52:
  %t56 = add i64 %t43, %t54
  %t57 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t56
  %t58 = load float, ptr %t57
  store i64 0, ptr %s4
  br label %bb59
bb59:
  %t62 = load i64, ptr %s4
  %t63 = icmp uge i64 %t62, %t41
  br i1 %t63, label %bb61, label %bb60
bb60:
  %t64 = mul i64 %t54, 16
  %t65 = add i64 %t20, %t64
  %t66 = add i64 %t42, %t62
  %t67 = add i64 %t65, %t66
  %t68 = getelementptr [4194304 x float], ptr %packed0, i64 0, i64 %t67
  %t69 = load float, ptr %t68
  %t70 = fmul contract float %t58, %t69
  %t71 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t62
  %t72 = load float, ptr %t71
  %t73 = fadd contract float %t72, %t70
  store float %t73, ptr %t71
  %t74 = add i64 %t62, 1
  store i64 %t74, ptr %s4
  br label %bb59
bb61:
  %t75 = add i64 %t54, 1
  store i64 %t75, ptr %s3
  br label %bb51
bb53:
  %t76 = add i64 %t26, %t35
  store i64 0, ptr %s4
  br label %bb77
bb77:
  %t80 = load i64, ptr %s4
  %t81 = icmp uge i64 %t80, %t41
  br i1 %t81, label %bb79, label %bb78
bb78:
  %t82 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t80
  %t83 = load float, ptr %t82
  %t84 = add i64 %t76, %t80
  %t85 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t84
  store float %t83, ptr %t85
  %t86 = add i64 %t80, 1
  store i64 %t86, ptr %s4
  br label %bb77
bb79:
  br label %bb40
bb40:
  %t87 = add i64 %t24, 1
  store i64 %t87, ptr %s1
  br label %bb21
bb23:
  store i64 %t9, ptr %s1
  br label %bb88
bb88:
  %t91 = load i64, ptr %s1
  %t92 = add i64 %t91, 4
  %t93 = icmp ule i64 %t92, %t10
  br i1 %t93, label %bb89, label %bb90
bb89:
  %t94 = mul i64 %t91, 2048
  %t95 = mul i64 %t91, 2048
  %t96 = mul i64 %t91, 2048
  %t97 = add i64 2048, %t96
  %t98 = mul i64 %t91, 2048
  %t99 = add i64 4096, %t98
  %t100 = mul i64 %t91, 2048
  %t101 = add i64 6144, %t100
  store i64 0, ptr %s4
  br label %bb102
bb102:
  %t105 = load i64, ptr %s4
  %t106 = icmp uge i64 %t105, 16
  br i1 %t106, label %bb104, label %bb103
bb103:
  %t107 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t105
  store float 0x0000000000000000, ptr %t107
  %t108 = add i64 %t105, 1
  store i64 %t108, ptr %s4
  br label %bb102
bb104:
  store i64 0, ptr %s4
  br label %bb109
bb109:
  %t112 = load i64, ptr %s4
  %t113 = icmp uge i64 %t112, 16
  br i1 %t113, label %bb111, label %bb110
bb110:
  %t114 = add i64 %t112, 16
  %t115 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t114
  store float 0x0000000000000000, ptr %t115
  %t116 = add i64 %t112, 1
  store i64 %t116, ptr %s4
  br label %bb109
bb111:
  store i64 0, ptr %s4
  br label %bb117
bb117:
  %t120 = load i64, ptr %s4
  %t121 = icmp uge i64 %t120, 16
  br i1 %t121, label %bb119, label %bb118
bb118:
  %t122 = add i64 %t120, 32
  %t123 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t122
  store float 0x0000000000000000, ptr %t123
  %t124 = add i64 %t120, 1
  store i64 %t124, ptr %s4
  br label %bb117
bb119:
  store i64 0, ptr %s4
  br label %bb125
bb125:
  %t128 = load i64, ptr %s4
  %t129 = icmp uge i64 %t128, 16
  br i1 %t129, label %bb127, label %bb126
bb126:
  %t130 = add i64 %t128, 48
  %t131 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t130
  store float 0x0000000000000000, ptr %t131
  %t132 = add i64 %t128, 1
  store i64 %t132, ptr %s4
  br label %bb125
bb127:
  store i64 0, ptr %s3
  br label %bb133
bb133:
  %t136 = load i64, ptr %s3
  %t139 = add i64 %t136, 1
  %t140 = icmp ult i64 %t139, 2048
  br i1 %t140, label %bb134, label %bb137
bb134:
  %t141 = add i64 %t95, %t136
  %t142 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t141
  %t143 = load float, ptr %t142
  %t144 = add i64 %t97, %t136
  %t145 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t144
  %t146 = load float, ptr %t145
  %t147 = add i64 %t99, %t136
  %t148 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t147
  %t149 = load float, ptr %t148
  %t150 = add i64 %t101, %t136
  %t151 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t150
  %t152 = load float, ptr %t151
  %t153 = add i64 %t95, %t139
  %t154 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t153
  %t155 = load float, ptr %t154
  %t156 = add i64 %t97, %t139
  %t157 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t156
  %t158 = load float, ptr %t157
  %t159 = add i64 %t99, %t139
  %t160 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t159
  %t161 = load float, ptr %t160
  %t162 = add i64 %t101, %t139
  %t163 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t162
  %t164 = load float, ptr %t163
  %t165 = add i64 %t136, 2
  %t166 = mul i64 %t165, 16
  %t167 = add i64 %t20, %t166
  %t168 = getelementptr [4194304 x float], ptr %packed0, i64 0, i64 %t167
  call void @llvm.prefetch.p0(ptr %t168, i32 0, i32 3, i32 1)
  store i64 0, ptr %s4
  br label %bb169
bb169:
  %t172 = load i64, ptr %s4
  %t173 = icmp uge i64 %t172, 16
  br i1 %t173, label %bb171, label %bb170
bb170:
  %t174 = mul i64 %t136, 16
  %t175 = add i64 %t20, %t174
  %t176 = add i64 %t175, %t172
  %t177 = getelementptr [4194304 x float], ptr %packed0, i64 0, i64 %t176
  %t178 = load float, ptr %t177
  %t179 = fmul contract float %t143, %t178
  %t180 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t172
  %t181 = load float, ptr %t180
  %t182 = fadd contract float %t181, %t179
  store float %t182, ptr %t180
  %t183 = fmul contract float %t146, %t178
  %t184 = add i64 %t172, 16
  %t185 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t184
  %t186 = load float, ptr %t185
  %t187 = fadd contract float %t186, %t183
  store float %t187, ptr %t185
  %t188 = fmul contract float %t149, %t178
  %t189 = add i64 %t172, 32
  %t190 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t189
  %t191 = load float, ptr %t190
  %t192 = fadd contract float %t191, %t188
  store float %t192, ptr %t190
  %t193 = fmul contract float %t152, %t178
  %t194 = add i64 %t172, 48
  %t195 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t194
  %t196 = load float, ptr %t195
  %t197 = fadd contract float %t196, %t193
  store float %t197, ptr %t195
  %t198 = mul i64 %t139, 16
  %t199 = add i64 %t20, %t198
  %t200 = add i64 %t199, %t172
  %t201 = getelementptr [4194304 x float], ptr %packed0, i64 0, i64 %t200
  %t202 = load float, ptr %t201
  %t203 = fmul contract float %t155, %t202
  %t204 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t172
  %t205 = load float, ptr %t204
  %t206 = fadd contract float %t205, %t203
  store float %t206, ptr %t204
  %t207 = fmul contract float %t158, %t202
  %t208 = add i64 %t172, 16
  %t209 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t208
  %t210 = load float, ptr %t209
  %t211 = fadd contract float %t210, %t207
  store float %t211, ptr %t209
  %t212 = fmul contract float %t161, %t202
  %t213 = add i64 %t172, 32
  %t214 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t213
  %t215 = load float, ptr %t214
  %t216 = fadd contract float %t215, %t212
  store float %t216, ptr %t214
  %t217 = fmul contract float %t164, %t202
  %t218 = add i64 %t172, 48
  %t219 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t218
  %t220 = load float, ptr %t219
  %t221 = fadd contract float %t220, %t217
  store float %t221, ptr %t219
  %t222 = add i64 %t172, 1
  store i64 %t222, ptr %s4
  br label %bb169
bb171:
  %t223 = add i64 %t136, 2
  store i64 %t223, ptr %s3
  br label %bb133
bb137:
  %t224 = icmp ult i64 %t136, 2048
  br i1 %t224, label %bb138, label %bb135
bb138:
  %t225 = add i64 %t95, %t136
  %t226 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t225
  %t227 = load float, ptr %t226
  %t228 = add i64 %t97, %t136
  %t229 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t228
  %t230 = load float, ptr %t229
  %t231 = add i64 %t99, %t136
  %t232 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t231
  %t233 = load float, ptr %t232
  %t234 = add i64 %t101, %t136
  %t235 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t234
  %t236 = load float, ptr %t235
  store i64 0, ptr %s4
  br label %bb237
bb237:
  %t240 = load i64, ptr %s4
  %t241 = icmp uge i64 %t240, 16
  br i1 %t241, label %bb239, label %bb238
bb238:
  %t242 = mul i64 %t136, 16
  %t243 = add i64 %t20, %t242
  %t244 = add i64 %t243, %t240
  %t245 = getelementptr [4194304 x float], ptr %packed0, i64 0, i64 %t244
  %t246 = load float, ptr %t245
  %t247 = fmul contract float %t227, %t246
  %t248 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t240
  %t249 = load float, ptr %t248
  %t250 = fadd contract float %t249, %t247
  store float %t250, ptr %t248
  %t251 = fmul contract float %t230, %t246
  %t252 = add i64 %t240, 16
  %t253 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t252
  %t254 = load float, ptr %t253
  %t255 = fadd contract float %t254, %t251
  store float %t255, ptr %t253
  %t256 = fmul contract float %t233, %t246
  %t257 = add i64 %t240, 32
  %t258 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t257
  %t259 = load float, ptr %t258
  %t260 = fadd contract float %t259, %t256
  store float %t260, ptr %t258
  %t261 = fmul contract float %t236, %t246
  %t262 = add i64 %t240, 48
  %t263 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t262
  %t264 = load float, ptr %t263
  %t265 = fadd contract float %t264, %t261
  store float %t265, ptr %t263
  %t266 = add i64 %t240, 1
  store i64 %t266, ptr %s4
  br label %bb237
bb239:
  br label %bb135
bb135:
  %t267 = add i64 %t94, %t16
  store i64 0, ptr %s4
  br label %bb268
bb268:
  %t271 = load i64, ptr %s4
  %t272 = icmp uge i64 %t271, 16
  br i1 %t272, label %bb270, label %bb269
bb269:
  %t273 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t271
  %t274 = load float, ptr %t273
  %t275 = add i64 %t267, %t271
  %t276 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t275
  store float %t274, ptr %t276
  %t277 = add i64 %t271, 1
  store i64 %t277, ptr %s4
  br label %bb268
bb270:
  %t278 = add i64 %t267, 2048
  store i64 0, ptr %s4
  br label %bb279
bb279:
  %t282 = load i64, ptr %s4
  %t283 = icmp uge i64 %t282, 16
  br i1 %t283, label %bb281, label %bb280
bb280:
  %t284 = add i64 %t282, 16
  %t285 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t284
  %t286 = load float, ptr %t285
  %t287 = add i64 %t278, %t282
  %t288 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t287
  store float %t286, ptr %t288
  %t289 = add i64 %t282, 1
  store i64 %t289, ptr %s4
  br label %bb279
bb281:
  %t290 = add i64 %t267, 4096
  store i64 0, ptr %s4
  br label %bb291
bb291:
  %t294 = load i64, ptr %s4
  %t295 = icmp uge i64 %t294, 16
  br i1 %t295, label %bb293, label %bb292
bb292:
  %t296 = add i64 %t294, 32
  %t297 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t296
  %t298 = load float, ptr %t297
  %t299 = add i64 %t290, %t294
  %t300 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t299
  store float %t298, ptr %t300
  %t301 = add i64 %t294, 1
  store i64 %t301, ptr %s4
  br label %bb291
bb293:
  %t302 = add i64 %t267, 6144
  store i64 0, ptr %s4
  br label %bb303
bb303:
  %t306 = load i64, ptr %s4
  %t307 = icmp uge i64 %t306, 16
  br i1 %t307, label %bb305, label %bb304
bb304:
  %t308 = add i64 %t306, 48
  %t309 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t308
  %t310 = load float, ptr %t309
  %t311 = add i64 %t302, %t306
  %t312 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t311
  store float %t310, ptr %t312
  %t313 = add i64 %t306, 1
  store i64 %t313, ptr %s4
  br label %bb303
bb305:
  store i64 %t92, ptr %s1
  br label %bb88
bb90:
  br label %bb314
bb314:
  %t317 = load i64, ptr %s1
  %t318 = icmp uge i64 %t317, %t7
  br i1 %t318, label %bb316, label %bb315
bb315:
  %t319 = mul i64 %t317, 2048
  %t320 = sub i64 %lo, %t319
  %t321 = icmp slt i64 %t320, 0
  %t322 = select i1 %t321, i64 0, i64 %t320
  %t323 = sub i64 %hi, %t319
  %t324 = icmp sgt i64 %t323, 2048
  %t325 = select i1 %t324, i64 2048, i64 %t323
  %t326 = add i64 %t16, 16
  %t327 = icmp ult i64 %t322, %t16
  %t328 = select i1 %t327, i64 %t16, i64 %t322
  %t329 = icmp ugt i64 %t325, %t326
  %t330 = select i1 %t329, i64 %t326, i64 %t325
  %t331 = icmp ult i64 %t328, %t330
  br i1 %t331, label %bb332, label %bb333
bb332:
  %t334 = sub i64 %t330, %t328
  %t335 = sub i64 %t328, %t16
  %t336 = mul i64 %t317, 2048
  store i64 0, ptr %s4
  br label %bb337
bb337:
  %t340 = load i64, ptr %s4
  %t341 = icmp uge i64 %t340, %t334
  br i1 %t341, label %bb339, label %bb338
bb338:
  %t342 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t340
  store float 0x0000000000000000, ptr %t342
  %t343 = add i64 %t340, 1
  store i64 %t343, ptr %s4
  br label %bb337
bb339:
  store i64 0, ptr %s3
  br label %bb344
bb344:
  %t347 = load i64, ptr %s3
  %t348 = icmp uge i64 %t347, 2048
  br i1 %t348, label %bb346, label %bb345
bb345:
  %t349 = add i64 %t336, %t347
  %t350 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t349
  %t351 = load float, ptr %t350
  store i64 0, ptr %s4
  br label %bb352
bb352:
  %t355 = load i64, ptr %s4
  %t356 = icmp uge i64 %t355, %t334
  br i1 %t356, label %bb354, label %bb353
bb353:
  %t357 = mul i64 %t347, 16
  %t358 = add i64 %t20, %t357
  %t359 = add i64 %t335, %t355
  %t360 = add i64 %t358, %t359
  %t361 = getelementptr [4194304 x float], ptr %packed0, i64 0, i64 %t360
  %t362 = load float, ptr %t361
  %t363 = fmul contract float %t351, %t362
  %t364 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t355
  %t365 = load float, ptr %t364
  %t366 = fadd contract float %t365, %t363
  store float %t366, ptr %t364
  %t367 = add i64 %t355, 1
  store i64 %t367, ptr %s4
  br label %bb352
bb354:
  %t368 = add i64 %t347, 1
  store i64 %t368, ptr %s3
  br label %bb344
bb346:
  %t369 = add i64 %t319, %t328
  store i64 0, ptr %s4
  br label %bb370
bb370:
  %t373 = load i64, ptr %s4
  %t374 = icmp uge i64 %t373, %t334
  br i1 %t374, label %bb372, label %bb371
bb371:
  %t375 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t373
  %t376 = load float, ptr %t375
  %t377 = add i64 %t369, %t373
  %t378 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t377
  store float %t376, ptr %t378
  %t379 = add i64 %t373, 1
  store i64 %t379, ptr %s4
  br label %bb370
bb372:
  br label %bb333
bb333:
  %t380 = add i64 %t317, 1
  store i64 %t380, ptr %s1
  br label %bb314
bb316:
  %t381 = add i64 %t16, 16
  store i64 %t381, ptr %s2
  br label %bb11
bb13:
  %t382 = icmp ult i64 %t16, 2048
  br i1 %t382, label %bb14, label %bb15
bb14:
  %t383 = sub i64 2048, %t16
  %t384 = icmp ult i64 %t383, 16
  %t385 = select i1 %t384, i64 %t383, i64 16
  %t386 = udiv i64 %t16, 16
  %t387 = mul i64 %t386, 32768
  store i64 %t5, ptr %s1
  br label %bb388
bb388:
  %t391 = load i64, ptr %s1
  %t392 = icmp uge i64 %t391, %t9
  br i1 %t392, label %bb390, label %bb389
bb389:
  %t393 = mul i64 %t391, 2048
  %t394 = sub i64 %lo, %t393
  %t395 = icmp slt i64 %t394, 0
  %t396 = select i1 %t395, i64 0, i64 %t394
  %t397 = sub i64 %hi, %t393
  %t398 = icmp sgt i64 %t397, 2048
  %t399 = select i1 %t398, i64 2048, i64 %t397
  %t400 = add i64 %t16, %t385
  %t401 = icmp ult i64 %t396, %t16
  %t402 = select i1 %t401, i64 %t16, i64 %t396
  %t403 = icmp ugt i64 %t399, %t400
  %t404 = select i1 %t403, i64 %t400, i64 %t399
  %t405 = icmp ult i64 %t402, %t404
  br i1 %t405, label %bb406, label %bb407
bb406:
  %t408 = sub i64 %t404, %t402
  %t409 = sub i64 %t402, %t16
  %t410 = mul i64 %t391, 2048
  store i64 0, ptr %s4
  br label %bb411
bb411:
  %t414 = load i64, ptr %s4
  %t415 = icmp uge i64 %t414, %t408
  br i1 %t415, label %bb413, label %bb412
bb412:
  %t416 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t414
  store float 0x0000000000000000, ptr %t416
  %t417 = add i64 %t414, 1
  store i64 %t417, ptr %s4
  br label %bb411
bb413:
  store i64 0, ptr %s3
  br label %bb418
bb418:
  %t421 = load i64, ptr %s3
  %t422 = icmp uge i64 %t421, 2048
  br i1 %t422, label %bb420, label %bb419
bb419:
  %t423 = add i64 %t410, %t421
  %t424 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t423
  %t425 = load float, ptr %t424
  store i64 0, ptr %s4
  br label %bb426
bb426:
  %t429 = load i64, ptr %s4
  %t430 = icmp uge i64 %t429, %t408
  br i1 %t430, label %bb428, label %bb427
bb427:
  %t431 = mul i64 %t421, 16
  %t432 = add i64 %t387, %t431
  %t433 = add i64 %t409, %t429
  %t434 = add i64 %t432, %t433
  %t435 = getelementptr [4194304 x float], ptr %packed0, i64 0, i64 %t434
  %t436 = load float, ptr %t435
  %t437 = fmul contract float %t425, %t436
  %t438 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t429
  %t439 = load float, ptr %t438
  %t440 = fadd contract float %t439, %t437
  store float %t440, ptr %t438
  %t441 = add i64 %t429, 1
  store i64 %t441, ptr %s4
  br label %bb426
bb428:
  %t442 = add i64 %t421, 1
  store i64 %t442, ptr %s3
  br label %bb418
bb420:
  %t443 = add i64 %t393, %t402
  store i64 0, ptr %s4
  br label %bb444
bb444:
  %t447 = load i64, ptr %s4
  %t448 = icmp uge i64 %t447, %t408
  br i1 %t448, label %bb446, label %bb445
bb445:
  %t449 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t447
  %t450 = load float, ptr %t449
  %t451 = add i64 %t443, %t447
  %t452 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t451
  store float %t450, ptr %t452
  %t453 = add i64 %t447, 1
  store i64 %t453, ptr %s4
  br label %bb444
bb446:
  br label %bb407
bb407:
  %t454 = add i64 %t391, 1
  store i64 %t454, ptr %s1
  br label %bb388
bb390:
  store i64 %t9, ptr %s1
  br label %bb455
bb455:
  %t458 = load i64, ptr %s1
  %t459 = add i64 %t458, 4
  %t460 = icmp ule i64 %t459, %t10
  br i1 %t460, label %bb456, label %bb457
bb456:
  %t461 = mul i64 %t458, 2048
  %t462 = mul i64 %t458, 2048
  %t463 = mul i64 %t458, 2048
  %t464 = add i64 2048, %t463
  %t465 = mul i64 %t458, 2048
  %t466 = add i64 4096, %t465
  %t467 = mul i64 %t458, 2048
  %t468 = add i64 6144, %t467
  store i64 0, ptr %s4
  br label %bb469
bb469:
  %t472 = load i64, ptr %s4
  %t473 = icmp uge i64 %t472, %t385
  br i1 %t473, label %bb471, label %bb470
bb470:
  %t474 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t472
  store float 0x0000000000000000, ptr %t474
  %t475 = add i64 %t472, 1
  store i64 %t475, ptr %s4
  br label %bb469
bb471:
  store i64 0, ptr %s4
  br label %bb476
bb476:
  %t479 = load i64, ptr %s4
  %t480 = icmp uge i64 %t479, %t385
  br i1 %t480, label %bb478, label %bb477
bb477:
  %t481 = add i64 %t479, 16
  %t482 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t481
  store float 0x0000000000000000, ptr %t482
  %t483 = add i64 %t479, 1
  store i64 %t483, ptr %s4
  br label %bb476
bb478:
  store i64 0, ptr %s4
  br label %bb484
bb484:
  %t487 = load i64, ptr %s4
  %t488 = icmp uge i64 %t487, %t385
  br i1 %t488, label %bb486, label %bb485
bb485:
  %t489 = add i64 %t487, 32
  %t490 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t489
  store float 0x0000000000000000, ptr %t490
  %t491 = add i64 %t487, 1
  store i64 %t491, ptr %s4
  br label %bb484
bb486:
  store i64 0, ptr %s4
  br label %bb492
bb492:
  %t495 = load i64, ptr %s4
  %t496 = icmp uge i64 %t495, %t385
  br i1 %t496, label %bb494, label %bb493
bb493:
  %t497 = add i64 %t495, 48
  %t498 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t497
  store float 0x0000000000000000, ptr %t498
  %t499 = add i64 %t495, 1
  store i64 %t499, ptr %s4
  br label %bb492
bb494:
  store i64 0, ptr %s3
  br label %bb500
bb500:
  %t503 = load i64, ptr %s3
  %t504 = icmp uge i64 %t503, 2048
  br i1 %t504, label %bb502, label %bb501
bb501:
  %t505 = add i64 %t462, %t503
  %t506 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t505
  %t507 = load float, ptr %t506
  %t508 = add i64 %t464, %t503
  %t509 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t508
  %t510 = load float, ptr %t509
  %t511 = add i64 %t466, %t503
  %t512 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t511
  %t513 = load float, ptr %t512
  %t514 = add i64 %t468, %t503
  %t515 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t514
  %t516 = load float, ptr %t515
  store i64 0, ptr %s4
  br label %bb517
bb517:
  %t520 = load i64, ptr %s4
  %t521 = icmp uge i64 %t520, %t385
  br i1 %t521, label %bb519, label %bb518
bb518:
  %t522 = mul i64 %t503, 16
  %t523 = add i64 %t387, %t522
  %t524 = add i64 %t523, %t520
  %t525 = getelementptr [4194304 x float], ptr %packed0, i64 0, i64 %t524
  %t526 = load float, ptr %t525
  %t527 = fmul contract float %t507, %t526
  %t528 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t520
  %t529 = load float, ptr %t528
  %t530 = fadd contract float %t529, %t527
  store float %t530, ptr %t528
  %t531 = fmul contract float %t510, %t526
  %t532 = add i64 %t520, 16
  %t533 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t532
  %t534 = load float, ptr %t533
  %t535 = fadd contract float %t534, %t531
  store float %t535, ptr %t533
  %t536 = fmul contract float %t513, %t526
  %t537 = add i64 %t520, 32
  %t538 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t537
  %t539 = load float, ptr %t538
  %t540 = fadd contract float %t539, %t536
  store float %t540, ptr %t538
  %t541 = fmul contract float %t516, %t526
  %t542 = add i64 %t520, 48
  %t543 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t542
  %t544 = load float, ptr %t543
  %t545 = fadd contract float %t544, %t541
  store float %t545, ptr %t543
  %t546 = add i64 %t520, 1
  store i64 %t546, ptr %s4
  br label %bb517
bb519:
  %t547 = add i64 %t503, 1
  store i64 %t547, ptr %s3
  br label %bb500
bb502:
  %t548 = add i64 %t461, %t16
  store i64 0, ptr %s4
  br label %bb549
bb549:
  %t552 = load i64, ptr %s4
  %t553 = icmp uge i64 %t552, %t385
  br i1 %t553, label %bb551, label %bb550
bb550:
  %t554 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t552
  %t555 = load float, ptr %t554
  %t556 = add i64 %t548, %t552
  %t557 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t556
  store float %t555, ptr %t557
  %t558 = add i64 %t552, 1
  store i64 %t558, ptr %s4
  br label %bb549
bb551:
  %t559 = add i64 %t548, 2048
  store i64 0, ptr %s4
  br label %bb560
bb560:
  %t563 = load i64, ptr %s4
  %t564 = icmp uge i64 %t563, %t385
  br i1 %t564, label %bb562, label %bb561
bb561:
  %t565 = add i64 %t563, 16
  %t566 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t565
  %t567 = load float, ptr %t566
  %t568 = add i64 %t559, %t563
  %t569 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t568
  store float %t567, ptr %t569
  %t570 = add i64 %t563, 1
  store i64 %t570, ptr %s4
  br label %bb560
bb562:
  %t571 = add i64 %t548, 4096
  store i64 0, ptr %s4
  br label %bb572
bb572:
  %t575 = load i64, ptr %s4
  %t576 = icmp uge i64 %t575, %t385
  br i1 %t576, label %bb574, label %bb573
bb573:
  %t577 = add i64 %t575, 32
  %t578 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t577
  %t579 = load float, ptr %t578
  %t580 = add i64 %t571, %t575
  %t581 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t580
  store float %t579, ptr %t581
  %t582 = add i64 %t575, 1
  store i64 %t582, ptr %s4
  br label %bb572
bb574:
  %t583 = add i64 %t548, 6144
  store i64 0, ptr %s4
  br label %bb584
bb584:
  %t587 = load i64, ptr %s4
  %t588 = icmp uge i64 %t587, %t385
  br i1 %t588, label %bb586, label %bb585
bb585:
  %t589 = add i64 %t587, 48
  %t590 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t589
  %t591 = load float, ptr %t590
  %t592 = add i64 %t583, %t587
  %t593 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t592
  store float %t591, ptr %t593
  %t594 = add i64 %t587, 1
  store i64 %t594, ptr %s4
  br label %bb584
bb586:
  store i64 %t459, ptr %s1
  br label %bb455
bb457:
  br label %bb595
bb595:
  %t598 = load i64, ptr %s1
  %t599 = icmp uge i64 %t598, %t7
  br i1 %t599, label %bb597, label %bb596
bb596:
  %t600 = mul i64 %t598, 2048
  %t601 = sub i64 %lo, %t600
  %t602 = icmp slt i64 %t601, 0
  %t603 = select i1 %t602, i64 0, i64 %t601
  %t604 = sub i64 %hi, %t600
  %t605 = icmp sgt i64 %t604, 2048
  %t606 = select i1 %t605, i64 2048, i64 %t604
  %t607 = add i64 %t16, %t385
  %t608 = icmp ult i64 %t603, %t16
  %t609 = select i1 %t608, i64 %t16, i64 %t603
  %t610 = icmp ugt i64 %t606, %t607
  %t611 = select i1 %t610, i64 %t607, i64 %t606
  %t612 = icmp ult i64 %t609, %t611
  br i1 %t612, label %bb613, label %bb614
bb613:
  %t615 = sub i64 %t611, %t609
  %t616 = sub i64 %t609, %t16
  %t617 = mul i64 %t598, 2048
  store i64 0, ptr %s4
  br label %bb618
bb618:
  %t621 = load i64, ptr %s4
  %t622 = icmp uge i64 %t621, %t615
  br i1 %t622, label %bb620, label %bb619
bb619:
  %t623 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t621
  store float 0x0000000000000000, ptr %t623
  %t624 = add i64 %t621, 1
  store i64 %t624, ptr %s4
  br label %bb618
bb620:
  store i64 0, ptr %s3
  br label %bb625
bb625:
  %t628 = load i64, ptr %s3
  %t629 = icmp uge i64 %t628, 2048
  br i1 %t629, label %bb627, label %bb626
bb626:
  %t630 = add i64 %t617, %t628
  %t631 = getelementptr [4194304 x float], ptr %o4, i64 0, i64 %t630
  %t632 = load float, ptr %t631
  store i64 0, ptr %s4
  br label %bb633
bb633:
  %t636 = load i64, ptr %s4
  %t637 = icmp uge i64 %t636, %t615
  br i1 %t637, label %bb635, label %bb634
bb634:
  %t638 = mul i64 %t628, 16
  %t639 = add i64 %t387, %t638
  %t640 = add i64 %t616, %t636
  %t641 = add i64 %t639, %t640
  %t642 = getelementptr [4194304 x float], ptr %packed0, i64 0, i64 %t641
  %t643 = load float, ptr %t642
  %t644 = fmul contract float %t632, %t643
  %t645 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t636
  %t646 = load float, ptr %t645
  %t647 = fadd contract float %t646, %t644
  store float %t647, ptr %t645
  %t648 = add i64 %t636, 1
  store i64 %t648, ptr %s4
  br label %bb633
bb635:
  %t649 = add i64 %t628, 1
  store i64 %t649, ptr %s3
  br label %bb625
bb627:
  %t650 = add i64 %t600, %t609
  store i64 0, ptr %s4
  br label %bb651
bb651:
  %t654 = load i64, ptr %s4
  %t655 = icmp uge i64 %t654, %t615
  br i1 %t655, label %bb653, label %bb652
bb652:
  %t656 = getelementptr [64 x float], ptr %s0, i64 0, i64 %t654
  %t657 = load float, ptr %t656
  %t658 = add i64 %t650, %t654
  %t659 = getelementptr [4194304 x float], ptr %o7, i64 0, i64 %t658
  store float %t657, ptr %t659
  %t660 = add i64 %t654, 1
  store i64 %t660, ptr %s4
  br label %bb651
bb653:
  br label %bb614
bb614:
  %t661 = add i64 %t598, 1
  store i64 %t661, ptr %s1
  br label %bb595
bb597:
  br label %bb15
bb15:
  ret void
}

define internal void @task6(i64 %lo, i64 %hi, ptr %frame) {
entry:
  %s0 = alloca i64
  %s1 = alloca i64
  %s2 = alloca i64
  %pack_field0 = getelementptr %Frame, ptr %frame, i32 0, i32 12
  %packed0 = load ptr, ptr %pack_field0
  %o5 = getelementptr %Frame, ptr %frame, i32 0, i32 3
  store i64 0, ptr %s0
  br label %bb3
bb3:
  %t15 = load i64, ptr %s0
  %t16 = icmp uge i64 %t15, 128
  br i1 %t16, label %bb5, label %bb4
bb4:
  %t17 = mul i64 %t15, 16
  %t18 = mul i64 %t15, 32768
  store i64 0, ptr %s1
  br label %bb6
bb6:
  %t19 = load i64, ptr %s1
  %t20 = icmp uge i64 %t19, 2048
  br i1 %t20, label %bb8, label %bb7
bb7:
  %t21 = mul i64 %t19, 16
  %t22 = add i64 %t18, %t21
  store i64 0, ptr %s2
  br label %bb9
bb9:
  %t23 = load i64, ptr %s2
  %t24 = icmp uge i64 %t23, 16
  br i1 %t24, label %bb11, label %bb10
bb10:
  %t25 = add i64 %t17, %t23
  %t26 = add i64 %t22, %t23
  %t27 = getelementptr [4194304 x float], ptr %packed0, i64 0, i64 %t26
  %t28 = icmp ult i64 %t25, 2048
  br i1 %t28, label %bb12, label %bb13
bb12:
  %t29 = mul i64 %t19, 2048
  %t30 = add i64 %t29, %t25
  %t31 = getelementptr [4194304 x float], ptr %o5, i64 0, i64 %t30
  %t32 = load float, ptr %t31
  store float %t32, ptr %t27
  br label %bb14
bb13:
  store float zeroinitializer, ptr %t27
  br label %bb14
bb14:
  %t33 = add i64 %t23, 1
  store i64 %t33, ptr %s2
  br label %bb9
bb11:
  %t34 = add i64 %t19, 1
  store i64 %t34, ptr %s1
  br label %bb6
bb8:
  %t35 = add i64 %t15, 1
  store i64 %t35, ptr %s0
  br label %bb3
bb5:
  %t36 = call ptr @flow_par_begin(i32 1)
  call void @flow_par_task(ptr %t36, i32 0, i32 1, ptr @task6_slice, i64 4194304, i32 4194305)
  call void @flow_par_launch(ptr %t36, ptr %frame)
  call void @flow_par_finish(ptr %t36)
  ret void
}

define internal void @flow_main() {
entry:
  call void @flow_perf_begin()
  %frame = alloca %Frame
  %pack0 = alloca [4194304 x float], align 64
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
  %pack_field0 = getelementptr %Frame, ptr %frame, i32 0, i32 12
  store ptr %pack0, ptr %pack_field0
  %h = call ptr @flow_par_begin(i32 7)
  call void @flow_par_task(ptr %h, i32 0, i32 1, ptr @task0, i64 4194304, i32 12582914)
  call void @flow_par_task(ptr %h, i32 1, i32 1, ptr @task1, i64 2048, i32 4196354)
  call void @flow_par_task(ptr %h, i32 2, i32 0, ptr @task2, i64 6, i32 1)
  call void @flow_par_task(ptr %h, i32 3, i32 1, ptr @task3, i64 4194304, i32 8388610)
  call void @flow_par_task(ptr %h, i32 4, i32 1, ptr @task4, i64 4194304, i32 8388610)
  call void @flow_par_task(ptr %h, i32 5, i32 0, ptr @task5, i64 4, i32 4194306)
  call void @flow_par_task(ptr %h, i32 6, i32 0, ptr @task6, i64 4194304, i32 4194305)
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
  call void @flow_perf_end()
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
  store i32 2048, ptr %t12
  %t13 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  store i32 2048, ptr %t13
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
  %t60 = getelementptr [4194304 x float], ptr %t56, i64 0, i64 %t59
  %t61 = load float, ptr %t60
  store float %t61, ptr %o17
  %t62 = load ptr, ptr %o4
  %t63 = getelementptr { ptr, i32 }, ptr %o18, i32 0, i32 1
  %t64 = load i32, ptr %t63
  %t65 = sext i32 %t64 to i64
  %t66 = getelementptr [4194304 x float], ptr %t62, i64 0, i64 %t65
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
  store i32 2048, ptr %t8
  %t9 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  store i32 2048, ptr %t9
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
  %t44 = icmp uge i64 %t43, 2048
  br i1 %t44, label %bb42, label %bb41
bb41:
  %t45 = getelementptr [2048 x i32], ptr %t37, i64 0, i64 %t43
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

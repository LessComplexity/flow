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

define internal float @fn0({ ptr, ptr, i32, i32 } %arg) {
entry:
  %o0 = alloca { ptr, ptr, i32, i32 }
  %o1 = alloca float
  %o2 = alloca ptr
  %o3 = alloca ptr
  %o4 = alloca i32
  %o5 = alloca i32
  %o6 = alloca { i32, float }
  %o7 = alloca { i32, float }
  %o8 = alloca i32
  %o9 = alloca float
  %o10 = alloca { i32, i32 }
  %o11 = alloca i1
  %o12 = alloca { i32, i32 }
  %o13 = alloca i32
  %o14 = alloca { i32, i32 }
  %o15 = alloca i32
  %o16 = alloca { ptr, i32 }
  %o17 = alloca float
  %o18 = alloca { i32, i32 }
  %o19 = alloca i32
  %o20 = alloca { i32, i32 }
  %o21 = alloca i32
  %o22 = alloca { ptr, i32 }
  %o23 = alloca float
  %o24 = alloca { float, float }
  %o25 = alloca float
  %o26 = alloca { float, float }
  %o27 = alloca float
  %o28 = alloca { i32, i32 }
  %o29 = alloca i32
  %o30 = alloca { i32, float }
  %o31 = alloca { { i32, float }, i1 }
  %o32 = alloca { float, i1 }
  store { ptr, ptr, i32, i32 } %arg, ptr %o0
  %t0 = getelementptr { ptr, ptr, i32, i32 }, ptr %o0, i32 0, i32 0
  %t1 = load ptr, ptr %t0
  store ptr %t1, ptr %o2
  %t2 = getelementptr { ptr, ptr, i32, i32 }, ptr %o0, i32 0, i32 1
  %t3 = load ptr, ptr %t2
  store ptr %t3, ptr %o3
  %t4 = getelementptr { ptr, ptr, i32, i32 }, ptr %o0, i32 0, i32 2
  %t5 = load i32, ptr %t4
  store i32 %t5, ptr %o4
  %t6 = getelementptr { ptr, ptr, i32, i32 }, ptr %o0, i32 0, i32 3
  %t7 = load i32, ptr %t6
  store i32 %t7, ptr %o5
  %t8 = getelementptr { i32, float }, ptr %o6, i32 0, i32 0
  store i32 0, ptr %t8
  %t9 = getelementptr { i32, float }, ptr %o6, i32 0, i32 1
  store float 0x0000000000000000, ptr %t9
  %t10 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 1
  store i32 16, ptr %t10
  %t11 = load i32, ptr %o4
  %t12 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 0
  store i32 %t11, ptr %t12
  %t13 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 0
  %t14 = load i32, ptr %t13
  %t15 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 1
  %t16 = load i32, ptr %t15
  %t17 = mul i32 %t14, %t16
  store i32 %t17, ptr %o13
  %t18 = load { i32, float }, ptr %o6
  store { i32, float } %t18, ptr %o7
  br label %bb19
bb19:
  %t23 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  store i32 16, ptr %t23
  %t24 = getelementptr { i32, float }, ptr %o7, i32 0, i32 0
  %t25 = load i32, ptr %t24
  store i32 %t25, ptr %o8
  %t26 = getelementptr { i32, float }, ptr %o7, i32 0, i32 1
  %t27 = load float, ptr %t26
  store float %t27, ptr %o9
  %t28 = load i32, ptr %o8
  %t29 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 0
  store i32 %t28, ptr %t29
  %t30 = load float, ptr %o9
  %t31 = getelementptr { float, i1 }, ptr %o32, i32 0, i32 0
  store float %t30, ptr %t31
  %t32 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 0
  %t33 = load i32, ptr %t32
  %t34 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  %t35 = load i32, ptr %t34
  %t36 = icmp slt i32 %t33, %t35
  store i1 %t36, ptr %o11
  %t37 = load i1, ptr %o11
  %t38 = getelementptr { float, i1 }, ptr %o32, i32 0, i32 1
  store i1 %t37, ptr %t38
  %t39 = getelementptr { float, i1 }, ptr %o32, i32 0, i32 1
  %t40 = load i1, ptr %t39
  br i1 %t40, label %bb20, label %bb21
bb20:
  %t41 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 1
  store i32 16, ptr %t41
  %t42 = getelementptr { i32, i32 }, ptr %o28, i32 0, i32 1
  store i32 1, ptr %t42
  %t43 = load ptr, ptr %o2
  %t44 = getelementptr { ptr, i32 }, ptr %o16, i32 0, i32 0
  store ptr %t43, ptr %t44
  %t45 = load ptr, ptr %o3
  %t46 = getelementptr { ptr, i32 }, ptr %o22, i32 0, i32 0
  store ptr %t45, ptr %t46
  %t47 = load i32, ptr %o5
  %t48 = getelementptr { i32, i32 }, ptr %o20, i32 0, i32 1
  store i32 %t47, ptr %t48
  %t49 = load i32, ptr %o13
  %t50 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 0
  store i32 %t49, ptr %t50
  %t51 = load i32, ptr %o8
  %t52 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 1
  store i32 %t51, ptr %t52
  %t53 = load i32, ptr %o8
  %t54 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 0
  store i32 %t53, ptr %t54
  %t55 = load i32, ptr %o8
  %t56 = getelementptr { i32, i32 }, ptr %o28, i32 0, i32 0
  store i32 %t55, ptr %t56
  %t57 = load float, ptr %o9
  %t58 = getelementptr { float, float }, ptr %o26, i32 0, i32 0
  store float %t57, ptr %t58
  %t59 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 0
  %t60 = load i32, ptr %t59
  %t61 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 1
  %t62 = load i32, ptr %t61
  %t63 = add i32 %t60, %t62
  store i32 %t63, ptr %o15
  %t64 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 0
  %t65 = load i32, ptr %t64
  %t66 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 1
  %t67 = load i32, ptr %t66
  %t68 = mul i32 %t65, %t67
  store i32 %t68, ptr %o19
  %t69 = getelementptr { i32, i32 }, ptr %o28, i32 0, i32 0
  %t70 = load i32, ptr %t69
  %t71 = getelementptr { i32, i32 }, ptr %o28, i32 0, i32 1
  %t72 = load i32, ptr %t71
  %t73 = add i32 %t70, %t72
  store i32 %t73, ptr %o29
  %t74 = load i1, ptr %o11
  %t75 = getelementptr { { i32, float }, i1 }, ptr %o31, i32 0, i32 1
  store i1 %t74, ptr %t75
  %t76 = load i32, ptr %o15
  %t77 = getelementptr { ptr, i32 }, ptr %o16, i32 0, i32 1
  store i32 %t76, ptr %t77
  %t78 = load i32, ptr %o19
  %t79 = getelementptr { i32, i32 }, ptr %o20, i32 0, i32 0
  store i32 %t78, ptr %t79
  %t80 = load i32, ptr %o29
  %t81 = getelementptr { i32, float }, ptr %o30, i32 0, i32 0
  store i32 %t80, ptr %t81
  %t82 = load ptr, ptr %o2
  %t83 = getelementptr { ptr, i32 }, ptr %o16, i32 0, i32 1
  %t84 = load i32, ptr %t83
  %t85 = sext i32 %t84 to i64
  %t86 = icmp slt i64 %t85, 0
  %t87 = icmp sge i64 %t85, 256
  %t88 = or i1 %t86, %t87
  br i1 %t88, label %bb89, label %bb90
bb89:
  call void @flow_trap(i32 1)
  unreachable
bb90:
  %t91 = getelementptr [256 x float], ptr %t82, i64 0, i64 %t85
  %t92 = load float, ptr %t91
  store float %t92, ptr %o17
  %t93 = getelementptr { i32, i32 }, ptr %o20, i32 0, i32 0
  %t94 = load i32, ptr %t93
  %t95 = getelementptr { i32, i32 }, ptr %o20, i32 0, i32 1
  %t96 = load i32, ptr %t95
  %t97 = add i32 %t94, %t96
  store i32 %t97, ptr %o21
  %t98 = load float, ptr %o17
  %t99 = getelementptr { float, float }, ptr %o24, i32 0, i32 0
  store float %t98, ptr %t99
  %t100 = load i32, ptr %o21
  %t101 = getelementptr { ptr, i32 }, ptr %o22, i32 0, i32 1
  store i32 %t100, ptr %t101
  %t102 = load ptr, ptr %o3
  %t103 = getelementptr { ptr, i32 }, ptr %o22, i32 0, i32 1
  %t104 = load i32, ptr %t103
  %t105 = sext i32 %t104 to i64
  %t106 = icmp slt i64 %t105, 0
  %t107 = icmp sge i64 %t105, 256
  %t108 = or i1 %t106, %t107
  br i1 %t108, label %bb109, label %bb110
bb109:
  call void @flow_trap(i32 1)
  unreachable
bb110:
  %t111 = getelementptr [256 x float], ptr %t102, i64 0, i64 %t105
  %t112 = load float, ptr %t111
  store float %t112, ptr %o23
  %t113 = load float, ptr %o23
  %t114 = getelementptr { float, float }, ptr %o24, i32 0, i32 1
  store float %t113, ptr %t114
  %t115 = getelementptr { float, float }, ptr %o24, i32 0, i32 0
  %t116 = load float, ptr %t115
  %t117 = getelementptr { float, float }, ptr %o24, i32 0, i32 1
  %t118 = load float, ptr %t117
  %t119 = fmul float %t116, %t118
  store float %t119, ptr %o25
  %t120 = load float, ptr %o25
  %t121 = getelementptr { float, float }, ptr %o26, i32 0, i32 1
  store float %t120, ptr %t121
  %t122 = getelementptr { float, float }, ptr %o26, i32 0, i32 0
  %t123 = load float, ptr %t122
  %t124 = getelementptr { float, float }, ptr %o26, i32 0, i32 1
  %t125 = load float, ptr %t124
  %t126 = fadd float %t123, %t125
  store float %t126, ptr %o27
  %t127 = load float, ptr %o27
  %t128 = getelementptr { i32, float }, ptr %o30, i32 0, i32 1
  store float %t127, ptr %t128
  %t129 = load { i32, float }, ptr %o30
  %t130 = getelementptr { { i32, float }, i1 }, ptr %o31, i32 0, i32 0
  store { i32, float } %t129, ptr %t130
  %t131 = getelementptr { { i32, float }, i1 }, ptr %o31, i32 0, i32 0
  %t132 = load { i32, float }, ptr %t131
  store { i32, float } %t132, ptr %o7
  br label %bb19
bb21:
  %t133 = getelementptr { float, i1 }, ptr %o32, i32 0, i32 0
  %t134 = load float, ptr %t133
  store float %t134, ptr %o1
  br label %bb22
bb22:
  %t135 = load float, ptr %o1
  ret float %t135
}

define internal [256 x float] @fn1({ ptr, ptr } %arg) {
entry:
  %o0 = alloca { ptr, ptr }
  %o1 = alloca [256 x float]
  %o2 = alloca ptr
  %o3 = alloca ptr
  %o4 = alloca { [256 x float], i32 }
  %o5 = alloca { [256 x float], i32 }
  %o6 = alloca [256 x float]
  %o7 = alloca i32
  %o8 = alloca { i32, i32 }
  %o9 = alloca i1
  %o10 = alloca { i32, i32 }
  %o11 = alloca i32
  %o12 = alloca { i32, i32 }
  %o13 = alloca i32
  %o14 = alloca { ptr, ptr, i32, i32 }
  %o15 = alloca float
  %o16 = alloca { ptr, i32, float }
  %o18 = alloca { i32, i32 }
  %o19 = alloca i32
  %o20 = alloca { [256 x float], i32 }
  %o21 = alloca { { [256 x float], i32 }, i1 }
  %o22 = alloca { [256 x float], i1 }
  %s67 = alloca { ptr, ptr, i32, i32 }
  store { ptr, ptr } %arg, ptr %o0
  %t0 = getelementptr { ptr, ptr }, ptr %o0, i32 0, i32 0
  %t1 = load ptr, ptr %t0
  store ptr %t1, ptr %o2
  %t2 = getelementptr { ptr, ptr }, ptr %o0, i32 0, i32 1
  %t3 = load ptr, ptr %t2
  store ptr %t3, ptr %o3
  %t4 = getelementptr { [256 x float], i32 }, ptr %o4, i32 0, i32 1
  store i32 0, ptr %t4
  %t5 = load ptr, ptr %o3
  %t6 = getelementptr { [256 x float], i32 }, ptr %o4, i32 0, i32 0
  call void @llvm.memcpy.p0.p0.i64(ptr %t6, ptr %t5, i64 ptrtoint (ptr getelementptr ([256 x float], ptr null, i64 1) to i64), i1 false)
  %t7 = load { [256 x float], i32 }, ptr %o4
  store { [256 x float], i32 } %t7, ptr %o5
  br label %bb8
bb8:
  %t12 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  store i32 256, ptr %t12
  %t13 = getelementptr { [256 x float], i32 }, ptr %o5, i32 0, i32 0
  call void @llvm.memcpy.p0.p0.i64(ptr %o6, ptr %t13, i64 ptrtoint (ptr getelementptr ([256 x float], ptr null, i64 1) to i64), i1 false)
  %t14 = getelementptr { [256 x float], i32 }, ptr %o5, i32 0, i32 1
  %t15 = load i32, ptr %t14
  store i32 %t15, ptr %o7
  %t16 = getelementptr { [256 x float], i1 }, ptr %o22, i32 0, i32 0
  call void @llvm.memcpy.p0.p0.i64(ptr %t16, ptr %o6, i64 ptrtoint (ptr getelementptr ([256 x float], ptr null, i64 1) to i64), i1 false)
  %t17 = load i32, ptr %o7
  %t18 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  store i32 %t17, ptr %t18
  %t19 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  %t20 = load i32, ptr %t19
  %t21 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  %t22 = load i32, ptr %t21
  %t23 = icmp slt i32 %t20, %t22
  store i1 %t23, ptr %o9
  %t24 = load i1, ptr %o9
  %t25 = getelementptr { [256 x float], i1 }, ptr %o22, i32 0, i32 1
  store i1 %t24, ptr %t25
  %t26 = getelementptr { [256 x float], i1 }, ptr %o22, i32 0, i32 1
  %t27 = load i1, ptr %t26
  br i1 %t27, label %bb9, label %bb10
bb9:
  %t28 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  store i32 16, ptr %t28
  %t29 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 1
  store i32 16, ptr %t29
  %t30 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 1
  store i32 1, ptr %t30
  %t31 = load ptr, ptr %o2
  %t32 = getelementptr { ptr, ptr, i32, i32 }, ptr %o14, i32 0, i32 0
  store ptr %t31, ptr %t32
  %t33 = load ptr, ptr %o3
  %t34 = getelementptr { ptr, ptr, i32, i32 }, ptr %o14, i32 0, i32 1
  store ptr %t33, ptr %t34
  %t35 = getelementptr { ptr, i32, float }, ptr %o16, i32 0, i32 0
  store ptr %o6, ptr %t35
  %t36 = load i32, ptr %o7
  %t37 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 0
  store i32 %t36, ptr %t37
  %t38 = load i32, ptr %o7
  %t39 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 0
  store i32 %t38, ptr %t39
  %t40 = load i32, ptr %o7
  %t41 = getelementptr { ptr, i32, float }, ptr %o16, i32 0, i32 1
  store i32 %t40, ptr %t41
  %t42 = load i32, ptr %o7
  %t43 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 0
  store i32 %t42, ptr %t43
  %t44 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 0
  %t45 = load i32, ptr %t44
  %t46 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  %t47 = load i32, ptr %t46
  %t48 = sdiv i32 %t45, %t47
  store i32 %t48, ptr %o11
  %t49 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 0
  %t50 = load i32, ptr %t49
  %t51 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 1
  %t52 = load i32, ptr %t51
  %t53 = srem i32 %t50, %t52
  store i32 %t53, ptr %o13
  %t54 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 0
  %t55 = load i32, ptr %t54
  %t56 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 1
  %t57 = load i32, ptr %t56
  %t58 = add i32 %t55, %t57
  store i32 %t58, ptr %o19
  %t59 = load i1, ptr %o9
  %t60 = getelementptr { { [256 x float], i32 }, i1 }, ptr %o21, i32 0, i32 1
  store i1 %t59, ptr %t60
  %t61 = load i32, ptr %o11
  %t62 = getelementptr { ptr, ptr, i32, i32 }, ptr %o14, i32 0, i32 2
  store i32 %t61, ptr %t62
  %t63 = load i32, ptr %o13
  %t64 = getelementptr { ptr, ptr, i32, i32 }, ptr %o14, i32 0, i32 3
  store i32 %t63, ptr %t64
  %t65 = load i32, ptr %o19
  %t66 = getelementptr { [256 x float], i32 }, ptr %o20, i32 0, i32 1
  store i32 %t65, ptr %t66
  %t68 = load ptr, ptr %o2
  %t69 = getelementptr { ptr, ptr, i32, i32 }, ptr %s67, i32 0, i32 0
  store ptr %t68, ptr %t69
  %t70 = load ptr, ptr %o3
  %t71 = getelementptr { ptr, ptr, i32, i32 }, ptr %s67, i32 0, i32 1
  store ptr %t70, ptr %t71
  %t72 = getelementptr { ptr, ptr, i32, i32 }, ptr %o14, i32 0, i32 2
  %t73 = load i32, ptr %t72
  %t74 = getelementptr { ptr, ptr, i32, i32 }, ptr %s67, i32 0, i32 2
  store i32 %t73, ptr %t74
  %t75 = getelementptr { ptr, ptr, i32, i32 }, ptr %o14, i32 0, i32 3
  %t76 = load i32, ptr %t75
  %t77 = getelementptr { ptr, ptr, i32, i32 }, ptr %s67, i32 0, i32 3
  store i32 %t76, ptr %t77
  %t78 = load { ptr, ptr, i32, i32 }, ptr %s67
  %t79 = call float @fn0({ ptr, ptr, i32, i32 } %t78)
  store float %t79, ptr %o15
  %t80 = load float, ptr %o15
  %t81 = getelementptr { ptr, i32, float }, ptr %o16, i32 0, i32 2
  store float %t80, ptr %t81
  %t82 = getelementptr { ptr, i32, float }, ptr %o16, i32 0, i32 1
  %t83 = load i32, ptr %t82
  %t84 = sext i32 %t83 to i64
  %t85 = icmp slt i64 %t84, 0
  %t86 = icmp sge i64 %t84, 256
  %t87 = or i1 %t85, %t86
  br i1 %t87, label %bb88, label %bb89
bb88:
  call void @flow_trap(i32 1)
  unreachable
bb89:
  %t90 = getelementptr [256 x float], ptr %o6, i64 0, i64 %t84
  %t91 = getelementptr { ptr, i32, float }, ptr %o16, i32 0, i32 2
  %t92 = load float, ptr %t91
  store float %t92, ptr %t90
  %t93 = getelementptr { [256 x float], i32 }, ptr %o20, i32 0, i32 0
  call void @llvm.memcpy.p0.p0.i64(ptr %t93, ptr %o6, i64 ptrtoint (ptr getelementptr ([256 x float], ptr null, i64 1) to i64), i1 false)
  %t94 = load { [256 x float], i32 }, ptr %o20
  %t95 = getelementptr { { [256 x float], i32 }, i1 }, ptr %o21, i32 0, i32 0
  store { [256 x float], i32 } %t94, ptr %t95
  %t96 = getelementptr { { [256 x float], i32 }, i1 }, ptr %o21, i32 0, i32 0
  %t97 = load { [256 x float], i32 }, ptr %t96
  store { [256 x float], i32 } %t97, ptr %o5
  br label %bb8
bb10:
  call void @llvm.memcpy.p0.p0.i64(ptr %o1, ptr %o6, i64 ptrtoint (ptr getelementptr ([256 x float], ptr null, i64 1) to i64), i1 false)
  br label %bb11
bb11:
  %t98 = load [256 x float], ptr %o1
  ret [256 x float] %t98
}

define internal void @flow_main() {
entry:
  %o2 = alloca [256 x i32]
  %o3 = alloca [256 x float]
  %o4 = alloca [256 x float]
  %o5 = alloca { ptr, ptr }
  %o6 = alloca [256 x float]
  %o7 = alloca { ptr, i32 }
  %o8 = alloca float
  %o9 = alloca float
  %o11 = alloca { ptr, i32 }
  %o12 = alloca float
  %o13 = alloca float
  %s0 = alloca i64
  %s11 = alloca i64
  %s22 = alloca i64
  %s35 = alloca { ptr, ptr }
  store i64 0, ptr %s0
  br label %bb1
bb1:
  %t4 = load i64, ptr %s0
  %t5 = icmp uge i64 %t4, 256
  br i1 %t5, label %bb3, label %bb2
bb2:
  %t6 = trunc i64 %t4 to i32
  %t7 = getelementptr [256 x i32], ptr %o2, i64 0, i64 %t4
  store i32 %t6, ptr %t7
  %t8 = add i64 %t4, 1
  store i64 %t8, ptr %s0
  br label %bb1
bb3:
  %t9 = getelementptr { ptr, i32 }, ptr %o7, i32 0, i32 1
  store i32 0, ptr %t9
  %t10 = getelementptr { ptr, i32 }, ptr %o11, i32 0, i32 1
  store i32 255, ptr %t10
  store i64 0, ptr %s11
  br label %bb12
bb12:
  %t15 = load i64, ptr %s11
  %t16 = icmp uge i64 %t15, 256
  br i1 %t16, label %bb14, label %bb13
bb13:
  %t17 = getelementptr [256 x i32], ptr %o2, i64 0, i64 %t15
  %t18 = load i32, ptr %t17
  %t19 = call float @fn3(i32 %t18)
  %t20 = getelementptr [256 x float], ptr %o3, i64 0, i64 %t15
  store float %t19, ptr %t20
  %t21 = add i64 %t15, 1
  store i64 %t21, ptr %s11
  br label %bb12
bb14:
  store i64 0, ptr %s22
  br label %bb23
bb23:
  %t26 = load i64, ptr %s22
  %t27 = icmp uge i64 %t26, 256
  br i1 %t27, label %bb25, label %bb24
bb24:
  %t28 = getelementptr [256 x i32], ptr %o2, i64 0, i64 %t26
  %t29 = load i32, ptr %t28
  %t30 = call float @fn4(i32 %t29)
  %t31 = getelementptr [256 x float], ptr %o4, i64 0, i64 %t26
  store float %t30, ptr %t31
  %t32 = add i64 %t26, 1
  store i64 %t32, ptr %s22
  br label %bb23
bb25:
  %t33 = getelementptr { ptr, ptr }, ptr %o5, i32 0, i32 0
  store ptr %o3, ptr %t33
  %t34 = getelementptr { ptr, ptr }, ptr %o5, i32 0, i32 1
  store ptr %o4, ptr %t34
  %t36 = getelementptr { ptr, ptr }, ptr %s35, i32 0, i32 0
  store ptr %o3, ptr %t36
  %t37 = getelementptr { ptr, ptr }, ptr %s35, i32 0, i32 1
  store ptr %o4, ptr %t37
  %t38 = load { ptr, ptr }, ptr %s35
  %t39 = call [256 x float] @fn1({ ptr, ptr } %t38)
  store [256 x float] %t39, ptr %o6
  %t40 = getelementptr { ptr, i32 }, ptr %o7, i32 0, i32 0
  store ptr %o6, ptr %t40
  %t41 = getelementptr { ptr, i32 }, ptr %o11, i32 0, i32 0
  store ptr %o6, ptr %t41
  %t42 = getelementptr { ptr, i32 }, ptr %o7, i32 0, i32 1
  %t43 = load i32, ptr %t42
  %t44 = sext i32 %t43 to i64
  %t45 = getelementptr [256 x float], ptr %o6, i64 0, i64 %t44
  %t46 = load float, ptr %t45
  store float %t46, ptr %o8
  %t47 = getelementptr { ptr, i32 }, ptr %o11, i32 0, i32 1
  %t48 = load i32, ptr %t47
  %t49 = sext i32 %t48 to i64
  %t50 = getelementptr [256 x float], ptr %o6, i64 0, i64 %t49
  %t51 = load float, ptr %t50
  store float %t51, ptr %o12
  %t52 = load float, ptr %o8
  store float %t52, ptr %o9
  %t53 = load float, ptr %o12
  store float %t53, ptr %o13
  %t54 = load float, ptr %o9
  call void @flow_print_f32(float %t54, i1 zeroext true)
  %t55 = load float, ptr %o13
  call void @flow_print_f32(float %t55, i1 zeroext true)
  ret void
}

define internal float @fn3(i32 %arg) readonly nounwind willreturn {
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

define internal float @fn4(i32 %arg) readonly nounwind willreturn {
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

define i32 @main() {
entry:
  call void @flow_main()
  ret i32 0
}

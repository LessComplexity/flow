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
  %o16 = alloca { [16 x float], i32 }
  %o17 = alloca float
  %o18 = alloca { i32, i32 }
  %o19 = alloca i32
  %o20 = alloca { i32, i32 }
  %o21 = alloca i32
  %o22 = alloca { [16 x float], i32 }
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
  store i32 4, ptr %t10
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
  store i32 4, ptr %t23
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
  store i32 4, ptr %t41
  %t42 = getelementptr { i32, i32 }, ptr %o28, i32 0, i32 1
  store i32 1, ptr %t42
  %t43 = load ptr, ptr %o2
  %t44 = load [16 x float], ptr %t43
  %t45 = getelementptr { [16 x float], i32 }, ptr %o16, i32 0, i32 0
  store [16 x float] %t44, ptr %t45
  %t46 = load ptr, ptr %o3
  %t47 = load [16 x float], ptr %t46
  %t48 = getelementptr { [16 x float], i32 }, ptr %o22, i32 0, i32 0
  store [16 x float] %t47, ptr %t48
  %t49 = load i32, ptr %o5
  %t50 = getelementptr { i32, i32 }, ptr %o20, i32 0, i32 1
  store i32 %t49, ptr %t50
  %t51 = load i32, ptr %o13
  %t52 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 0
  store i32 %t51, ptr %t52
  %t53 = load i32, ptr %o8
  %t54 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 1
  store i32 %t53, ptr %t54
  %t55 = load i32, ptr %o8
  %t56 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 0
  store i32 %t55, ptr %t56
  %t57 = load i32, ptr %o8
  %t58 = getelementptr { i32, i32 }, ptr %o28, i32 0, i32 0
  store i32 %t57, ptr %t58
  %t59 = load float, ptr %o9
  %t60 = getelementptr { float, float }, ptr %o26, i32 0, i32 0
  store float %t59, ptr %t60
  %t61 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 0
  %t62 = load i32, ptr %t61
  %t63 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 1
  %t64 = load i32, ptr %t63
  %t65 = add i32 %t62, %t64
  store i32 %t65, ptr %o15
  %t66 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 0
  %t67 = load i32, ptr %t66
  %t68 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 1
  %t69 = load i32, ptr %t68
  %t70 = mul i32 %t67, %t69
  store i32 %t70, ptr %o19
  %t71 = getelementptr { i32, i32 }, ptr %o28, i32 0, i32 0
  %t72 = load i32, ptr %t71
  %t73 = getelementptr { i32, i32 }, ptr %o28, i32 0, i32 1
  %t74 = load i32, ptr %t73
  %t75 = add i32 %t72, %t74
  store i32 %t75, ptr %o29
  %t76 = load i1, ptr %o11
  %t77 = getelementptr { { i32, float }, i1 }, ptr %o31, i32 0, i32 1
  store i1 %t76, ptr %t77
  %t78 = load i32, ptr %o15
  %t79 = getelementptr { [16 x float], i32 }, ptr %o16, i32 0, i32 1
  store i32 %t78, ptr %t79
  %t80 = load i32, ptr %o19
  %t81 = getelementptr { i32, i32 }, ptr %o20, i32 0, i32 0
  store i32 %t80, ptr %t81
  %t82 = load i32, ptr %o29
  %t83 = getelementptr { i32, float }, ptr %o30, i32 0, i32 0
  store i32 %t82, ptr %t83
  %t84 = load ptr, ptr %o2
  %t85 = getelementptr { [16 x float], i32 }, ptr %o16, i32 0, i32 1
  %t86 = load i32, ptr %t85
  %t87 = sext i32 %t86 to i64
  %t88 = icmp slt i64 %t87, 0
  %t89 = icmp sge i64 %t87, 16
  %t90 = or i1 %t88, %t89
  br i1 %t90, label %bb91, label %bb92
bb91:
  call void @flow_trap(i32 1)
  unreachable
bb92:
  %t93 = getelementptr [16 x float], ptr %t84, i64 0, i64 %t87
  %t94 = load float, ptr %t93
  store float %t94, ptr %o17
  %t95 = getelementptr { i32, i32 }, ptr %o20, i32 0, i32 0
  %t96 = load i32, ptr %t95
  %t97 = getelementptr { i32, i32 }, ptr %o20, i32 0, i32 1
  %t98 = load i32, ptr %t97
  %t99 = add i32 %t96, %t98
  store i32 %t99, ptr %o21
  %t100 = load float, ptr %o17
  %t101 = getelementptr { float, float }, ptr %o24, i32 0, i32 0
  store float %t100, ptr %t101
  %t102 = load i32, ptr %o21
  %t103 = getelementptr { [16 x float], i32 }, ptr %o22, i32 0, i32 1
  store i32 %t102, ptr %t103
  %t104 = load ptr, ptr %o3
  %t105 = getelementptr { [16 x float], i32 }, ptr %o22, i32 0, i32 1
  %t106 = load i32, ptr %t105
  %t107 = sext i32 %t106 to i64
  %t108 = icmp slt i64 %t107, 0
  %t109 = icmp sge i64 %t107, 16
  %t110 = or i1 %t108, %t109
  br i1 %t110, label %bb111, label %bb112
bb111:
  call void @flow_trap(i32 1)
  unreachable
bb112:
  %t113 = getelementptr [16 x float], ptr %t104, i64 0, i64 %t107
  %t114 = load float, ptr %t113
  store float %t114, ptr %o23
  %t115 = load float, ptr %o23
  %t116 = getelementptr { float, float }, ptr %o24, i32 0, i32 1
  store float %t115, ptr %t116
  %t117 = getelementptr { float, float }, ptr %o24, i32 0, i32 0
  %t118 = load float, ptr %t117
  %t119 = getelementptr { float, float }, ptr %o24, i32 0, i32 1
  %t120 = load float, ptr %t119
  %t121 = fmul float %t118, %t120
  store float %t121, ptr %o25
  %t122 = load float, ptr %o25
  %t123 = getelementptr { float, float }, ptr %o26, i32 0, i32 1
  store float %t122, ptr %t123
  %t124 = getelementptr { float, float }, ptr %o26, i32 0, i32 0
  %t125 = load float, ptr %t124
  %t126 = getelementptr { float, float }, ptr %o26, i32 0, i32 1
  %t127 = load float, ptr %t126
  %t128 = fadd float %t125, %t127
  store float %t128, ptr %o27
  %t129 = load float, ptr %o27
  %t130 = getelementptr { i32, float }, ptr %o30, i32 0, i32 1
  store float %t129, ptr %t130
  %t131 = load { i32, float }, ptr %o30
  %t132 = getelementptr { { i32, float }, i1 }, ptr %o31, i32 0, i32 0
  store { i32, float } %t131, ptr %t132
  %t133 = getelementptr { { i32, float }, i1 }, ptr %o31, i32 0, i32 0
  %t134 = load { i32, float }, ptr %t133
  store { i32, float } %t134, ptr %o7
  br label %bb19
bb21:
  %t135 = getelementptr { float, i1 }, ptr %o32, i32 0, i32 0
  %t136 = load float, ptr %t135
  store float %t136, ptr %o1
  br label %bb22
bb22:
  %t137 = load float, ptr %o1
  ret float %t137
}

define internal [16 x float] @fn1({ ptr, ptr } %arg) {
entry:
  %o0 = alloca { ptr, ptr }
  %o1 = alloca [16 x float]
  %o2 = alloca ptr
  %o3 = alloca ptr
  %o4 = alloca { [16 x float], i32 }
  %o5 = alloca { [16 x float], i32 }
  %o6 = alloca [16 x float]
  %o7 = alloca i32
  %o8 = alloca { i32, i32 }
  %o9 = alloca i1
  %o10 = alloca { i32, i32 }
  %o11 = alloca i32
  %o12 = alloca { i32, i32 }
  %o13 = alloca i32
  %o14 = alloca { [16 x float], [16 x float], i32, i32 }
  %o15 = alloca float
  %o16 = alloca { [16 x float], i32, float }
  %o17 = alloca [16 x float]
  %o18 = alloca { i32, i32 }
  %o19 = alloca i32
  %o20 = alloca { [16 x float], i32 }
  %o21 = alloca { { [16 x float], i32 }, i1 }
  %o22 = alloca { [16 x float], i1 }
  %s91 = alloca { ptr, ptr, i32, i32 }
  store { ptr, ptr } %arg, ptr %o0
  %t0 = getelementptr { ptr, ptr }, ptr %o0, i32 0, i32 0
  %t1 = load ptr, ptr %t0
  store ptr %t1, ptr %o2
  %t2 = getelementptr { ptr, ptr }, ptr %o0, i32 0, i32 1
  %t3 = load ptr, ptr %t2
  store ptr %t3, ptr %o3
  %t4 = getelementptr { [16 x float], i32 }, ptr %o4, i32 0, i32 1
  store i32 0, ptr %t4
  %t5 = load ptr, ptr %o3
  %t6 = load [16 x float], ptr %t5
  %t7 = getelementptr { [16 x float], i32 }, ptr %o4, i32 0, i32 0
  store [16 x float] %t6, ptr %t7
  %t8 = load { [16 x float], i32 }, ptr %o4
  store { [16 x float], i32 } %t8, ptr %o5
  br label %bb9
bb9:
  %t13 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  store i32 16, ptr %t13
  %t14 = getelementptr { [16 x float], i32 }, ptr %o5, i32 0, i32 0
  %t15 = load [16 x float], ptr %t14
  store [16 x float] %t15, ptr %o6
  %t16 = getelementptr { [16 x float], i32 }, ptr %o5, i32 0, i32 1
  %t17 = load i32, ptr %t16
  store i32 %t17, ptr %o7
  %t18 = load [16 x float], ptr %o6
  %t19 = getelementptr { [16 x float], i1 }, ptr %o22, i32 0, i32 0
  store [16 x float] %t18, ptr %t19
  %t20 = load i32, ptr %o7
  %t21 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  store i32 %t20, ptr %t21
  %t22 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  %t23 = load i32, ptr %t22
  %t24 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  %t25 = load i32, ptr %t24
  %t26 = icmp slt i32 %t23, %t25
  store i1 %t26, ptr %o9
  %t27 = load i1, ptr %o9
  %t28 = getelementptr { [16 x float], i1 }, ptr %o22, i32 0, i32 1
  store i1 %t27, ptr %t28
  %t29 = getelementptr { [16 x float], i1 }, ptr %o22, i32 0, i32 1
  %t30 = load i1, ptr %t29
  br i1 %t30, label %bb10, label %bb11
bb10:
  %t31 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  store i32 4, ptr %t31
  %t32 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 1
  store i32 4, ptr %t32
  %t33 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 1
  store i32 1, ptr %t33
  %t34 = load ptr, ptr %o2
  %t35 = load [16 x float], ptr %t34
  %t36 = getelementptr { [16 x float], [16 x float], i32, i32 }, ptr %o14, i32 0, i32 0
  store [16 x float] %t35, ptr %t36
  %t37 = load ptr, ptr %o3
  %t38 = load [16 x float], ptr %t37
  %t39 = getelementptr { [16 x float], [16 x float], i32, i32 }, ptr %o14, i32 0, i32 1
  store [16 x float] %t38, ptr %t39
  %t40 = load [16 x float], ptr %o6
  %t41 = getelementptr { [16 x float], i32, float }, ptr %o16, i32 0, i32 0
  store [16 x float] %t40, ptr %t41
  %t42 = load i32, ptr %o7
  %t43 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 0
  store i32 %t42, ptr %t43
  %t44 = load i32, ptr %o7
  %t45 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 0
  store i32 %t44, ptr %t45
  %t46 = load i32, ptr %o7
  %t47 = getelementptr { [16 x float], i32, float }, ptr %o16, i32 0, i32 1
  store i32 %t46, ptr %t47
  %t48 = load i32, ptr %o7
  %t49 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 0
  store i32 %t48, ptr %t49
  %t50 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 0
  %t51 = load i32, ptr %t50
  %t52 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  %t53 = load i32, ptr %t52
  %t54 = icmp eq i32 %t53, 0
  br i1 %t54, label %bb55, label %bb56
bb55:
  call void @flow_trap(i32 0)
  unreachable
bb56:
  %t57 = icmp eq i32 %t53, -1
  %t58 = icmp eq i32 %t51, -2147483648
  %t59 = and i1 %t57, %t58
  br i1 %t59, label %bb60, label %bb61
bb60:
  store i32 -2147483648, ptr %o11
  br label %bb62
bb61:
  %t63 = sdiv i32 %t51, %t53
  store i32 %t63, ptr %o11
  br label %bb62
bb62:
  %t64 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 0
  %t65 = load i32, ptr %t64
  %t66 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 1
  %t67 = load i32, ptr %t66
  %t68 = icmp eq i32 %t67, 0
  br i1 %t68, label %bb69, label %bb70
bb69:
  call void @flow_trap(i32 0)
  unreachable
bb70:
  %t71 = icmp eq i32 %t67, -1
  %t72 = icmp eq i32 %t65, -2147483648
  %t73 = and i1 %t71, %t72
  br i1 %t73, label %bb74, label %bb75
bb74:
  store i32 0, ptr %o13
  br label %bb76
bb75:
  %t77 = srem i32 %t65, %t67
  store i32 %t77, ptr %o13
  br label %bb76
bb76:
  %t78 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 0
  %t79 = load i32, ptr %t78
  %t80 = getelementptr { i32, i32 }, ptr %o18, i32 0, i32 1
  %t81 = load i32, ptr %t80
  %t82 = add i32 %t79, %t81
  store i32 %t82, ptr %o19
  %t83 = load i1, ptr %o9
  %t84 = getelementptr { { [16 x float], i32 }, i1 }, ptr %o21, i32 0, i32 1
  store i1 %t83, ptr %t84
  %t85 = load i32, ptr %o11
  %t86 = getelementptr { [16 x float], [16 x float], i32, i32 }, ptr %o14, i32 0, i32 2
  store i32 %t85, ptr %t86
  %t87 = load i32, ptr %o13
  %t88 = getelementptr { [16 x float], [16 x float], i32, i32 }, ptr %o14, i32 0, i32 3
  store i32 %t87, ptr %t88
  %t89 = load i32, ptr %o19
  %t90 = getelementptr { [16 x float], i32 }, ptr %o20, i32 0, i32 1
  store i32 %t89, ptr %t90
  %t92 = load ptr, ptr %o2
  %t93 = getelementptr { ptr, ptr, i32, i32 }, ptr %s91, i32 0, i32 0
  store ptr %t92, ptr %t93
  %t94 = load ptr, ptr %o3
  %t95 = getelementptr { ptr, ptr, i32, i32 }, ptr %s91, i32 0, i32 1
  store ptr %t94, ptr %t95
  %t96 = getelementptr { [16 x float], [16 x float], i32, i32 }, ptr %o14, i32 0, i32 2
  %t97 = load i32, ptr %t96
  %t98 = getelementptr { ptr, ptr, i32, i32 }, ptr %s91, i32 0, i32 2
  store i32 %t97, ptr %t98
  %t99 = getelementptr { [16 x float], [16 x float], i32, i32 }, ptr %o14, i32 0, i32 3
  %t100 = load i32, ptr %t99
  %t101 = getelementptr { ptr, ptr, i32, i32 }, ptr %s91, i32 0, i32 3
  store i32 %t100, ptr %t101
  %t102 = load { ptr, ptr, i32, i32 }, ptr %s91
  %t103 = call float @fn0({ ptr, ptr, i32, i32 } %t102)
  store float %t103, ptr %o15
  %t104 = load float, ptr %o15
  %t105 = getelementptr { [16 x float], i32, float }, ptr %o16, i32 0, i32 2
  store float %t104, ptr %t105
  %t106 = getelementptr { [16 x float], i32, float }, ptr %o16, i32 0, i32 0
  %t107 = getelementptr { [16 x float], i32, float }, ptr %o16, i32 0, i32 1
  %t108 = load i32, ptr %t107
  %t109 = sext i32 %t108 to i64
  %t110 = icmp slt i64 %t109, 0
  %t111 = icmp sge i64 %t109, 16
  %t112 = or i1 %t110, %t111
  br i1 %t112, label %bb113, label %bb114
bb113:
  call void @flow_trap(i32 1)
  unreachable
bb114:
  call void @llvm.memcpy.p0.p0.i64(ptr %o17, ptr %t106, i64 ptrtoint (ptr getelementptr ([16 x float], ptr null, i64 1) to i64), i1 false)
  %t115 = getelementptr [16 x float], ptr %o17, i64 0, i64 %t109
  %t116 = getelementptr { [16 x float], i32, float }, ptr %o16, i32 0, i32 2
  %t117 = load float, ptr %t116
  store float %t117, ptr %t115
  %t118 = load [16 x float], ptr %o17
  %t119 = getelementptr { [16 x float], i32 }, ptr %o20, i32 0, i32 0
  store [16 x float] %t118, ptr %t119
  %t120 = load { [16 x float], i32 }, ptr %o20
  %t121 = getelementptr { { [16 x float], i32 }, i1 }, ptr %o21, i32 0, i32 0
  store { [16 x float], i32 } %t120, ptr %t121
  %t122 = getelementptr { { [16 x float], i32 }, i1 }, ptr %o21, i32 0, i32 0
  %t123 = load { [16 x float], i32 }, ptr %t122
  store { [16 x float], i32 } %t123, ptr %o5
  br label %bb9
bb11:
  %t124 = getelementptr { [16 x float], i1 }, ptr %o22, i32 0, i32 0
  %t125 = load [16 x float], ptr %t124
  store [16 x float] %t125, ptr %o1
  br label %bb12
bb12:
  %t126 = load [16 x float], ptr %o1
  ret [16 x float] %t126
}

define internal void @flow_main() {
entry:
  %o2 = alloca [16 x float]
  %o3 = alloca [16 x float]
  %o4 = alloca { [16 x float], [16 x float] }
  %o5 = alloca [16 x float]
  %o6 = alloca { [16 x float], i32 }
  %o7 = alloca float
  %o8 = alloca float
  %o10 = alloca { [16 x float], i32 }
  %o11 = alloca float
  %o12 = alloca float
  %s38 = alloca { ptr, ptr }
  %t0 = getelementptr [16 x float], ptr %o2, i64 0, i64 0
  store float 0xC042800000000000, ptr %t0
  %t1 = getelementptr [16 x float], ptr %o2, i64 0, i64 1
  store float 0xC03E000000000000, ptr %t1
  %t2 = getelementptr [16 x float], ptr %o2, i64 0, i64 2
  store float 0xC037000000000000, ptr %t2
  %t3 = getelementptr [16 x float], ptr %o2, i64 0, i64 3
  store float 0xC030000000000000, ptr %t3
  %t4 = getelementptr [16 x float], ptr %o2, i64 0, i64 4
  store float 0xC022000000000000, ptr %t4
  %t5 = getelementptr [16 x float], ptr %o2, i64 0, i64 5
  store float 0xC000000000000000, ptr %t5
  %t6 = getelementptr [16 x float], ptr %o2, i64 0, i64 6
  store float 0x4014000000000000, ptr %t6
  %t7 = getelementptr [16 x float], ptr %o2, i64 0, i64 7
  store float 0x4028000000000000, ptr %t7
  %t8 = getelementptr [16 x float], ptr %o2, i64 0, i64 8
  store float 0x4033000000000000, ptr %t8
  %t9 = getelementptr [16 x float], ptr %o2, i64 0, i64 9
  store float 0x403A000000000000, ptr %t9
  %t10 = getelementptr [16 x float], ptr %o2, i64 0, i64 10
  store float 0x4040800000000000, ptr %t10
  %t11 = getelementptr [16 x float], ptr %o2, i64 0, i64 11
  store float 0x4044000000000000, ptr %t11
  %t12 = getelementptr [16 x float], ptr %o2, i64 0, i64 12
  store float 0x4047800000000000, ptr %t12
  %t13 = getelementptr [16 x float], ptr %o2, i64 0, i64 13
  store float 0xC047800000000000, ptr %t13
  %t14 = getelementptr [16 x float], ptr %o2, i64 0, i64 14
  store float 0xC044000000000000, ptr %t14
  %t15 = getelementptr [16 x float], ptr %o2, i64 0, i64 15
  store float 0xC040800000000000, ptr %t15
  %t16 = getelementptr [16 x float], ptr %o3, i64 0, i64 0
  store float 0x401C000000000000, ptr %t16
  %t17 = getelementptr [16 x float], ptr %o3, i64 0, i64 1
  store float 0x402C000000000000, ptr %t17
  %t18 = getelementptr [16 x float], ptr %o3, i64 0, i64 2
  store float 0x4035000000000000, ptr %t18
  %t19 = getelementptr [16 x float], ptr %o3, i64 0, i64 3
  store float 0x403C000000000000, ptr %t19
  %t20 = getelementptr [16 x float], ptr %o3, i64 0, i64 4
  store float 0x4041800000000000, ptr %t20
  %t21 = getelementptr [16 x float], ptr %o3, i64 0, i64 5
  store float 0x4045000000000000, ptr %t21
  %t22 = getelementptr [16 x float], ptr %o3, i64 0, i64 6
  store float 0x4048800000000000, ptr %t22
  %t23 = getelementptr [16 x float], ptr %o3, i64 0, i64 7
  store float 0xC046800000000000, ptr %t23
  %t24 = getelementptr [16 x float], ptr %o3, i64 0, i64 8
  store float 0xC043000000000000, ptr %t24
  %t25 = getelementptr [16 x float], ptr %o3, i64 0, i64 9
  store float 0xC03F000000000000, ptr %t25
  %t26 = getelementptr [16 x float], ptr %o3, i64 0, i64 10
  store float 0xC038000000000000, ptr %t26
  %t27 = getelementptr [16 x float], ptr %o3, i64 0, i64 11
  store float 0xC031000000000000, ptr %t27
  %t28 = getelementptr [16 x float], ptr %o3, i64 0, i64 12
  store float 0xC024000000000000, ptr %t28
  %t29 = getelementptr [16 x float], ptr %o3, i64 0, i64 13
  store float 0xC008000000000000, ptr %t29
  %t30 = getelementptr [16 x float], ptr %o3, i64 0, i64 14
  store float 0x4010000000000000, ptr %t30
  %t31 = getelementptr [16 x float], ptr %o3, i64 0, i64 15
  store float 0x4026000000000000, ptr %t31
  %t32 = getelementptr { [16 x float], i32 }, ptr %o6, i32 0, i32 1
  store i32 0, ptr %t32
  %t33 = getelementptr { [16 x float], i32 }, ptr %o10, i32 0, i32 1
  store i32 15, ptr %t33
  %t34 = load [16 x float], ptr %o2
  %t35 = getelementptr { [16 x float], [16 x float] }, ptr %o4, i32 0, i32 0
  store [16 x float] %t34, ptr %t35
  %t36 = load [16 x float], ptr %o3
  %t37 = getelementptr { [16 x float], [16 x float] }, ptr %o4, i32 0, i32 1
  store [16 x float] %t36, ptr %t37
  %t39 = getelementptr { [16 x float], [16 x float] }, ptr %o4, i32 0, i32 0
  %t40 = getelementptr { ptr, ptr }, ptr %s38, i32 0, i32 0
  store ptr %t39, ptr %t40
  %t41 = getelementptr { [16 x float], [16 x float] }, ptr %o4, i32 0, i32 1
  %t42 = getelementptr { ptr, ptr }, ptr %s38, i32 0, i32 1
  store ptr %t41, ptr %t42
  %t43 = load { ptr, ptr }, ptr %s38
  %t44 = call [16 x float] @fn1({ ptr, ptr } %t43)
  store [16 x float] %t44, ptr %o5
  %t45 = load [16 x float], ptr %o5
  %t46 = getelementptr { [16 x float], i32 }, ptr %o6, i32 0, i32 0
  store [16 x float] %t45, ptr %t46
  %t47 = load [16 x float], ptr %o5
  %t48 = getelementptr { [16 x float], i32 }, ptr %o10, i32 0, i32 0
  store [16 x float] %t47, ptr %t48
  %t49 = getelementptr { [16 x float], i32 }, ptr %o6, i32 0, i32 0
  %t50 = getelementptr { [16 x float], i32 }, ptr %o6, i32 0, i32 1
  %t51 = load i32, ptr %t50
  %t52 = sext i32 %t51 to i64
  %t53 = icmp slt i64 %t52, 0
  %t54 = icmp sge i64 %t52, 16
  %t55 = or i1 %t53, %t54
  br i1 %t55, label %bb56, label %bb57
bb56:
  call void @flow_trap(i32 1)
  unreachable
bb57:
  %t58 = getelementptr [16 x float], ptr %t49, i64 0, i64 %t52
  %t59 = load float, ptr %t58
  store float %t59, ptr %o7
  %t60 = getelementptr { [16 x float], i32 }, ptr %o10, i32 0, i32 0
  %t61 = getelementptr { [16 x float], i32 }, ptr %o10, i32 0, i32 1
  %t62 = load i32, ptr %t61
  %t63 = sext i32 %t62 to i64
  %t64 = icmp slt i64 %t63, 0
  %t65 = icmp sge i64 %t63, 16
  %t66 = or i1 %t64, %t65
  br i1 %t66, label %bb67, label %bb68
bb67:
  call void @flow_trap(i32 1)
  unreachable
bb68:
  %t69 = getelementptr [16 x float], ptr %t60, i64 0, i64 %t63
  %t70 = load float, ptr %t69
  store float %t70, ptr %o11
  %t71 = load float, ptr %o7
  store float %t71, ptr %o8
  %t72 = load float, ptr %o11
  store float %t72, ptr %o12
  %t73 = load float, ptr %o8
  call void @flow_print_f32(float %t73, i1 zeroext true)
  %t74 = load float, ptr %o12
  call void @flow_print_f32(float %t74, i1 zeroext true)
  ret void
}

define i32 @main() {
entry:
  call void @flow_main()
  ret i32 0
}

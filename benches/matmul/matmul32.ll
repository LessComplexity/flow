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
  store i32 32, ptr %t10
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
  store i32 32, ptr %t23
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
  store i32 32, ptr %t41
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
  %t87 = icmp sge i64 %t85, 1024
  %t88 = or i1 %t86, %t87
  br i1 %t88, label %bb89, label %bb90
bb89:
  call void @flow_trap(i32 1)
  unreachable
bb90:
  %t91 = getelementptr [1024 x float], ptr %t82, i64 0, i64 %t85
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
  %t107 = icmp sge i64 %t105, 1024
  %t108 = or i1 %t106, %t107
  br i1 %t108, label %bb109, label %bb110
bb109:
  call void @flow_trap(i32 1)
  unreachable
bb110:
  %t111 = getelementptr [1024 x float], ptr %t102, i64 0, i64 %t105
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

define internal [1024 x float] @fn1({ ptr, ptr } %arg) {
entry:
  %o0 = alloca { ptr, ptr }
  %o1 = alloca [1024 x float]
  %o2 = alloca ptr
  %o3 = alloca ptr
  %o4 = alloca { [1024 x float], i32 }
  %o5 = alloca { [1024 x float], i32 }
  %o6 = alloca [1024 x float]
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
  %o20 = alloca { [1024 x float], i32 }
  %o21 = alloca { { [1024 x float], i32 }, i1 }
  %o22 = alloca { [1024 x float], i1 }
  %s67 = alloca { ptr, ptr, i32, i32 }
  store { ptr, ptr } %arg, ptr %o0
  %t0 = getelementptr { ptr, ptr }, ptr %o0, i32 0, i32 0
  %t1 = load ptr, ptr %t0
  store ptr %t1, ptr %o2
  %t2 = getelementptr { ptr, ptr }, ptr %o0, i32 0, i32 1
  %t3 = load ptr, ptr %t2
  store ptr %t3, ptr %o3
  %t4 = getelementptr { [1024 x float], i32 }, ptr %o4, i32 0, i32 1
  store i32 0, ptr %t4
  %t5 = load ptr, ptr %o3
  %t6 = getelementptr { [1024 x float], i32 }, ptr %o4, i32 0, i32 0
  call void @llvm.memcpy.p0.p0.i64(ptr %t6, ptr %t5, i64 ptrtoint (ptr getelementptr ([1024 x float], ptr null, i64 1) to i64), i1 false)
  %t7 = load { [1024 x float], i32 }, ptr %o4
  store { [1024 x float], i32 } %t7, ptr %o5
  br label %bb8
bb8:
  %t12 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  store i32 1024, ptr %t12
  %t13 = getelementptr { [1024 x float], i32 }, ptr %o5, i32 0, i32 0
  call void @llvm.memcpy.p0.p0.i64(ptr %o6, ptr %t13, i64 ptrtoint (ptr getelementptr ([1024 x float], ptr null, i64 1) to i64), i1 false)
  %t14 = getelementptr { [1024 x float], i32 }, ptr %o5, i32 0, i32 1
  %t15 = load i32, ptr %t14
  store i32 %t15, ptr %o7
  %t16 = getelementptr { [1024 x float], i1 }, ptr %o22, i32 0, i32 0
  call void @llvm.memcpy.p0.p0.i64(ptr %t16, ptr %o6, i64 ptrtoint (ptr getelementptr ([1024 x float], ptr null, i64 1) to i64), i1 false)
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
  %t25 = getelementptr { [1024 x float], i1 }, ptr %o22, i32 0, i32 1
  store i1 %t24, ptr %t25
  %t26 = getelementptr { [1024 x float], i1 }, ptr %o22, i32 0, i32 1
  %t27 = load i1, ptr %t26
  br i1 %t27, label %bb9, label %bb10
bb9:
  %t28 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  store i32 32, ptr %t28
  %t29 = getelementptr { i32, i32 }, ptr %o12, i32 0, i32 1
  store i32 32, ptr %t29
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
  %t60 = getelementptr { { [1024 x float], i32 }, i1 }, ptr %o21, i32 0, i32 1
  store i1 %t59, ptr %t60
  %t61 = load i32, ptr %o11
  %t62 = getelementptr { ptr, ptr, i32, i32 }, ptr %o14, i32 0, i32 2
  store i32 %t61, ptr %t62
  %t63 = load i32, ptr %o13
  %t64 = getelementptr { ptr, ptr, i32, i32 }, ptr %o14, i32 0, i32 3
  store i32 %t63, ptr %t64
  %t65 = load i32, ptr %o19
  %t66 = getelementptr { [1024 x float], i32 }, ptr %o20, i32 0, i32 1
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
  %t86 = icmp sge i64 %t84, 1024
  %t87 = or i1 %t85, %t86
  br i1 %t87, label %bb88, label %bb89
bb88:
  call void @flow_trap(i32 1)
  unreachable
bb89:
  %t90 = getelementptr [1024 x float], ptr %o6, i64 0, i64 %t84
  %t91 = getelementptr { ptr, i32, float }, ptr %o16, i32 0, i32 2
  %t92 = load float, ptr %t91
  store float %t92, ptr %t90
  %t93 = getelementptr { [1024 x float], i32 }, ptr %o20, i32 0, i32 0
  call void @llvm.memcpy.p0.p0.i64(ptr %t93, ptr %o6, i64 ptrtoint (ptr getelementptr ([1024 x float], ptr null, i64 1) to i64), i1 false)
  %t94 = load { [1024 x float], i32 }, ptr %o20
  %t95 = getelementptr { { [1024 x float], i32 }, i1 }, ptr %o21, i32 0, i32 0
  store { [1024 x float], i32 } %t94, ptr %t95
  %t96 = getelementptr { { [1024 x float], i32 }, i1 }, ptr %o21, i32 0, i32 0
  %t97 = load { [1024 x float], i32 }, ptr %t96
  store { [1024 x float], i32 } %t97, ptr %o5
  br label %bb8
bb10:
  call void @llvm.memcpy.p0.p0.i64(ptr %o1, ptr %o6, i64 ptrtoint (ptr getelementptr ([1024 x float], ptr null, i64 1) to i64), i1 false)
  br label %bb11
bb11:
  %t98 = load [1024 x float], ptr %o1
  ret [1024 x float] %t98
}

define internal void @flow_main() {
entry:
  %o2 = alloca [1024 x float]
  %o3 = alloca [1024 x float]
  %o4 = alloca { ptr, ptr }
  %o5 = alloca [1024 x float]
  %o6 = alloca { ptr, i32 }
  %o7 = alloca float
  %o8 = alloca float
  %o10 = alloca { ptr, i32 }
  %o11 = alloca float
  %o12 = alloca float
  %s2052 = alloca { ptr, ptr }
  %t0 = getelementptr [1024 x float], ptr %o2, i64 0, i64 0
  store float 0xC042800000000000, ptr %t0
  %t1 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1
  store float 0xC03E000000000000, ptr %t1
  %t2 = getelementptr [1024 x float], ptr %o2, i64 0, i64 2
  store float 0xC037000000000000, ptr %t2
  %t3 = getelementptr [1024 x float], ptr %o2, i64 0, i64 3
  store float 0xC030000000000000, ptr %t3
  %t4 = getelementptr [1024 x float], ptr %o2, i64 0, i64 4
  store float 0xC022000000000000, ptr %t4
  %t5 = getelementptr [1024 x float], ptr %o2, i64 0, i64 5
  store float 0xC000000000000000, ptr %t5
  %t6 = getelementptr [1024 x float], ptr %o2, i64 0, i64 6
  store float 0x4014000000000000, ptr %t6
  %t7 = getelementptr [1024 x float], ptr %o2, i64 0, i64 7
  store float 0x4028000000000000, ptr %t7
  %t8 = getelementptr [1024 x float], ptr %o2, i64 0, i64 8
  store float 0x4033000000000000, ptr %t8
  %t9 = getelementptr [1024 x float], ptr %o2, i64 0, i64 9
  store float 0x403A000000000000, ptr %t9
  %t10 = getelementptr [1024 x float], ptr %o2, i64 0, i64 10
  store float 0x4040800000000000, ptr %t10
  %t11 = getelementptr [1024 x float], ptr %o2, i64 0, i64 11
  store float 0x4044000000000000, ptr %t11
  %t12 = getelementptr [1024 x float], ptr %o2, i64 0, i64 12
  store float 0x4047800000000000, ptr %t12
  %t13 = getelementptr [1024 x float], ptr %o2, i64 0, i64 13
  store float 0xC047800000000000, ptr %t13
  %t14 = getelementptr [1024 x float], ptr %o2, i64 0, i64 14
  store float 0xC044000000000000, ptr %t14
  %t15 = getelementptr [1024 x float], ptr %o2, i64 0, i64 15
  store float 0xC040800000000000, ptr %t15
  %t16 = getelementptr [1024 x float], ptr %o2, i64 0, i64 16
  store float 0xC03A000000000000, ptr %t16
  %t17 = getelementptr [1024 x float], ptr %o2, i64 0, i64 17
  store float 0xC033000000000000, ptr %t17
  %t18 = getelementptr [1024 x float], ptr %o2, i64 0, i64 18
  store float 0xC028000000000000, ptr %t18
  %t19 = getelementptr [1024 x float], ptr %o2, i64 0, i64 19
  store float 0xC014000000000000, ptr %t19
  %t20 = getelementptr [1024 x float], ptr %o2, i64 0, i64 20
  store float 0x4000000000000000, ptr %t20
  %t21 = getelementptr [1024 x float], ptr %o2, i64 0, i64 21
  store float 0x4022000000000000, ptr %t21
  %t22 = getelementptr [1024 x float], ptr %o2, i64 0, i64 22
  store float 0x4030000000000000, ptr %t22
  %t23 = getelementptr [1024 x float], ptr %o2, i64 0, i64 23
  store float 0x4037000000000000, ptr %t23
  %t24 = getelementptr [1024 x float], ptr %o2, i64 0, i64 24
  store float 0x403E000000000000, ptr %t24
  %t25 = getelementptr [1024 x float], ptr %o2, i64 0, i64 25
  store float 0x4042800000000000, ptr %t25
  %t26 = getelementptr [1024 x float], ptr %o2, i64 0, i64 26
  store float 0x4046000000000000, ptr %t26
  %t27 = getelementptr [1024 x float], ptr %o2, i64 0, i64 27
  store float 0xC049000000000000, ptr %t27
  %t28 = getelementptr [1024 x float], ptr %o2, i64 0, i64 28
  store float 0xC045800000000000, ptr %t28
  %t29 = getelementptr [1024 x float], ptr %o2, i64 0, i64 29
  store float 0xC042000000000000, ptr %t29
  %t30 = getelementptr [1024 x float], ptr %o2, i64 0, i64 30
  store float 0xC03D000000000000, ptr %t30
  %t31 = getelementptr [1024 x float], ptr %o2, i64 0, i64 31
  store float 0xC036000000000000, ptr %t31
  %t32 = getelementptr [1024 x float], ptr %o2, i64 0, i64 32
  store float 0xC02E000000000000, ptr %t32
  %t33 = getelementptr [1024 x float], ptr %o2, i64 0, i64 33
  store float 0xC020000000000000, ptr %t33
  %t34 = getelementptr [1024 x float], ptr %o2, i64 0, i64 34
  store float 0xBFF0000000000000, ptr %t34
  %t35 = getelementptr [1024 x float], ptr %o2, i64 0, i64 35
  store float 0x4018000000000000, ptr %t35
  %t36 = getelementptr [1024 x float], ptr %o2, i64 0, i64 36
  store float 0x402A000000000000, ptr %t36
  %t37 = getelementptr [1024 x float], ptr %o2, i64 0, i64 37
  store float 0x4034000000000000, ptr %t37
  %t38 = getelementptr [1024 x float], ptr %o2, i64 0, i64 38
  store float 0x403B000000000000, ptr %t38
  %t39 = getelementptr [1024 x float], ptr %o2, i64 0, i64 39
  store float 0x4041000000000000, ptr %t39
  %t40 = getelementptr [1024 x float], ptr %o2, i64 0, i64 40
  store float 0x4044800000000000, ptr %t40
  %t41 = getelementptr [1024 x float], ptr %o2, i64 0, i64 41
  store float 0x4048000000000000, ptr %t41
  %t42 = getelementptr [1024 x float], ptr %o2, i64 0, i64 42
  store float 0xC047000000000000, ptr %t42
  %t43 = getelementptr [1024 x float], ptr %o2, i64 0, i64 43
  store float 0xC043800000000000, ptr %t43
  %t44 = getelementptr [1024 x float], ptr %o2, i64 0, i64 44
  store float 0xC040000000000000, ptr %t44
  %t45 = getelementptr [1024 x float], ptr %o2, i64 0, i64 45
  store float 0xC039000000000000, ptr %t45
  %t46 = getelementptr [1024 x float], ptr %o2, i64 0, i64 46
  store float 0xC032000000000000, ptr %t46
  %t47 = getelementptr [1024 x float], ptr %o2, i64 0, i64 47
  store float 0xC026000000000000, ptr %t47
  %t48 = getelementptr [1024 x float], ptr %o2, i64 0, i64 48
  store float 0xC010000000000000, ptr %t48
  %t49 = getelementptr [1024 x float], ptr %o2, i64 0, i64 49
  store float 0x4008000000000000, ptr %t49
  %t50 = getelementptr [1024 x float], ptr %o2, i64 0, i64 50
  store float 0x4024000000000000, ptr %t50
  %t51 = getelementptr [1024 x float], ptr %o2, i64 0, i64 51
  store float 0x4031000000000000, ptr %t51
  %t52 = getelementptr [1024 x float], ptr %o2, i64 0, i64 52
  store float 0x4038000000000000, ptr %t52
  %t53 = getelementptr [1024 x float], ptr %o2, i64 0, i64 53
  store float 0x403F000000000000, ptr %t53
  %t54 = getelementptr [1024 x float], ptr %o2, i64 0, i64 54
  store float 0x4043000000000000, ptr %t54
  %t55 = getelementptr [1024 x float], ptr %o2, i64 0, i64 55
  store float 0x4046800000000000, ptr %t55
  %t56 = getelementptr [1024 x float], ptr %o2, i64 0, i64 56
  store float 0xC048800000000000, ptr %t56
  %t57 = getelementptr [1024 x float], ptr %o2, i64 0, i64 57
  store float 0xC045000000000000, ptr %t57
  %t58 = getelementptr [1024 x float], ptr %o2, i64 0, i64 58
  store float 0xC041800000000000, ptr %t58
  %t59 = getelementptr [1024 x float], ptr %o2, i64 0, i64 59
  store float 0xC03C000000000000, ptr %t59
  %t60 = getelementptr [1024 x float], ptr %o2, i64 0, i64 60
  store float 0xC035000000000000, ptr %t60
  %t61 = getelementptr [1024 x float], ptr %o2, i64 0, i64 61
  store float 0xC02C000000000000, ptr %t61
  %t62 = getelementptr [1024 x float], ptr %o2, i64 0, i64 62
  store float 0xC01C000000000000, ptr %t62
  %t63 = getelementptr [1024 x float], ptr %o2, i64 0, i64 63
  store float 0x0000000000000000, ptr %t63
  %t64 = getelementptr [1024 x float], ptr %o2, i64 0, i64 64
  store float 0x401C000000000000, ptr %t64
  %t65 = getelementptr [1024 x float], ptr %o2, i64 0, i64 65
  store float 0x402C000000000000, ptr %t65
  %t66 = getelementptr [1024 x float], ptr %o2, i64 0, i64 66
  store float 0x4035000000000000, ptr %t66
  %t67 = getelementptr [1024 x float], ptr %o2, i64 0, i64 67
  store float 0x403C000000000000, ptr %t67
  %t68 = getelementptr [1024 x float], ptr %o2, i64 0, i64 68
  store float 0x4041800000000000, ptr %t68
  %t69 = getelementptr [1024 x float], ptr %o2, i64 0, i64 69
  store float 0x4045000000000000, ptr %t69
  %t70 = getelementptr [1024 x float], ptr %o2, i64 0, i64 70
  store float 0x4048800000000000, ptr %t70
  %t71 = getelementptr [1024 x float], ptr %o2, i64 0, i64 71
  store float 0xC046800000000000, ptr %t71
  %t72 = getelementptr [1024 x float], ptr %o2, i64 0, i64 72
  store float 0xC043000000000000, ptr %t72
  %t73 = getelementptr [1024 x float], ptr %o2, i64 0, i64 73
  store float 0xC03F000000000000, ptr %t73
  %t74 = getelementptr [1024 x float], ptr %o2, i64 0, i64 74
  store float 0xC038000000000000, ptr %t74
  %t75 = getelementptr [1024 x float], ptr %o2, i64 0, i64 75
  store float 0xC031000000000000, ptr %t75
  %t76 = getelementptr [1024 x float], ptr %o2, i64 0, i64 76
  store float 0xC024000000000000, ptr %t76
  %t77 = getelementptr [1024 x float], ptr %o2, i64 0, i64 77
  store float 0xC008000000000000, ptr %t77
  %t78 = getelementptr [1024 x float], ptr %o2, i64 0, i64 78
  store float 0x4010000000000000, ptr %t78
  %t79 = getelementptr [1024 x float], ptr %o2, i64 0, i64 79
  store float 0x4026000000000000, ptr %t79
  %t80 = getelementptr [1024 x float], ptr %o2, i64 0, i64 80
  store float 0x4032000000000000, ptr %t80
  %t81 = getelementptr [1024 x float], ptr %o2, i64 0, i64 81
  store float 0x4039000000000000, ptr %t81
  %t82 = getelementptr [1024 x float], ptr %o2, i64 0, i64 82
  store float 0x4040000000000000, ptr %t82
  %t83 = getelementptr [1024 x float], ptr %o2, i64 0, i64 83
  store float 0x4043800000000000, ptr %t83
  %t84 = getelementptr [1024 x float], ptr %o2, i64 0, i64 84
  store float 0x4047000000000000, ptr %t84
  %t85 = getelementptr [1024 x float], ptr %o2, i64 0, i64 85
  store float 0xC048000000000000, ptr %t85
  %t86 = getelementptr [1024 x float], ptr %o2, i64 0, i64 86
  store float 0xC044800000000000, ptr %t86
  %t87 = getelementptr [1024 x float], ptr %o2, i64 0, i64 87
  store float 0xC041000000000000, ptr %t87
  %t88 = getelementptr [1024 x float], ptr %o2, i64 0, i64 88
  store float 0xC03B000000000000, ptr %t88
  %t89 = getelementptr [1024 x float], ptr %o2, i64 0, i64 89
  store float 0xC034000000000000, ptr %t89
  %t90 = getelementptr [1024 x float], ptr %o2, i64 0, i64 90
  store float 0xC02A000000000000, ptr %t90
  %t91 = getelementptr [1024 x float], ptr %o2, i64 0, i64 91
  store float 0xC018000000000000, ptr %t91
  %t92 = getelementptr [1024 x float], ptr %o2, i64 0, i64 92
  store float 0x3FF0000000000000, ptr %t92
  %t93 = getelementptr [1024 x float], ptr %o2, i64 0, i64 93
  store float 0x4020000000000000, ptr %t93
  %t94 = getelementptr [1024 x float], ptr %o2, i64 0, i64 94
  store float 0x402E000000000000, ptr %t94
  %t95 = getelementptr [1024 x float], ptr %o2, i64 0, i64 95
  store float 0x4036000000000000, ptr %t95
  %t96 = getelementptr [1024 x float], ptr %o2, i64 0, i64 96
  store float 0x403D000000000000, ptr %t96
  %t97 = getelementptr [1024 x float], ptr %o2, i64 0, i64 97
  store float 0x4042000000000000, ptr %t97
  %t98 = getelementptr [1024 x float], ptr %o2, i64 0, i64 98
  store float 0x4045800000000000, ptr %t98
  %t99 = getelementptr [1024 x float], ptr %o2, i64 0, i64 99
  store float 0x4049000000000000, ptr %t99
  %t100 = getelementptr [1024 x float], ptr %o2, i64 0, i64 100
  store float 0xC046000000000000, ptr %t100
  %t101 = getelementptr [1024 x float], ptr %o2, i64 0, i64 101
  store float 0xC042800000000000, ptr %t101
  %t102 = getelementptr [1024 x float], ptr %o2, i64 0, i64 102
  store float 0xC03E000000000000, ptr %t102
  %t103 = getelementptr [1024 x float], ptr %o2, i64 0, i64 103
  store float 0xC037000000000000, ptr %t103
  %t104 = getelementptr [1024 x float], ptr %o2, i64 0, i64 104
  store float 0xC030000000000000, ptr %t104
  %t105 = getelementptr [1024 x float], ptr %o2, i64 0, i64 105
  store float 0xC022000000000000, ptr %t105
  %t106 = getelementptr [1024 x float], ptr %o2, i64 0, i64 106
  store float 0xC000000000000000, ptr %t106
  %t107 = getelementptr [1024 x float], ptr %o2, i64 0, i64 107
  store float 0x4014000000000000, ptr %t107
  %t108 = getelementptr [1024 x float], ptr %o2, i64 0, i64 108
  store float 0x4028000000000000, ptr %t108
  %t109 = getelementptr [1024 x float], ptr %o2, i64 0, i64 109
  store float 0x4033000000000000, ptr %t109
  %t110 = getelementptr [1024 x float], ptr %o2, i64 0, i64 110
  store float 0x403A000000000000, ptr %t110
  %t111 = getelementptr [1024 x float], ptr %o2, i64 0, i64 111
  store float 0x4040800000000000, ptr %t111
  %t112 = getelementptr [1024 x float], ptr %o2, i64 0, i64 112
  store float 0x4044000000000000, ptr %t112
  %t113 = getelementptr [1024 x float], ptr %o2, i64 0, i64 113
  store float 0x4047800000000000, ptr %t113
  %t114 = getelementptr [1024 x float], ptr %o2, i64 0, i64 114
  store float 0xC047800000000000, ptr %t114
  %t115 = getelementptr [1024 x float], ptr %o2, i64 0, i64 115
  store float 0xC044000000000000, ptr %t115
  %t116 = getelementptr [1024 x float], ptr %o2, i64 0, i64 116
  store float 0xC040800000000000, ptr %t116
  %t117 = getelementptr [1024 x float], ptr %o2, i64 0, i64 117
  store float 0xC03A000000000000, ptr %t117
  %t118 = getelementptr [1024 x float], ptr %o2, i64 0, i64 118
  store float 0xC033000000000000, ptr %t118
  %t119 = getelementptr [1024 x float], ptr %o2, i64 0, i64 119
  store float 0xC028000000000000, ptr %t119
  %t120 = getelementptr [1024 x float], ptr %o2, i64 0, i64 120
  store float 0xC014000000000000, ptr %t120
  %t121 = getelementptr [1024 x float], ptr %o2, i64 0, i64 121
  store float 0x4000000000000000, ptr %t121
  %t122 = getelementptr [1024 x float], ptr %o2, i64 0, i64 122
  store float 0x4022000000000000, ptr %t122
  %t123 = getelementptr [1024 x float], ptr %o2, i64 0, i64 123
  store float 0x4030000000000000, ptr %t123
  %t124 = getelementptr [1024 x float], ptr %o2, i64 0, i64 124
  store float 0x4037000000000000, ptr %t124
  %t125 = getelementptr [1024 x float], ptr %o2, i64 0, i64 125
  store float 0x403E000000000000, ptr %t125
  %t126 = getelementptr [1024 x float], ptr %o2, i64 0, i64 126
  store float 0x4042800000000000, ptr %t126
  %t127 = getelementptr [1024 x float], ptr %o2, i64 0, i64 127
  store float 0x4046000000000000, ptr %t127
  %t128 = getelementptr [1024 x float], ptr %o2, i64 0, i64 128
  store float 0xC049000000000000, ptr %t128
  %t129 = getelementptr [1024 x float], ptr %o2, i64 0, i64 129
  store float 0xC045800000000000, ptr %t129
  %t130 = getelementptr [1024 x float], ptr %o2, i64 0, i64 130
  store float 0xC042000000000000, ptr %t130
  %t131 = getelementptr [1024 x float], ptr %o2, i64 0, i64 131
  store float 0xC03D000000000000, ptr %t131
  %t132 = getelementptr [1024 x float], ptr %o2, i64 0, i64 132
  store float 0xC036000000000000, ptr %t132
  %t133 = getelementptr [1024 x float], ptr %o2, i64 0, i64 133
  store float 0xC02E000000000000, ptr %t133
  %t134 = getelementptr [1024 x float], ptr %o2, i64 0, i64 134
  store float 0xC020000000000000, ptr %t134
  %t135 = getelementptr [1024 x float], ptr %o2, i64 0, i64 135
  store float 0xBFF0000000000000, ptr %t135
  %t136 = getelementptr [1024 x float], ptr %o2, i64 0, i64 136
  store float 0x4018000000000000, ptr %t136
  %t137 = getelementptr [1024 x float], ptr %o2, i64 0, i64 137
  store float 0x402A000000000000, ptr %t137
  %t138 = getelementptr [1024 x float], ptr %o2, i64 0, i64 138
  store float 0x4034000000000000, ptr %t138
  %t139 = getelementptr [1024 x float], ptr %o2, i64 0, i64 139
  store float 0x403B000000000000, ptr %t139
  %t140 = getelementptr [1024 x float], ptr %o2, i64 0, i64 140
  store float 0x4041000000000000, ptr %t140
  %t141 = getelementptr [1024 x float], ptr %o2, i64 0, i64 141
  store float 0x4044800000000000, ptr %t141
  %t142 = getelementptr [1024 x float], ptr %o2, i64 0, i64 142
  store float 0x4048000000000000, ptr %t142
  %t143 = getelementptr [1024 x float], ptr %o2, i64 0, i64 143
  store float 0xC047000000000000, ptr %t143
  %t144 = getelementptr [1024 x float], ptr %o2, i64 0, i64 144
  store float 0xC043800000000000, ptr %t144
  %t145 = getelementptr [1024 x float], ptr %o2, i64 0, i64 145
  store float 0xC040000000000000, ptr %t145
  %t146 = getelementptr [1024 x float], ptr %o2, i64 0, i64 146
  store float 0xC039000000000000, ptr %t146
  %t147 = getelementptr [1024 x float], ptr %o2, i64 0, i64 147
  store float 0xC032000000000000, ptr %t147
  %t148 = getelementptr [1024 x float], ptr %o2, i64 0, i64 148
  store float 0xC026000000000000, ptr %t148
  %t149 = getelementptr [1024 x float], ptr %o2, i64 0, i64 149
  store float 0xC010000000000000, ptr %t149
  %t150 = getelementptr [1024 x float], ptr %o2, i64 0, i64 150
  store float 0x4008000000000000, ptr %t150
  %t151 = getelementptr [1024 x float], ptr %o2, i64 0, i64 151
  store float 0x4024000000000000, ptr %t151
  %t152 = getelementptr [1024 x float], ptr %o2, i64 0, i64 152
  store float 0x4031000000000000, ptr %t152
  %t153 = getelementptr [1024 x float], ptr %o2, i64 0, i64 153
  store float 0x4038000000000000, ptr %t153
  %t154 = getelementptr [1024 x float], ptr %o2, i64 0, i64 154
  store float 0x403F000000000000, ptr %t154
  %t155 = getelementptr [1024 x float], ptr %o2, i64 0, i64 155
  store float 0x4043000000000000, ptr %t155
  %t156 = getelementptr [1024 x float], ptr %o2, i64 0, i64 156
  store float 0x4046800000000000, ptr %t156
  %t157 = getelementptr [1024 x float], ptr %o2, i64 0, i64 157
  store float 0xC048800000000000, ptr %t157
  %t158 = getelementptr [1024 x float], ptr %o2, i64 0, i64 158
  store float 0xC045000000000000, ptr %t158
  %t159 = getelementptr [1024 x float], ptr %o2, i64 0, i64 159
  store float 0xC041800000000000, ptr %t159
  %t160 = getelementptr [1024 x float], ptr %o2, i64 0, i64 160
  store float 0xC03C000000000000, ptr %t160
  %t161 = getelementptr [1024 x float], ptr %o2, i64 0, i64 161
  store float 0xC035000000000000, ptr %t161
  %t162 = getelementptr [1024 x float], ptr %o2, i64 0, i64 162
  store float 0xC02C000000000000, ptr %t162
  %t163 = getelementptr [1024 x float], ptr %o2, i64 0, i64 163
  store float 0xC01C000000000000, ptr %t163
  %t164 = getelementptr [1024 x float], ptr %o2, i64 0, i64 164
  store float 0x0000000000000000, ptr %t164
  %t165 = getelementptr [1024 x float], ptr %o2, i64 0, i64 165
  store float 0x401C000000000000, ptr %t165
  %t166 = getelementptr [1024 x float], ptr %o2, i64 0, i64 166
  store float 0x402C000000000000, ptr %t166
  %t167 = getelementptr [1024 x float], ptr %o2, i64 0, i64 167
  store float 0x4035000000000000, ptr %t167
  %t168 = getelementptr [1024 x float], ptr %o2, i64 0, i64 168
  store float 0x403C000000000000, ptr %t168
  %t169 = getelementptr [1024 x float], ptr %o2, i64 0, i64 169
  store float 0x4041800000000000, ptr %t169
  %t170 = getelementptr [1024 x float], ptr %o2, i64 0, i64 170
  store float 0x4045000000000000, ptr %t170
  %t171 = getelementptr [1024 x float], ptr %o2, i64 0, i64 171
  store float 0x4048800000000000, ptr %t171
  %t172 = getelementptr [1024 x float], ptr %o2, i64 0, i64 172
  store float 0xC046800000000000, ptr %t172
  %t173 = getelementptr [1024 x float], ptr %o2, i64 0, i64 173
  store float 0xC043000000000000, ptr %t173
  %t174 = getelementptr [1024 x float], ptr %o2, i64 0, i64 174
  store float 0xC03F000000000000, ptr %t174
  %t175 = getelementptr [1024 x float], ptr %o2, i64 0, i64 175
  store float 0xC038000000000000, ptr %t175
  %t176 = getelementptr [1024 x float], ptr %o2, i64 0, i64 176
  store float 0xC031000000000000, ptr %t176
  %t177 = getelementptr [1024 x float], ptr %o2, i64 0, i64 177
  store float 0xC024000000000000, ptr %t177
  %t178 = getelementptr [1024 x float], ptr %o2, i64 0, i64 178
  store float 0xC008000000000000, ptr %t178
  %t179 = getelementptr [1024 x float], ptr %o2, i64 0, i64 179
  store float 0x4010000000000000, ptr %t179
  %t180 = getelementptr [1024 x float], ptr %o2, i64 0, i64 180
  store float 0x4026000000000000, ptr %t180
  %t181 = getelementptr [1024 x float], ptr %o2, i64 0, i64 181
  store float 0x4032000000000000, ptr %t181
  %t182 = getelementptr [1024 x float], ptr %o2, i64 0, i64 182
  store float 0x4039000000000000, ptr %t182
  %t183 = getelementptr [1024 x float], ptr %o2, i64 0, i64 183
  store float 0x4040000000000000, ptr %t183
  %t184 = getelementptr [1024 x float], ptr %o2, i64 0, i64 184
  store float 0x4043800000000000, ptr %t184
  %t185 = getelementptr [1024 x float], ptr %o2, i64 0, i64 185
  store float 0x4047000000000000, ptr %t185
  %t186 = getelementptr [1024 x float], ptr %o2, i64 0, i64 186
  store float 0xC048000000000000, ptr %t186
  %t187 = getelementptr [1024 x float], ptr %o2, i64 0, i64 187
  store float 0xC044800000000000, ptr %t187
  %t188 = getelementptr [1024 x float], ptr %o2, i64 0, i64 188
  store float 0xC041000000000000, ptr %t188
  %t189 = getelementptr [1024 x float], ptr %o2, i64 0, i64 189
  store float 0xC03B000000000000, ptr %t189
  %t190 = getelementptr [1024 x float], ptr %o2, i64 0, i64 190
  store float 0xC034000000000000, ptr %t190
  %t191 = getelementptr [1024 x float], ptr %o2, i64 0, i64 191
  store float 0xC02A000000000000, ptr %t191
  %t192 = getelementptr [1024 x float], ptr %o2, i64 0, i64 192
  store float 0xC018000000000000, ptr %t192
  %t193 = getelementptr [1024 x float], ptr %o2, i64 0, i64 193
  store float 0x3FF0000000000000, ptr %t193
  %t194 = getelementptr [1024 x float], ptr %o2, i64 0, i64 194
  store float 0x4020000000000000, ptr %t194
  %t195 = getelementptr [1024 x float], ptr %o2, i64 0, i64 195
  store float 0x402E000000000000, ptr %t195
  %t196 = getelementptr [1024 x float], ptr %o2, i64 0, i64 196
  store float 0x4036000000000000, ptr %t196
  %t197 = getelementptr [1024 x float], ptr %o2, i64 0, i64 197
  store float 0x403D000000000000, ptr %t197
  %t198 = getelementptr [1024 x float], ptr %o2, i64 0, i64 198
  store float 0x4042000000000000, ptr %t198
  %t199 = getelementptr [1024 x float], ptr %o2, i64 0, i64 199
  store float 0x4045800000000000, ptr %t199
  %t200 = getelementptr [1024 x float], ptr %o2, i64 0, i64 200
  store float 0x4049000000000000, ptr %t200
  %t201 = getelementptr [1024 x float], ptr %o2, i64 0, i64 201
  store float 0xC046000000000000, ptr %t201
  %t202 = getelementptr [1024 x float], ptr %o2, i64 0, i64 202
  store float 0xC042800000000000, ptr %t202
  %t203 = getelementptr [1024 x float], ptr %o2, i64 0, i64 203
  store float 0xC03E000000000000, ptr %t203
  %t204 = getelementptr [1024 x float], ptr %o2, i64 0, i64 204
  store float 0xC037000000000000, ptr %t204
  %t205 = getelementptr [1024 x float], ptr %o2, i64 0, i64 205
  store float 0xC030000000000000, ptr %t205
  %t206 = getelementptr [1024 x float], ptr %o2, i64 0, i64 206
  store float 0xC022000000000000, ptr %t206
  %t207 = getelementptr [1024 x float], ptr %o2, i64 0, i64 207
  store float 0xC000000000000000, ptr %t207
  %t208 = getelementptr [1024 x float], ptr %o2, i64 0, i64 208
  store float 0x4014000000000000, ptr %t208
  %t209 = getelementptr [1024 x float], ptr %o2, i64 0, i64 209
  store float 0x4028000000000000, ptr %t209
  %t210 = getelementptr [1024 x float], ptr %o2, i64 0, i64 210
  store float 0x4033000000000000, ptr %t210
  %t211 = getelementptr [1024 x float], ptr %o2, i64 0, i64 211
  store float 0x403A000000000000, ptr %t211
  %t212 = getelementptr [1024 x float], ptr %o2, i64 0, i64 212
  store float 0x4040800000000000, ptr %t212
  %t213 = getelementptr [1024 x float], ptr %o2, i64 0, i64 213
  store float 0x4044000000000000, ptr %t213
  %t214 = getelementptr [1024 x float], ptr %o2, i64 0, i64 214
  store float 0x4047800000000000, ptr %t214
  %t215 = getelementptr [1024 x float], ptr %o2, i64 0, i64 215
  store float 0xC047800000000000, ptr %t215
  %t216 = getelementptr [1024 x float], ptr %o2, i64 0, i64 216
  store float 0xC044000000000000, ptr %t216
  %t217 = getelementptr [1024 x float], ptr %o2, i64 0, i64 217
  store float 0xC040800000000000, ptr %t217
  %t218 = getelementptr [1024 x float], ptr %o2, i64 0, i64 218
  store float 0xC03A000000000000, ptr %t218
  %t219 = getelementptr [1024 x float], ptr %o2, i64 0, i64 219
  store float 0xC033000000000000, ptr %t219
  %t220 = getelementptr [1024 x float], ptr %o2, i64 0, i64 220
  store float 0xC028000000000000, ptr %t220
  %t221 = getelementptr [1024 x float], ptr %o2, i64 0, i64 221
  store float 0xC014000000000000, ptr %t221
  %t222 = getelementptr [1024 x float], ptr %o2, i64 0, i64 222
  store float 0x4000000000000000, ptr %t222
  %t223 = getelementptr [1024 x float], ptr %o2, i64 0, i64 223
  store float 0x4022000000000000, ptr %t223
  %t224 = getelementptr [1024 x float], ptr %o2, i64 0, i64 224
  store float 0x4030000000000000, ptr %t224
  %t225 = getelementptr [1024 x float], ptr %o2, i64 0, i64 225
  store float 0x4037000000000000, ptr %t225
  %t226 = getelementptr [1024 x float], ptr %o2, i64 0, i64 226
  store float 0x403E000000000000, ptr %t226
  %t227 = getelementptr [1024 x float], ptr %o2, i64 0, i64 227
  store float 0x4042800000000000, ptr %t227
  %t228 = getelementptr [1024 x float], ptr %o2, i64 0, i64 228
  store float 0x4046000000000000, ptr %t228
  %t229 = getelementptr [1024 x float], ptr %o2, i64 0, i64 229
  store float 0xC049000000000000, ptr %t229
  %t230 = getelementptr [1024 x float], ptr %o2, i64 0, i64 230
  store float 0xC045800000000000, ptr %t230
  %t231 = getelementptr [1024 x float], ptr %o2, i64 0, i64 231
  store float 0xC042000000000000, ptr %t231
  %t232 = getelementptr [1024 x float], ptr %o2, i64 0, i64 232
  store float 0xC03D000000000000, ptr %t232
  %t233 = getelementptr [1024 x float], ptr %o2, i64 0, i64 233
  store float 0xC036000000000000, ptr %t233
  %t234 = getelementptr [1024 x float], ptr %o2, i64 0, i64 234
  store float 0xC02E000000000000, ptr %t234
  %t235 = getelementptr [1024 x float], ptr %o2, i64 0, i64 235
  store float 0xC020000000000000, ptr %t235
  %t236 = getelementptr [1024 x float], ptr %o2, i64 0, i64 236
  store float 0xBFF0000000000000, ptr %t236
  %t237 = getelementptr [1024 x float], ptr %o2, i64 0, i64 237
  store float 0x4018000000000000, ptr %t237
  %t238 = getelementptr [1024 x float], ptr %o2, i64 0, i64 238
  store float 0x402A000000000000, ptr %t238
  %t239 = getelementptr [1024 x float], ptr %o2, i64 0, i64 239
  store float 0x4034000000000000, ptr %t239
  %t240 = getelementptr [1024 x float], ptr %o2, i64 0, i64 240
  store float 0x403B000000000000, ptr %t240
  %t241 = getelementptr [1024 x float], ptr %o2, i64 0, i64 241
  store float 0x4041000000000000, ptr %t241
  %t242 = getelementptr [1024 x float], ptr %o2, i64 0, i64 242
  store float 0x4044800000000000, ptr %t242
  %t243 = getelementptr [1024 x float], ptr %o2, i64 0, i64 243
  store float 0x4048000000000000, ptr %t243
  %t244 = getelementptr [1024 x float], ptr %o2, i64 0, i64 244
  store float 0xC047000000000000, ptr %t244
  %t245 = getelementptr [1024 x float], ptr %o2, i64 0, i64 245
  store float 0xC043800000000000, ptr %t245
  %t246 = getelementptr [1024 x float], ptr %o2, i64 0, i64 246
  store float 0xC040000000000000, ptr %t246
  %t247 = getelementptr [1024 x float], ptr %o2, i64 0, i64 247
  store float 0xC039000000000000, ptr %t247
  %t248 = getelementptr [1024 x float], ptr %o2, i64 0, i64 248
  store float 0xC032000000000000, ptr %t248
  %t249 = getelementptr [1024 x float], ptr %o2, i64 0, i64 249
  store float 0xC026000000000000, ptr %t249
  %t250 = getelementptr [1024 x float], ptr %o2, i64 0, i64 250
  store float 0xC010000000000000, ptr %t250
  %t251 = getelementptr [1024 x float], ptr %o2, i64 0, i64 251
  store float 0x4008000000000000, ptr %t251
  %t252 = getelementptr [1024 x float], ptr %o2, i64 0, i64 252
  store float 0x4024000000000000, ptr %t252
  %t253 = getelementptr [1024 x float], ptr %o2, i64 0, i64 253
  store float 0x4031000000000000, ptr %t253
  %t254 = getelementptr [1024 x float], ptr %o2, i64 0, i64 254
  store float 0x4038000000000000, ptr %t254
  %t255 = getelementptr [1024 x float], ptr %o2, i64 0, i64 255
  store float 0x403F000000000000, ptr %t255
  %t256 = getelementptr [1024 x float], ptr %o2, i64 0, i64 256
  store float 0x4043000000000000, ptr %t256
  %t257 = getelementptr [1024 x float], ptr %o2, i64 0, i64 257
  store float 0x4046800000000000, ptr %t257
  %t258 = getelementptr [1024 x float], ptr %o2, i64 0, i64 258
  store float 0xC048800000000000, ptr %t258
  %t259 = getelementptr [1024 x float], ptr %o2, i64 0, i64 259
  store float 0xC045000000000000, ptr %t259
  %t260 = getelementptr [1024 x float], ptr %o2, i64 0, i64 260
  store float 0xC041800000000000, ptr %t260
  %t261 = getelementptr [1024 x float], ptr %o2, i64 0, i64 261
  store float 0xC03C000000000000, ptr %t261
  %t262 = getelementptr [1024 x float], ptr %o2, i64 0, i64 262
  store float 0xC035000000000000, ptr %t262
  %t263 = getelementptr [1024 x float], ptr %o2, i64 0, i64 263
  store float 0xC02C000000000000, ptr %t263
  %t264 = getelementptr [1024 x float], ptr %o2, i64 0, i64 264
  store float 0xC01C000000000000, ptr %t264
  %t265 = getelementptr [1024 x float], ptr %o2, i64 0, i64 265
  store float 0x0000000000000000, ptr %t265
  %t266 = getelementptr [1024 x float], ptr %o2, i64 0, i64 266
  store float 0x401C000000000000, ptr %t266
  %t267 = getelementptr [1024 x float], ptr %o2, i64 0, i64 267
  store float 0x402C000000000000, ptr %t267
  %t268 = getelementptr [1024 x float], ptr %o2, i64 0, i64 268
  store float 0x4035000000000000, ptr %t268
  %t269 = getelementptr [1024 x float], ptr %o2, i64 0, i64 269
  store float 0x403C000000000000, ptr %t269
  %t270 = getelementptr [1024 x float], ptr %o2, i64 0, i64 270
  store float 0x4041800000000000, ptr %t270
  %t271 = getelementptr [1024 x float], ptr %o2, i64 0, i64 271
  store float 0x4045000000000000, ptr %t271
  %t272 = getelementptr [1024 x float], ptr %o2, i64 0, i64 272
  store float 0x4048800000000000, ptr %t272
  %t273 = getelementptr [1024 x float], ptr %o2, i64 0, i64 273
  store float 0xC046800000000000, ptr %t273
  %t274 = getelementptr [1024 x float], ptr %o2, i64 0, i64 274
  store float 0xC043000000000000, ptr %t274
  %t275 = getelementptr [1024 x float], ptr %o2, i64 0, i64 275
  store float 0xC03F000000000000, ptr %t275
  %t276 = getelementptr [1024 x float], ptr %o2, i64 0, i64 276
  store float 0xC038000000000000, ptr %t276
  %t277 = getelementptr [1024 x float], ptr %o2, i64 0, i64 277
  store float 0xC031000000000000, ptr %t277
  %t278 = getelementptr [1024 x float], ptr %o2, i64 0, i64 278
  store float 0xC024000000000000, ptr %t278
  %t279 = getelementptr [1024 x float], ptr %o2, i64 0, i64 279
  store float 0xC008000000000000, ptr %t279
  %t280 = getelementptr [1024 x float], ptr %o2, i64 0, i64 280
  store float 0x4010000000000000, ptr %t280
  %t281 = getelementptr [1024 x float], ptr %o2, i64 0, i64 281
  store float 0x4026000000000000, ptr %t281
  %t282 = getelementptr [1024 x float], ptr %o2, i64 0, i64 282
  store float 0x4032000000000000, ptr %t282
  %t283 = getelementptr [1024 x float], ptr %o2, i64 0, i64 283
  store float 0x4039000000000000, ptr %t283
  %t284 = getelementptr [1024 x float], ptr %o2, i64 0, i64 284
  store float 0x4040000000000000, ptr %t284
  %t285 = getelementptr [1024 x float], ptr %o2, i64 0, i64 285
  store float 0x4043800000000000, ptr %t285
  %t286 = getelementptr [1024 x float], ptr %o2, i64 0, i64 286
  store float 0x4047000000000000, ptr %t286
  %t287 = getelementptr [1024 x float], ptr %o2, i64 0, i64 287
  store float 0xC048000000000000, ptr %t287
  %t288 = getelementptr [1024 x float], ptr %o2, i64 0, i64 288
  store float 0xC044800000000000, ptr %t288
  %t289 = getelementptr [1024 x float], ptr %o2, i64 0, i64 289
  store float 0xC041000000000000, ptr %t289
  %t290 = getelementptr [1024 x float], ptr %o2, i64 0, i64 290
  store float 0xC03B000000000000, ptr %t290
  %t291 = getelementptr [1024 x float], ptr %o2, i64 0, i64 291
  store float 0xC034000000000000, ptr %t291
  %t292 = getelementptr [1024 x float], ptr %o2, i64 0, i64 292
  store float 0xC02A000000000000, ptr %t292
  %t293 = getelementptr [1024 x float], ptr %o2, i64 0, i64 293
  store float 0xC018000000000000, ptr %t293
  %t294 = getelementptr [1024 x float], ptr %o2, i64 0, i64 294
  store float 0x3FF0000000000000, ptr %t294
  %t295 = getelementptr [1024 x float], ptr %o2, i64 0, i64 295
  store float 0x4020000000000000, ptr %t295
  %t296 = getelementptr [1024 x float], ptr %o2, i64 0, i64 296
  store float 0x402E000000000000, ptr %t296
  %t297 = getelementptr [1024 x float], ptr %o2, i64 0, i64 297
  store float 0x4036000000000000, ptr %t297
  %t298 = getelementptr [1024 x float], ptr %o2, i64 0, i64 298
  store float 0x403D000000000000, ptr %t298
  %t299 = getelementptr [1024 x float], ptr %o2, i64 0, i64 299
  store float 0x4042000000000000, ptr %t299
  %t300 = getelementptr [1024 x float], ptr %o2, i64 0, i64 300
  store float 0x4045800000000000, ptr %t300
  %t301 = getelementptr [1024 x float], ptr %o2, i64 0, i64 301
  store float 0x4049000000000000, ptr %t301
  %t302 = getelementptr [1024 x float], ptr %o2, i64 0, i64 302
  store float 0xC046000000000000, ptr %t302
  %t303 = getelementptr [1024 x float], ptr %o2, i64 0, i64 303
  store float 0xC042800000000000, ptr %t303
  %t304 = getelementptr [1024 x float], ptr %o2, i64 0, i64 304
  store float 0xC03E000000000000, ptr %t304
  %t305 = getelementptr [1024 x float], ptr %o2, i64 0, i64 305
  store float 0xC037000000000000, ptr %t305
  %t306 = getelementptr [1024 x float], ptr %o2, i64 0, i64 306
  store float 0xC030000000000000, ptr %t306
  %t307 = getelementptr [1024 x float], ptr %o2, i64 0, i64 307
  store float 0xC022000000000000, ptr %t307
  %t308 = getelementptr [1024 x float], ptr %o2, i64 0, i64 308
  store float 0xC000000000000000, ptr %t308
  %t309 = getelementptr [1024 x float], ptr %o2, i64 0, i64 309
  store float 0x4014000000000000, ptr %t309
  %t310 = getelementptr [1024 x float], ptr %o2, i64 0, i64 310
  store float 0x4028000000000000, ptr %t310
  %t311 = getelementptr [1024 x float], ptr %o2, i64 0, i64 311
  store float 0x4033000000000000, ptr %t311
  %t312 = getelementptr [1024 x float], ptr %o2, i64 0, i64 312
  store float 0x403A000000000000, ptr %t312
  %t313 = getelementptr [1024 x float], ptr %o2, i64 0, i64 313
  store float 0x4040800000000000, ptr %t313
  %t314 = getelementptr [1024 x float], ptr %o2, i64 0, i64 314
  store float 0x4044000000000000, ptr %t314
  %t315 = getelementptr [1024 x float], ptr %o2, i64 0, i64 315
  store float 0x4047800000000000, ptr %t315
  %t316 = getelementptr [1024 x float], ptr %o2, i64 0, i64 316
  store float 0xC047800000000000, ptr %t316
  %t317 = getelementptr [1024 x float], ptr %o2, i64 0, i64 317
  store float 0xC044000000000000, ptr %t317
  %t318 = getelementptr [1024 x float], ptr %o2, i64 0, i64 318
  store float 0xC040800000000000, ptr %t318
  %t319 = getelementptr [1024 x float], ptr %o2, i64 0, i64 319
  store float 0xC03A000000000000, ptr %t319
  %t320 = getelementptr [1024 x float], ptr %o2, i64 0, i64 320
  store float 0xC033000000000000, ptr %t320
  %t321 = getelementptr [1024 x float], ptr %o2, i64 0, i64 321
  store float 0xC028000000000000, ptr %t321
  %t322 = getelementptr [1024 x float], ptr %o2, i64 0, i64 322
  store float 0xC014000000000000, ptr %t322
  %t323 = getelementptr [1024 x float], ptr %o2, i64 0, i64 323
  store float 0x4000000000000000, ptr %t323
  %t324 = getelementptr [1024 x float], ptr %o2, i64 0, i64 324
  store float 0x4022000000000000, ptr %t324
  %t325 = getelementptr [1024 x float], ptr %o2, i64 0, i64 325
  store float 0x4030000000000000, ptr %t325
  %t326 = getelementptr [1024 x float], ptr %o2, i64 0, i64 326
  store float 0x4037000000000000, ptr %t326
  %t327 = getelementptr [1024 x float], ptr %o2, i64 0, i64 327
  store float 0x403E000000000000, ptr %t327
  %t328 = getelementptr [1024 x float], ptr %o2, i64 0, i64 328
  store float 0x4042800000000000, ptr %t328
  %t329 = getelementptr [1024 x float], ptr %o2, i64 0, i64 329
  store float 0x4046000000000000, ptr %t329
  %t330 = getelementptr [1024 x float], ptr %o2, i64 0, i64 330
  store float 0xC049000000000000, ptr %t330
  %t331 = getelementptr [1024 x float], ptr %o2, i64 0, i64 331
  store float 0xC045800000000000, ptr %t331
  %t332 = getelementptr [1024 x float], ptr %o2, i64 0, i64 332
  store float 0xC042000000000000, ptr %t332
  %t333 = getelementptr [1024 x float], ptr %o2, i64 0, i64 333
  store float 0xC03D000000000000, ptr %t333
  %t334 = getelementptr [1024 x float], ptr %o2, i64 0, i64 334
  store float 0xC036000000000000, ptr %t334
  %t335 = getelementptr [1024 x float], ptr %o2, i64 0, i64 335
  store float 0xC02E000000000000, ptr %t335
  %t336 = getelementptr [1024 x float], ptr %o2, i64 0, i64 336
  store float 0xC020000000000000, ptr %t336
  %t337 = getelementptr [1024 x float], ptr %o2, i64 0, i64 337
  store float 0xBFF0000000000000, ptr %t337
  %t338 = getelementptr [1024 x float], ptr %o2, i64 0, i64 338
  store float 0x4018000000000000, ptr %t338
  %t339 = getelementptr [1024 x float], ptr %o2, i64 0, i64 339
  store float 0x402A000000000000, ptr %t339
  %t340 = getelementptr [1024 x float], ptr %o2, i64 0, i64 340
  store float 0x4034000000000000, ptr %t340
  %t341 = getelementptr [1024 x float], ptr %o2, i64 0, i64 341
  store float 0x403B000000000000, ptr %t341
  %t342 = getelementptr [1024 x float], ptr %o2, i64 0, i64 342
  store float 0x4041000000000000, ptr %t342
  %t343 = getelementptr [1024 x float], ptr %o2, i64 0, i64 343
  store float 0x4044800000000000, ptr %t343
  %t344 = getelementptr [1024 x float], ptr %o2, i64 0, i64 344
  store float 0x4048000000000000, ptr %t344
  %t345 = getelementptr [1024 x float], ptr %o2, i64 0, i64 345
  store float 0xC047000000000000, ptr %t345
  %t346 = getelementptr [1024 x float], ptr %o2, i64 0, i64 346
  store float 0xC043800000000000, ptr %t346
  %t347 = getelementptr [1024 x float], ptr %o2, i64 0, i64 347
  store float 0xC040000000000000, ptr %t347
  %t348 = getelementptr [1024 x float], ptr %o2, i64 0, i64 348
  store float 0xC039000000000000, ptr %t348
  %t349 = getelementptr [1024 x float], ptr %o2, i64 0, i64 349
  store float 0xC032000000000000, ptr %t349
  %t350 = getelementptr [1024 x float], ptr %o2, i64 0, i64 350
  store float 0xC026000000000000, ptr %t350
  %t351 = getelementptr [1024 x float], ptr %o2, i64 0, i64 351
  store float 0xC010000000000000, ptr %t351
  %t352 = getelementptr [1024 x float], ptr %o2, i64 0, i64 352
  store float 0x4008000000000000, ptr %t352
  %t353 = getelementptr [1024 x float], ptr %o2, i64 0, i64 353
  store float 0x4024000000000000, ptr %t353
  %t354 = getelementptr [1024 x float], ptr %o2, i64 0, i64 354
  store float 0x4031000000000000, ptr %t354
  %t355 = getelementptr [1024 x float], ptr %o2, i64 0, i64 355
  store float 0x4038000000000000, ptr %t355
  %t356 = getelementptr [1024 x float], ptr %o2, i64 0, i64 356
  store float 0x403F000000000000, ptr %t356
  %t357 = getelementptr [1024 x float], ptr %o2, i64 0, i64 357
  store float 0x4043000000000000, ptr %t357
  %t358 = getelementptr [1024 x float], ptr %o2, i64 0, i64 358
  store float 0x4046800000000000, ptr %t358
  %t359 = getelementptr [1024 x float], ptr %o2, i64 0, i64 359
  store float 0xC048800000000000, ptr %t359
  %t360 = getelementptr [1024 x float], ptr %o2, i64 0, i64 360
  store float 0xC045000000000000, ptr %t360
  %t361 = getelementptr [1024 x float], ptr %o2, i64 0, i64 361
  store float 0xC041800000000000, ptr %t361
  %t362 = getelementptr [1024 x float], ptr %o2, i64 0, i64 362
  store float 0xC03C000000000000, ptr %t362
  %t363 = getelementptr [1024 x float], ptr %o2, i64 0, i64 363
  store float 0xC035000000000000, ptr %t363
  %t364 = getelementptr [1024 x float], ptr %o2, i64 0, i64 364
  store float 0xC02C000000000000, ptr %t364
  %t365 = getelementptr [1024 x float], ptr %o2, i64 0, i64 365
  store float 0xC01C000000000000, ptr %t365
  %t366 = getelementptr [1024 x float], ptr %o2, i64 0, i64 366
  store float 0x0000000000000000, ptr %t366
  %t367 = getelementptr [1024 x float], ptr %o2, i64 0, i64 367
  store float 0x401C000000000000, ptr %t367
  %t368 = getelementptr [1024 x float], ptr %o2, i64 0, i64 368
  store float 0x402C000000000000, ptr %t368
  %t369 = getelementptr [1024 x float], ptr %o2, i64 0, i64 369
  store float 0x4035000000000000, ptr %t369
  %t370 = getelementptr [1024 x float], ptr %o2, i64 0, i64 370
  store float 0x403C000000000000, ptr %t370
  %t371 = getelementptr [1024 x float], ptr %o2, i64 0, i64 371
  store float 0x4041800000000000, ptr %t371
  %t372 = getelementptr [1024 x float], ptr %o2, i64 0, i64 372
  store float 0x4045000000000000, ptr %t372
  %t373 = getelementptr [1024 x float], ptr %o2, i64 0, i64 373
  store float 0x4048800000000000, ptr %t373
  %t374 = getelementptr [1024 x float], ptr %o2, i64 0, i64 374
  store float 0xC046800000000000, ptr %t374
  %t375 = getelementptr [1024 x float], ptr %o2, i64 0, i64 375
  store float 0xC043000000000000, ptr %t375
  %t376 = getelementptr [1024 x float], ptr %o2, i64 0, i64 376
  store float 0xC03F000000000000, ptr %t376
  %t377 = getelementptr [1024 x float], ptr %o2, i64 0, i64 377
  store float 0xC038000000000000, ptr %t377
  %t378 = getelementptr [1024 x float], ptr %o2, i64 0, i64 378
  store float 0xC031000000000000, ptr %t378
  %t379 = getelementptr [1024 x float], ptr %o2, i64 0, i64 379
  store float 0xC024000000000000, ptr %t379
  %t380 = getelementptr [1024 x float], ptr %o2, i64 0, i64 380
  store float 0xC008000000000000, ptr %t380
  %t381 = getelementptr [1024 x float], ptr %o2, i64 0, i64 381
  store float 0x4010000000000000, ptr %t381
  %t382 = getelementptr [1024 x float], ptr %o2, i64 0, i64 382
  store float 0x4026000000000000, ptr %t382
  %t383 = getelementptr [1024 x float], ptr %o2, i64 0, i64 383
  store float 0x4032000000000000, ptr %t383
  %t384 = getelementptr [1024 x float], ptr %o2, i64 0, i64 384
  store float 0x4039000000000000, ptr %t384
  %t385 = getelementptr [1024 x float], ptr %o2, i64 0, i64 385
  store float 0x4040000000000000, ptr %t385
  %t386 = getelementptr [1024 x float], ptr %o2, i64 0, i64 386
  store float 0x4043800000000000, ptr %t386
  %t387 = getelementptr [1024 x float], ptr %o2, i64 0, i64 387
  store float 0x4047000000000000, ptr %t387
  %t388 = getelementptr [1024 x float], ptr %o2, i64 0, i64 388
  store float 0xC048000000000000, ptr %t388
  %t389 = getelementptr [1024 x float], ptr %o2, i64 0, i64 389
  store float 0xC044800000000000, ptr %t389
  %t390 = getelementptr [1024 x float], ptr %o2, i64 0, i64 390
  store float 0xC041000000000000, ptr %t390
  %t391 = getelementptr [1024 x float], ptr %o2, i64 0, i64 391
  store float 0xC03B000000000000, ptr %t391
  %t392 = getelementptr [1024 x float], ptr %o2, i64 0, i64 392
  store float 0xC034000000000000, ptr %t392
  %t393 = getelementptr [1024 x float], ptr %o2, i64 0, i64 393
  store float 0xC02A000000000000, ptr %t393
  %t394 = getelementptr [1024 x float], ptr %o2, i64 0, i64 394
  store float 0xC018000000000000, ptr %t394
  %t395 = getelementptr [1024 x float], ptr %o2, i64 0, i64 395
  store float 0x3FF0000000000000, ptr %t395
  %t396 = getelementptr [1024 x float], ptr %o2, i64 0, i64 396
  store float 0x4020000000000000, ptr %t396
  %t397 = getelementptr [1024 x float], ptr %o2, i64 0, i64 397
  store float 0x402E000000000000, ptr %t397
  %t398 = getelementptr [1024 x float], ptr %o2, i64 0, i64 398
  store float 0x4036000000000000, ptr %t398
  %t399 = getelementptr [1024 x float], ptr %o2, i64 0, i64 399
  store float 0x403D000000000000, ptr %t399
  %t400 = getelementptr [1024 x float], ptr %o2, i64 0, i64 400
  store float 0x4042000000000000, ptr %t400
  %t401 = getelementptr [1024 x float], ptr %o2, i64 0, i64 401
  store float 0x4045800000000000, ptr %t401
  %t402 = getelementptr [1024 x float], ptr %o2, i64 0, i64 402
  store float 0x4049000000000000, ptr %t402
  %t403 = getelementptr [1024 x float], ptr %o2, i64 0, i64 403
  store float 0xC046000000000000, ptr %t403
  %t404 = getelementptr [1024 x float], ptr %o2, i64 0, i64 404
  store float 0xC042800000000000, ptr %t404
  %t405 = getelementptr [1024 x float], ptr %o2, i64 0, i64 405
  store float 0xC03E000000000000, ptr %t405
  %t406 = getelementptr [1024 x float], ptr %o2, i64 0, i64 406
  store float 0xC037000000000000, ptr %t406
  %t407 = getelementptr [1024 x float], ptr %o2, i64 0, i64 407
  store float 0xC030000000000000, ptr %t407
  %t408 = getelementptr [1024 x float], ptr %o2, i64 0, i64 408
  store float 0xC022000000000000, ptr %t408
  %t409 = getelementptr [1024 x float], ptr %o2, i64 0, i64 409
  store float 0xC000000000000000, ptr %t409
  %t410 = getelementptr [1024 x float], ptr %o2, i64 0, i64 410
  store float 0x4014000000000000, ptr %t410
  %t411 = getelementptr [1024 x float], ptr %o2, i64 0, i64 411
  store float 0x4028000000000000, ptr %t411
  %t412 = getelementptr [1024 x float], ptr %o2, i64 0, i64 412
  store float 0x4033000000000000, ptr %t412
  %t413 = getelementptr [1024 x float], ptr %o2, i64 0, i64 413
  store float 0x403A000000000000, ptr %t413
  %t414 = getelementptr [1024 x float], ptr %o2, i64 0, i64 414
  store float 0x4040800000000000, ptr %t414
  %t415 = getelementptr [1024 x float], ptr %o2, i64 0, i64 415
  store float 0x4044000000000000, ptr %t415
  %t416 = getelementptr [1024 x float], ptr %o2, i64 0, i64 416
  store float 0x4047800000000000, ptr %t416
  %t417 = getelementptr [1024 x float], ptr %o2, i64 0, i64 417
  store float 0xC047800000000000, ptr %t417
  %t418 = getelementptr [1024 x float], ptr %o2, i64 0, i64 418
  store float 0xC044000000000000, ptr %t418
  %t419 = getelementptr [1024 x float], ptr %o2, i64 0, i64 419
  store float 0xC040800000000000, ptr %t419
  %t420 = getelementptr [1024 x float], ptr %o2, i64 0, i64 420
  store float 0xC03A000000000000, ptr %t420
  %t421 = getelementptr [1024 x float], ptr %o2, i64 0, i64 421
  store float 0xC033000000000000, ptr %t421
  %t422 = getelementptr [1024 x float], ptr %o2, i64 0, i64 422
  store float 0xC028000000000000, ptr %t422
  %t423 = getelementptr [1024 x float], ptr %o2, i64 0, i64 423
  store float 0xC014000000000000, ptr %t423
  %t424 = getelementptr [1024 x float], ptr %o2, i64 0, i64 424
  store float 0x4000000000000000, ptr %t424
  %t425 = getelementptr [1024 x float], ptr %o2, i64 0, i64 425
  store float 0x4022000000000000, ptr %t425
  %t426 = getelementptr [1024 x float], ptr %o2, i64 0, i64 426
  store float 0x4030000000000000, ptr %t426
  %t427 = getelementptr [1024 x float], ptr %o2, i64 0, i64 427
  store float 0x4037000000000000, ptr %t427
  %t428 = getelementptr [1024 x float], ptr %o2, i64 0, i64 428
  store float 0x403E000000000000, ptr %t428
  %t429 = getelementptr [1024 x float], ptr %o2, i64 0, i64 429
  store float 0x4042800000000000, ptr %t429
  %t430 = getelementptr [1024 x float], ptr %o2, i64 0, i64 430
  store float 0x4046000000000000, ptr %t430
  %t431 = getelementptr [1024 x float], ptr %o2, i64 0, i64 431
  store float 0xC049000000000000, ptr %t431
  %t432 = getelementptr [1024 x float], ptr %o2, i64 0, i64 432
  store float 0xC045800000000000, ptr %t432
  %t433 = getelementptr [1024 x float], ptr %o2, i64 0, i64 433
  store float 0xC042000000000000, ptr %t433
  %t434 = getelementptr [1024 x float], ptr %o2, i64 0, i64 434
  store float 0xC03D000000000000, ptr %t434
  %t435 = getelementptr [1024 x float], ptr %o2, i64 0, i64 435
  store float 0xC036000000000000, ptr %t435
  %t436 = getelementptr [1024 x float], ptr %o2, i64 0, i64 436
  store float 0xC02E000000000000, ptr %t436
  %t437 = getelementptr [1024 x float], ptr %o2, i64 0, i64 437
  store float 0xC020000000000000, ptr %t437
  %t438 = getelementptr [1024 x float], ptr %o2, i64 0, i64 438
  store float 0xBFF0000000000000, ptr %t438
  %t439 = getelementptr [1024 x float], ptr %o2, i64 0, i64 439
  store float 0x4018000000000000, ptr %t439
  %t440 = getelementptr [1024 x float], ptr %o2, i64 0, i64 440
  store float 0x402A000000000000, ptr %t440
  %t441 = getelementptr [1024 x float], ptr %o2, i64 0, i64 441
  store float 0x4034000000000000, ptr %t441
  %t442 = getelementptr [1024 x float], ptr %o2, i64 0, i64 442
  store float 0x403B000000000000, ptr %t442
  %t443 = getelementptr [1024 x float], ptr %o2, i64 0, i64 443
  store float 0x4041000000000000, ptr %t443
  %t444 = getelementptr [1024 x float], ptr %o2, i64 0, i64 444
  store float 0x4044800000000000, ptr %t444
  %t445 = getelementptr [1024 x float], ptr %o2, i64 0, i64 445
  store float 0x4048000000000000, ptr %t445
  %t446 = getelementptr [1024 x float], ptr %o2, i64 0, i64 446
  store float 0xC047000000000000, ptr %t446
  %t447 = getelementptr [1024 x float], ptr %o2, i64 0, i64 447
  store float 0xC043800000000000, ptr %t447
  %t448 = getelementptr [1024 x float], ptr %o2, i64 0, i64 448
  store float 0xC040000000000000, ptr %t448
  %t449 = getelementptr [1024 x float], ptr %o2, i64 0, i64 449
  store float 0xC039000000000000, ptr %t449
  %t450 = getelementptr [1024 x float], ptr %o2, i64 0, i64 450
  store float 0xC032000000000000, ptr %t450
  %t451 = getelementptr [1024 x float], ptr %o2, i64 0, i64 451
  store float 0xC026000000000000, ptr %t451
  %t452 = getelementptr [1024 x float], ptr %o2, i64 0, i64 452
  store float 0xC010000000000000, ptr %t452
  %t453 = getelementptr [1024 x float], ptr %o2, i64 0, i64 453
  store float 0x4008000000000000, ptr %t453
  %t454 = getelementptr [1024 x float], ptr %o2, i64 0, i64 454
  store float 0x4024000000000000, ptr %t454
  %t455 = getelementptr [1024 x float], ptr %o2, i64 0, i64 455
  store float 0x4031000000000000, ptr %t455
  %t456 = getelementptr [1024 x float], ptr %o2, i64 0, i64 456
  store float 0x4038000000000000, ptr %t456
  %t457 = getelementptr [1024 x float], ptr %o2, i64 0, i64 457
  store float 0x403F000000000000, ptr %t457
  %t458 = getelementptr [1024 x float], ptr %o2, i64 0, i64 458
  store float 0x4043000000000000, ptr %t458
  %t459 = getelementptr [1024 x float], ptr %o2, i64 0, i64 459
  store float 0x4046800000000000, ptr %t459
  %t460 = getelementptr [1024 x float], ptr %o2, i64 0, i64 460
  store float 0xC048800000000000, ptr %t460
  %t461 = getelementptr [1024 x float], ptr %o2, i64 0, i64 461
  store float 0xC045000000000000, ptr %t461
  %t462 = getelementptr [1024 x float], ptr %o2, i64 0, i64 462
  store float 0xC041800000000000, ptr %t462
  %t463 = getelementptr [1024 x float], ptr %o2, i64 0, i64 463
  store float 0xC03C000000000000, ptr %t463
  %t464 = getelementptr [1024 x float], ptr %o2, i64 0, i64 464
  store float 0xC035000000000000, ptr %t464
  %t465 = getelementptr [1024 x float], ptr %o2, i64 0, i64 465
  store float 0xC02C000000000000, ptr %t465
  %t466 = getelementptr [1024 x float], ptr %o2, i64 0, i64 466
  store float 0xC01C000000000000, ptr %t466
  %t467 = getelementptr [1024 x float], ptr %o2, i64 0, i64 467
  store float 0x0000000000000000, ptr %t467
  %t468 = getelementptr [1024 x float], ptr %o2, i64 0, i64 468
  store float 0x401C000000000000, ptr %t468
  %t469 = getelementptr [1024 x float], ptr %o2, i64 0, i64 469
  store float 0x402C000000000000, ptr %t469
  %t470 = getelementptr [1024 x float], ptr %o2, i64 0, i64 470
  store float 0x4035000000000000, ptr %t470
  %t471 = getelementptr [1024 x float], ptr %o2, i64 0, i64 471
  store float 0x403C000000000000, ptr %t471
  %t472 = getelementptr [1024 x float], ptr %o2, i64 0, i64 472
  store float 0x4041800000000000, ptr %t472
  %t473 = getelementptr [1024 x float], ptr %o2, i64 0, i64 473
  store float 0x4045000000000000, ptr %t473
  %t474 = getelementptr [1024 x float], ptr %o2, i64 0, i64 474
  store float 0x4048800000000000, ptr %t474
  %t475 = getelementptr [1024 x float], ptr %o2, i64 0, i64 475
  store float 0xC046800000000000, ptr %t475
  %t476 = getelementptr [1024 x float], ptr %o2, i64 0, i64 476
  store float 0xC043000000000000, ptr %t476
  %t477 = getelementptr [1024 x float], ptr %o2, i64 0, i64 477
  store float 0xC03F000000000000, ptr %t477
  %t478 = getelementptr [1024 x float], ptr %o2, i64 0, i64 478
  store float 0xC038000000000000, ptr %t478
  %t479 = getelementptr [1024 x float], ptr %o2, i64 0, i64 479
  store float 0xC031000000000000, ptr %t479
  %t480 = getelementptr [1024 x float], ptr %o2, i64 0, i64 480
  store float 0xC024000000000000, ptr %t480
  %t481 = getelementptr [1024 x float], ptr %o2, i64 0, i64 481
  store float 0xC008000000000000, ptr %t481
  %t482 = getelementptr [1024 x float], ptr %o2, i64 0, i64 482
  store float 0x4010000000000000, ptr %t482
  %t483 = getelementptr [1024 x float], ptr %o2, i64 0, i64 483
  store float 0x4026000000000000, ptr %t483
  %t484 = getelementptr [1024 x float], ptr %o2, i64 0, i64 484
  store float 0x4032000000000000, ptr %t484
  %t485 = getelementptr [1024 x float], ptr %o2, i64 0, i64 485
  store float 0x4039000000000000, ptr %t485
  %t486 = getelementptr [1024 x float], ptr %o2, i64 0, i64 486
  store float 0x4040000000000000, ptr %t486
  %t487 = getelementptr [1024 x float], ptr %o2, i64 0, i64 487
  store float 0x4043800000000000, ptr %t487
  %t488 = getelementptr [1024 x float], ptr %o2, i64 0, i64 488
  store float 0x4047000000000000, ptr %t488
  %t489 = getelementptr [1024 x float], ptr %o2, i64 0, i64 489
  store float 0xC048000000000000, ptr %t489
  %t490 = getelementptr [1024 x float], ptr %o2, i64 0, i64 490
  store float 0xC044800000000000, ptr %t490
  %t491 = getelementptr [1024 x float], ptr %o2, i64 0, i64 491
  store float 0xC041000000000000, ptr %t491
  %t492 = getelementptr [1024 x float], ptr %o2, i64 0, i64 492
  store float 0xC03B000000000000, ptr %t492
  %t493 = getelementptr [1024 x float], ptr %o2, i64 0, i64 493
  store float 0xC034000000000000, ptr %t493
  %t494 = getelementptr [1024 x float], ptr %o2, i64 0, i64 494
  store float 0xC02A000000000000, ptr %t494
  %t495 = getelementptr [1024 x float], ptr %o2, i64 0, i64 495
  store float 0xC018000000000000, ptr %t495
  %t496 = getelementptr [1024 x float], ptr %o2, i64 0, i64 496
  store float 0x3FF0000000000000, ptr %t496
  %t497 = getelementptr [1024 x float], ptr %o2, i64 0, i64 497
  store float 0x4020000000000000, ptr %t497
  %t498 = getelementptr [1024 x float], ptr %o2, i64 0, i64 498
  store float 0x402E000000000000, ptr %t498
  %t499 = getelementptr [1024 x float], ptr %o2, i64 0, i64 499
  store float 0x4036000000000000, ptr %t499
  %t500 = getelementptr [1024 x float], ptr %o2, i64 0, i64 500
  store float 0x403D000000000000, ptr %t500
  %t501 = getelementptr [1024 x float], ptr %o2, i64 0, i64 501
  store float 0x4042000000000000, ptr %t501
  %t502 = getelementptr [1024 x float], ptr %o2, i64 0, i64 502
  store float 0x4045800000000000, ptr %t502
  %t503 = getelementptr [1024 x float], ptr %o2, i64 0, i64 503
  store float 0x4049000000000000, ptr %t503
  %t504 = getelementptr [1024 x float], ptr %o2, i64 0, i64 504
  store float 0xC046000000000000, ptr %t504
  %t505 = getelementptr [1024 x float], ptr %o2, i64 0, i64 505
  store float 0xC042800000000000, ptr %t505
  %t506 = getelementptr [1024 x float], ptr %o2, i64 0, i64 506
  store float 0xC03E000000000000, ptr %t506
  %t507 = getelementptr [1024 x float], ptr %o2, i64 0, i64 507
  store float 0xC037000000000000, ptr %t507
  %t508 = getelementptr [1024 x float], ptr %o2, i64 0, i64 508
  store float 0xC030000000000000, ptr %t508
  %t509 = getelementptr [1024 x float], ptr %o2, i64 0, i64 509
  store float 0xC022000000000000, ptr %t509
  %t510 = getelementptr [1024 x float], ptr %o2, i64 0, i64 510
  store float 0xC000000000000000, ptr %t510
  %t511 = getelementptr [1024 x float], ptr %o2, i64 0, i64 511
  store float 0x4014000000000000, ptr %t511
  %t512 = getelementptr [1024 x float], ptr %o2, i64 0, i64 512
  store float 0x4028000000000000, ptr %t512
  %t513 = getelementptr [1024 x float], ptr %o2, i64 0, i64 513
  store float 0x4033000000000000, ptr %t513
  %t514 = getelementptr [1024 x float], ptr %o2, i64 0, i64 514
  store float 0x403A000000000000, ptr %t514
  %t515 = getelementptr [1024 x float], ptr %o2, i64 0, i64 515
  store float 0x4040800000000000, ptr %t515
  %t516 = getelementptr [1024 x float], ptr %o2, i64 0, i64 516
  store float 0x4044000000000000, ptr %t516
  %t517 = getelementptr [1024 x float], ptr %o2, i64 0, i64 517
  store float 0x4047800000000000, ptr %t517
  %t518 = getelementptr [1024 x float], ptr %o2, i64 0, i64 518
  store float 0xC047800000000000, ptr %t518
  %t519 = getelementptr [1024 x float], ptr %o2, i64 0, i64 519
  store float 0xC044000000000000, ptr %t519
  %t520 = getelementptr [1024 x float], ptr %o2, i64 0, i64 520
  store float 0xC040800000000000, ptr %t520
  %t521 = getelementptr [1024 x float], ptr %o2, i64 0, i64 521
  store float 0xC03A000000000000, ptr %t521
  %t522 = getelementptr [1024 x float], ptr %o2, i64 0, i64 522
  store float 0xC033000000000000, ptr %t522
  %t523 = getelementptr [1024 x float], ptr %o2, i64 0, i64 523
  store float 0xC028000000000000, ptr %t523
  %t524 = getelementptr [1024 x float], ptr %o2, i64 0, i64 524
  store float 0xC014000000000000, ptr %t524
  %t525 = getelementptr [1024 x float], ptr %o2, i64 0, i64 525
  store float 0x4000000000000000, ptr %t525
  %t526 = getelementptr [1024 x float], ptr %o2, i64 0, i64 526
  store float 0x4022000000000000, ptr %t526
  %t527 = getelementptr [1024 x float], ptr %o2, i64 0, i64 527
  store float 0x4030000000000000, ptr %t527
  %t528 = getelementptr [1024 x float], ptr %o2, i64 0, i64 528
  store float 0x4037000000000000, ptr %t528
  %t529 = getelementptr [1024 x float], ptr %o2, i64 0, i64 529
  store float 0x403E000000000000, ptr %t529
  %t530 = getelementptr [1024 x float], ptr %o2, i64 0, i64 530
  store float 0x4042800000000000, ptr %t530
  %t531 = getelementptr [1024 x float], ptr %o2, i64 0, i64 531
  store float 0x4046000000000000, ptr %t531
  %t532 = getelementptr [1024 x float], ptr %o2, i64 0, i64 532
  store float 0xC049000000000000, ptr %t532
  %t533 = getelementptr [1024 x float], ptr %o2, i64 0, i64 533
  store float 0xC045800000000000, ptr %t533
  %t534 = getelementptr [1024 x float], ptr %o2, i64 0, i64 534
  store float 0xC042000000000000, ptr %t534
  %t535 = getelementptr [1024 x float], ptr %o2, i64 0, i64 535
  store float 0xC03D000000000000, ptr %t535
  %t536 = getelementptr [1024 x float], ptr %o2, i64 0, i64 536
  store float 0xC036000000000000, ptr %t536
  %t537 = getelementptr [1024 x float], ptr %o2, i64 0, i64 537
  store float 0xC02E000000000000, ptr %t537
  %t538 = getelementptr [1024 x float], ptr %o2, i64 0, i64 538
  store float 0xC020000000000000, ptr %t538
  %t539 = getelementptr [1024 x float], ptr %o2, i64 0, i64 539
  store float 0xBFF0000000000000, ptr %t539
  %t540 = getelementptr [1024 x float], ptr %o2, i64 0, i64 540
  store float 0x4018000000000000, ptr %t540
  %t541 = getelementptr [1024 x float], ptr %o2, i64 0, i64 541
  store float 0x402A000000000000, ptr %t541
  %t542 = getelementptr [1024 x float], ptr %o2, i64 0, i64 542
  store float 0x4034000000000000, ptr %t542
  %t543 = getelementptr [1024 x float], ptr %o2, i64 0, i64 543
  store float 0x403B000000000000, ptr %t543
  %t544 = getelementptr [1024 x float], ptr %o2, i64 0, i64 544
  store float 0x4041000000000000, ptr %t544
  %t545 = getelementptr [1024 x float], ptr %o2, i64 0, i64 545
  store float 0x4044800000000000, ptr %t545
  %t546 = getelementptr [1024 x float], ptr %o2, i64 0, i64 546
  store float 0x4048000000000000, ptr %t546
  %t547 = getelementptr [1024 x float], ptr %o2, i64 0, i64 547
  store float 0xC047000000000000, ptr %t547
  %t548 = getelementptr [1024 x float], ptr %o2, i64 0, i64 548
  store float 0xC043800000000000, ptr %t548
  %t549 = getelementptr [1024 x float], ptr %o2, i64 0, i64 549
  store float 0xC040000000000000, ptr %t549
  %t550 = getelementptr [1024 x float], ptr %o2, i64 0, i64 550
  store float 0xC039000000000000, ptr %t550
  %t551 = getelementptr [1024 x float], ptr %o2, i64 0, i64 551
  store float 0xC032000000000000, ptr %t551
  %t552 = getelementptr [1024 x float], ptr %o2, i64 0, i64 552
  store float 0xC026000000000000, ptr %t552
  %t553 = getelementptr [1024 x float], ptr %o2, i64 0, i64 553
  store float 0xC010000000000000, ptr %t553
  %t554 = getelementptr [1024 x float], ptr %o2, i64 0, i64 554
  store float 0x4008000000000000, ptr %t554
  %t555 = getelementptr [1024 x float], ptr %o2, i64 0, i64 555
  store float 0x4024000000000000, ptr %t555
  %t556 = getelementptr [1024 x float], ptr %o2, i64 0, i64 556
  store float 0x4031000000000000, ptr %t556
  %t557 = getelementptr [1024 x float], ptr %o2, i64 0, i64 557
  store float 0x4038000000000000, ptr %t557
  %t558 = getelementptr [1024 x float], ptr %o2, i64 0, i64 558
  store float 0x403F000000000000, ptr %t558
  %t559 = getelementptr [1024 x float], ptr %o2, i64 0, i64 559
  store float 0x4043000000000000, ptr %t559
  %t560 = getelementptr [1024 x float], ptr %o2, i64 0, i64 560
  store float 0x4046800000000000, ptr %t560
  %t561 = getelementptr [1024 x float], ptr %o2, i64 0, i64 561
  store float 0xC048800000000000, ptr %t561
  %t562 = getelementptr [1024 x float], ptr %o2, i64 0, i64 562
  store float 0xC045000000000000, ptr %t562
  %t563 = getelementptr [1024 x float], ptr %o2, i64 0, i64 563
  store float 0xC041800000000000, ptr %t563
  %t564 = getelementptr [1024 x float], ptr %o2, i64 0, i64 564
  store float 0xC03C000000000000, ptr %t564
  %t565 = getelementptr [1024 x float], ptr %o2, i64 0, i64 565
  store float 0xC035000000000000, ptr %t565
  %t566 = getelementptr [1024 x float], ptr %o2, i64 0, i64 566
  store float 0xC02C000000000000, ptr %t566
  %t567 = getelementptr [1024 x float], ptr %o2, i64 0, i64 567
  store float 0xC01C000000000000, ptr %t567
  %t568 = getelementptr [1024 x float], ptr %o2, i64 0, i64 568
  store float 0x0000000000000000, ptr %t568
  %t569 = getelementptr [1024 x float], ptr %o2, i64 0, i64 569
  store float 0x401C000000000000, ptr %t569
  %t570 = getelementptr [1024 x float], ptr %o2, i64 0, i64 570
  store float 0x402C000000000000, ptr %t570
  %t571 = getelementptr [1024 x float], ptr %o2, i64 0, i64 571
  store float 0x4035000000000000, ptr %t571
  %t572 = getelementptr [1024 x float], ptr %o2, i64 0, i64 572
  store float 0x403C000000000000, ptr %t572
  %t573 = getelementptr [1024 x float], ptr %o2, i64 0, i64 573
  store float 0x4041800000000000, ptr %t573
  %t574 = getelementptr [1024 x float], ptr %o2, i64 0, i64 574
  store float 0x4045000000000000, ptr %t574
  %t575 = getelementptr [1024 x float], ptr %o2, i64 0, i64 575
  store float 0x4048800000000000, ptr %t575
  %t576 = getelementptr [1024 x float], ptr %o2, i64 0, i64 576
  store float 0xC046800000000000, ptr %t576
  %t577 = getelementptr [1024 x float], ptr %o2, i64 0, i64 577
  store float 0xC043000000000000, ptr %t577
  %t578 = getelementptr [1024 x float], ptr %o2, i64 0, i64 578
  store float 0xC03F000000000000, ptr %t578
  %t579 = getelementptr [1024 x float], ptr %o2, i64 0, i64 579
  store float 0xC038000000000000, ptr %t579
  %t580 = getelementptr [1024 x float], ptr %o2, i64 0, i64 580
  store float 0xC031000000000000, ptr %t580
  %t581 = getelementptr [1024 x float], ptr %o2, i64 0, i64 581
  store float 0xC024000000000000, ptr %t581
  %t582 = getelementptr [1024 x float], ptr %o2, i64 0, i64 582
  store float 0xC008000000000000, ptr %t582
  %t583 = getelementptr [1024 x float], ptr %o2, i64 0, i64 583
  store float 0x4010000000000000, ptr %t583
  %t584 = getelementptr [1024 x float], ptr %o2, i64 0, i64 584
  store float 0x4026000000000000, ptr %t584
  %t585 = getelementptr [1024 x float], ptr %o2, i64 0, i64 585
  store float 0x4032000000000000, ptr %t585
  %t586 = getelementptr [1024 x float], ptr %o2, i64 0, i64 586
  store float 0x4039000000000000, ptr %t586
  %t587 = getelementptr [1024 x float], ptr %o2, i64 0, i64 587
  store float 0x4040000000000000, ptr %t587
  %t588 = getelementptr [1024 x float], ptr %o2, i64 0, i64 588
  store float 0x4043800000000000, ptr %t588
  %t589 = getelementptr [1024 x float], ptr %o2, i64 0, i64 589
  store float 0x4047000000000000, ptr %t589
  %t590 = getelementptr [1024 x float], ptr %o2, i64 0, i64 590
  store float 0xC048000000000000, ptr %t590
  %t591 = getelementptr [1024 x float], ptr %o2, i64 0, i64 591
  store float 0xC044800000000000, ptr %t591
  %t592 = getelementptr [1024 x float], ptr %o2, i64 0, i64 592
  store float 0xC041000000000000, ptr %t592
  %t593 = getelementptr [1024 x float], ptr %o2, i64 0, i64 593
  store float 0xC03B000000000000, ptr %t593
  %t594 = getelementptr [1024 x float], ptr %o2, i64 0, i64 594
  store float 0xC034000000000000, ptr %t594
  %t595 = getelementptr [1024 x float], ptr %o2, i64 0, i64 595
  store float 0xC02A000000000000, ptr %t595
  %t596 = getelementptr [1024 x float], ptr %o2, i64 0, i64 596
  store float 0xC018000000000000, ptr %t596
  %t597 = getelementptr [1024 x float], ptr %o2, i64 0, i64 597
  store float 0x3FF0000000000000, ptr %t597
  %t598 = getelementptr [1024 x float], ptr %o2, i64 0, i64 598
  store float 0x4020000000000000, ptr %t598
  %t599 = getelementptr [1024 x float], ptr %o2, i64 0, i64 599
  store float 0x402E000000000000, ptr %t599
  %t600 = getelementptr [1024 x float], ptr %o2, i64 0, i64 600
  store float 0x4036000000000000, ptr %t600
  %t601 = getelementptr [1024 x float], ptr %o2, i64 0, i64 601
  store float 0x403D000000000000, ptr %t601
  %t602 = getelementptr [1024 x float], ptr %o2, i64 0, i64 602
  store float 0x4042000000000000, ptr %t602
  %t603 = getelementptr [1024 x float], ptr %o2, i64 0, i64 603
  store float 0x4045800000000000, ptr %t603
  %t604 = getelementptr [1024 x float], ptr %o2, i64 0, i64 604
  store float 0x4049000000000000, ptr %t604
  %t605 = getelementptr [1024 x float], ptr %o2, i64 0, i64 605
  store float 0xC046000000000000, ptr %t605
  %t606 = getelementptr [1024 x float], ptr %o2, i64 0, i64 606
  store float 0xC042800000000000, ptr %t606
  %t607 = getelementptr [1024 x float], ptr %o2, i64 0, i64 607
  store float 0xC03E000000000000, ptr %t607
  %t608 = getelementptr [1024 x float], ptr %o2, i64 0, i64 608
  store float 0xC037000000000000, ptr %t608
  %t609 = getelementptr [1024 x float], ptr %o2, i64 0, i64 609
  store float 0xC030000000000000, ptr %t609
  %t610 = getelementptr [1024 x float], ptr %o2, i64 0, i64 610
  store float 0xC022000000000000, ptr %t610
  %t611 = getelementptr [1024 x float], ptr %o2, i64 0, i64 611
  store float 0xC000000000000000, ptr %t611
  %t612 = getelementptr [1024 x float], ptr %o2, i64 0, i64 612
  store float 0x4014000000000000, ptr %t612
  %t613 = getelementptr [1024 x float], ptr %o2, i64 0, i64 613
  store float 0x4028000000000000, ptr %t613
  %t614 = getelementptr [1024 x float], ptr %o2, i64 0, i64 614
  store float 0x4033000000000000, ptr %t614
  %t615 = getelementptr [1024 x float], ptr %o2, i64 0, i64 615
  store float 0x403A000000000000, ptr %t615
  %t616 = getelementptr [1024 x float], ptr %o2, i64 0, i64 616
  store float 0x4040800000000000, ptr %t616
  %t617 = getelementptr [1024 x float], ptr %o2, i64 0, i64 617
  store float 0x4044000000000000, ptr %t617
  %t618 = getelementptr [1024 x float], ptr %o2, i64 0, i64 618
  store float 0x4047800000000000, ptr %t618
  %t619 = getelementptr [1024 x float], ptr %o2, i64 0, i64 619
  store float 0xC047800000000000, ptr %t619
  %t620 = getelementptr [1024 x float], ptr %o2, i64 0, i64 620
  store float 0xC044000000000000, ptr %t620
  %t621 = getelementptr [1024 x float], ptr %o2, i64 0, i64 621
  store float 0xC040800000000000, ptr %t621
  %t622 = getelementptr [1024 x float], ptr %o2, i64 0, i64 622
  store float 0xC03A000000000000, ptr %t622
  %t623 = getelementptr [1024 x float], ptr %o2, i64 0, i64 623
  store float 0xC033000000000000, ptr %t623
  %t624 = getelementptr [1024 x float], ptr %o2, i64 0, i64 624
  store float 0xC028000000000000, ptr %t624
  %t625 = getelementptr [1024 x float], ptr %o2, i64 0, i64 625
  store float 0xC014000000000000, ptr %t625
  %t626 = getelementptr [1024 x float], ptr %o2, i64 0, i64 626
  store float 0x4000000000000000, ptr %t626
  %t627 = getelementptr [1024 x float], ptr %o2, i64 0, i64 627
  store float 0x4022000000000000, ptr %t627
  %t628 = getelementptr [1024 x float], ptr %o2, i64 0, i64 628
  store float 0x4030000000000000, ptr %t628
  %t629 = getelementptr [1024 x float], ptr %o2, i64 0, i64 629
  store float 0x4037000000000000, ptr %t629
  %t630 = getelementptr [1024 x float], ptr %o2, i64 0, i64 630
  store float 0x403E000000000000, ptr %t630
  %t631 = getelementptr [1024 x float], ptr %o2, i64 0, i64 631
  store float 0x4042800000000000, ptr %t631
  %t632 = getelementptr [1024 x float], ptr %o2, i64 0, i64 632
  store float 0x4046000000000000, ptr %t632
  %t633 = getelementptr [1024 x float], ptr %o2, i64 0, i64 633
  store float 0xC049000000000000, ptr %t633
  %t634 = getelementptr [1024 x float], ptr %o2, i64 0, i64 634
  store float 0xC045800000000000, ptr %t634
  %t635 = getelementptr [1024 x float], ptr %o2, i64 0, i64 635
  store float 0xC042000000000000, ptr %t635
  %t636 = getelementptr [1024 x float], ptr %o2, i64 0, i64 636
  store float 0xC03D000000000000, ptr %t636
  %t637 = getelementptr [1024 x float], ptr %o2, i64 0, i64 637
  store float 0xC036000000000000, ptr %t637
  %t638 = getelementptr [1024 x float], ptr %o2, i64 0, i64 638
  store float 0xC02E000000000000, ptr %t638
  %t639 = getelementptr [1024 x float], ptr %o2, i64 0, i64 639
  store float 0xC020000000000000, ptr %t639
  %t640 = getelementptr [1024 x float], ptr %o2, i64 0, i64 640
  store float 0xBFF0000000000000, ptr %t640
  %t641 = getelementptr [1024 x float], ptr %o2, i64 0, i64 641
  store float 0x4018000000000000, ptr %t641
  %t642 = getelementptr [1024 x float], ptr %o2, i64 0, i64 642
  store float 0x402A000000000000, ptr %t642
  %t643 = getelementptr [1024 x float], ptr %o2, i64 0, i64 643
  store float 0x4034000000000000, ptr %t643
  %t644 = getelementptr [1024 x float], ptr %o2, i64 0, i64 644
  store float 0x403B000000000000, ptr %t644
  %t645 = getelementptr [1024 x float], ptr %o2, i64 0, i64 645
  store float 0x4041000000000000, ptr %t645
  %t646 = getelementptr [1024 x float], ptr %o2, i64 0, i64 646
  store float 0x4044800000000000, ptr %t646
  %t647 = getelementptr [1024 x float], ptr %o2, i64 0, i64 647
  store float 0x4048000000000000, ptr %t647
  %t648 = getelementptr [1024 x float], ptr %o2, i64 0, i64 648
  store float 0xC047000000000000, ptr %t648
  %t649 = getelementptr [1024 x float], ptr %o2, i64 0, i64 649
  store float 0xC043800000000000, ptr %t649
  %t650 = getelementptr [1024 x float], ptr %o2, i64 0, i64 650
  store float 0xC040000000000000, ptr %t650
  %t651 = getelementptr [1024 x float], ptr %o2, i64 0, i64 651
  store float 0xC039000000000000, ptr %t651
  %t652 = getelementptr [1024 x float], ptr %o2, i64 0, i64 652
  store float 0xC032000000000000, ptr %t652
  %t653 = getelementptr [1024 x float], ptr %o2, i64 0, i64 653
  store float 0xC026000000000000, ptr %t653
  %t654 = getelementptr [1024 x float], ptr %o2, i64 0, i64 654
  store float 0xC010000000000000, ptr %t654
  %t655 = getelementptr [1024 x float], ptr %o2, i64 0, i64 655
  store float 0x4008000000000000, ptr %t655
  %t656 = getelementptr [1024 x float], ptr %o2, i64 0, i64 656
  store float 0x4024000000000000, ptr %t656
  %t657 = getelementptr [1024 x float], ptr %o2, i64 0, i64 657
  store float 0x4031000000000000, ptr %t657
  %t658 = getelementptr [1024 x float], ptr %o2, i64 0, i64 658
  store float 0x4038000000000000, ptr %t658
  %t659 = getelementptr [1024 x float], ptr %o2, i64 0, i64 659
  store float 0x403F000000000000, ptr %t659
  %t660 = getelementptr [1024 x float], ptr %o2, i64 0, i64 660
  store float 0x4043000000000000, ptr %t660
  %t661 = getelementptr [1024 x float], ptr %o2, i64 0, i64 661
  store float 0x4046800000000000, ptr %t661
  %t662 = getelementptr [1024 x float], ptr %o2, i64 0, i64 662
  store float 0xC048800000000000, ptr %t662
  %t663 = getelementptr [1024 x float], ptr %o2, i64 0, i64 663
  store float 0xC045000000000000, ptr %t663
  %t664 = getelementptr [1024 x float], ptr %o2, i64 0, i64 664
  store float 0xC041800000000000, ptr %t664
  %t665 = getelementptr [1024 x float], ptr %o2, i64 0, i64 665
  store float 0xC03C000000000000, ptr %t665
  %t666 = getelementptr [1024 x float], ptr %o2, i64 0, i64 666
  store float 0xC035000000000000, ptr %t666
  %t667 = getelementptr [1024 x float], ptr %o2, i64 0, i64 667
  store float 0xC02C000000000000, ptr %t667
  %t668 = getelementptr [1024 x float], ptr %o2, i64 0, i64 668
  store float 0xC01C000000000000, ptr %t668
  %t669 = getelementptr [1024 x float], ptr %o2, i64 0, i64 669
  store float 0x0000000000000000, ptr %t669
  %t670 = getelementptr [1024 x float], ptr %o2, i64 0, i64 670
  store float 0x401C000000000000, ptr %t670
  %t671 = getelementptr [1024 x float], ptr %o2, i64 0, i64 671
  store float 0x402C000000000000, ptr %t671
  %t672 = getelementptr [1024 x float], ptr %o2, i64 0, i64 672
  store float 0x4035000000000000, ptr %t672
  %t673 = getelementptr [1024 x float], ptr %o2, i64 0, i64 673
  store float 0x403C000000000000, ptr %t673
  %t674 = getelementptr [1024 x float], ptr %o2, i64 0, i64 674
  store float 0x4041800000000000, ptr %t674
  %t675 = getelementptr [1024 x float], ptr %o2, i64 0, i64 675
  store float 0x4045000000000000, ptr %t675
  %t676 = getelementptr [1024 x float], ptr %o2, i64 0, i64 676
  store float 0x4048800000000000, ptr %t676
  %t677 = getelementptr [1024 x float], ptr %o2, i64 0, i64 677
  store float 0xC046800000000000, ptr %t677
  %t678 = getelementptr [1024 x float], ptr %o2, i64 0, i64 678
  store float 0xC043000000000000, ptr %t678
  %t679 = getelementptr [1024 x float], ptr %o2, i64 0, i64 679
  store float 0xC03F000000000000, ptr %t679
  %t680 = getelementptr [1024 x float], ptr %o2, i64 0, i64 680
  store float 0xC038000000000000, ptr %t680
  %t681 = getelementptr [1024 x float], ptr %o2, i64 0, i64 681
  store float 0xC031000000000000, ptr %t681
  %t682 = getelementptr [1024 x float], ptr %o2, i64 0, i64 682
  store float 0xC024000000000000, ptr %t682
  %t683 = getelementptr [1024 x float], ptr %o2, i64 0, i64 683
  store float 0xC008000000000000, ptr %t683
  %t684 = getelementptr [1024 x float], ptr %o2, i64 0, i64 684
  store float 0x4010000000000000, ptr %t684
  %t685 = getelementptr [1024 x float], ptr %o2, i64 0, i64 685
  store float 0x4026000000000000, ptr %t685
  %t686 = getelementptr [1024 x float], ptr %o2, i64 0, i64 686
  store float 0x4032000000000000, ptr %t686
  %t687 = getelementptr [1024 x float], ptr %o2, i64 0, i64 687
  store float 0x4039000000000000, ptr %t687
  %t688 = getelementptr [1024 x float], ptr %o2, i64 0, i64 688
  store float 0x4040000000000000, ptr %t688
  %t689 = getelementptr [1024 x float], ptr %o2, i64 0, i64 689
  store float 0x4043800000000000, ptr %t689
  %t690 = getelementptr [1024 x float], ptr %o2, i64 0, i64 690
  store float 0x4047000000000000, ptr %t690
  %t691 = getelementptr [1024 x float], ptr %o2, i64 0, i64 691
  store float 0xC048000000000000, ptr %t691
  %t692 = getelementptr [1024 x float], ptr %o2, i64 0, i64 692
  store float 0xC044800000000000, ptr %t692
  %t693 = getelementptr [1024 x float], ptr %o2, i64 0, i64 693
  store float 0xC041000000000000, ptr %t693
  %t694 = getelementptr [1024 x float], ptr %o2, i64 0, i64 694
  store float 0xC03B000000000000, ptr %t694
  %t695 = getelementptr [1024 x float], ptr %o2, i64 0, i64 695
  store float 0xC034000000000000, ptr %t695
  %t696 = getelementptr [1024 x float], ptr %o2, i64 0, i64 696
  store float 0xC02A000000000000, ptr %t696
  %t697 = getelementptr [1024 x float], ptr %o2, i64 0, i64 697
  store float 0xC018000000000000, ptr %t697
  %t698 = getelementptr [1024 x float], ptr %o2, i64 0, i64 698
  store float 0x3FF0000000000000, ptr %t698
  %t699 = getelementptr [1024 x float], ptr %o2, i64 0, i64 699
  store float 0x4020000000000000, ptr %t699
  %t700 = getelementptr [1024 x float], ptr %o2, i64 0, i64 700
  store float 0x402E000000000000, ptr %t700
  %t701 = getelementptr [1024 x float], ptr %o2, i64 0, i64 701
  store float 0x4036000000000000, ptr %t701
  %t702 = getelementptr [1024 x float], ptr %o2, i64 0, i64 702
  store float 0x403D000000000000, ptr %t702
  %t703 = getelementptr [1024 x float], ptr %o2, i64 0, i64 703
  store float 0x4042000000000000, ptr %t703
  %t704 = getelementptr [1024 x float], ptr %o2, i64 0, i64 704
  store float 0x4045800000000000, ptr %t704
  %t705 = getelementptr [1024 x float], ptr %o2, i64 0, i64 705
  store float 0x4049000000000000, ptr %t705
  %t706 = getelementptr [1024 x float], ptr %o2, i64 0, i64 706
  store float 0xC046000000000000, ptr %t706
  %t707 = getelementptr [1024 x float], ptr %o2, i64 0, i64 707
  store float 0xC042800000000000, ptr %t707
  %t708 = getelementptr [1024 x float], ptr %o2, i64 0, i64 708
  store float 0xC03E000000000000, ptr %t708
  %t709 = getelementptr [1024 x float], ptr %o2, i64 0, i64 709
  store float 0xC037000000000000, ptr %t709
  %t710 = getelementptr [1024 x float], ptr %o2, i64 0, i64 710
  store float 0xC030000000000000, ptr %t710
  %t711 = getelementptr [1024 x float], ptr %o2, i64 0, i64 711
  store float 0xC022000000000000, ptr %t711
  %t712 = getelementptr [1024 x float], ptr %o2, i64 0, i64 712
  store float 0xC000000000000000, ptr %t712
  %t713 = getelementptr [1024 x float], ptr %o2, i64 0, i64 713
  store float 0x4014000000000000, ptr %t713
  %t714 = getelementptr [1024 x float], ptr %o2, i64 0, i64 714
  store float 0x4028000000000000, ptr %t714
  %t715 = getelementptr [1024 x float], ptr %o2, i64 0, i64 715
  store float 0x4033000000000000, ptr %t715
  %t716 = getelementptr [1024 x float], ptr %o2, i64 0, i64 716
  store float 0x403A000000000000, ptr %t716
  %t717 = getelementptr [1024 x float], ptr %o2, i64 0, i64 717
  store float 0x4040800000000000, ptr %t717
  %t718 = getelementptr [1024 x float], ptr %o2, i64 0, i64 718
  store float 0x4044000000000000, ptr %t718
  %t719 = getelementptr [1024 x float], ptr %o2, i64 0, i64 719
  store float 0x4047800000000000, ptr %t719
  %t720 = getelementptr [1024 x float], ptr %o2, i64 0, i64 720
  store float 0xC047800000000000, ptr %t720
  %t721 = getelementptr [1024 x float], ptr %o2, i64 0, i64 721
  store float 0xC044000000000000, ptr %t721
  %t722 = getelementptr [1024 x float], ptr %o2, i64 0, i64 722
  store float 0xC040800000000000, ptr %t722
  %t723 = getelementptr [1024 x float], ptr %o2, i64 0, i64 723
  store float 0xC03A000000000000, ptr %t723
  %t724 = getelementptr [1024 x float], ptr %o2, i64 0, i64 724
  store float 0xC033000000000000, ptr %t724
  %t725 = getelementptr [1024 x float], ptr %o2, i64 0, i64 725
  store float 0xC028000000000000, ptr %t725
  %t726 = getelementptr [1024 x float], ptr %o2, i64 0, i64 726
  store float 0xC014000000000000, ptr %t726
  %t727 = getelementptr [1024 x float], ptr %o2, i64 0, i64 727
  store float 0x4000000000000000, ptr %t727
  %t728 = getelementptr [1024 x float], ptr %o2, i64 0, i64 728
  store float 0x4022000000000000, ptr %t728
  %t729 = getelementptr [1024 x float], ptr %o2, i64 0, i64 729
  store float 0x4030000000000000, ptr %t729
  %t730 = getelementptr [1024 x float], ptr %o2, i64 0, i64 730
  store float 0x4037000000000000, ptr %t730
  %t731 = getelementptr [1024 x float], ptr %o2, i64 0, i64 731
  store float 0x403E000000000000, ptr %t731
  %t732 = getelementptr [1024 x float], ptr %o2, i64 0, i64 732
  store float 0x4042800000000000, ptr %t732
  %t733 = getelementptr [1024 x float], ptr %o2, i64 0, i64 733
  store float 0x4046000000000000, ptr %t733
  %t734 = getelementptr [1024 x float], ptr %o2, i64 0, i64 734
  store float 0xC049000000000000, ptr %t734
  %t735 = getelementptr [1024 x float], ptr %o2, i64 0, i64 735
  store float 0xC045800000000000, ptr %t735
  %t736 = getelementptr [1024 x float], ptr %o2, i64 0, i64 736
  store float 0xC042000000000000, ptr %t736
  %t737 = getelementptr [1024 x float], ptr %o2, i64 0, i64 737
  store float 0xC03D000000000000, ptr %t737
  %t738 = getelementptr [1024 x float], ptr %o2, i64 0, i64 738
  store float 0xC036000000000000, ptr %t738
  %t739 = getelementptr [1024 x float], ptr %o2, i64 0, i64 739
  store float 0xC02E000000000000, ptr %t739
  %t740 = getelementptr [1024 x float], ptr %o2, i64 0, i64 740
  store float 0xC020000000000000, ptr %t740
  %t741 = getelementptr [1024 x float], ptr %o2, i64 0, i64 741
  store float 0xBFF0000000000000, ptr %t741
  %t742 = getelementptr [1024 x float], ptr %o2, i64 0, i64 742
  store float 0x4018000000000000, ptr %t742
  %t743 = getelementptr [1024 x float], ptr %o2, i64 0, i64 743
  store float 0x402A000000000000, ptr %t743
  %t744 = getelementptr [1024 x float], ptr %o2, i64 0, i64 744
  store float 0x4034000000000000, ptr %t744
  %t745 = getelementptr [1024 x float], ptr %o2, i64 0, i64 745
  store float 0x403B000000000000, ptr %t745
  %t746 = getelementptr [1024 x float], ptr %o2, i64 0, i64 746
  store float 0x4041000000000000, ptr %t746
  %t747 = getelementptr [1024 x float], ptr %o2, i64 0, i64 747
  store float 0x4044800000000000, ptr %t747
  %t748 = getelementptr [1024 x float], ptr %o2, i64 0, i64 748
  store float 0x4048000000000000, ptr %t748
  %t749 = getelementptr [1024 x float], ptr %o2, i64 0, i64 749
  store float 0xC047000000000000, ptr %t749
  %t750 = getelementptr [1024 x float], ptr %o2, i64 0, i64 750
  store float 0xC043800000000000, ptr %t750
  %t751 = getelementptr [1024 x float], ptr %o2, i64 0, i64 751
  store float 0xC040000000000000, ptr %t751
  %t752 = getelementptr [1024 x float], ptr %o2, i64 0, i64 752
  store float 0xC039000000000000, ptr %t752
  %t753 = getelementptr [1024 x float], ptr %o2, i64 0, i64 753
  store float 0xC032000000000000, ptr %t753
  %t754 = getelementptr [1024 x float], ptr %o2, i64 0, i64 754
  store float 0xC026000000000000, ptr %t754
  %t755 = getelementptr [1024 x float], ptr %o2, i64 0, i64 755
  store float 0xC010000000000000, ptr %t755
  %t756 = getelementptr [1024 x float], ptr %o2, i64 0, i64 756
  store float 0x4008000000000000, ptr %t756
  %t757 = getelementptr [1024 x float], ptr %o2, i64 0, i64 757
  store float 0x4024000000000000, ptr %t757
  %t758 = getelementptr [1024 x float], ptr %o2, i64 0, i64 758
  store float 0x4031000000000000, ptr %t758
  %t759 = getelementptr [1024 x float], ptr %o2, i64 0, i64 759
  store float 0x4038000000000000, ptr %t759
  %t760 = getelementptr [1024 x float], ptr %o2, i64 0, i64 760
  store float 0x403F000000000000, ptr %t760
  %t761 = getelementptr [1024 x float], ptr %o2, i64 0, i64 761
  store float 0x4043000000000000, ptr %t761
  %t762 = getelementptr [1024 x float], ptr %o2, i64 0, i64 762
  store float 0x4046800000000000, ptr %t762
  %t763 = getelementptr [1024 x float], ptr %o2, i64 0, i64 763
  store float 0xC048800000000000, ptr %t763
  %t764 = getelementptr [1024 x float], ptr %o2, i64 0, i64 764
  store float 0xC045000000000000, ptr %t764
  %t765 = getelementptr [1024 x float], ptr %o2, i64 0, i64 765
  store float 0xC041800000000000, ptr %t765
  %t766 = getelementptr [1024 x float], ptr %o2, i64 0, i64 766
  store float 0xC03C000000000000, ptr %t766
  %t767 = getelementptr [1024 x float], ptr %o2, i64 0, i64 767
  store float 0xC035000000000000, ptr %t767
  %t768 = getelementptr [1024 x float], ptr %o2, i64 0, i64 768
  store float 0xC02C000000000000, ptr %t768
  %t769 = getelementptr [1024 x float], ptr %o2, i64 0, i64 769
  store float 0xC01C000000000000, ptr %t769
  %t770 = getelementptr [1024 x float], ptr %o2, i64 0, i64 770
  store float 0x0000000000000000, ptr %t770
  %t771 = getelementptr [1024 x float], ptr %o2, i64 0, i64 771
  store float 0x401C000000000000, ptr %t771
  %t772 = getelementptr [1024 x float], ptr %o2, i64 0, i64 772
  store float 0x402C000000000000, ptr %t772
  %t773 = getelementptr [1024 x float], ptr %o2, i64 0, i64 773
  store float 0x4035000000000000, ptr %t773
  %t774 = getelementptr [1024 x float], ptr %o2, i64 0, i64 774
  store float 0x403C000000000000, ptr %t774
  %t775 = getelementptr [1024 x float], ptr %o2, i64 0, i64 775
  store float 0x4041800000000000, ptr %t775
  %t776 = getelementptr [1024 x float], ptr %o2, i64 0, i64 776
  store float 0x4045000000000000, ptr %t776
  %t777 = getelementptr [1024 x float], ptr %o2, i64 0, i64 777
  store float 0x4048800000000000, ptr %t777
  %t778 = getelementptr [1024 x float], ptr %o2, i64 0, i64 778
  store float 0xC046800000000000, ptr %t778
  %t779 = getelementptr [1024 x float], ptr %o2, i64 0, i64 779
  store float 0xC043000000000000, ptr %t779
  %t780 = getelementptr [1024 x float], ptr %o2, i64 0, i64 780
  store float 0xC03F000000000000, ptr %t780
  %t781 = getelementptr [1024 x float], ptr %o2, i64 0, i64 781
  store float 0xC038000000000000, ptr %t781
  %t782 = getelementptr [1024 x float], ptr %o2, i64 0, i64 782
  store float 0xC031000000000000, ptr %t782
  %t783 = getelementptr [1024 x float], ptr %o2, i64 0, i64 783
  store float 0xC024000000000000, ptr %t783
  %t784 = getelementptr [1024 x float], ptr %o2, i64 0, i64 784
  store float 0xC008000000000000, ptr %t784
  %t785 = getelementptr [1024 x float], ptr %o2, i64 0, i64 785
  store float 0x4010000000000000, ptr %t785
  %t786 = getelementptr [1024 x float], ptr %o2, i64 0, i64 786
  store float 0x4026000000000000, ptr %t786
  %t787 = getelementptr [1024 x float], ptr %o2, i64 0, i64 787
  store float 0x4032000000000000, ptr %t787
  %t788 = getelementptr [1024 x float], ptr %o2, i64 0, i64 788
  store float 0x4039000000000000, ptr %t788
  %t789 = getelementptr [1024 x float], ptr %o2, i64 0, i64 789
  store float 0x4040000000000000, ptr %t789
  %t790 = getelementptr [1024 x float], ptr %o2, i64 0, i64 790
  store float 0x4043800000000000, ptr %t790
  %t791 = getelementptr [1024 x float], ptr %o2, i64 0, i64 791
  store float 0x4047000000000000, ptr %t791
  %t792 = getelementptr [1024 x float], ptr %o2, i64 0, i64 792
  store float 0xC048000000000000, ptr %t792
  %t793 = getelementptr [1024 x float], ptr %o2, i64 0, i64 793
  store float 0xC044800000000000, ptr %t793
  %t794 = getelementptr [1024 x float], ptr %o2, i64 0, i64 794
  store float 0xC041000000000000, ptr %t794
  %t795 = getelementptr [1024 x float], ptr %o2, i64 0, i64 795
  store float 0xC03B000000000000, ptr %t795
  %t796 = getelementptr [1024 x float], ptr %o2, i64 0, i64 796
  store float 0xC034000000000000, ptr %t796
  %t797 = getelementptr [1024 x float], ptr %o2, i64 0, i64 797
  store float 0xC02A000000000000, ptr %t797
  %t798 = getelementptr [1024 x float], ptr %o2, i64 0, i64 798
  store float 0xC018000000000000, ptr %t798
  %t799 = getelementptr [1024 x float], ptr %o2, i64 0, i64 799
  store float 0x3FF0000000000000, ptr %t799
  %t800 = getelementptr [1024 x float], ptr %o2, i64 0, i64 800
  store float 0x4020000000000000, ptr %t800
  %t801 = getelementptr [1024 x float], ptr %o2, i64 0, i64 801
  store float 0x402E000000000000, ptr %t801
  %t802 = getelementptr [1024 x float], ptr %o2, i64 0, i64 802
  store float 0x4036000000000000, ptr %t802
  %t803 = getelementptr [1024 x float], ptr %o2, i64 0, i64 803
  store float 0x403D000000000000, ptr %t803
  %t804 = getelementptr [1024 x float], ptr %o2, i64 0, i64 804
  store float 0x4042000000000000, ptr %t804
  %t805 = getelementptr [1024 x float], ptr %o2, i64 0, i64 805
  store float 0x4045800000000000, ptr %t805
  %t806 = getelementptr [1024 x float], ptr %o2, i64 0, i64 806
  store float 0x4049000000000000, ptr %t806
  %t807 = getelementptr [1024 x float], ptr %o2, i64 0, i64 807
  store float 0xC046000000000000, ptr %t807
  %t808 = getelementptr [1024 x float], ptr %o2, i64 0, i64 808
  store float 0xC042800000000000, ptr %t808
  %t809 = getelementptr [1024 x float], ptr %o2, i64 0, i64 809
  store float 0xC03E000000000000, ptr %t809
  %t810 = getelementptr [1024 x float], ptr %o2, i64 0, i64 810
  store float 0xC037000000000000, ptr %t810
  %t811 = getelementptr [1024 x float], ptr %o2, i64 0, i64 811
  store float 0xC030000000000000, ptr %t811
  %t812 = getelementptr [1024 x float], ptr %o2, i64 0, i64 812
  store float 0xC022000000000000, ptr %t812
  %t813 = getelementptr [1024 x float], ptr %o2, i64 0, i64 813
  store float 0xC000000000000000, ptr %t813
  %t814 = getelementptr [1024 x float], ptr %o2, i64 0, i64 814
  store float 0x4014000000000000, ptr %t814
  %t815 = getelementptr [1024 x float], ptr %o2, i64 0, i64 815
  store float 0x4028000000000000, ptr %t815
  %t816 = getelementptr [1024 x float], ptr %o2, i64 0, i64 816
  store float 0x4033000000000000, ptr %t816
  %t817 = getelementptr [1024 x float], ptr %o2, i64 0, i64 817
  store float 0x403A000000000000, ptr %t817
  %t818 = getelementptr [1024 x float], ptr %o2, i64 0, i64 818
  store float 0x4040800000000000, ptr %t818
  %t819 = getelementptr [1024 x float], ptr %o2, i64 0, i64 819
  store float 0x4044000000000000, ptr %t819
  %t820 = getelementptr [1024 x float], ptr %o2, i64 0, i64 820
  store float 0x4047800000000000, ptr %t820
  %t821 = getelementptr [1024 x float], ptr %o2, i64 0, i64 821
  store float 0xC047800000000000, ptr %t821
  %t822 = getelementptr [1024 x float], ptr %o2, i64 0, i64 822
  store float 0xC044000000000000, ptr %t822
  %t823 = getelementptr [1024 x float], ptr %o2, i64 0, i64 823
  store float 0xC040800000000000, ptr %t823
  %t824 = getelementptr [1024 x float], ptr %o2, i64 0, i64 824
  store float 0xC03A000000000000, ptr %t824
  %t825 = getelementptr [1024 x float], ptr %o2, i64 0, i64 825
  store float 0xC033000000000000, ptr %t825
  %t826 = getelementptr [1024 x float], ptr %o2, i64 0, i64 826
  store float 0xC028000000000000, ptr %t826
  %t827 = getelementptr [1024 x float], ptr %o2, i64 0, i64 827
  store float 0xC014000000000000, ptr %t827
  %t828 = getelementptr [1024 x float], ptr %o2, i64 0, i64 828
  store float 0x4000000000000000, ptr %t828
  %t829 = getelementptr [1024 x float], ptr %o2, i64 0, i64 829
  store float 0x4022000000000000, ptr %t829
  %t830 = getelementptr [1024 x float], ptr %o2, i64 0, i64 830
  store float 0x4030000000000000, ptr %t830
  %t831 = getelementptr [1024 x float], ptr %o2, i64 0, i64 831
  store float 0x4037000000000000, ptr %t831
  %t832 = getelementptr [1024 x float], ptr %o2, i64 0, i64 832
  store float 0x403E000000000000, ptr %t832
  %t833 = getelementptr [1024 x float], ptr %o2, i64 0, i64 833
  store float 0x4042800000000000, ptr %t833
  %t834 = getelementptr [1024 x float], ptr %o2, i64 0, i64 834
  store float 0x4046000000000000, ptr %t834
  %t835 = getelementptr [1024 x float], ptr %o2, i64 0, i64 835
  store float 0xC049000000000000, ptr %t835
  %t836 = getelementptr [1024 x float], ptr %o2, i64 0, i64 836
  store float 0xC045800000000000, ptr %t836
  %t837 = getelementptr [1024 x float], ptr %o2, i64 0, i64 837
  store float 0xC042000000000000, ptr %t837
  %t838 = getelementptr [1024 x float], ptr %o2, i64 0, i64 838
  store float 0xC03D000000000000, ptr %t838
  %t839 = getelementptr [1024 x float], ptr %o2, i64 0, i64 839
  store float 0xC036000000000000, ptr %t839
  %t840 = getelementptr [1024 x float], ptr %o2, i64 0, i64 840
  store float 0xC02E000000000000, ptr %t840
  %t841 = getelementptr [1024 x float], ptr %o2, i64 0, i64 841
  store float 0xC020000000000000, ptr %t841
  %t842 = getelementptr [1024 x float], ptr %o2, i64 0, i64 842
  store float 0xBFF0000000000000, ptr %t842
  %t843 = getelementptr [1024 x float], ptr %o2, i64 0, i64 843
  store float 0x4018000000000000, ptr %t843
  %t844 = getelementptr [1024 x float], ptr %o2, i64 0, i64 844
  store float 0x402A000000000000, ptr %t844
  %t845 = getelementptr [1024 x float], ptr %o2, i64 0, i64 845
  store float 0x4034000000000000, ptr %t845
  %t846 = getelementptr [1024 x float], ptr %o2, i64 0, i64 846
  store float 0x403B000000000000, ptr %t846
  %t847 = getelementptr [1024 x float], ptr %o2, i64 0, i64 847
  store float 0x4041000000000000, ptr %t847
  %t848 = getelementptr [1024 x float], ptr %o2, i64 0, i64 848
  store float 0x4044800000000000, ptr %t848
  %t849 = getelementptr [1024 x float], ptr %o2, i64 0, i64 849
  store float 0x4048000000000000, ptr %t849
  %t850 = getelementptr [1024 x float], ptr %o2, i64 0, i64 850
  store float 0xC047000000000000, ptr %t850
  %t851 = getelementptr [1024 x float], ptr %o2, i64 0, i64 851
  store float 0xC043800000000000, ptr %t851
  %t852 = getelementptr [1024 x float], ptr %o2, i64 0, i64 852
  store float 0xC040000000000000, ptr %t852
  %t853 = getelementptr [1024 x float], ptr %o2, i64 0, i64 853
  store float 0xC039000000000000, ptr %t853
  %t854 = getelementptr [1024 x float], ptr %o2, i64 0, i64 854
  store float 0xC032000000000000, ptr %t854
  %t855 = getelementptr [1024 x float], ptr %o2, i64 0, i64 855
  store float 0xC026000000000000, ptr %t855
  %t856 = getelementptr [1024 x float], ptr %o2, i64 0, i64 856
  store float 0xC010000000000000, ptr %t856
  %t857 = getelementptr [1024 x float], ptr %o2, i64 0, i64 857
  store float 0x4008000000000000, ptr %t857
  %t858 = getelementptr [1024 x float], ptr %o2, i64 0, i64 858
  store float 0x4024000000000000, ptr %t858
  %t859 = getelementptr [1024 x float], ptr %o2, i64 0, i64 859
  store float 0x4031000000000000, ptr %t859
  %t860 = getelementptr [1024 x float], ptr %o2, i64 0, i64 860
  store float 0x4038000000000000, ptr %t860
  %t861 = getelementptr [1024 x float], ptr %o2, i64 0, i64 861
  store float 0x403F000000000000, ptr %t861
  %t862 = getelementptr [1024 x float], ptr %o2, i64 0, i64 862
  store float 0x4043000000000000, ptr %t862
  %t863 = getelementptr [1024 x float], ptr %o2, i64 0, i64 863
  store float 0x4046800000000000, ptr %t863
  %t864 = getelementptr [1024 x float], ptr %o2, i64 0, i64 864
  store float 0xC048800000000000, ptr %t864
  %t865 = getelementptr [1024 x float], ptr %o2, i64 0, i64 865
  store float 0xC045000000000000, ptr %t865
  %t866 = getelementptr [1024 x float], ptr %o2, i64 0, i64 866
  store float 0xC041800000000000, ptr %t866
  %t867 = getelementptr [1024 x float], ptr %o2, i64 0, i64 867
  store float 0xC03C000000000000, ptr %t867
  %t868 = getelementptr [1024 x float], ptr %o2, i64 0, i64 868
  store float 0xC035000000000000, ptr %t868
  %t869 = getelementptr [1024 x float], ptr %o2, i64 0, i64 869
  store float 0xC02C000000000000, ptr %t869
  %t870 = getelementptr [1024 x float], ptr %o2, i64 0, i64 870
  store float 0xC01C000000000000, ptr %t870
  %t871 = getelementptr [1024 x float], ptr %o2, i64 0, i64 871
  store float 0x0000000000000000, ptr %t871
  %t872 = getelementptr [1024 x float], ptr %o2, i64 0, i64 872
  store float 0x401C000000000000, ptr %t872
  %t873 = getelementptr [1024 x float], ptr %o2, i64 0, i64 873
  store float 0x402C000000000000, ptr %t873
  %t874 = getelementptr [1024 x float], ptr %o2, i64 0, i64 874
  store float 0x4035000000000000, ptr %t874
  %t875 = getelementptr [1024 x float], ptr %o2, i64 0, i64 875
  store float 0x403C000000000000, ptr %t875
  %t876 = getelementptr [1024 x float], ptr %o2, i64 0, i64 876
  store float 0x4041800000000000, ptr %t876
  %t877 = getelementptr [1024 x float], ptr %o2, i64 0, i64 877
  store float 0x4045000000000000, ptr %t877
  %t878 = getelementptr [1024 x float], ptr %o2, i64 0, i64 878
  store float 0x4048800000000000, ptr %t878
  %t879 = getelementptr [1024 x float], ptr %o2, i64 0, i64 879
  store float 0xC046800000000000, ptr %t879
  %t880 = getelementptr [1024 x float], ptr %o2, i64 0, i64 880
  store float 0xC043000000000000, ptr %t880
  %t881 = getelementptr [1024 x float], ptr %o2, i64 0, i64 881
  store float 0xC03F000000000000, ptr %t881
  %t882 = getelementptr [1024 x float], ptr %o2, i64 0, i64 882
  store float 0xC038000000000000, ptr %t882
  %t883 = getelementptr [1024 x float], ptr %o2, i64 0, i64 883
  store float 0xC031000000000000, ptr %t883
  %t884 = getelementptr [1024 x float], ptr %o2, i64 0, i64 884
  store float 0xC024000000000000, ptr %t884
  %t885 = getelementptr [1024 x float], ptr %o2, i64 0, i64 885
  store float 0xC008000000000000, ptr %t885
  %t886 = getelementptr [1024 x float], ptr %o2, i64 0, i64 886
  store float 0x4010000000000000, ptr %t886
  %t887 = getelementptr [1024 x float], ptr %o2, i64 0, i64 887
  store float 0x4026000000000000, ptr %t887
  %t888 = getelementptr [1024 x float], ptr %o2, i64 0, i64 888
  store float 0x4032000000000000, ptr %t888
  %t889 = getelementptr [1024 x float], ptr %o2, i64 0, i64 889
  store float 0x4039000000000000, ptr %t889
  %t890 = getelementptr [1024 x float], ptr %o2, i64 0, i64 890
  store float 0x4040000000000000, ptr %t890
  %t891 = getelementptr [1024 x float], ptr %o2, i64 0, i64 891
  store float 0x4043800000000000, ptr %t891
  %t892 = getelementptr [1024 x float], ptr %o2, i64 0, i64 892
  store float 0x4047000000000000, ptr %t892
  %t893 = getelementptr [1024 x float], ptr %o2, i64 0, i64 893
  store float 0xC048000000000000, ptr %t893
  %t894 = getelementptr [1024 x float], ptr %o2, i64 0, i64 894
  store float 0xC044800000000000, ptr %t894
  %t895 = getelementptr [1024 x float], ptr %o2, i64 0, i64 895
  store float 0xC041000000000000, ptr %t895
  %t896 = getelementptr [1024 x float], ptr %o2, i64 0, i64 896
  store float 0xC03B000000000000, ptr %t896
  %t897 = getelementptr [1024 x float], ptr %o2, i64 0, i64 897
  store float 0xC034000000000000, ptr %t897
  %t898 = getelementptr [1024 x float], ptr %o2, i64 0, i64 898
  store float 0xC02A000000000000, ptr %t898
  %t899 = getelementptr [1024 x float], ptr %o2, i64 0, i64 899
  store float 0xC018000000000000, ptr %t899
  %t900 = getelementptr [1024 x float], ptr %o2, i64 0, i64 900
  store float 0x3FF0000000000000, ptr %t900
  %t901 = getelementptr [1024 x float], ptr %o2, i64 0, i64 901
  store float 0x4020000000000000, ptr %t901
  %t902 = getelementptr [1024 x float], ptr %o2, i64 0, i64 902
  store float 0x402E000000000000, ptr %t902
  %t903 = getelementptr [1024 x float], ptr %o2, i64 0, i64 903
  store float 0x4036000000000000, ptr %t903
  %t904 = getelementptr [1024 x float], ptr %o2, i64 0, i64 904
  store float 0x403D000000000000, ptr %t904
  %t905 = getelementptr [1024 x float], ptr %o2, i64 0, i64 905
  store float 0x4042000000000000, ptr %t905
  %t906 = getelementptr [1024 x float], ptr %o2, i64 0, i64 906
  store float 0x4045800000000000, ptr %t906
  %t907 = getelementptr [1024 x float], ptr %o2, i64 0, i64 907
  store float 0x4049000000000000, ptr %t907
  %t908 = getelementptr [1024 x float], ptr %o2, i64 0, i64 908
  store float 0xC046000000000000, ptr %t908
  %t909 = getelementptr [1024 x float], ptr %o2, i64 0, i64 909
  store float 0xC042800000000000, ptr %t909
  %t910 = getelementptr [1024 x float], ptr %o2, i64 0, i64 910
  store float 0xC03E000000000000, ptr %t910
  %t911 = getelementptr [1024 x float], ptr %o2, i64 0, i64 911
  store float 0xC037000000000000, ptr %t911
  %t912 = getelementptr [1024 x float], ptr %o2, i64 0, i64 912
  store float 0xC030000000000000, ptr %t912
  %t913 = getelementptr [1024 x float], ptr %o2, i64 0, i64 913
  store float 0xC022000000000000, ptr %t913
  %t914 = getelementptr [1024 x float], ptr %o2, i64 0, i64 914
  store float 0xC000000000000000, ptr %t914
  %t915 = getelementptr [1024 x float], ptr %o2, i64 0, i64 915
  store float 0x4014000000000000, ptr %t915
  %t916 = getelementptr [1024 x float], ptr %o2, i64 0, i64 916
  store float 0x4028000000000000, ptr %t916
  %t917 = getelementptr [1024 x float], ptr %o2, i64 0, i64 917
  store float 0x4033000000000000, ptr %t917
  %t918 = getelementptr [1024 x float], ptr %o2, i64 0, i64 918
  store float 0x403A000000000000, ptr %t918
  %t919 = getelementptr [1024 x float], ptr %o2, i64 0, i64 919
  store float 0x4040800000000000, ptr %t919
  %t920 = getelementptr [1024 x float], ptr %o2, i64 0, i64 920
  store float 0x4044000000000000, ptr %t920
  %t921 = getelementptr [1024 x float], ptr %o2, i64 0, i64 921
  store float 0x4047800000000000, ptr %t921
  %t922 = getelementptr [1024 x float], ptr %o2, i64 0, i64 922
  store float 0xC047800000000000, ptr %t922
  %t923 = getelementptr [1024 x float], ptr %o2, i64 0, i64 923
  store float 0xC044000000000000, ptr %t923
  %t924 = getelementptr [1024 x float], ptr %o2, i64 0, i64 924
  store float 0xC040800000000000, ptr %t924
  %t925 = getelementptr [1024 x float], ptr %o2, i64 0, i64 925
  store float 0xC03A000000000000, ptr %t925
  %t926 = getelementptr [1024 x float], ptr %o2, i64 0, i64 926
  store float 0xC033000000000000, ptr %t926
  %t927 = getelementptr [1024 x float], ptr %o2, i64 0, i64 927
  store float 0xC028000000000000, ptr %t927
  %t928 = getelementptr [1024 x float], ptr %o2, i64 0, i64 928
  store float 0xC014000000000000, ptr %t928
  %t929 = getelementptr [1024 x float], ptr %o2, i64 0, i64 929
  store float 0x4000000000000000, ptr %t929
  %t930 = getelementptr [1024 x float], ptr %o2, i64 0, i64 930
  store float 0x4022000000000000, ptr %t930
  %t931 = getelementptr [1024 x float], ptr %o2, i64 0, i64 931
  store float 0x4030000000000000, ptr %t931
  %t932 = getelementptr [1024 x float], ptr %o2, i64 0, i64 932
  store float 0x4037000000000000, ptr %t932
  %t933 = getelementptr [1024 x float], ptr %o2, i64 0, i64 933
  store float 0x403E000000000000, ptr %t933
  %t934 = getelementptr [1024 x float], ptr %o2, i64 0, i64 934
  store float 0x4042800000000000, ptr %t934
  %t935 = getelementptr [1024 x float], ptr %o2, i64 0, i64 935
  store float 0x4046000000000000, ptr %t935
  %t936 = getelementptr [1024 x float], ptr %o2, i64 0, i64 936
  store float 0xC049000000000000, ptr %t936
  %t937 = getelementptr [1024 x float], ptr %o2, i64 0, i64 937
  store float 0xC045800000000000, ptr %t937
  %t938 = getelementptr [1024 x float], ptr %o2, i64 0, i64 938
  store float 0xC042000000000000, ptr %t938
  %t939 = getelementptr [1024 x float], ptr %o2, i64 0, i64 939
  store float 0xC03D000000000000, ptr %t939
  %t940 = getelementptr [1024 x float], ptr %o2, i64 0, i64 940
  store float 0xC036000000000000, ptr %t940
  %t941 = getelementptr [1024 x float], ptr %o2, i64 0, i64 941
  store float 0xC02E000000000000, ptr %t941
  %t942 = getelementptr [1024 x float], ptr %o2, i64 0, i64 942
  store float 0xC020000000000000, ptr %t942
  %t943 = getelementptr [1024 x float], ptr %o2, i64 0, i64 943
  store float 0xBFF0000000000000, ptr %t943
  %t944 = getelementptr [1024 x float], ptr %o2, i64 0, i64 944
  store float 0x4018000000000000, ptr %t944
  %t945 = getelementptr [1024 x float], ptr %o2, i64 0, i64 945
  store float 0x402A000000000000, ptr %t945
  %t946 = getelementptr [1024 x float], ptr %o2, i64 0, i64 946
  store float 0x4034000000000000, ptr %t946
  %t947 = getelementptr [1024 x float], ptr %o2, i64 0, i64 947
  store float 0x403B000000000000, ptr %t947
  %t948 = getelementptr [1024 x float], ptr %o2, i64 0, i64 948
  store float 0x4041000000000000, ptr %t948
  %t949 = getelementptr [1024 x float], ptr %o2, i64 0, i64 949
  store float 0x4044800000000000, ptr %t949
  %t950 = getelementptr [1024 x float], ptr %o2, i64 0, i64 950
  store float 0x4048000000000000, ptr %t950
  %t951 = getelementptr [1024 x float], ptr %o2, i64 0, i64 951
  store float 0xC047000000000000, ptr %t951
  %t952 = getelementptr [1024 x float], ptr %o2, i64 0, i64 952
  store float 0xC043800000000000, ptr %t952
  %t953 = getelementptr [1024 x float], ptr %o2, i64 0, i64 953
  store float 0xC040000000000000, ptr %t953
  %t954 = getelementptr [1024 x float], ptr %o2, i64 0, i64 954
  store float 0xC039000000000000, ptr %t954
  %t955 = getelementptr [1024 x float], ptr %o2, i64 0, i64 955
  store float 0xC032000000000000, ptr %t955
  %t956 = getelementptr [1024 x float], ptr %o2, i64 0, i64 956
  store float 0xC026000000000000, ptr %t956
  %t957 = getelementptr [1024 x float], ptr %o2, i64 0, i64 957
  store float 0xC010000000000000, ptr %t957
  %t958 = getelementptr [1024 x float], ptr %o2, i64 0, i64 958
  store float 0x4008000000000000, ptr %t958
  %t959 = getelementptr [1024 x float], ptr %o2, i64 0, i64 959
  store float 0x4024000000000000, ptr %t959
  %t960 = getelementptr [1024 x float], ptr %o2, i64 0, i64 960
  store float 0x4031000000000000, ptr %t960
  %t961 = getelementptr [1024 x float], ptr %o2, i64 0, i64 961
  store float 0x4038000000000000, ptr %t961
  %t962 = getelementptr [1024 x float], ptr %o2, i64 0, i64 962
  store float 0x403F000000000000, ptr %t962
  %t963 = getelementptr [1024 x float], ptr %o2, i64 0, i64 963
  store float 0x4043000000000000, ptr %t963
  %t964 = getelementptr [1024 x float], ptr %o2, i64 0, i64 964
  store float 0x4046800000000000, ptr %t964
  %t965 = getelementptr [1024 x float], ptr %o2, i64 0, i64 965
  store float 0xC048800000000000, ptr %t965
  %t966 = getelementptr [1024 x float], ptr %o2, i64 0, i64 966
  store float 0xC045000000000000, ptr %t966
  %t967 = getelementptr [1024 x float], ptr %o2, i64 0, i64 967
  store float 0xC041800000000000, ptr %t967
  %t968 = getelementptr [1024 x float], ptr %o2, i64 0, i64 968
  store float 0xC03C000000000000, ptr %t968
  %t969 = getelementptr [1024 x float], ptr %o2, i64 0, i64 969
  store float 0xC035000000000000, ptr %t969
  %t970 = getelementptr [1024 x float], ptr %o2, i64 0, i64 970
  store float 0xC02C000000000000, ptr %t970
  %t971 = getelementptr [1024 x float], ptr %o2, i64 0, i64 971
  store float 0xC01C000000000000, ptr %t971
  %t972 = getelementptr [1024 x float], ptr %o2, i64 0, i64 972
  store float 0x0000000000000000, ptr %t972
  %t973 = getelementptr [1024 x float], ptr %o2, i64 0, i64 973
  store float 0x401C000000000000, ptr %t973
  %t974 = getelementptr [1024 x float], ptr %o2, i64 0, i64 974
  store float 0x402C000000000000, ptr %t974
  %t975 = getelementptr [1024 x float], ptr %o2, i64 0, i64 975
  store float 0x4035000000000000, ptr %t975
  %t976 = getelementptr [1024 x float], ptr %o2, i64 0, i64 976
  store float 0x403C000000000000, ptr %t976
  %t977 = getelementptr [1024 x float], ptr %o2, i64 0, i64 977
  store float 0x4041800000000000, ptr %t977
  %t978 = getelementptr [1024 x float], ptr %o2, i64 0, i64 978
  store float 0x4045000000000000, ptr %t978
  %t979 = getelementptr [1024 x float], ptr %o2, i64 0, i64 979
  store float 0x4048800000000000, ptr %t979
  %t980 = getelementptr [1024 x float], ptr %o2, i64 0, i64 980
  store float 0xC046800000000000, ptr %t980
  %t981 = getelementptr [1024 x float], ptr %o2, i64 0, i64 981
  store float 0xC043000000000000, ptr %t981
  %t982 = getelementptr [1024 x float], ptr %o2, i64 0, i64 982
  store float 0xC03F000000000000, ptr %t982
  %t983 = getelementptr [1024 x float], ptr %o2, i64 0, i64 983
  store float 0xC038000000000000, ptr %t983
  %t984 = getelementptr [1024 x float], ptr %o2, i64 0, i64 984
  store float 0xC031000000000000, ptr %t984
  %t985 = getelementptr [1024 x float], ptr %o2, i64 0, i64 985
  store float 0xC024000000000000, ptr %t985
  %t986 = getelementptr [1024 x float], ptr %o2, i64 0, i64 986
  store float 0xC008000000000000, ptr %t986
  %t987 = getelementptr [1024 x float], ptr %o2, i64 0, i64 987
  store float 0x4010000000000000, ptr %t987
  %t988 = getelementptr [1024 x float], ptr %o2, i64 0, i64 988
  store float 0x4026000000000000, ptr %t988
  %t989 = getelementptr [1024 x float], ptr %o2, i64 0, i64 989
  store float 0x4032000000000000, ptr %t989
  %t990 = getelementptr [1024 x float], ptr %o2, i64 0, i64 990
  store float 0x4039000000000000, ptr %t990
  %t991 = getelementptr [1024 x float], ptr %o2, i64 0, i64 991
  store float 0x4040000000000000, ptr %t991
  %t992 = getelementptr [1024 x float], ptr %o2, i64 0, i64 992
  store float 0x4043800000000000, ptr %t992
  %t993 = getelementptr [1024 x float], ptr %o2, i64 0, i64 993
  store float 0x4047000000000000, ptr %t993
  %t994 = getelementptr [1024 x float], ptr %o2, i64 0, i64 994
  store float 0xC048000000000000, ptr %t994
  %t995 = getelementptr [1024 x float], ptr %o2, i64 0, i64 995
  store float 0xC044800000000000, ptr %t995
  %t996 = getelementptr [1024 x float], ptr %o2, i64 0, i64 996
  store float 0xC041000000000000, ptr %t996
  %t997 = getelementptr [1024 x float], ptr %o2, i64 0, i64 997
  store float 0xC03B000000000000, ptr %t997
  %t998 = getelementptr [1024 x float], ptr %o2, i64 0, i64 998
  store float 0xC034000000000000, ptr %t998
  %t999 = getelementptr [1024 x float], ptr %o2, i64 0, i64 999
  store float 0xC02A000000000000, ptr %t999
  %t1000 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1000
  store float 0xC018000000000000, ptr %t1000
  %t1001 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1001
  store float 0x3FF0000000000000, ptr %t1001
  %t1002 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1002
  store float 0x4020000000000000, ptr %t1002
  %t1003 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1003
  store float 0x402E000000000000, ptr %t1003
  %t1004 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1004
  store float 0x4036000000000000, ptr %t1004
  %t1005 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1005
  store float 0x403D000000000000, ptr %t1005
  %t1006 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1006
  store float 0x4042000000000000, ptr %t1006
  %t1007 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1007
  store float 0x4045800000000000, ptr %t1007
  %t1008 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1008
  store float 0x4049000000000000, ptr %t1008
  %t1009 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1009
  store float 0xC046000000000000, ptr %t1009
  %t1010 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1010
  store float 0xC042800000000000, ptr %t1010
  %t1011 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1011
  store float 0xC03E000000000000, ptr %t1011
  %t1012 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1012
  store float 0xC037000000000000, ptr %t1012
  %t1013 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1013
  store float 0xC030000000000000, ptr %t1013
  %t1014 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1014
  store float 0xC022000000000000, ptr %t1014
  %t1015 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1015
  store float 0xC000000000000000, ptr %t1015
  %t1016 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1016
  store float 0x4014000000000000, ptr %t1016
  %t1017 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1017
  store float 0x4028000000000000, ptr %t1017
  %t1018 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1018
  store float 0x4033000000000000, ptr %t1018
  %t1019 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1019
  store float 0x403A000000000000, ptr %t1019
  %t1020 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1020
  store float 0x4040800000000000, ptr %t1020
  %t1021 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1021
  store float 0x4044000000000000, ptr %t1021
  %t1022 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1022
  store float 0x4047800000000000, ptr %t1022
  %t1023 = getelementptr [1024 x float], ptr %o2, i64 0, i64 1023
  store float 0xC047800000000000, ptr %t1023
  %t1024 = getelementptr [1024 x float], ptr %o3, i64 0, i64 0
  store float 0x401C000000000000, ptr %t1024
  %t1025 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1
  store float 0x402C000000000000, ptr %t1025
  %t1026 = getelementptr [1024 x float], ptr %o3, i64 0, i64 2
  store float 0x4035000000000000, ptr %t1026
  %t1027 = getelementptr [1024 x float], ptr %o3, i64 0, i64 3
  store float 0x403C000000000000, ptr %t1027
  %t1028 = getelementptr [1024 x float], ptr %o3, i64 0, i64 4
  store float 0x4041800000000000, ptr %t1028
  %t1029 = getelementptr [1024 x float], ptr %o3, i64 0, i64 5
  store float 0x4045000000000000, ptr %t1029
  %t1030 = getelementptr [1024 x float], ptr %o3, i64 0, i64 6
  store float 0x4048800000000000, ptr %t1030
  %t1031 = getelementptr [1024 x float], ptr %o3, i64 0, i64 7
  store float 0xC046800000000000, ptr %t1031
  %t1032 = getelementptr [1024 x float], ptr %o3, i64 0, i64 8
  store float 0xC043000000000000, ptr %t1032
  %t1033 = getelementptr [1024 x float], ptr %o3, i64 0, i64 9
  store float 0xC03F000000000000, ptr %t1033
  %t1034 = getelementptr [1024 x float], ptr %o3, i64 0, i64 10
  store float 0xC038000000000000, ptr %t1034
  %t1035 = getelementptr [1024 x float], ptr %o3, i64 0, i64 11
  store float 0xC031000000000000, ptr %t1035
  %t1036 = getelementptr [1024 x float], ptr %o3, i64 0, i64 12
  store float 0xC024000000000000, ptr %t1036
  %t1037 = getelementptr [1024 x float], ptr %o3, i64 0, i64 13
  store float 0xC008000000000000, ptr %t1037
  %t1038 = getelementptr [1024 x float], ptr %o3, i64 0, i64 14
  store float 0x4010000000000000, ptr %t1038
  %t1039 = getelementptr [1024 x float], ptr %o3, i64 0, i64 15
  store float 0x4026000000000000, ptr %t1039
  %t1040 = getelementptr [1024 x float], ptr %o3, i64 0, i64 16
  store float 0x4032000000000000, ptr %t1040
  %t1041 = getelementptr [1024 x float], ptr %o3, i64 0, i64 17
  store float 0x4039000000000000, ptr %t1041
  %t1042 = getelementptr [1024 x float], ptr %o3, i64 0, i64 18
  store float 0x4040000000000000, ptr %t1042
  %t1043 = getelementptr [1024 x float], ptr %o3, i64 0, i64 19
  store float 0x4043800000000000, ptr %t1043
  %t1044 = getelementptr [1024 x float], ptr %o3, i64 0, i64 20
  store float 0x4047000000000000, ptr %t1044
  %t1045 = getelementptr [1024 x float], ptr %o3, i64 0, i64 21
  store float 0xC048000000000000, ptr %t1045
  %t1046 = getelementptr [1024 x float], ptr %o3, i64 0, i64 22
  store float 0xC044800000000000, ptr %t1046
  %t1047 = getelementptr [1024 x float], ptr %o3, i64 0, i64 23
  store float 0xC041000000000000, ptr %t1047
  %t1048 = getelementptr [1024 x float], ptr %o3, i64 0, i64 24
  store float 0xC03B000000000000, ptr %t1048
  %t1049 = getelementptr [1024 x float], ptr %o3, i64 0, i64 25
  store float 0xC034000000000000, ptr %t1049
  %t1050 = getelementptr [1024 x float], ptr %o3, i64 0, i64 26
  store float 0xC02A000000000000, ptr %t1050
  %t1051 = getelementptr [1024 x float], ptr %o3, i64 0, i64 27
  store float 0xC018000000000000, ptr %t1051
  %t1052 = getelementptr [1024 x float], ptr %o3, i64 0, i64 28
  store float 0x3FF0000000000000, ptr %t1052
  %t1053 = getelementptr [1024 x float], ptr %o3, i64 0, i64 29
  store float 0x4020000000000000, ptr %t1053
  %t1054 = getelementptr [1024 x float], ptr %o3, i64 0, i64 30
  store float 0x402E000000000000, ptr %t1054
  %t1055 = getelementptr [1024 x float], ptr %o3, i64 0, i64 31
  store float 0x4036000000000000, ptr %t1055
  %t1056 = getelementptr [1024 x float], ptr %o3, i64 0, i64 32
  store float 0x403D000000000000, ptr %t1056
  %t1057 = getelementptr [1024 x float], ptr %o3, i64 0, i64 33
  store float 0x4042000000000000, ptr %t1057
  %t1058 = getelementptr [1024 x float], ptr %o3, i64 0, i64 34
  store float 0x4045800000000000, ptr %t1058
  %t1059 = getelementptr [1024 x float], ptr %o3, i64 0, i64 35
  store float 0x4049000000000000, ptr %t1059
  %t1060 = getelementptr [1024 x float], ptr %o3, i64 0, i64 36
  store float 0xC046000000000000, ptr %t1060
  %t1061 = getelementptr [1024 x float], ptr %o3, i64 0, i64 37
  store float 0xC042800000000000, ptr %t1061
  %t1062 = getelementptr [1024 x float], ptr %o3, i64 0, i64 38
  store float 0xC03E000000000000, ptr %t1062
  %t1063 = getelementptr [1024 x float], ptr %o3, i64 0, i64 39
  store float 0xC037000000000000, ptr %t1063
  %t1064 = getelementptr [1024 x float], ptr %o3, i64 0, i64 40
  store float 0xC030000000000000, ptr %t1064
  %t1065 = getelementptr [1024 x float], ptr %o3, i64 0, i64 41
  store float 0xC022000000000000, ptr %t1065
  %t1066 = getelementptr [1024 x float], ptr %o3, i64 0, i64 42
  store float 0xC000000000000000, ptr %t1066
  %t1067 = getelementptr [1024 x float], ptr %o3, i64 0, i64 43
  store float 0x4014000000000000, ptr %t1067
  %t1068 = getelementptr [1024 x float], ptr %o3, i64 0, i64 44
  store float 0x4028000000000000, ptr %t1068
  %t1069 = getelementptr [1024 x float], ptr %o3, i64 0, i64 45
  store float 0x4033000000000000, ptr %t1069
  %t1070 = getelementptr [1024 x float], ptr %o3, i64 0, i64 46
  store float 0x403A000000000000, ptr %t1070
  %t1071 = getelementptr [1024 x float], ptr %o3, i64 0, i64 47
  store float 0x4040800000000000, ptr %t1071
  %t1072 = getelementptr [1024 x float], ptr %o3, i64 0, i64 48
  store float 0x4044000000000000, ptr %t1072
  %t1073 = getelementptr [1024 x float], ptr %o3, i64 0, i64 49
  store float 0x4047800000000000, ptr %t1073
  %t1074 = getelementptr [1024 x float], ptr %o3, i64 0, i64 50
  store float 0xC047800000000000, ptr %t1074
  %t1075 = getelementptr [1024 x float], ptr %o3, i64 0, i64 51
  store float 0xC044000000000000, ptr %t1075
  %t1076 = getelementptr [1024 x float], ptr %o3, i64 0, i64 52
  store float 0xC040800000000000, ptr %t1076
  %t1077 = getelementptr [1024 x float], ptr %o3, i64 0, i64 53
  store float 0xC03A000000000000, ptr %t1077
  %t1078 = getelementptr [1024 x float], ptr %o3, i64 0, i64 54
  store float 0xC033000000000000, ptr %t1078
  %t1079 = getelementptr [1024 x float], ptr %o3, i64 0, i64 55
  store float 0xC028000000000000, ptr %t1079
  %t1080 = getelementptr [1024 x float], ptr %o3, i64 0, i64 56
  store float 0xC014000000000000, ptr %t1080
  %t1081 = getelementptr [1024 x float], ptr %o3, i64 0, i64 57
  store float 0x4000000000000000, ptr %t1081
  %t1082 = getelementptr [1024 x float], ptr %o3, i64 0, i64 58
  store float 0x4022000000000000, ptr %t1082
  %t1083 = getelementptr [1024 x float], ptr %o3, i64 0, i64 59
  store float 0x4030000000000000, ptr %t1083
  %t1084 = getelementptr [1024 x float], ptr %o3, i64 0, i64 60
  store float 0x4037000000000000, ptr %t1084
  %t1085 = getelementptr [1024 x float], ptr %o3, i64 0, i64 61
  store float 0x403E000000000000, ptr %t1085
  %t1086 = getelementptr [1024 x float], ptr %o3, i64 0, i64 62
  store float 0x4042800000000000, ptr %t1086
  %t1087 = getelementptr [1024 x float], ptr %o3, i64 0, i64 63
  store float 0x4046000000000000, ptr %t1087
  %t1088 = getelementptr [1024 x float], ptr %o3, i64 0, i64 64
  store float 0xC049000000000000, ptr %t1088
  %t1089 = getelementptr [1024 x float], ptr %o3, i64 0, i64 65
  store float 0xC045800000000000, ptr %t1089
  %t1090 = getelementptr [1024 x float], ptr %o3, i64 0, i64 66
  store float 0xC042000000000000, ptr %t1090
  %t1091 = getelementptr [1024 x float], ptr %o3, i64 0, i64 67
  store float 0xC03D000000000000, ptr %t1091
  %t1092 = getelementptr [1024 x float], ptr %o3, i64 0, i64 68
  store float 0xC036000000000000, ptr %t1092
  %t1093 = getelementptr [1024 x float], ptr %o3, i64 0, i64 69
  store float 0xC02E000000000000, ptr %t1093
  %t1094 = getelementptr [1024 x float], ptr %o3, i64 0, i64 70
  store float 0xC020000000000000, ptr %t1094
  %t1095 = getelementptr [1024 x float], ptr %o3, i64 0, i64 71
  store float 0xBFF0000000000000, ptr %t1095
  %t1096 = getelementptr [1024 x float], ptr %o3, i64 0, i64 72
  store float 0x4018000000000000, ptr %t1096
  %t1097 = getelementptr [1024 x float], ptr %o3, i64 0, i64 73
  store float 0x402A000000000000, ptr %t1097
  %t1098 = getelementptr [1024 x float], ptr %o3, i64 0, i64 74
  store float 0x4034000000000000, ptr %t1098
  %t1099 = getelementptr [1024 x float], ptr %o3, i64 0, i64 75
  store float 0x403B000000000000, ptr %t1099
  %t1100 = getelementptr [1024 x float], ptr %o3, i64 0, i64 76
  store float 0x4041000000000000, ptr %t1100
  %t1101 = getelementptr [1024 x float], ptr %o3, i64 0, i64 77
  store float 0x4044800000000000, ptr %t1101
  %t1102 = getelementptr [1024 x float], ptr %o3, i64 0, i64 78
  store float 0x4048000000000000, ptr %t1102
  %t1103 = getelementptr [1024 x float], ptr %o3, i64 0, i64 79
  store float 0xC047000000000000, ptr %t1103
  %t1104 = getelementptr [1024 x float], ptr %o3, i64 0, i64 80
  store float 0xC043800000000000, ptr %t1104
  %t1105 = getelementptr [1024 x float], ptr %o3, i64 0, i64 81
  store float 0xC040000000000000, ptr %t1105
  %t1106 = getelementptr [1024 x float], ptr %o3, i64 0, i64 82
  store float 0xC039000000000000, ptr %t1106
  %t1107 = getelementptr [1024 x float], ptr %o3, i64 0, i64 83
  store float 0xC032000000000000, ptr %t1107
  %t1108 = getelementptr [1024 x float], ptr %o3, i64 0, i64 84
  store float 0xC026000000000000, ptr %t1108
  %t1109 = getelementptr [1024 x float], ptr %o3, i64 0, i64 85
  store float 0xC010000000000000, ptr %t1109
  %t1110 = getelementptr [1024 x float], ptr %o3, i64 0, i64 86
  store float 0x4008000000000000, ptr %t1110
  %t1111 = getelementptr [1024 x float], ptr %o3, i64 0, i64 87
  store float 0x4024000000000000, ptr %t1111
  %t1112 = getelementptr [1024 x float], ptr %o3, i64 0, i64 88
  store float 0x4031000000000000, ptr %t1112
  %t1113 = getelementptr [1024 x float], ptr %o3, i64 0, i64 89
  store float 0x4038000000000000, ptr %t1113
  %t1114 = getelementptr [1024 x float], ptr %o3, i64 0, i64 90
  store float 0x403F000000000000, ptr %t1114
  %t1115 = getelementptr [1024 x float], ptr %o3, i64 0, i64 91
  store float 0x4043000000000000, ptr %t1115
  %t1116 = getelementptr [1024 x float], ptr %o3, i64 0, i64 92
  store float 0x4046800000000000, ptr %t1116
  %t1117 = getelementptr [1024 x float], ptr %o3, i64 0, i64 93
  store float 0xC048800000000000, ptr %t1117
  %t1118 = getelementptr [1024 x float], ptr %o3, i64 0, i64 94
  store float 0xC045000000000000, ptr %t1118
  %t1119 = getelementptr [1024 x float], ptr %o3, i64 0, i64 95
  store float 0xC041800000000000, ptr %t1119
  %t1120 = getelementptr [1024 x float], ptr %o3, i64 0, i64 96
  store float 0xC03C000000000000, ptr %t1120
  %t1121 = getelementptr [1024 x float], ptr %o3, i64 0, i64 97
  store float 0xC035000000000000, ptr %t1121
  %t1122 = getelementptr [1024 x float], ptr %o3, i64 0, i64 98
  store float 0xC02C000000000000, ptr %t1122
  %t1123 = getelementptr [1024 x float], ptr %o3, i64 0, i64 99
  store float 0xC01C000000000000, ptr %t1123
  %t1124 = getelementptr [1024 x float], ptr %o3, i64 0, i64 100
  store float 0x0000000000000000, ptr %t1124
  %t1125 = getelementptr [1024 x float], ptr %o3, i64 0, i64 101
  store float 0x401C000000000000, ptr %t1125
  %t1126 = getelementptr [1024 x float], ptr %o3, i64 0, i64 102
  store float 0x402C000000000000, ptr %t1126
  %t1127 = getelementptr [1024 x float], ptr %o3, i64 0, i64 103
  store float 0x4035000000000000, ptr %t1127
  %t1128 = getelementptr [1024 x float], ptr %o3, i64 0, i64 104
  store float 0x403C000000000000, ptr %t1128
  %t1129 = getelementptr [1024 x float], ptr %o3, i64 0, i64 105
  store float 0x4041800000000000, ptr %t1129
  %t1130 = getelementptr [1024 x float], ptr %o3, i64 0, i64 106
  store float 0x4045000000000000, ptr %t1130
  %t1131 = getelementptr [1024 x float], ptr %o3, i64 0, i64 107
  store float 0x4048800000000000, ptr %t1131
  %t1132 = getelementptr [1024 x float], ptr %o3, i64 0, i64 108
  store float 0xC046800000000000, ptr %t1132
  %t1133 = getelementptr [1024 x float], ptr %o3, i64 0, i64 109
  store float 0xC043000000000000, ptr %t1133
  %t1134 = getelementptr [1024 x float], ptr %o3, i64 0, i64 110
  store float 0xC03F000000000000, ptr %t1134
  %t1135 = getelementptr [1024 x float], ptr %o3, i64 0, i64 111
  store float 0xC038000000000000, ptr %t1135
  %t1136 = getelementptr [1024 x float], ptr %o3, i64 0, i64 112
  store float 0xC031000000000000, ptr %t1136
  %t1137 = getelementptr [1024 x float], ptr %o3, i64 0, i64 113
  store float 0xC024000000000000, ptr %t1137
  %t1138 = getelementptr [1024 x float], ptr %o3, i64 0, i64 114
  store float 0xC008000000000000, ptr %t1138
  %t1139 = getelementptr [1024 x float], ptr %o3, i64 0, i64 115
  store float 0x4010000000000000, ptr %t1139
  %t1140 = getelementptr [1024 x float], ptr %o3, i64 0, i64 116
  store float 0x4026000000000000, ptr %t1140
  %t1141 = getelementptr [1024 x float], ptr %o3, i64 0, i64 117
  store float 0x4032000000000000, ptr %t1141
  %t1142 = getelementptr [1024 x float], ptr %o3, i64 0, i64 118
  store float 0x4039000000000000, ptr %t1142
  %t1143 = getelementptr [1024 x float], ptr %o3, i64 0, i64 119
  store float 0x4040000000000000, ptr %t1143
  %t1144 = getelementptr [1024 x float], ptr %o3, i64 0, i64 120
  store float 0x4043800000000000, ptr %t1144
  %t1145 = getelementptr [1024 x float], ptr %o3, i64 0, i64 121
  store float 0x4047000000000000, ptr %t1145
  %t1146 = getelementptr [1024 x float], ptr %o3, i64 0, i64 122
  store float 0xC048000000000000, ptr %t1146
  %t1147 = getelementptr [1024 x float], ptr %o3, i64 0, i64 123
  store float 0xC044800000000000, ptr %t1147
  %t1148 = getelementptr [1024 x float], ptr %o3, i64 0, i64 124
  store float 0xC041000000000000, ptr %t1148
  %t1149 = getelementptr [1024 x float], ptr %o3, i64 0, i64 125
  store float 0xC03B000000000000, ptr %t1149
  %t1150 = getelementptr [1024 x float], ptr %o3, i64 0, i64 126
  store float 0xC034000000000000, ptr %t1150
  %t1151 = getelementptr [1024 x float], ptr %o3, i64 0, i64 127
  store float 0xC02A000000000000, ptr %t1151
  %t1152 = getelementptr [1024 x float], ptr %o3, i64 0, i64 128
  store float 0xC018000000000000, ptr %t1152
  %t1153 = getelementptr [1024 x float], ptr %o3, i64 0, i64 129
  store float 0x3FF0000000000000, ptr %t1153
  %t1154 = getelementptr [1024 x float], ptr %o3, i64 0, i64 130
  store float 0x4020000000000000, ptr %t1154
  %t1155 = getelementptr [1024 x float], ptr %o3, i64 0, i64 131
  store float 0x402E000000000000, ptr %t1155
  %t1156 = getelementptr [1024 x float], ptr %o3, i64 0, i64 132
  store float 0x4036000000000000, ptr %t1156
  %t1157 = getelementptr [1024 x float], ptr %o3, i64 0, i64 133
  store float 0x403D000000000000, ptr %t1157
  %t1158 = getelementptr [1024 x float], ptr %o3, i64 0, i64 134
  store float 0x4042000000000000, ptr %t1158
  %t1159 = getelementptr [1024 x float], ptr %o3, i64 0, i64 135
  store float 0x4045800000000000, ptr %t1159
  %t1160 = getelementptr [1024 x float], ptr %o3, i64 0, i64 136
  store float 0x4049000000000000, ptr %t1160
  %t1161 = getelementptr [1024 x float], ptr %o3, i64 0, i64 137
  store float 0xC046000000000000, ptr %t1161
  %t1162 = getelementptr [1024 x float], ptr %o3, i64 0, i64 138
  store float 0xC042800000000000, ptr %t1162
  %t1163 = getelementptr [1024 x float], ptr %o3, i64 0, i64 139
  store float 0xC03E000000000000, ptr %t1163
  %t1164 = getelementptr [1024 x float], ptr %o3, i64 0, i64 140
  store float 0xC037000000000000, ptr %t1164
  %t1165 = getelementptr [1024 x float], ptr %o3, i64 0, i64 141
  store float 0xC030000000000000, ptr %t1165
  %t1166 = getelementptr [1024 x float], ptr %o3, i64 0, i64 142
  store float 0xC022000000000000, ptr %t1166
  %t1167 = getelementptr [1024 x float], ptr %o3, i64 0, i64 143
  store float 0xC000000000000000, ptr %t1167
  %t1168 = getelementptr [1024 x float], ptr %o3, i64 0, i64 144
  store float 0x4014000000000000, ptr %t1168
  %t1169 = getelementptr [1024 x float], ptr %o3, i64 0, i64 145
  store float 0x4028000000000000, ptr %t1169
  %t1170 = getelementptr [1024 x float], ptr %o3, i64 0, i64 146
  store float 0x4033000000000000, ptr %t1170
  %t1171 = getelementptr [1024 x float], ptr %o3, i64 0, i64 147
  store float 0x403A000000000000, ptr %t1171
  %t1172 = getelementptr [1024 x float], ptr %o3, i64 0, i64 148
  store float 0x4040800000000000, ptr %t1172
  %t1173 = getelementptr [1024 x float], ptr %o3, i64 0, i64 149
  store float 0x4044000000000000, ptr %t1173
  %t1174 = getelementptr [1024 x float], ptr %o3, i64 0, i64 150
  store float 0x4047800000000000, ptr %t1174
  %t1175 = getelementptr [1024 x float], ptr %o3, i64 0, i64 151
  store float 0xC047800000000000, ptr %t1175
  %t1176 = getelementptr [1024 x float], ptr %o3, i64 0, i64 152
  store float 0xC044000000000000, ptr %t1176
  %t1177 = getelementptr [1024 x float], ptr %o3, i64 0, i64 153
  store float 0xC040800000000000, ptr %t1177
  %t1178 = getelementptr [1024 x float], ptr %o3, i64 0, i64 154
  store float 0xC03A000000000000, ptr %t1178
  %t1179 = getelementptr [1024 x float], ptr %o3, i64 0, i64 155
  store float 0xC033000000000000, ptr %t1179
  %t1180 = getelementptr [1024 x float], ptr %o3, i64 0, i64 156
  store float 0xC028000000000000, ptr %t1180
  %t1181 = getelementptr [1024 x float], ptr %o3, i64 0, i64 157
  store float 0xC014000000000000, ptr %t1181
  %t1182 = getelementptr [1024 x float], ptr %o3, i64 0, i64 158
  store float 0x4000000000000000, ptr %t1182
  %t1183 = getelementptr [1024 x float], ptr %o3, i64 0, i64 159
  store float 0x4022000000000000, ptr %t1183
  %t1184 = getelementptr [1024 x float], ptr %o3, i64 0, i64 160
  store float 0x4030000000000000, ptr %t1184
  %t1185 = getelementptr [1024 x float], ptr %o3, i64 0, i64 161
  store float 0x4037000000000000, ptr %t1185
  %t1186 = getelementptr [1024 x float], ptr %o3, i64 0, i64 162
  store float 0x403E000000000000, ptr %t1186
  %t1187 = getelementptr [1024 x float], ptr %o3, i64 0, i64 163
  store float 0x4042800000000000, ptr %t1187
  %t1188 = getelementptr [1024 x float], ptr %o3, i64 0, i64 164
  store float 0x4046000000000000, ptr %t1188
  %t1189 = getelementptr [1024 x float], ptr %o3, i64 0, i64 165
  store float 0xC049000000000000, ptr %t1189
  %t1190 = getelementptr [1024 x float], ptr %o3, i64 0, i64 166
  store float 0xC045800000000000, ptr %t1190
  %t1191 = getelementptr [1024 x float], ptr %o3, i64 0, i64 167
  store float 0xC042000000000000, ptr %t1191
  %t1192 = getelementptr [1024 x float], ptr %o3, i64 0, i64 168
  store float 0xC03D000000000000, ptr %t1192
  %t1193 = getelementptr [1024 x float], ptr %o3, i64 0, i64 169
  store float 0xC036000000000000, ptr %t1193
  %t1194 = getelementptr [1024 x float], ptr %o3, i64 0, i64 170
  store float 0xC02E000000000000, ptr %t1194
  %t1195 = getelementptr [1024 x float], ptr %o3, i64 0, i64 171
  store float 0xC020000000000000, ptr %t1195
  %t1196 = getelementptr [1024 x float], ptr %o3, i64 0, i64 172
  store float 0xBFF0000000000000, ptr %t1196
  %t1197 = getelementptr [1024 x float], ptr %o3, i64 0, i64 173
  store float 0x4018000000000000, ptr %t1197
  %t1198 = getelementptr [1024 x float], ptr %o3, i64 0, i64 174
  store float 0x402A000000000000, ptr %t1198
  %t1199 = getelementptr [1024 x float], ptr %o3, i64 0, i64 175
  store float 0x4034000000000000, ptr %t1199
  %t1200 = getelementptr [1024 x float], ptr %o3, i64 0, i64 176
  store float 0x403B000000000000, ptr %t1200
  %t1201 = getelementptr [1024 x float], ptr %o3, i64 0, i64 177
  store float 0x4041000000000000, ptr %t1201
  %t1202 = getelementptr [1024 x float], ptr %o3, i64 0, i64 178
  store float 0x4044800000000000, ptr %t1202
  %t1203 = getelementptr [1024 x float], ptr %o3, i64 0, i64 179
  store float 0x4048000000000000, ptr %t1203
  %t1204 = getelementptr [1024 x float], ptr %o3, i64 0, i64 180
  store float 0xC047000000000000, ptr %t1204
  %t1205 = getelementptr [1024 x float], ptr %o3, i64 0, i64 181
  store float 0xC043800000000000, ptr %t1205
  %t1206 = getelementptr [1024 x float], ptr %o3, i64 0, i64 182
  store float 0xC040000000000000, ptr %t1206
  %t1207 = getelementptr [1024 x float], ptr %o3, i64 0, i64 183
  store float 0xC039000000000000, ptr %t1207
  %t1208 = getelementptr [1024 x float], ptr %o3, i64 0, i64 184
  store float 0xC032000000000000, ptr %t1208
  %t1209 = getelementptr [1024 x float], ptr %o3, i64 0, i64 185
  store float 0xC026000000000000, ptr %t1209
  %t1210 = getelementptr [1024 x float], ptr %o3, i64 0, i64 186
  store float 0xC010000000000000, ptr %t1210
  %t1211 = getelementptr [1024 x float], ptr %o3, i64 0, i64 187
  store float 0x4008000000000000, ptr %t1211
  %t1212 = getelementptr [1024 x float], ptr %o3, i64 0, i64 188
  store float 0x4024000000000000, ptr %t1212
  %t1213 = getelementptr [1024 x float], ptr %o3, i64 0, i64 189
  store float 0x4031000000000000, ptr %t1213
  %t1214 = getelementptr [1024 x float], ptr %o3, i64 0, i64 190
  store float 0x4038000000000000, ptr %t1214
  %t1215 = getelementptr [1024 x float], ptr %o3, i64 0, i64 191
  store float 0x403F000000000000, ptr %t1215
  %t1216 = getelementptr [1024 x float], ptr %o3, i64 0, i64 192
  store float 0x4043000000000000, ptr %t1216
  %t1217 = getelementptr [1024 x float], ptr %o3, i64 0, i64 193
  store float 0x4046800000000000, ptr %t1217
  %t1218 = getelementptr [1024 x float], ptr %o3, i64 0, i64 194
  store float 0xC048800000000000, ptr %t1218
  %t1219 = getelementptr [1024 x float], ptr %o3, i64 0, i64 195
  store float 0xC045000000000000, ptr %t1219
  %t1220 = getelementptr [1024 x float], ptr %o3, i64 0, i64 196
  store float 0xC041800000000000, ptr %t1220
  %t1221 = getelementptr [1024 x float], ptr %o3, i64 0, i64 197
  store float 0xC03C000000000000, ptr %t1221
  %t1222 = getelementptr [1024 x float], ptr %o3, i64 0, i64 198
  store float 0xC035000000000000, ptr %t1222
  %t1223 = getelementptr [1024 x float], ptr %o3, i64 0, i64 199
  store float 0xC02C000000000000, ptr %t1223
  %t1224 = getelementptr [1024 x float], ptr %o3, i64 0, i64 200
  store float 0xC01C000000000000, ptr %t1224
  %t1225 = getelementptr [1024 x float], ptr %o3, i64 0, i64 201
  store float 0x0000000000000000, ptr %t1225
  %t1226 = getelementptr [1024 x float], ptr %o3, i64 0, i64 202
  store float 0x401C000000000000, ptr %t1226
  %t1227 = getelementptr [1024 x float], ptr %o3, i64 0, i64 203
  store float 0x402C000000000000, ptr %t1227
  %t1228 = getelementptr [1024 x float], ptr %o3, i64 0, i64 204
  store float 0x4035000000000000, ptr %t1228
  %t1229 = getelementptr [1024 x float], ptr %o3, i64 0, i64 205
  store float 0x403C000000000000, ptr %t1229
  %t1230 = getelementptr [1024 x float], ptr %o3, i64 0, i64 206
  store float 0x4041800000000000, ptr %t1230
  %t1231 = getelementptr [1024 x float], ptr %o3, i64 0, i64 207
  store float 0x4045000000000000, ptr %t1231
  %t1232 = getelementptr [1024 x float], ptr %o3, i64 0, i64 208
  store float 0x4048800000000000, ptr %t1232
  %t1233 = getelementptr [1024 x float], ptr %o3, i64 0, i64 209
  store float 0xC046800000000000, ptr %t1233
  %t1234 = getelementptr [1024 x float], ptr %o3, i64 0, i64 210
  store float 0xC043000000000000, ptr %t1234
  %t1235 = getelementptr [1024 x float], ptr %o3, i64 0, i64 211
  store float 0xC03F000000000000, ptr %t1235
  %t1236 = getelementptr [1024 x float], ptr %o3, i64 0, i64 212
  store float 0xC038000000000000, ptr %t1236
  %t1237 = getelementptr [1024 x float], ptr %o3, i64 0, i64 213
  store float 0xC031000000000000, ptr %t1237
  %t1238 = getelementptr [1024 x float], ptr %o3, i64 0, i64 214
  store float 0xC024000000000000, ptr %t1238
  %t1239 = getelementptr [1024 x float], ptr %o3, i64 0, i64 215
  store float 0xC008000000000000, ptr %t1239
  %t1240 = getelementptr [1024 x float], ptr %o3, i64 0, i64 216
  store float 0x4010000000000000, ptr %t1240
  %t1241 = getelementptr [1024 x float], ptr %o3, i64 0, i64 217
  store float 0x4026000000000000, ptr %t1241
  %t1242 = getelementptr [1024 x float], ptr %o3, i64 0, i64 218
  store float 0x4032000000000000, ptr %t1242
  %t1243 = getelementptr [1024 x float], ptr %o3, i64 0, i64 219
  store float 0x4039000000000000, ptr %t1243
  %t1244 = getelementptr [1024 x float], ptr %o3, i64 0, i64 220
  store float 0x4040000000000000, ptr %t1244
  %t1245 = getelementptr [1024 x float], ptr %o3, i64 0, i64 221
  store float 0x4043800000000000, ptr %t1245
  %t1246 = getelementptr [1024 x float], ptr %o3, i64 0, i64 222
  store float 0x4047000000000000, ptr %t1246
  %t1247 = getelementptr [1024 x float], ptr %o3, i64 0, i64 223
  store float 0xC048000000000000, ptr %t1247
  %t1248 = getelementptr [1024 x float], ptr %o3, i64 0, i64 224
  store float 0xC044800000000000, ptr %t1248
  %t1249 = getelementptr [1024 x float], ptr %o3, i64 0, i64 225
  store float 0xC041000000000000, ptr %t1249
  %t1250 = getelementptr [1024 x float], ptr %o3, i64 0, i64 226
  store float 0xC03B000000000000, ptr %t1250
  %t1251 = getelementptr [1024 x float], ptr %o3, i64 0, i64 227
  store float 0xC034000000000000, ptr %t1251
  %t1252 = getelementptr [1024 x float], ptr %o3, i64 0, i64 228
  store float 0xC02A000000000000, ptr %t1252
  %t1253 = getelementptr [1024 x float], ptr %o3, i64 0, i64 229
  store float 0xC018000000000000, ptr %t1253
  %t1254 = getelementptr [1024 x float], ptr %o3, i64 0, i64 230
  store float 0x3FF0000000000000, ptr %t1254
  %t1255 = getelementptr [1024 x float], ptr %o3, i64 0, i64 231
  store float 0x4020000000000000, ptr %t1255
  %t1256 = getelementptr [1024 x float], ptr %o3, i64 0, i64 232
  store float 0x402E000000000000, ptr %t1256
  %t1257 = getelementptr [1024 x float], ptr %o3, i64 0, i64 233
  store float 0x4036000000000000, ptr %t1257
  %t1258 = getelementptr [1024 x float], ptr %o3, i64 0, i64 234
  store float 0x403D000000000000, ptr %t1258
  %t1259 = getelementptr [1024 x float], ptr %o3, i64 0, i64 235
  store float 0x4042000000000000, ptr %t1259
  %t1260 = getelementptr [1024 x float], ptr %o3, i64 0, i64 236
  store float 0x4045800000000000, ptr %t1260
  %t1261 = getelementptr [1024 x float], ptr %o3, i64 0, i64 237
  store float 0x4049000000000000, ptr %t1261
  %t1262 = getelementptr [1024 x float], ptr %o3, i64 0, i64 238
  store float 0xC046000000000000, ptr %t1262
  %t1263 = getelementptr [1024 x float], ptr %o3, i64 0, i64 239
  store float 0xC042800000000000, ptr %t1263
  %t1264 = getelementptr [1024 x float], ptr %o3, i64 0, i64 240
  store float 0xC03E000000000000, ptr %t1264
  %t1265 = getelementptr [1024 x float], ptr %o3, i64 0, i64 241
  store float 0xC037000000000000, ptr %t1265
  %t1266 = getelementptr [1024 x float], ptr %o3, i64 0, i64 242
  store float 0xC030000000000000, ptr %t1266
  %t1267 = getelementptr [1024 x float], ptr %o3, i64 0, i64 243
  store float 0xC022000000000000, ptr %t1267
  %t1268 = getelementptr [1024 x float], ptr %o3, i64 0, i64 244
  store float 0xC000000000000000, ptr %t1268
  %t1269 = getelementptr [1024 x float], ptr %o3, i64 0, i64 245
  store float 0x4014000000000000, ptr %t1269
  %t1270 = getelementptr [1024 x float], ptr %o3, i64 0, i64 246
  store float 0x4028000000000000, ptr %t1270
  %t1271 = getelementptr [1024 x float], ptr %o3, i64 0, i64 247
  store float 0x4033000000000000, ptr %t1271
  %t1272 = getelementptr [1024 x float], ptr %o3, i64 0, i64 248
  store float 0x403A000000000000, ptr %t1272
  %t1273 = getelementptr [1024 x float], ptr %o3, i64 0, i64 249
  store float 0x4040800000000000, ptr %t1273
  %t1274 = getelementptr [1024 x float], ptr %o3, i64 0, i64 250
  store float 0x4044000000000000, ptr %t1274
  %t1275 = getelementptr [1024 x float], ptr %o3, i64 0, i64 251
  store float 0x4047800000000000, ptr %t1275
  %t1276 = getelementptr [1024 x float], ptr %o3, i64 0, i64 252
  store float 0xC047800000000000, ptr %t1276
  %t1277 = getelementptr [1024 x float], ptr %o3, i64 0, i64 253
  store float 0xC044000000000000, ptr %t1277
  %t1278 = getelementptr [1024 x float], ptr %o3, i64 0, i64 254
  store float 0xC040800000000000, ptr %t1278
  %t1279 = getelementptr [1024 x float], ptr %o3, i64 0, i64 255
  store float 0xC03A000000000000, ptr %t1279
  %t1280 = getelementptr [1024 x float], ptr %o3, i64 0, i64 256
  store float 0xC033000000000000, ptr %t1280
  %t1281 = getelementptr [1024 x float], ptr %o3, i64 0, i64 257
  store float 0xC028000000000000, ptr %t1281
  %t1282 = getelementptr [1024 x float], ptr %o3, i64 0, i64 258
  store float 0xC014000000000000, ptr %t1282
  %t1283 = getelementptr [1024 x float], ptr %o3, i64 0, i64 259
  store float 0x4000000000000000, ptr %t1283
  %t1284 = getelementptr [1024 x float], ptr %o3, i64 0, i64 260
  store float 0x4022000000000000, ptr %t1284
  %t1285 = getelementptr [1024 x float], ptr %o3, i64 0, i64 261
  store float 0x4030000000000000, ptr %t1285
  %t1286 = getelementptr [1024 x float], ptr %o3, i64 0, i64 262
  store float 0x4037000000000000, ptr %t1286
  %t1287 = getelementptr [1024 x float], ptr %o3, i64 0, i64 263
  store float 0x403E000000000000, ptr %t1287
  %t1288 = getelementptr [1024 x float], ptr %o3, i64 0, i64 264
  store float 0x4042800000000000, ptr %t1288
  %t1289 = getelementptr [1024 x float], ptr %o3, i64 0, i64 265
  store float 0x4046000000000000, ptr %t1289
  %t1290 = getelementptr [1024 x float], ptr %o3, i64 0, i64 266
  store float 0xC049000000000000, ptr %t1290
  %t1291 = getelementptr [1024 x float], ptr %o3, i64 0, i64 267
  store float 0xC045800000000000, ptr %t1291
  %t1292 = getelementptr [1024 x float], ptr %o3, i64 0, i64 268
  store float 0xC042000000000000, ptr %t1292
  %t1293 = getelementptr [1024 x float], ptr %o3, i64 0, i64 269
  store float 0xC03D000000000000, ptr %t1293
  %t1294 = getelementptr [1024 x float], ptr %o3, i64 0, i64 270
  store float 0xC036000000000000, ptr %t1294
  %t1295 = getelementptr [1024 x float], ptr %o3, i64 0, i64 271
  store float 0xC02E000000000000, ptr %t1295
  %t1296 = getelementptr [1024 x float], ptr %o3, i64 0, i64 272
  store float 0xC020000000000000, ptr %t1296
  %t1297 = getelementptr [1024 x float], ptr %o3, i64 0, i64 273
  store float 0xBFF0000000000000, ptr %t1297
  %t1298 = getelementptr [1024 x float], ptr %o3, i64 0, i64 274
  store float 0x4018000000000000, ptr %t1298
  %t1299 = getelementptr [1024 x float], ptr %o3, i64 0, i64 275
  store float 0x402A000000000000, ptr %t1299
  %t1300 = getelementptr [1024 x float], ptr %o3, i64 0, i64 276
  store float 0x4034000000000000, ptr %t1300
  %t1301 = getelementptr [1024 x float], ptr %o3, i64 0, i64 277
  store float 0x403B000000000000, ptr %t1301
  %t1302 = getelementptr [1024 x float], ptr %o3, i64 0, i64 278
  store float 0x4041000000000000, ptr %t1302
  %t1303 = getelementptr [1024 x float], ptr %o3, i64 0, i64 279
  store float 0x4044800000000000, ptr %t1303
  %t1304 = getelementptr [1024 x float], ptr %o3, i64 0, i64 280
  store float 0x4048000000000000, ptr %t1304
  %t1305 = getelementptr [1024 x float], ptr %o3, i64 0, i64 281
  store float 0xC047000000000000, ptr %t1305
  %t1306 = getelementptr [1024 x float], ptr %o3, i64 0, i64 282
  store float 0xC043800000000000, ptr %t1306
  %t1307 = getelementptr [1024 x float], ptr %o3, i64 0, i64 283
  store float 0xC040000000000000, ptr %t1307
  %t1308 = getelementptr [1024 x float], ptr %o3, i64 0, i64 284
  store float 0xC039000000000000, ptr %t1308
  %t1309 = getelementptr [1024 x float], ptr %o3, i64 0, i64 285
  store float 0xC032000000000000, ptr %t1309
  %t1310 = getelementptr [1024 x float], ptr %o3, i64 0, i64 286
  store float 0xC026000000000000, ptr %t1310
  %t1311 = getelementptr [1024 x float], ptr %o3, i64 0, i64 287
  store float 0xC010000000000000, ptr %t1311
  %t1312 = getelementptr [1024 x float], ptr %o3, i64 0, i64 288
  store float 0x4008000000000000, ptr %t1312
  %t1313 = getelementptr [1024 x float], ptr %o3, i64 0, i64 289
  store float 0x4024000000000000, ptr %t1313
  %t1314 = getelementptr [1024 x float], ptr %o3, i64 0, i64 290
  store float 0x4031000000000000, ptr %t1314
  %t1315 = getelementptr [1024 x float], ptr %o3, i64 0, i64 291
  store float 0x4038000000000000, ptr %t1315
  %t1316 = getelementptr [1024 x float], ptr %o3, i64 0, i64 292
  store float 0x403F000000000000, ptr %t1316
  %t1317 = getelementptr [1024 x float], ptr %o3, i64 0, i64 293
  store float 0x4043000000000000, ptr %t1317
  %t1318 = getelementptr [1024 x float], ptr %o3, i64 0, i64 294
  store float 0x4046800000000000, ptr %t1318
  %t1319 = getelementptr [1024 x float], ptr %o3, i64 0, i64 295
  store float 0xC048800000000000, ptr %t1319
  %t1320 = getelementptr [1024 x float], ptr %o3, i64 0, i64 296
  store float 0xC045000000000000, ptr %t1320
  %t1321 = getelementptr [1024 x float], ptr %o3, i64 0, i64 297
  store float 0xC041800000000000, ptr %t1321
  %t1322 = getelementptr [1024 x float], ptr %o3, i64 0, i64 298
  store float 0xC03C000000000000, ptr %t1322
  %t1323 = getelementptr [1024 x float], ptr %o3, i64 0, i64 299
  store float 0xC035000000000000, ptr %t1323
  %t1324 = getelementptr [1024 x float], ptr %o3, i64 0, i64 300
  store float 0xC02C000000000000, ptr %t1324
  %t1325 = getelementptr [1024 x float], ptr %o3, i64 0, i64 301
  store float 0xC01C000000000000, ptr %t1325
  %t1326 = getelementptr [1024 x float], ptr %o3, i64 0, i64 302
  store float 0x0000000000000000, ptr %t1326
  %t1327 = getelementptr [1024 x float], ptr %o3, i64 0, i64 303
  store float 0x401C000000000000, ptr %t1327
  %t1328 = getelementptr [1024 x float], ptr %o3, i64 0, i64 304
  store float 0x402C000000000000, ptr %t1328
  %t1329 = getelementptr [1024 x float], ptr %o3, i64 0, i64 305
  store float 0x4035000000000000, ptr %t1329
  %t1330 = getelementptr [1024 x float], ptr %o3, i64 0, i64 306
  store float 0x403C000000000000, ptr %t1330
  %t1331 = getelementptr [1024 x float], ptr %o3, i64 0, i64 307
  store float 0x4041800000000000, ptr %t1331
  %t1332 = getelementptr [1024 x float], ptr %o3, i64 0, i64 308
  store float 0x4045000000000000, ptr %t1332
  %t1333 = getelementptr [1024 x float], ptr %o3, i64 0, i64 309
  store float 0x4048800000000000, ptr %t1333
  %t1334 = getelementptr [1024 x float], ptr %o3, i64 0, i64 310
  store float 0xC046800000000000, ptr %t1334
  %t1335 = getelementptr [1024 x float], ptr %o3, i64 0, i64 311
  store float 0xC043000000000000, ptr %t1335
  %t1336 = getelementptr [1024 x float], ptr %o3, i64 0, i64 312
  store float 0xC03F000000000000, ptr %t1336
  %t1337 = getelementptr [1024 x float], ptr %o3, i64 0, i64 313
  store float 0xC038000000000000, ptr %t1337
  %t1338 = getelementptr [1024 x float], ptr %o3, i64 0, i64 314
  store float 0xC031000000000000, ptr %t1338
  %t1339 = getelementptr [1024 x float], ptr %o3, i64 0, i64 315
  store float 0xC024000000000000, ptr %t1339
  %t1340 = getelementptr [1024 x float], ptr %o3, i64 0, i64 316
  store float 0xC008000000000000, ptr %t1340
  %t1341 = getelementptr [1024 x float], ptr %o3, i64 0, i64 317
  store float 0x4010000000000000, ptr %t1341
  %t1342 = getelementptr [1024 x float], ptr %o3, i64 0, i64 318
  store float 0x4026000000000000, ptr %t1342
  %t1343 = getelementptr [1024 x float], ptr %o3, i64 0, i64 319
  store float 0x4032000000000000, ptr %t1343
  %t1344 = getelementptr [1024 x float], ptr %o3, i64 0, i64 320
  store float 0x4039000000000000, ptr %t1344
  %t1345 = getelementptr [1024 x float], ptr %o3, i64 0, i64 321
  store float 0x4040000000000000, ptr %t1345
  %t1346 = getelementptr [1024 x float], ptr %o3, i64 0, i64 322
  store float 0x4043800000000000, ptr %t1346
  %t1347 = getelementptr [1024 x float], ptr %o3, i64 0, i64 323
  store float 0x4047000000000000, ptr %t1347
  %t1348 = getelementptr [1024 x float], ptr %o3, i64 0, i64 324
  store float 0xC048000000000000, ptr %t1348
  %t1349 = getelementptr [1024 x float], ptr %o3, i64 0, i64 325
  store float 0xC044800000000000, ptr %t1349
  %t1350 = getelementptr [1024 x float], ptr %o3, i64 0, i64 326
  store float 0xC041000000000000, ptr %t1350
  %t1351 = getelementptr [1024 x float], ptr %o3, i64 0, i64 327
  store float 0xC03B000000000000, ptr %t1351
  %t1352 = getelementptr [1024 x float], ptr %o3, i64 0, i64 328
  store float 0xC034000000000000, ptr %t1352
  %t1353 = getelementptr [1024 x float], ptr %o3, i64 0, i64 329
  store float 0xC02A000000000000, ptr %t1353
  %t1354 = getelementptr [1024 x float], ptr %o3, i64 0, i64 330
  store float 0xC018000000000000, ptr %t1354
  %t1355 = getelementptr [1024 x float], ptr %o3, i64 0, i64 331
  store float 0x3FF0000000000000, ptr %t1355
  %t1356 = getelementptr [1024 x float], ptr %o3, i64 0, i64 332
  store float 0x4020000000000000, ptr %t1356
  %t1357 = getelementptr [1024 x float], ptr %o3, i64 0, i64 333
  store float 0x402E000000000000, ptr %t1357
  %t1358 = getelementptr [1024 x float], ptr %o3, i64 0, i64 334
  store float 0x4036000000000000, ptr %t1358
  %t1359 = getelementptr [1024 x float], ptr %o3, i64 0, i64 335
  store float 0x403D000000000000, ptr %t1359
  %t1360 = getelementptr [1024 x float], ptr %o3, i64 0, i64 336
  store float 0x4042000000000000, ptr %t1360
  %t1361 = getelementptr [1024 x float], ptr %o3, i64 0, i64 337
  store float 0x4045800000000000, ptr %t1361
  %t1362 = getelementptr [1024 x float], ptr %o3, i64 0, i64 338
  store float 0x4049000000000000, ptr %t1362
  %t1363 = getelementptr [1024 x float], ptr %o3, i64 0, i64 339
  store float 0xC046000000000000, ptr %t1363
  %t1364 = getelementptr [1024 x float], ptr %o3, i64 0, i64 340
  store float 0xC042800000000000, ptr %t1364
  %t1365 = getelementptr [1024 x float], ptr %o3, i64 0, i64 341
  store float 0xC03E000000000000, ptr %t1365
  %t1366 = getelementptr [1024 x float], ptr %o3, i64 0, i64 342
  store float 0xC037000000000000, ptr %t1366
  %t1367 = getelementptr [1024 x float], ptr %o3, i64 0, i64 343
  store float 0xC030000000000000, ptr %t1367
  %t1368 = getelementptr [1024 x float], ptr %o3, i64 0, i64 344
  store float 0xC022000000000000, ptr %t1368
  %t1369 = getelementptr [1024 x float], ptr %o3, i64 0, i64 345
  store float 0xC000000000000000, ptr %t1369
  %t1370 = getelementptr [1024 x float], ptr %o3, i64 0, i64 346
  store float 0x4014000000000000, ptr %t1370
  %t1371 = getelementptr [1024 x float], ptr %o3, i64 0, i64 347
  store float 0x4028000000000000, ptr %t1371
  %t1372 = getelementptr [1024 x float], ptr %o3, i64 0, i64 348
  store float 0x4033000000000000, ptr %t1372
  %t1373 = getelementptr [1024 x float], ptr %o3, i64 0, i64 349
  store float 0x403A000000000000, ptr %t1373
  %t1374 = getelementptr [1024 x float], ptr %o3, i64 0, i64 350
  store float 0x4040800000000000, ptr %t1374
  %t1375 = getelementptr [1024 x float], ptr %o3, i64 0, i64 351
  store float 0x4044000000000000, ptr %t1375
  %t1376 = getelementptr [1024 x float], ptr %o3, i64 0, i64 352
  store float 0x4047800000000000, ptr %t1376
  %t1377 = getelementptr [1024 x float], ptr %o3, i64 0, i64 353
  store float 0xC047800000000000, ptr %t1377
  %t1378 = getelementptr [1024 x float], ptr %o3, i64 0, i64 354
  store float 0xC044000000000000, ptr %t1378
  %t1379 = getelementptr [1024 x float], ptr %o3, i64 0, i64 355
  store float 0xC040800000000000, ptr %t1379
  %t1380 = getelementptr [1024 x float], ptr %o3, i64 0, i64 356
  store float 0xC03A000000000000, ptr %t1380
  %t1381 = getelementptr [1024 x float], ptr %o3, i64 0, i64 357
  store float 0xC033000000000000, ptr %t1381
  %t1382 = getelementptr [1024 x float], ptr %o3, i64 0, i64 358
  store float 0xC028000000000000, ptr %t1382
  %t1383 = getelementptr [1024 x float], ptr %o3, i64 0, i64 359
  store float 0xC014000000000000, ptr %t1383
  %t1384 = getelementptr [1024 x float], ptr %o3, i64 0, i64 360
  store float 0x4000000000000000, ptr %t1384
  %t1385 = getelementptr [1024 x float], ptr %o3, i64 0, i64 361
  store float 0x4022000000000000, ptr %t1385
  %t1386 = getelementptr [1024 x float], ptr %o3, i64 0, i64 362
  store float 0x4030000000000000, ptr %t1386
  %t1387 = getelementptr [1024 x float], ptr %o3, i64 0, i64 363
  store float 0x4037000000000000, ptr %t1387
  %t1388 = getelementptr [1024 x float], ptr %o3, i64 0, i64 364
  store float 0x403E000000000000, ptr %t1388
  %t1389 = getelementptr [1024 x float], ptr %o3, i64 0, i64 365
  store float 0x4042800000000000, ptr %t1389
  %t1390 = getelementptr [1024 x float], ptr %o3, i64 0, i64 366
  store float 0x4046000000000000, ptr %t1390
  %t1391 = getelementptr [1024 x float], ptr %o3, i64 0, i64 367
  store float 0xC049000000000000, ptr %t1391
  %t1392 = getelementptr [1024 x float], ptr %o3, i64 0, i64 368
  store float 0xC045800000000000, ptr %t1392
  %t1393 = getelementptr [1024 x float], ptr %o3, i64 0, i64 369
  store float 0xC042000000000000, ptr %t1393
  %t1394 = getelementptr [1024 x float], ptr %o3, i64 0, i64 370
  store float 0xC03D000000000000, ptr %t1394
  %t1395 = getelementptr [1024 x float], ptr %o3, i64 0, i64 371
  store float 0xC036000000000000, ptr %t1395
  %t1396 = getelementptr [1024 x float], ptr %o3, i64 0, i64 372
  store float 0xC02E000000000000, ptr %t1396
  %t1397 = getelementptr [1024 x float], ptr %o3, i64 0, i64 373
  store float 0xC020000000000000, ptr %t1397
  %t1398 = getelementptr [1024 x float], ptr %o3, i64 0, i64 374
  store float 0xBFF0000000000000, ptr %t1398
  %t1399 = getelementptr [1024 x float], ptr %o3, i64 0, i64 375
  store float 0x4018000000000000, ptr %t1399
  %t1400 = getelementptr [1024 x float], ptr %o3, i64 0, i64 376
  store float 0x402A000000000000, ptr %t1400
  %t1401 = getelementptr [1024 x float], ptr %o3, i64 0, i64 377
  store float 0x4034000000000000, ptr %t1401
  %t1402 = getelementptr [1024 x float], ptr %o3, i64 0, i64 378
  store float 0x403B000000000000, ptr %t1402
  %t1403 = getelementptr [1024 x float], ptr %o3, i64 0, i64 379
  store float 0x4041000000000000, ptr %t1403
  %t1404 = getelementptr [1024 x float], ptr %o3, i64 0, i64 380
  store float 0x4044800000000000, ptr %t1404
  %t1405 = getelementptr [1024 x float], ptr %o3, i64 0, i64 381
  store float 0x4048000000000000, ptr %t1405
  %t1406 = getelementptr [1024 x float], ptr %o3, i64 0, i64 382
  store float 0xC047000000000000, ptr %t1406
  %t1407 = getelementptr [1024 x float], ptr %o3, i64 0, i64 383
  store float 0xC043800000000000, ptr %t1407
  %t1408 = getelementptr [1024 x float], ptr %o3, i64 0, i64 384
  store float 0xC040000000000000, ptr %t1408
  %t1409 = getelementptr [1024 x float], ptr %o3, i64 0, i64 385
  store float 0xC039000000000000, ptr %t1409
  %t1410 = getelementptr [1024 x float], ptr %o3, i64 0, i64 386
  store float 0xC032000000000000, ptr %t1410
  %t1411 = getelementptr [1024 x float], ptr %o3, i64 0, i64 387
  store float 0xC026000000000000, ptr %t1411
  %t1412 = getelementptr [1024 x float], ptr %o3, i64 0, i64 388
  store float 0xC010000000000000, ptr %t1412
  %t1413 = getelementptr [1024 x float], ptr %o3, i64 0, i64 389
  store float 0x4008000000000000, ptr %t1413
  %t1414 = getelementptr [1024 x float], ptr %o3, i64 0, i64 390
  store float 0x4024000000000000, ptr %t1414
  %t1415 = getelementptr [1024 x float], ptr %o3, i64 0, i64 391
  store float 0x4031000000000000, ptr %t1415
  %t1416 = getelementptr [1024 x float], ptr %o3, i64 0, i64 392
  store float 0x4038000000000000, ptr %t1416
  %t1417 = getelementptr [1024 x float], ptr %o3, i64 0, i64 393
  store float 0x403F000000000000, ptr %t1417
  %t1418 = getelementptr [1024 x float], ptr %o3, i64 0, i64 394
  store float 0x4043000000000000, ptr %t1418
  %t1419 = getelementptr [1024 x float], ptr %o3, i64 0, i64 395
  store float 0x4046800000000000, ptr %t1419
  %t1420 = getelementptr [1024 x float], ptr %o3, i64 0, i64 396
  store float 0xC048800000000000, ptr %t1420
  %t1421 = getelementptr [1024 x float], ptr %o3, i64 0, i64 397
  store float 0xC045000000000000, ptr %t1421
  %t1422 = getelementptr [1024 x float], ptr %o3, i64 0, i64 398
  store float 0xC041800000000000, ptr %t1422
  %t1423 = getelementptr [1024 x float], ptr %o3, i64 0, i64 399
  store float 0xC03C000000000000, ptr %t1423
  %t1424 = getelementptr [1024 x float], ptr %o3, i64 0, i64 400
  store float 0xC035000000000000, ptr %t1424
  %t1425 = getelementptr [1024 x float], ptr %o3, i64 0, i64 401
  store float 0xC02C000000000000, ptr %t1425
  %t1426 = getelementptr [1024 x float], ptr %o3, i64 0, i64 402
  store float 0xC01C000000000000, ptr %t1426
  %t1427 = getelementptr [1024 x float], ptr %o3, i64 0, i64 403
  store float 0x0000000000000000, ptr %t1427
  %t1428 = getelementptr [1024 x float], ptr %o3, i64 0, i64 404
  store float 0x401C000000000000, ptr %t1428
  %t1429 = getelementptr [1024 x float], ptr %o3, i64 0, i64 405
  store float 0x402C000000000000, ptr %t1429
  %t1430 = getelementptr [1024 x float], ptr %o3, i64 0, i64 406
  store float 0x4035000000000000, ptr %t1430
  %t1431 = getelementptr [1024 x float], ptr %o3, i64 0, i64 407
  store float 0x403C000000000000, ptr %t1431
  %t1432 = getelementptr [1024 x float], ptr %o3, i64 0, i64 408
  store float 0x4041800000000000, ptr %t1432
  %t1433 = getelementptr [1024 x float], ptr %o3, i64 0, i64 409
  store float 0x4045000000000000, ptr %t1433
  %t1434 = getelementptr [1024 x float], ptr %o3, i64 0, i64 410
  store float 0x4048800000000000, ptr %t1434
  %t1435 = getelementptr [1024 x float], ptr %o3, i64 0, i64 411
  store float 0xC046800000000000, ptr %t1435
  %t1436 = getelementptr [1024 x float], ptr %o3, i64 0, i64 412
  store float 0xC043000000000000, ptr %t1436
  %t1437 = getelementptr [1024 x float], ptr %o3, i64 0, i64 413
  store float 0xC03F000000000000, ptr %t1437
  %t1438 = getelementptr [1024 x float], ptr %o3, i64 0, i64 414
  store float 0xC038000000000000, ptr %t1438
  %t1439 = getelementptr [1024 x float], ptr %o3, i64 0, i64 415
  store float 0xC031000000000000, ptr %t1439
  %t1440 = getelementptr [1024 x float], ptr %o3, i64 0, i64 416
  store float 0xC024000000000000, ptr %t1440
  %t1441 = getelementptr [1024 x float], ptr %o3, i64 0, i64 417
  store float 0xC008000000000000, ptr %t1441
  %t1442 = getelementptr [1024 x float], ptr %o3, i64 0, i64 418
  store float 0x4010000000000000, ptr %t1442
  %t1443 = getelementptr [1024 x float], ptr %o3, i64 0, i64 419
  store float 0x4026000000000000, ptr %t1443
  %t1444 = getelementptr [1024 x float], ptr %o3, i64 0, i64 420
  store float 0x4032000000000000, ptr %t1444
  %t1445 = getelementptr [1024 x float], ptr %o3, i64 0, i64 421
  store float 0x4039000000000000, ptr %t1445
  %t1446 = getelementptr [1024 x float], ptr %o3, i64 0, i64 422
  store float 0x4040000000000000, ptr %t1446
  %t1447 = getelementptr [1024 x float], ptr %o3, i64 0, i64 423
  store float 0x4043800000000000, ptr %t1447
  %t1448 = getelementptr [1024 x float], ptr %o3, i64 0, i64 424
  store float 0x4047000000000000, ptr %t1448
  %t1449 = getelementptr [1024 x float], ptr %o3, i64 0, i64 425
  store float 0xC048000000000000, ptr %t1449
  %t1450 = getelementptr [1024 x float], ptr %o3, i64 0, i64 426
  store float 0xC044800000000000, ptr %t1450
  %t1451 = getelementptr [1024 x float], ptr %o3, i64 0, i64 427
  store float 0xC041000000000000, ptr %t1451
  %t1452 = getelementptr [1024 x float], ptr %o3, i64 0, i64 428
  store float 0xC03B000000000000, ptr %t1452
  %t1453 = getelementptr [1024 x float], ptr %o3, i64 0, i64 429
  store float 0xC034000000000000, ptr %t1453
  %t1454 = getelementptr [1024 x float], ptr %o3, i64 0, i64 430
  store float 0xC02A000000000000, ptr %t1454
  %t1455 = getelementptr [1024 x float], ptr %o3, i64 0, i64 431
  store float 0xC018000000000000, ptr %t1455
  %t1456 = getelementptr [1024 x float], ptr %o3, i64 0, i64 432
  store float 0x3FF0000000000000, ptr %t1456
  %t1457 = getelementptr [1024 x float], ptr %o3, i64 0, i64 433
  store float 0x4020000000000000, ptr %t1457
  %t1458 = getelementptr [1024 x float], ptr %o3, i64 0, i64 434
  store float 0x402E000000000000, ptr %t1458
  %t1459 = getelementptr [1024 x float], ptr %o3, i64 0, i64 435
  store float 0x4036000000000000, ptr %t1459
  %t1460 = getelementptr [1024 x float], ptr %o3, i64 0, i64 436
  store float 0x403D000000000000, ptr %t1460
  %t1461 = getelementptr [1024 x float], ptr %o3, i64 0, i64 437
  store float 0x4042000000000000, ptr %t1461
  %t1462 = getelementptr [1024 x float], ptr %o3, i64 0, i64 438
  store float 0x4045800000000000, ptr %t1462
  %t1463 = getelementptr [1024 x float], ptr %o3, i64 0, i64 439
  store float 0x4049000000000000, ptr %t1463
  %t1464 = getelementptr [1024 x float], ptr %o3, i64 0, i64 440
  store float 0xC046000000000000, ptr %t1464
  %t1465 = getelementptr [1024 x float], ptr %o3, i64 0, i64 441
  store float 0xC042800000000000, ptr %t1465
  %t1466 = getelementptr [1024 x float], ptr %o3, i64 0, i64 442
  store float 0xC03E000000000000, ptr %t1466
  %t1467 = getelementptr [1024 x float], ptr %o3, i64 0, i64 443
  store float 0xC037000000000000, ptr %t1467
  %t1468 = getelementptr [1024 x float], ptr %o3, i64 0, i64 444
  store float 0xC030000000000000, ptr %t1468
  %t1469 = getelementptr [1024 x float], ptr %o3, i64 0, i64 445
  store float 0xC022000000000000, ptr %t1469
  %t1470 = getelementptr [1024 x float], ptr %o3, i64 0, i64 446
  store float 0xC000000000000000, ptr %t1470
  %t1471 = getelementptr [1024 x float], ptr %o3, i64 0, i64 447
  store float 0x4014000000000000, ptr %t1471
  %t1472 = getelementptr [1024 x float], ptr %o3, i64 0, i64 448
  store float 0x4028000000000000, ptr %t1472
  %t1473 = getelementptr [1024 x float], ptr %o3, i64 0, i64 449
  store float 0x4033000000000000, ptr %t1473
  %t1474 = getelementptr [1024 x float], ptr %o3, i64 0, i64 450
  store float 0x403A000000000000, ptr %t1474
  %t1475 = getelementptr [1024 x float], ptr %o3, i64 0, i64 451
  store float 0x4040800000000000, ptr %t1475
  %t1476 = getelementptr [1024 x float], ptr %o3, i64 0, i64 452
  store float 0x4044000000000000, ptr %t1476
  %t1477 = getelementptr [1024 x float], ptr %o3, i64 0, i64 453
  store float 0x4047800000000000, ptr %t1477
  %t1478 = getelementptr [1024 x float], ptr %o3, i64 0, i64 454
  store float 0xC047800000000000, ptr %t1478
  %t1479 = getelementptr [1024 x float], ptr %o3, i64 0, i64 455
  store float 0xC044000000000000, ptr %t1479
  %t1480 = getelementptr [1024 x float], ptr %o3, i64 0, i64 456
  store float 0xC040800000000000, ptr %t1480
  %t1481 = getelementptr [1024 x float], ptr %o3, i64 0, i64 457
  store float 0xC03A000000000000, ptr %t1481
  %t1482 = getelementptr [1024 x float], ptr %o3, i64 0, i64 458
  store float 0xC033000000000000, ptr %t1482
  %t1483 = getelementptr [1024 x float], ptr %o3, i64 0, i64 459
  store float 0xC028000000000000, ptr %t1483
  %t1484 = getelementptr [1024 x float], ptr %o3, i64 0, i64 460
  store float 0xC014000000000000, ptr %t1484
  %t1485 = getelementptr [1024 x float], ptr %o3, i64 0, i64 461
  store float 0x4000000000000000, ptr %t1485
  %t1486 = getelementptr [1024 x float], ptr %o3, i64 0, i64 462
  store float 0x4022000000000000, ptr %t1486
  %t1487 = getelementptr [1024 x float], ptr %o3, i64 0, i64 463
  store float 0x4030000000000000, ptr %t1487
  %t1488 = getelementptr [1024 x float], ptr %o3, i64 0, i64 464
  store float 0x4037000000000000, ptr %t1488
  %t1489 = getelementptr [1024 x float], ptr %o3, i64 0, i64 465
  store float 0x403E000000000000, ptr %t1489
  %t1490 = getelementptr [1024 x float], ptr %o3, i64 0, i64 466
  store float 0x4042800000000000, ptr %t1490
  %t1491 = getelementptr [1024 x float], ptr %o3, i64 0, i64 467
  store float 0x4046000000000000, ptr %t1491
  %t1492 = getelementptr [1024 x float], ptr %o3, i64 0, i64 468
  store float 0xC049000000000000, ptr %t1492
  %t1493 = getelementptr [1024 x float], ptr %o3, i64 0, i64 469
  store float 0xC045800000000000, ptr %t1493
  %t1494 = getelementptr [1024 x float], ptr %o3, i64 0, i64 470
  store float 0xC042000000000000, ptr %t1494
  %t1495 = getelementptr [1024 x float], ptr %o3, i64 0, i64 471
  store float 0xC03D000000000000, ptr %t1495
  %t1496 = getelementptr [1024 x float], ptr %o3, i64 0, i64 472
  store float 0xC036000000000000, ptr %t1496
  %t1497 = getelementptr [1024 x float], ptr %o3, i64 0, i64 473
  store float 0xC02E000000000000, ptr %t1497
  %t1498 = getelementptr [1024 x float], ptr %o3, i64 0, i64 474
  store float 0xC020000000000000, ptr %t1498
  %t1499 = getelementptr [1024 x float], ptr %o3, i64 0, i64 475
  store float 0xBFF0000000000000, ptr %t1499
  %t1500 = getelementptr [1024 x float], ptr %o3, i64 0, i64 476
  store float 0x4018000000000000, ptr %t1500
  %t1501 = getelementptr [1024 x float], ptr %o3, i64 0, i64 477
  store float 0x402A000000000000, ptr %t1501
  %t1502 = getelementptr [1024 x float], ptr %o3, i64 0, i64 478
  store float 0x4034000000000000, ptr %t1502
  %t1503 = getelementptr [1024 x float], ptr %o3, i64 0, i64 479
  store float 0x403B000000000000, ptr %t1503
  %t1504 = getelementptr [1024 x float], ptr %o3, i64 0, i64 480
  store float 0x4041000000000000, ptr %t1504
  %t1505 = getelementptr [1024 x float], ptr %o3, i64 0, i64 481
  store float 0x4044800000000000, ptr %t1505
  %t1506 = getelementptr [1024 x float], ptr %o3, i64 0, i64 482
  store float 0x4048000000000000, ptr %t1506
  %t1507 = getelementptr [1024 x float], ptr %o3, i64 0, i64 483
  store float 0xC047000000000000, ptr %t1507
  %t1508 = getelementptr [1024 x float], ptr %o3, i64 0, i64 484
  store float 0xC043800000000000, ptr %t1508
  %t1509 = getelementptr [1024 x float], ptr %o3, i64 0, i64 485
  store float 0xC040000000000000, ptr %t1509
  %t1510 = getelementptr [1024 x float], ptr %o3, i64 0, i64 486
  store float 0xC039000000000000, ptr %t1510
  %t1511 = getelementptr [1024 x float], ptr %o3, i64 0, i64 487
  store float 0xC032000000000000, ptr %t1511
  %t1512 = getelementptr [1024 x float], ptr %o3, i64 0, i64 488
  store float 0xC026000000000000, ptr %t1512
  %t1513 = getelementptr [1024 x float], ptr %o3, i64 0, i64 489
  store float 0xC010000000000000, ptr %t1513
  %t1514 = getelementptr [1024 x float], ptr %o3, i64 0, i64 490
  store float 0x4008000000000000, ptr %t1514
  %t1515 = getelementptr [1024 x float], ptr %o3, i64 0, i64 491
  store float 0x4024000000000000, ptr %t1515
  %t1516 = getelementptr [1024 x float], ptr %o3, i64 0, i64 492
  store float 0x4031000000000000, ptr %t1516
  %t1517 = getelementptr [1024 x float], ptr %o3, i64 0, i64 493
  store float 0x4038000000000000, ptr %t1517
  %t1518 = getelementptr [1024 x float], ptr %o3, i64 0, i64 494
  store float 0x403F000000000000, ptr %t1518
  %t1519 = getelementptr [1024 x float], ptr %o3, i64 0, i64 495
  store float 0x4043000000000000, ptr %t1519
  %t1520 = getelementptr [1024 x float], ptr %o3, i64 0, i64 496
  store float 0x4046800000000000, ptr %t1520
  %t1521 = getelementptr [1024 x float], ptr %o3, i64 0, i64 497
  store float 0xC048800000000000, ptr %t1521
  %t1522 = getelementptr [1024 x float], ptr %o3, i64 0, i64 498
  store float 0xC045000000000000, ptr %t1522
  %t1523 = getelementptr [1024 x float], ptr %o3, i64 0, i64 499
  store float 0xC041800000000000, ptr %t1523
  %t1524 = getelementptr [1024 x float], ptr %o3, i64 0, i64 500
  store float 0xC03C000000000000, ptr %t1524
  %t1525 = getelementptr [1024 x float], ptr %o3, i64 0, i64 501
  store float 0xC035000000000000, ptr %t1525
  %t1526 = getelementptr [1024 x float], ptr %o3, i64 0, i64 502
  store float 0xC02C000000000000, ptr %t1526
  %t1527 = getelementptr [1024 x float], ptr %o3, i64 0, i64 503
  store float 0xC01C000000000000, ptr %t1527
  %t1528 = getelementptr [1024 x float], ptr %o3, i64 0, i64 504
  store float 0x0000000000000000, ptr %t1528
  %t1529 = getelementptr [1024 x float], ptr %o3, i64 0, i64 505
  store float 0x401C000000000000, ptr %t1529
  %t1530 = getelementptr [1024 x float], ptr %o3, i64 0, i64 506
  store float 0x402C000000000000, ptr %t1530
  %t1531 = getelementptr [1024 x float], ptr %o3, i64 0, i64 507
  store float 0x4035000000000000, ptr %t1531
  %t1532 = getelementptr [1024 x float], ptr %o3, i64 0, i64 508
  store float 0x403C000000000000, ptr %t1532
  %t1533 = getelementptr [1024 x float], ptr %o3, i64 0, i64 509
  store float 0x4041800000000000, ptr %t1533
  %t1534 = getelementptr [1024 x float], ptr %o3, i64 0, i64 510
  store float 0x4045000000000000, ptr %t1534
  %t1535 = getelementptr [1024 x float], ptr %o3, i64 0, i64 511
  store float 0x4048800000000000, ptr %t1535
  %t1536 = getelementptr [1024 x float], ptr %o3, i64 0, i64 512
  store float 0xC046800000000000, ptr %t1536
  %t1537 = getelementptr [1024 x float], ptr %o3, i64 0, i64 513
  store float 0xC043000000000000, ptr %t1537
  %t1538 = getelementptr [1024 x float], ptr %o3, i64 0, i64 514
  store float 0xC03F000000000000, ptr %t1538
  %t1539 = getelementptr [1024 x float], ptr %o3, i64 0, i64 515
  store float 0xC038000000000000, ptr %t1539
  %t1540 = getelementptr [1024 x float], ptr %o3, i64 0, i64 516
  store float 0xC031000000000000, ptr %t1540
  %t1541 = getelementptr [1024 x float], ptr %o3, i64 0, i64 517
  store float 0xC024000000000000, ptr %t1541
  %t1542 = getelementptr [1024 x float], ptr %o3, i64 0, i64 518
  store float 0xC008000000000000, ptr %t1542
  %t1543 = getelementptr [1024 x float], ptr %o3, i64 0, i64 519
  store float 0x4010000000000000, ptr %t1543
  %t1544 = getelementptr [1024 x float], ptr %o3, i64 0, i64 520
  store float 0x4026000000000000, ptr %t1544
  %t1545 = getelementptr [1024 x float], ptr %o3, i64 0, i64 521
  store float 0x4032000000000000, ptr %t1545
  %t1546 = getelementptr [1024 x float], ptr %o3, i64 0, i64 522
  store float 0x4039000000000000, ptr %t1546
  %t1547 = getelementptr [1024 x float], ptr %o3, i64 0, i64 523
  store float 0x4040000000000000, ptr %t1547
  %t1548 = getelementptr [1024 x float], ptr %o3, i64 0, i64 524
  store float 0x4043800000000000, ptr %t1548
  %t1549 = getelementptr [1024 x float], ptr %o3, i64 0, i64 525
  store float 0x4047000000000000, ptr %t1549
  %t1550 = getelementptr [1024 x float], ptr %o3, i64 0, i64 526
  store float 0xC048000000000000, ptr %t1550
  %t1551 = getelementptr [1024 x float], ptr %o3, i64 0, i64 527
  store float 0xC044800000000000, ptr %t1551
  %t1552 = getelementptr [1024 x float], ptr %o3, i64 0, i64 528
  store float 0xC041000000000000, ptr %t1552
  %t1553 = getelementptr [1024 x float], ptr %o3, i64 0, i64 529
  store float 0xC03B000000000000, ptr %t1553
  %t1554 = getelementptr [1024 x float], ptr %o3, i64 0, i64 530
  store float 0xC034000000000000, ptr %t1554
  %t1555 = getelementptr [1024 x float], ptr %o3, i64 0, i64 531
  store float 0xC02A000000000000, ptr %t1555
  %t1556 = getelementptr [1024 x float], ptr %o3, i64 0, i64 532
  store float 0xC018000000000000, ptr %t1556
  %t1557 = getelementptr [1024 x float], ptr %o3, i64 0, i64 533
  store float 0x3FF0000000000000, ptr %t1557
  %t1558 = getelementptr [1024 x float], ptr %o3, i64 0, i64 534
  store float 0x4020000000000000, ptr %t1558
  %t1559 = getelementptr [1024 x float], ptr %o3, i64 0, i64 535
  store float 0x402E000000000000, ptr %t1559
  %t1560 = getelementptr [1024 x float], ptr %o3, i64 0, i64 536
  store float 0x4036000000000000, ptr %t1560
  %t1561 = getelementptr [1024 x float], ptr %o3, i64 0, i64 537
  store float 0x403D000000000000, ptr %t1561
  %t1562 = getelementptr [1024 x float], ptr %o3, i64 0, i64 538
  store float 0x4042000000000000, ptr %t1562
  %t1563 = getelementptr [1024 x float], ptr %o3, i64 0, i64 539
  store float 0x4045800000000000, ptr %t1563
  %t1564 = getelementptr [1024 x float], ptr %o3, i64 0, i64 540
  store float 0x4049000000000000, ptr %t1564
  %t1565 = getelementptr [1024 x float], ptr %o3, i64 0, i64 541
  store float 0xC046000000000000, ptr %t1565
  %t1566 = getelementptr [1024 x float], ptr %o3, i64 0, i64 542
  store float 0xC042800000000000, ptr %t1566
  %t1567 = getelementptr [1024 x float], ptr %o3, i64 0, i64 543
  store float 0xC03E000000000000, ptr %t1567
  %t1568 = getelementptr [1024 x float], ptr %o3, i64 0, i64 544
  store float 0xC037000000000000, ptr %t1568
  %t1569 = getelementptr [1024 x float], ptr %o3, i64 0, i64 545
  store float 0xC030000000000000, ptr %t1569
  %t1570 = getelementptr [1024 x float], ptr %o3, i64 0, i64 546
  store float 0xC022000000000000, ptr %t1570
  %t1571 = getelementptr [1024 x float], ptr %o3, i64 0, i64 547
  store float 0xC000000000000000, ptr %t1571
  %t1572 = getelementptr [1024 x float], ptr %o3, i64 0, i64 548
  store float 0x4014000000000000, ptr %t1572
  %t1573 = getelementptr [1024 x float], ptr %o3, i64 0, i64 549
  store float 0x4028000000000000, ptr %t1573
  %t1574 = getelementptr [1024 x float], ptr %o3, i64 0, i64 550
  store float 0x4033000000000000, ptr %t1574
  %t1575 = getelementptr [1024 x float], ptr %o3, i64 0, i64 551
  store float 0x403A000000000000, ptr %t1575
  %t1576 = getelementptr [1024 x float], ptr %o3, i64 0, i64 552
  store float 0x4040800000000000, ptr %t1576
  %t1577 = getelementptr [1024 x float], ptr %o3, i64 0, i64 553
  store float 0x4044000000000000, ptr %t1577
  %t1578 = getelementptr [1024 x float], ptr %o3, i64 0, i64 554
  store float 0x4047800000000000, ptr %t1578
  %t1579 = getelementptr [1024 x float], ptr %o3, i64 0, i64 555
  store float 0xC047800000000000, ptr %t1579
  %t1580 = getelementptr [1024 x float], ptr %o3, i64 0, i64 556
  store float 0xC044000000000000, ptr %t1580
  %t1581 = getelementptr [1024 x float], ptr %o3, i64 0, i64 557
  store float 0xC040800000000000, ptr %t1581
  %t1582 = getelementptr [1024 x float], ptr %o3, i64 0, i64 558
  store float 0xC03A000000000000, ptr %t1582
  %t1583 = getelementptr [1024 x float], ptr %o3, i64 0, i64 559
  store float 0xC033000000000000, ptr %t1583
  %t1584 = getelementptr [1024 x float], ptr %o3, i64 0, i64 560
  store float 0xC028000000000000, ptr %t1584
  %t1585 = getelementptr [1024 x float], ptr %o3, i64 0, i64 561
  store float 0xC014000000000000, ptr %t1585
  %t1586 = getelementptr [1024 x float], ptr %o3, i64 0, i64 562
  store float 0x4000000000000000, ptr %t1586
  %t1587 = getelementptr [1024 x float], ptr %o3, i64 0, i64 563
  store float 0x4022000000000000, ptr %t1587
  %t1588 = getelementptr [1024 x float], ptr %o3, i64 0, i64 564
  store float 0x4030000000000000, ptr %t1588
  %t1589 = getelementptr [1024 x float], ptr %o3, i64 0, i64 565
  store float 0x4037000000000000, ptr %t1589
  %t1590 = getelementptr [1024 x float], ptr %o3, i64 0, i64 566
  store float 0x403E000000000000, ptr %t1590
  %t1591 = getelementptr [1024 x float], ptr %o3, i64 0, i64 567
  store float 0x4042800000000000, ptr %t1591
  %t1592 = getelementptr [1024 x float], ptr %o3, i64 0, i64 568
  store float 0x4046000000000000, ptr %t1592
  %t1593 = getelementptr [1024 x float], ptr %o3, i64 0, i64 569
  store float 0xC049000000000000, ptr %t1593
  %t1594 = getelementptr [1024 x float], ptr %o3, i64 0, i64 570
  store float 0xC045800000000000, ptr %t1594
  %t1595 = getelementptr [1024 x float], ptr %o3, i64 0, i64 571
  store float 0xC042000000000000, ptr %t1595
  %t1596 = getelementptr [1024 x float], ptr %o3, i64 0, i64 572
  store float 0xC03D000000000000, ptr %t1596
  %t1597 = getelementptr [1024 x float], ptr %o3, i64 0, i64 573
  store float 0xC036000000000000, ptr %t1597
  %t1598 = getelementptr [1024 x float], ptr %o3, i64 0, i64 574
  store float 0xC02E000000000000, ptr %t1598
  %t1599 = getelementptr [1024 x float], ptr %o3, i64 0, i64 575
  store float 0xC020000000000000, ptr %t1599
  %t1600 = getelementptr [1024 x float], ptr %o3, i64 0, i64 576
  store float 0xBFF0000000000000, ptr %t1600
  %t1601 = getelementptr [1024 x float], ptr %o3, i64 0, i64 577
  store float 0x4018000000000000, ptr %t1601
  %t1602 = getelementptr [1024 x float], ptr %o3, i64 0, i64 578
  store float 0x402A000000000000, ptr %t1602
  %t1603 = getelementptr [1024 x float], ptr %o3, i64 0, i64 579
  store float 0x4034000000000000, ptr %t1603
  %t1604 = getelementptr [1024 x float], ptr %o3, i64 0, i64 580
  store float 0x403B000000000000, ptr %t1604
  %t1605 = getelementptr [1024 x float], ptr %o3, i64 0, i64 581
  store float 0x4041000000000000, ptr %t1605
  %t1606 = getelementptr [1024 x float], ptr %o3, i64 0, i64 582
  store float 0x4044800000000000, ptr %t1606
  %t1607 = getelementptr [1024 x float], ptr %o3, i64 0, i64 583
  store float 0x4048000000000000, ptr %t1607
  %t1608 = getelementptr [1024 x float], ptr %o3, i64 0, i64 584
  store float 0xC047000000000000, ptr %t1608
  %t1609 = getelementptr [1024 x float], ptr %o3, i64 0, i64 585
  store float 0xC043800000000000, ptr %t1609
  %t1610 = getelementptr [1024 x float], ptr %o3, i64 0, i64 586
  store float 0xC040000000000000, ptr %t1610
  %t1611 = getelementptr [1024 x float], ptr %o3, i64 0, i64 587
  store float 0xC039000000000000, ptr %t1611
  %t1612 = getelementptr [1024 x float], ptr %o3, i64 0, i64 588
  store float 0xC032000000000000, ptr %t1612
  %t1613 = getelementptr [1024 x float], ptr %o3, i64 0, i64 589
  store float 0xC026000000000000, ptr %t1613
  %t1614 = getelementptr [1024 x float], ptr %o3, i64 0, i64 590
  store float 0xC010000000000000, ptr %t1614
  %t1615 = getelementptr [1024 x float], ptr %o3, i64 0, i64 591
  store float 0x4008000000000000, ptr %t1615
  %t1616 = getelementptr [1024 x float], ptr %o3, i64 0, i64 592
  store float 0x4024000000000000, ptr %t1616
  %t1617 = getelementptr [1024 x float], ptr %o3, i64 0, i64 593
  store float 0x4031000000000000, ptr %t1617
  %t1618 = getelementptr [1024 x float], ptr %o3, i64 0, i64 594
  store float 0x4038000000000000, ptr %t1618
  %t1619 = getelementptr [1024 x float], ptr %o3, i64 0, i64 595
  store float 0x403F000000000000, ptr %t1619
  %t1620 = getelementptr [1024 x float], ptr %o3, i64 0, i64 596
  store float 0x4043000000000000, ptr %t1620
  %t1621 = getelementptr [1024 x float], ptr %o3, i64 0, i64 597
  store float 0x4046800000000000, ptr %t1621
  %t1622 = getelementptr [1024 x float], ptr %o3, i64 0, i64 598
  store float 0xC048800000000000, ptr %t1622
  %t1623 = getelementptr [1024 x float], ptr %o3, i64 0, i64 599
  store float 0xC045000000000000, ptr %t1623
  %t1624 = getelementptr [1024 x float], ptr %o3, i64 0, i64 600
  store float 0xC041800000000000, ptr %t1624
  %t1625 = getelementptr [1024 x float], ptr %o3, i64 0, i64 601
  store float 0xC03C000000000000, ptr %t1625
  %t1626 = getelementptr [1024 x float], ptr %o3, i64 0, i64 602
  store float 0xC035000000000000, ptr %t1626
  %t1627 = getelementptr [1024 x float], ptr %o3, i64 0, i64 603
  store float 0xC02C000000000000, ptr %t1627
  %t1628 = getelementptr [1024 x float], ptr %o3, i64 0, i64 604
  store float 0xC01C000000000000, ptr %t1628
  %t1629 = getelementptr [1024 x float], ptr %o3, i64 0, i64 605
  store float 0x0000000000000000, ptr %t1629
  %t1630 = getelementptr [1024 x float], ptr %o3, i64 0, i64 606
  store float 0x401C000000000000, ptr %t1630
  %t1631 = getelementptr [1024 x float], ptr %o3, i64 0, i64 607
  store float 0x402C000000000000, ptr %t1631
  %t1632 = getelementptr [1024 x float], ptr %o3, i64 0, i64 608
  store float 0x4035000000000000, ptr %t1632
  %t1633 = getelementptr [1024 x float], ptr %o3, i64 0, i64 609
  store float 0x403C000000000000, ptr %t1633
  %t1634 = getelementptr [1024 x float], ptr %o3, i64 0, i64 610
  store float 0x4041800000000000, ptr %t1634
  %t1635 = getelementptr [1024 x float], ptr %o3, i64 0, i64 611
  store float 0x4045000000000000, ptr %t1635
  %t1636 = getelementptr [1024 x float], ptr %o3, i64 0, i64 612
  store float 0x4048800000000000, ptr %t1636
  %t1637 = getelementptr [1024 x float], ptr %o3, i64 0, i64 613
  store float 0xC046800000000000, ptr %t1637
  %t1638 = getelementptr [1024 x float], ptr %o3, i64 0, i64 614
  store float 0xC043000000000000, ptr %t1638
  %t1639 = getelementptr [1024 x float], ptr %o3, i64 0, i64 615
  store float 0xC03F000000000000, ptr %t1639
  %t1640 = getelementptr [1024 x float], ptr %o3, i64 0, i64 616
  store float 0xC038000000000000, ptr %t1640
  %t1641 = getelementptr [1024 x float], ptr %o3, i64 0, i64 617
  store float 0xC031000000000000, ptr %t1641
  %t1642 = getelementptr [1024 x float], ptr %o3, i64 0, i64 618
  store float 0xC024000000000000, ptr %t1642
  %t1643 = getelementptr [1024 x float], ptr %o3, i64 0, i64 619
  store float 0xC008000000000000, ptr %t1643
  %t1644 = getelementptr [1024 x float], ptr %o3, i64 0, i64 620
  store float 0x4010000000000000, ptr %t1644
  %t1645 = getelementptr [1024 x float], ptr %o3, i64 0, i64 621
  store float 0x4026000000000000, ptr %t1645
  %t1646 = getelementptr [1024 x float], ptr %o3, i64 0, i64 622
  store float 0x4032000000000000, ptr %t1646
  %t1647 = getelementptr [1024 x float], ptr %o3, i64 0, i64 623
  store float 0x4039000000000000, ptr %t1647
  %t1648 = getelementptr [1024 x float], ptr %o3, i64 0, i64 624
  store float 0x4040000000000000, ptr %t1648
  %t1649 = getelementptr [1024 x float], ptr %o3, i64 0, i64 625
  store float 0x4043800000000000, ptr %t1649
  %t1650 = getelementptr [1024 x float], ptr %o3, i64 0, i64 626
  store float 0x4047000000000000, ptr %t1650
  %t1651 = getelementptr [1024 x float], ptr %o3, i64 0, i64 627
  store float 0xC048000000000000, ptr %t1651
  %t1652 = getelementptr [1024 x float], ptr %o3, i64 0, i64 628
  store float 0xC044800000000000, ptr %t1652
  %t1653 = getelementptr [1024 x float], ptr %o3, i64 0, i64 629
  store float 0xC041000000000000, ptr %t1653
  %t1654 = getelementptr [1024 x float], ptr %o3, i64 0, i64 630
  store float 0xC03B000000000000, ptr %t1654
  %t1655 = getelementptr [1024 x float], ptr %o3, i64 0, i64 631
  store float 0xC034000000000000, ptr %t1655
  %t1656 = getelementptr [1024 x float], ptr %o3, i64 0, i64 632
  store float 0xC02A000000000000, ptr %t1656
  %t1657 = getelementptr [1024 x float], ptr %o3, i64 0, i64 633
  store float 0xC018000000000000, ptr %t1657
  %t1658 = getelementptr [1024 x float], ptr %o3, i64 0, i64 634
  store float 0x3FF0000000000000, ptr %t1658
  %t1659 = getelementptr [1024 x float], ptr %o3, i64 0, i64 635
  store float 0x4020000000000000, ptr %t1659
  %t1660 = getelementptr [1024 x float], ptr %o3, i64 0, i64 636
  store float 0x402E000000000000, ptr %t1660
  %t1661 = getelementptr [1024 x float], ptr %o3, i64 0, i64 637
  store float 0x4036000000000000, ptr %t1661
  %t1662 = getelementptr [1024 x float], ptr %o3, i64 0, i64 638
  store float 0x403D000000000000, ptr %t1662
  %t1663 = getelementptr [1024 x float], ptr %o3, i64 0, i64 639
  store float 0x4042000000000000, ptr %t1663
  %t1664 = getelementptr [1024 x float], ptr %o3, i64 0, i64 640
  store float 0x4045800000000000, ptr %t1664
  %t1665 = getelementptr [1024 x float], ptr %o3, i64 0, i64 641
  store float 0x4049000000000000, ptr %t1665
  %t1666 = getelementptr [1024 x float], ptr %o3, i64 0, i64 642
  store float 0xC046000000000000, ptr %t1666
  %t1667 = getelementptr [1024 x float], ptr %o3, i64 0, i64 643
  store float 0xC042800000000000, ptr %t1667
  %t1668 = getelementptr [1024 x float], ptr %o3, i64 0, i64 644
  store float 0xC03E000000000000, ptr %t1668
  %t1669 = getelementptr [1024 x float], ptr %o3, i64 0, i64 645
  store float 0xC037000000000000, ptr %t1669
  %t1670 = getelementptr [1024 x float], ptr %o3, i64 0, i64 646
  store float 0xC030000000000000, ptr %t1670
  %t1671 = getelementptr [1024 x float], ptr %o3, i64 0, i64 647
  store float 0xC022000000000000, ptr %t1671
  %t1672 = getelementptr [1024 x float], ptr %o3, i64 0, i64 648
  store float 0xC000000000000000, ptr %t1672
  %t1673 = getelementptr [1024 x float], ptr %o3, i64 0, i64 649
  store float 0x4014000000000000, ptr %t1673
  %t1674 = getelementptr [1024 x float], ptr %o3, i64 0, i64 650
  store float 0x4028000000000000, ptr %t1674
  %t1675 = getelementptr [1024 x float], ptr %o3, i64 0, i64 651
  store float 0x4033000000000000, ptr %t1675
  %t1676 = getelementptr [1024 x float], ptr %o3, i64 0, i64 652
  store float 0x403A000000000000, ptr %t1676
  %t1677 = getelementptr [1024 x float], ptr %o3, i64 0, i64 653
  store float 0x4040800000000000, ptr %t1677
  %t1678 = getelementptr [1024 x float], ptr %o3, i64 0, i64 654
  store float 0x4044000000000000, ptr %t1678
  %t1679 = getelementptr [1024 x float], ptr %o3, i64 0, i64 655
  store float 0x4047800000000000, ptr %t1679
  %t1680 = getelementptr [1024 x float], ptr %o3, i64 0, i64 656
  store float 0xC047800000000000, ptr %t1680
  %t1681 = getelementptr [1024 x float], ptr %o3, i64 0, i64 657
  store float 0xC044000000000000, ptr %t1681
  %t1682 = getelementptr [1024 x float], ptr %o3, i64 0, i64 658
  store float 0xC040800000000000, ptr %t1682
  %t1683 = getelementptr [1024 x float], ptr %o3, i64 0, i64 659
  store float 0xC03A000000000000, ptr %t1683
  %t1684 = getelementptr [1024 x float], ptr %o3, i64 0, i64 660
  store float 0xC033000000000000, ptr %t1684
  %t1685 = getelementptr [1024 x float], ptr %o3, i64 0, i64 661
  store float 0xC028000000000000, ptr %t1685
  %t1686 = getelementptr [1024 x float], ptr %o3, i64 0, i64 662
  store float 0xC014000000000000, ptr %t1686
  %t1687 = getelementptr [1024 x float], ptr %o3, i64 0, i64 663
  store float 0x4000000000000000, ptr %t1687
  %t1688 = getelementptr [1024 x float], ptr %o3, i64 0, i64 664
  store float 0x4022000000000000, ptr %t1688
  %t1689 = getelementptr [1024 x float], ptr %o3, i64 0, i64 665
  store float 0x4030000000000000, ptr %t1689
  %t1690 = getelementptr [1024 x float], ptr %o3, i64 0, i64 666
  store float 0x4037000000000000, ptr %t1690
  %t1691 = getelementptr [1024 x float], ptr %o3, i64 0, i64 667
  store float 0x403E000000000000, ptr %t1691
  %t1692 = getelementptr [1024 x float], ptr %o3, i64 0, i64 668
  store float 0x4042800000000000, ptr %t1692
  %t1693 = getelementptr [1024 x float], ptr %o3, i64 0, i64 669
  store float 0x4046000000000000, ptr %t1693
  %t1694 = getelementptr [1024 x float], ptr %o3, i64 0, i64 670
  store float 0xC049000000000000, ptr %t1694
  %t1695 = getelementptr [1024 x float], ptr %o3, i64 0, i64 671
  store float 0xC045800000000000, ptr %t1695
  %t1696 = getelementptr [1024 x float], ptr %o3, i64 0, i64 672
  store float 0xC042000000000000, ptr %t1696
  %t1697 = getelementptr [1024 x float], ptr %o3, i64 0, i64 673
  store float 0xC03D000000000000, ptr %t1697
  %t1698 = getelementptr [1024 x float], ptr %o3, i64 0, i64 674
  store float 0xC036000000000000, ptr %t1698
  %t1699 = getelementptr [1024 x float], ptr %o3, i64 0, i64 675
  store float 0xC02E000000000000, ptr %t1699
  %t1700 = getelementptr [1024 x float], ptr %o3, i64 0, i64 676
  store float 0xC020000000000000, ptr %t1700
  %t1701 = getelementptr [1024 x float], ptr %o3, i64 0, i64 677
  store float 0xBFF0000000000000, ptr %t1701
  %t1702 = getelementptr [1024 x float], ptr %o3, i64 0, i64 678
  store float 0x4018000000000000, ptr %t1702
  %t1703 = getelementptr [1024 x float], ptr %o3, i64 0, i64 679
  store float 0x402A000000000000, ptr %t1703
  %t1704 = getelementptr [1024 x float], ptr %o3, i64 0, i64 680
  store float 0x4034000000000000, ptr %t1704
  %t1705 = getelementptr [1024 x float], ptr %o3, i64 0, i64 681
  store float 0x403B000000000000, ptr %t1705
  %t1706 = getelementptr [1024 x float], ptr %o3, i64 0, i64 682
  store float 0x4041000000000000, ptr %t1706
  %t1707 = getelementptr [1024 x float], ptr %o3, i64 0, i64 683
  store float 0x4044800000000000, ptr %t1707
  %t1708 = getelementptr [1024 x float], ptr %o3, i64 0, i64 684
  store float 0x4048000000000000, ptr %t1708
  %t1709 = getelementptr [1024 x float], ptr %o3, i64 0, i64 685
  store float 0xC047000000000000, ptr %t1709
  %t1710 = getelementptr [1024 x float], ptr %o3, i64 0, i64 686
  store float 0xC043800000000000, ptr %t1710
  %t1711 = getelementptr [1024 x float], ptr %o3, i64 0, i64 687
  store float 0xC040000000000000, ptr %t1711
  %t1712 = getelementptr [1024 x float], ptr %o3, i64 0, i64 688
  store float 0xC039000000000000, ptr %t1712
  %t1713 = getelementptr [1024 x float], ptr %o3, i64 0, i64 689
  store float 0xC032000000000000, ptr %t1713
  %t1714 = getelementptr [1024 x float], ptr %o3, i64 0, i64 690
  store float 0xC026000000000000, ptr %t1714
  %t1715 = getelementptr [1024 x float], ptr %o3, i64 0, i64 691
  store float 0xC010000000000000, ptr %t1715
  %t1716 = getelementptr [1024 x float], ptr %o3, i64 0, i64 692
  store float 0x4008000000000000, ptr %t1716
  %t1717 = getelementptr [1024 x float], ptr %o3, i64 0, i64 693
  store float 0x4024000000000000, ptr %t1717
  %t1718 = getelementptr [1024 x float], ptr %o3, i64 0, i64 694
  store float 0x4031000000000000, ptr %t1718
  %t1719 = getelementptr [1024 x float], ptr %o3, i64 0, i64 695
  store float 0x4038000000000000, ptr %t1719
  %t1720 = getelementptr [1024 x float], ptr %o3, i64 0, i64 696
  store float 0x403F000000000000, ptr %t1720
  %t1721 = getelementptr [1024 x float], ptr %o3, i64 0, i64 697
  store float 0x4043000000000000, ptr %t1721
  %t1722 = getelementptr [1024 x float], ptr %o3, i64 0, i64 698
  store float 0x4046800000000000, ptr %t1722
  %t1723 = getelementptr [1024 x float], ptr %o3, i64 0, i64 699
  store float 0xC048800000000000, ptr %t1723
  %t1724 = getelementptr [1024 x float], ptr %o3, i64 0, i64 700
  store float 0xC045000000000000, ptr %t1724
  %t1725 = getelementptr [1024 x float], ptr %o3, i64 0, i64 701
  store float 0xC041800000000000, ptr %t1725
  %t1726 = getelementptr [1024 x float], ptr %o3, i64 0, i64 702
  store float 0xC03C000000000000, ptr %t1726
  %t1727 = getelementptr [1024 x float], ptr %o3, i64 0, i64 703
  store float 0xC035000000000000, ptr %t1727
  %t1728 = getelementptr [1024 x float], ptr %o3, i64 0, i64 704
  store float 0xC02C000000000000, ptr %t1728
  %t1729 = getelementptr [1024 x float], ptr %o3, i64 0, i64 705
  store float 0xC01C000000000000, ptr %t1729
  %t1730 = getelementptr [1024 x float], ptr %o3, i64 0, i64 706
  store float 0x0000000000000000, ptr %t1730
  %t1731 = getelementptr [1024 x float], ptr %o3, i64 0, i64 707
  store float 0x401C000000000000, ptr %t1731
  %t1732 = getelementptr [1024 x float], ptr %o3, i64 0, i64 708
  store float 0x402C000000000000, ptr %t1732
  %t1733 = getelementptr [1024 x float], ptr %o3, i64 0, i64 709
  store float 0x4035000000000000, ptr %t1733
  %t1734 = getelementptr [1024 x float], ptr %o3, i64 0, i64 710
  store float 0x403C000000000000, ptr %t1734
  %t1735 = getelementptr [1024 x float], ptr %o3, i64 0, i64 711
  store float 0x4041800000000000, ptr %t1735
  %t1736 = getelementptr [1024 x float], ptr %o3, i64 0, i64 712
  store float 0x4045000000000000, ptr %t1736
  %t1737 = getelementptr [1024 x float], ptr %o3, i64 0, i64 713
  store float 0x4048800000000000, ptr %t1737
  %t1738 = getelementptr [1024 x float], ptr %o3, i64 0, i64 714
  store float 0xC046800000000000, ptr %t1738
  %t1739 = getelementptr [1024 x float], ptr %o3, i64 0, i64 715
  store float 0xC043000000000000, ptr %t1739
  %t1740 = getelementptr [1024 x float], ptr %o3, i64 0, i64 716
  store float 0xC03F000000000000, ptr %t1740
  %t1741 = getelementptr [1024 x float], ptr %o3, i64 0, i64 717
  store float 0xC038000000000000, ptr %t1741
  %t1742 = getelementptr [1024 x float], ptr %o3, i64 0, i64 718
  store float 0xC031000000000000, ptr %t1742
  %t1743 = getelementptr [1024 x float], ptr %o3, i64 0, i64 719
  store float 0xC024000000000000, ptr %t1743
  %t1744 = getelementptr [1024 x float], ptr %o3, i64 0, i64 720
  store float 0xC008000000000000, ptr %t1744
  %t1745 = getelementptr [1024 x float], ptr %o3, i64 0, i64 721
  store float 0x4010000000000000, ptr %t1745
  %t1746 = getelementptr [1024 x float], ptr %o3, i64 0, i64 722
  store float 0x4026000000000000, ptr %t1746
  %t1747 = getelementptr [1024 x float], ptr %o3, i64 0, i64 723
  store float 0x4032000000000000, ptr %t1747
  %t1748 = getelementptr [1024 x float], ptr %o3, i64 0, i64 724
  store float 0x4039000000000000, ptr %t1748
  %t1749 = getelementptr [1024 x float], ptr %o3, i64 0, i64 725
  store float 0x4040000000000000, ptr %t1749
  %t1750 = getelementptr [1024 x float], ptr %o3, i64 0, i64 726
  store float 0x4043800000000000, ptr %t1750
  %t1751 = getelementptr [1024 x float], ptr %o3, i64 0, i64 727
  store float 0x4047000000000000, ptr %t1751
  %t1752 = getelementptr [1024 x float], ptr %o3, i64 0, i64 728
  store float 0xC048000000000000, ptr %t1752
  %t1753 = getelementptr [1024 x float], ptr %o3, i64 0, i64 729
  store float 0xC044800000000000, ptr %t1753
  %t1754 = getelementptr [1024 x float], ptr %o3, i64 0, i64 730
  store float 0xC041000000000000, ptr %t1754
  %t1755 = getelementptr [1024 x float], ptr %o3, i64 0, i64 731
  store float 0xC03B000000000000, ptr %t1755
  %t1756 = getelementptr [1024 x float], ptr %o3, i64 0, i64 732
  store float 0xC034000000000000, ptr %t1756
  %t1757 = getelementptr [1024 x float], ptr %o3, i64 0, i64 733
  store float 0xC02A000000000000, ptr %t1757
  %t1758 = getelementptr [1024 x float], ptr %o3, i64 0, i64 734
  store float 0xC018000000000000, ptr %t1758
  %t1759 = getelementptr [1024 x float], ptr %o3, i64 0, i64 735
  store float 0x3FF0000000000000, ptr %t1759
  %t1760 = getelementptr [1024 x float], ptr %o3, i64 0, i64 736
  store float 0x4020000000000000, ptr %t1760
  %t1761 = getelementptr [1024 x float], ptr %o3, i64 0, i64 737
  store float 0x402E000000000000, ptr %t1761
  %t1762 = getelementptr [1024 x float], ptr %o3, i64 0, i64 738
  store float 0x4036000000000000, ptr %t1762
  %t1763 = getelementptr [1024 x float], ptr %o3, i64 0, i64 739
  store float 0x403D000000000000, ptr %t1763
  %t1764 = getelementptr [1024 x float], ptr %o3, i64 0, i64 740
  store float 0x4042000000000000, ptr %t1764
  %t1765 = getelementptr [1024 x float], ptr %o3, i64 0, i64 741
  store float 0x4045800000000000, ptr %t1765
  %t1766 = getelementptr [1024 x float], ptr %o3, i64 0, i64 742
  store float 0x4049000000000000, ptr %t1766
  %t1767 = getelementptr [1024 x float], ptr %o3, i64 0, i64 743
  store float 0xC046000000000000, ptr %t1767
  %t1768 = getelementptr [1024 x float], ptr %o3, i64 0, i64 744
  store float 0xC042800000000000, ptr %t1768
  %t1769 = getelementptr [1024 x float], ptr %o3, i64 0, i64 745
  store float 0xC03E000000000000, ptr %t1769
  %t1770 = getelementptr [1024 x float], ptr %o3, i64 0, i64 746
  store float 0xC037000000000000, ptr %t1770
  %t1771 = getelementptr [1024 x float], ptr %o3, i64 0, i64 747
  store float 0xC030000000000000, ptr %t1771
  %t1772 = getelementptr [1024 x float], ptr %o3, i64 0, i64 748
  store float 0xC022000000000000, ptr %t1772
  %t1773 = getelementptr [1024 x float], ptr %o3, i64 0, i64 749
  store float 0xC000000000000000, ptr %t1773
  %t1774 = getelementptr [1024 x float], ptr %o3, i64 0, i64 750
  store float 0x4014000000000000, ptr %t1774
  %t1775 = getelementptr [1024 x float], ptr %o3, i64 0, i64 751
  store float 0x4028000000000000, ptr %t1775
  %t1776 = getelementptr [1024 x float], ptr %o3, i64 0, i64 752
  store float 0x4033000000000000, ptr %t1776
  %t1777 = getelementptr [1024 x float], ptr %o3, i64 0, i64 753
  store float 0x403A000000000000, ptr %t1777
  %t1778 = getelementptr [1024 x float], ptr %o3, i64 0, i64 754
  store float 0x4040800000000000, ptr %t1778
  %t1779 = getelementptr [1024 x float], ptr %o3, i64 0, i64 755
  store float 0x4044000000000000, ptr %t1779
  %t1780 = getelementptr [1024 x float], ptr %o3, i64 0, i64 756
  store float 0x4047800000000000, ptr %t1780
  %t1781 = getelementptr [1024 x float], ptr %o3, i64 0, i64 757
  store float 0xC047800000000000, ptr %t1781
  %t1782 = getelementptr [1024 x float], ptr %o3, i64 0, i64 758
  store float 0xC044000000000000, ptr %t1782
  %t1783 = getelementptr [1024 x float], ptr %o3, i64 0, i64 759
  store float 0xC040800000000000, ptr %t1783
  %t1784 = getelementptr [1024 x float], ptr %o3, i64 0, i64 760
  store float 0xC03A000000000000, ptr %t1784
  %t1785 = getelementptr [1024 x float], ptr %o3, i64 0, i64 761
  store float 0xC033000000000000, ptr %t1785
  %t1786 = getelementptr [1024 x float], ptr %o3, i64 0, i64 762
  store float 0xC028000000000000, ptr %t1786
  %t1787 = getelementptr [1024 x float], ptr %o3, i64 0, i64 763
  store float 0xC014000000000000, ptr %t1787
  %t1788 = getelementptr [1024 x float], ptr %o3, i64 0, i64 764
  store float 0x4000000000000000, ptr %t1788
  %t1789 = getelementptr [1024 x float], ptr %o3, i64 0, i64 765
  store float 0x4022000000000000, ptr %t1789
  %t1790 = getelementptr [1024 x float], ptr %o3, i64 0, i64 766
  store float 0x4030000000000000, ptr %t1790
  %t1791 = getelementptr [1024 x float], ptr %o3, i64 0, i64 767
  store float 0x4037000000000000, ptr %t1791
  %t1792 = getelementptr [1024 x float], ptr %o3, i64 0, i64 768
  store float 0x403E000000000000, ptr %t1792
  %t1793 = getelementptr [1024 x float], ptr %o3, i64 0, i64 769
  store float 0x4042800000000000, ptr %t1793
  %t1794 = getelementptr [1024 x float], ptr %o3, i64 0, i64 770
  store float 0x4046000000000000, ptr %t1794
  %t1795 = getelementptr [1024 x float], ptr %o3, i64 0, i64 771
  store float 0xC049000000000000, ptr %t1795
  %t1796 = getelementptr [1024 x float], ptr %o3, i64 0, i64 772
  store float 0xC045800000000000, ptr %t1796
  %t1797 = getelementptr [1024 x float], ptr %o3, i64 0, i64 773
  store float 0xC042000000000000, ptr %t1797
  %t1798 = getelementptr [1024 x float], ptr %o3, i64 0, i64 774
  store float 0xC03D000000000000, ptr %t1798
  %t1799 = getelementptr [1024 x float], ptr %o3, i64 0, i64 775
  store float 0xC036000000000000, ptr %t1799
  %t1800 = getelementptr [1024 x float], ptr %o3, i64 0, i64 776
  store float 0xC02E000000000000, ptr %t1800
  %t1801 = getelementptr [1024 x float], ptr %o3, i64 0, i64 777
  store float 0xC020000000000000, ptr %t1801
  %t1802 = getelementptr [1024 x float], ptr %o3, i64 0, i64 778
  store float 0xBFF0000000000000, ptr %t1802
  %t1803 = getelementptr [1024 x float], ptr %o3, i64 0, i64 779
  store float 0x4018000000000000, ptr %t1803
  %t1804 = getelementptr [1024 x float], ptr %o3, i64 0, i64 780
  store float 0x402A000000000000, ptr %t1804
  %t1805 = getelementptr [1024 x float], ptr %o3, i64 0, i64 781
  store float 0x4034000000000000, ptr %t1805
  %t1806 = getelementptr [1024 x float], ptr %o3, i64 0, i64 782
  store float 0x403B000000000000, ptr %t1806
  %t1807 = getelementptr [1024 x float], ptr %o3, i64 0, i64 783
  store float 0x4041000000000000, ptr %t1807
  %t1808 = getelementptr [1024 x float], ptr %o3, i64 0, i64 784
  store float 0x4044800000000000, ptr %t1808
  %t1809 = getelementptr [1024 x float], ptr %o3, i64 0, i64 785
  store float 0x4048000000000000, ptr %t1809
  %t1810 = getelementptr [1024 x float], ptr %o3, i64 0, i64 786
  store float 0xC047000000000000, ptr %t1810
  %t1811 = getelementptr [1024 x float], ptr %o3, i64 0, i64 787
  store float 0xC043800000000000, ptr %t1811
  %t1812 = getelementptr [1024 x float], ptr %o3, i64 0, i64 788
  store float 0xC040000000000000, ptr %t1812
  %t1813 = getelementptr [1024 x float], ptr %o3, i64 0, i64 789
  store float 0xC039000000000000, ptr %t1813
  %t1814 = getelementptr [1024 x float], ptr %o3, i64 0, i64 790
  store float 0xC032000000000000, ptr %t1814
  %t1815 = getelementptr [1024 x float], ptr %o3, i64 0, i64 791
  store float 0xC026000000000000, ptr %t1815
  %t1816 = getelementptr [1024 x float], ptr %o3, i64 0, i64 792
  store float 0xC010000000000000, ptr %t1816
  %t1817 = getelementptr [1024 x float], ptr %o3, i64 0, i64 793
  store float 0x4008000000000000, ptr %t1817
  %t1818 = getelementptr [1024 x float], ptr %o3, i64 0, i64 794
  store float 0x4024000000000000, ptr %t1818
  %t1819 = getelementptr [1024 x float], ptr %o3, i64 0, i64 795
  store float 0x4031000000000000, ptr %t1819
  %t1820 = getelementptr [1024 x float], ptr %o3, i64 0, i64 796
  store float 0x4038000000000000, ptr %t1820
  %t1821 = getelementptr [1024 x float], ptr %o3, i64 0, i64 797
  store float 0x403F000000000000, ptr %t1821
  %t1822 = getelementptr [1024 x float], ptr %o3, i64 0, i64 798
  store float 0x4043000000000000, ptr %t1822
  %t1823 = getelementptr [1024 x float], ptr %o3, i64 0, i64 799
  store float 0x4046800000000000, ptr %t1823
  %t1824 = getelementptr [1024 x float], ptr %o3, i64 0, i64 800
  store float 0xC048800000000000, ptr %t1824
  %t1825 = getelementptr [1024 x float], ptr %o3, i64 0, i64 801
  store float 0xC045000000000000, ptr %t1825
  %t1826 = getelementptr [1024 x float], ptr %o3, i64 0, i64 802
  store float 0xC041800000000000, ptr %t1826
  %t1827 = getelementptr [1024 x float], ptr %o3, i64 0, i64 803
  store float 0xC03C000000000000, ptr %t1827
  %t1828 = getelementptr [1024 x float], ptr %o3, i64 0, i64 804
  store float 0xC035000000000000, ptr %t1828
  %t1829 = getelementptr [1024 x float], ptr %o3, i64 0, i64 805
  store float 0xC02C000000000000, ptr %t1829
  %t1830 = getelementptr [1024 x float], ptr %o3, i64 0, i64 806
  store float 0xC01C000000000000, ptr %t1830
  %t1831 = getelementptr [1024 x float], ptr %o3, i64 0, i64 807
  store float 0x0000000000000000, ptr %t1831
  %t1832 = getelementptr [1024 x float], ptr %o3, i64 0, i64 808
  store float 0x401C000000000000, ptr %t1832
  %t1833 = getelementptr [1024 x float], ptr %o3, i64 0, i64 809
  store float 0x402C000000000000, ptr %t1833
  %t1834 = getelementptr [1024 x float], ptr %o3, i64 0, i64 810
  store float 0x4035000000000000, ptr %t1834
  %t1835 = getelementptr [1024 x float], ptr %o3, i64 0, i64 811
  store float 0x403C000000000000, ptr %t1835
  %t1836 = getelementptr [1024 x float], ptr %o3, i64 0, i64 812
  store float 0x4041800000000000, ptr %t1836
  %t1837 = getelementptr [1024 x float], ptr %o3, i64 0, i64 813
  store float 0x4045000000000000, ptr %t1837
  %t1838 = getelementptr [1024 x float], ptr %o3, i64 0, i64 814
  store float 0x4048800000000000, ptr %t1838
  %t1839 = getelementptr [1024 x float], ptr %o3, i64 0, i64 815
  store float 0xC046800000000000, ptr %t1839
  %t1840 = getelementptr [1024 x float], ptr %o3, i64 0, i64 816
  store float 0xC043000000000000, ptr %t1840
  %t1841 = getelementptr [1024 x float], ptr %o3, i64 0, i64 817
  store float 0xC03F000000000000, ptr %t1841
  %t1842 = getelementptr [1024 x float], ptr %o3, i64 0, i64 818
  store float 0xC038000000000000, ptr %t1842
  %t1843 = getelementptr [1024 x float], ptr %o3, i64 0, i64 819
  store float 0xC031000000000000, ptr %t1843
  %t1844 = getelementptr [1024 x float], ptr %o3, i64 0, i64 820
  store float 0xC024000000000000, ptr %t1844
  %t1845 = getelementptr [1024 x float], ptr %o3, i64 0, i64 821
  store float 0xC008000000000000, ptr %t1845
  %t1846 = getelementptr [1024 x float], ptr %o3, i64 0, i64 822
  store float 0x4010000000000000, ptr %t1846
  %t1847 = getelementptr [1024 x float], ptr %o3, i64 0, i64 823
  store float 0x4026000000000000, ptr %t1847
  %t1848 = getelementptr [1024 x float], ptr %o3, i64 0, i64 824
  store float 0x4032000000000000, ptr %t1848
  %t1849 = getelementptr [1024 x float], ptr %o3, i64 0, i64 825
  store float 0x4039000000000000, ptr %t1849
  %t1850 = getelementptr [1024 x float], ptr %o3, i64 0, i64 826
  store float 0x4040000000000000, ptr %t1850
  %t1851 = getelementptr [1024 x float], ptr %o3, i64 0, i64 827
  store float 0x4043800000000000, ptr %t1851
  %t1852 = getelementptr [1024 x float], ptr %o3, i64 0, i64 828
  store float 0x4047000000000000, ptr %t1852
  %t1853 = getelementptr [1024 x float], ptr %o3, i64 0, i64 829
  store float 0xC048000000000000, ptr %t1853
  %t1854 = getelementptr [1024 x float], ptr %o3, i64 0, i64 830
  store float 0xC044800000000000, ptr %t1854
  %t1855 = getelementptr [1024 x float], ptr %o3, i64 0, i64 831
  store float 0xC041000000000000, ptr %t1855
  %t1856 = getelementptr [1024 x float], ptr %o3, i64 0, i64 832
  store float 0xC03B000000000000, ptr %t1856
  %t1857 = getelementptr [1024 x float], ptr %o3, i64 0, i64 833
  store float 0xC034000000000000, ptr %t1857
  %t1858 = getelementptr [1024 x float], ptr %o3, i64 0, i64 834
  store float 0xC02A000000000000, ptr %t1858
  %t1859 = getelementptr [1024 x float], ptr %o3, i64 0, i64 835
  store float 0xC018000000000000, ptr %t1859
  %t1860 = getelementptr [1024 x float], ptr %o3, i64 0, i64 836
  store float 0x3FF0000000000000, ptr %t1860
  %t1861 = getelementptr [1024 x float], ptr %o3, i64 0, i64 837
  store float 0x4020000000000000, ptr %t1861
  %t1862 = getelementptr [1024 x float], ptr %o3, i64 0, i64 838
  store float 0x402E000000000000, ptr %t1862
  %t1863 = getelementptr [1024 x float], ptr %o3, i64 0, i64 839
  store float 0x4036000000000000, ptr %t1863
  %t1864 = getelementptr [1024 x float], ptr %o3, i64 0, i64 840
  store float 0x403D000000000000, ptr %t1864
  %t1865 = getelementptr [1024 x float], ptr %o3, i64 0, i64 841
  store float 0x4042000000000000, ptr %t1865
  %t1866 = getelementptr [1024 x float], ptr %o3, i64 0, i64 842
  store float 0x4045800000000000, ptr %t1866
  %t1867 = getelementptr [1024 x float], ptr %o3, i64 0, i64 843
  store float 0x4049000000000000, ptr %t1867
  %t1868 = getelementptr [1024 x float], ptr %o3, i64 0, i64 844
  store float 0xC046000000000000, ptr %t1868
  %t1869 = getelementptr [1024 x float], ptr %o3, i64 0, i64 845
  store float 0xC042800000000000, ptr %t1869
  %t1870 = getelementptr [1024 x float], ptr %o3, i64 0, i64 846
  store float 0xC03E000000000000, ptr %t1870
  %t1871 = getelementptr [1024 x float], ptr %o3, i64 0, i64 847
  store float 0xC037000000000000, ptr %t1871
  %t1872 = getelementptr [1024 x float], ptr %o3, i64 0, i64 848
  store float 0xC030000000000000, ptr %t1872
  %t1873 = getelementptr [1024 x float], ptr %o3, i64 0, i64 849
  store float 0xC022000000000000, ptr %t1873
  %t1874 = getelementptr [1024 x float], ptr %o3, i64 0, i64 850
  store float 0xC000000000000000, ptr %t1874
  %t1875 = getelementptr [1024 x float], ptr %o3, i64 0, i64 851
  store float 0x4014000000000000, ptr %t1875
  %t1876 = getelementptr [1024 x float], ptr %o3, i64 0, i64 852
  store float 0x4028000000000000, ptr %t1876
  %t1877 = getelementptr [1024 x float], ptr %o3, i64 0, i64 853
  store float 0x4033000000000000, ptr %t1877
  %t1878 = getelementptr [1024 x float], ptr %o3, i64 0, i64 854
  store float 0x403A000000000000, ptr %t1878
  %t1879 = getelementptr [1024 x float], ptr %o3, i64 0, i64 855
  store float 0x4040800000000000, ptr %t1879
  %t1880 = getelementptr [1024 x float], ptr %o3, i64 0, i64 856
  store float 0x4044000000000000, ptr %t1880
  %t1881 = getelementptr [1024 x float], ptr %o3, i64 0, i64 857
  store float 0x4047800000000000, ptr %t1881
  %t1882 = getelementptr [1024 x float], ptr %o3, i64 0, i64 858
  store float 0xC047800000000000, ptr %t1882
  %t1883 = getelementptr [1024 x float], ptr %o3, i64 0, i64 859
  store float 0xC044000000000000, ptr %t1883
  %t1884 = getelementptr [1024 x float], ptr %o3, i64 0, i64 860
  store float 0xC040800000000000, ptr %t1884
  %t1885 = getelementptr [1024 x float], ptr %o3, i64 0, i64 861
  store float 0xC03A000000000000, ptr %t1885
  %t1886 = getelementptr [1024 x float], ptr %o3, i64 0, i64 862
  store float 0xC033000000000000, ptr %t1886
  %t1887 = getelementptr [1024 x float], ptr %o3, i64 0, i64 863
  store float 0xC028000000000000, ptr %t1887
  %t1888 = getelementptr [1024 x float], ptr %o3, i64 0, i64 864
  store float 0xC014000000000000, ptr %t1888
  %t1889 = getelementptr [1024 x float], ptr %o3, i64 0, i64 865
  store float 0x4000000000000000, ptr %t1889
  %t1890 = getelementptr [1024 x float], ptr %o3, i64 0, i64 866
  store float 0x4022000000000000, ptr %t1890
  %t1891 = getelementptr [1024 x float], ptr %o3, i64 0, i64 867
  store float 0x4030000000000000, ptr %t1891
  %t1892 = getelementptr [1024 x float], ptr %o3, i64 0, i64 868
  store float 0x4037000000000000, ptr %t1892
  %t1893 = getelementptr [1024 x float], ptr %o3, i64 0, i64 869
  store float 0x403E000000000000, ptr %t1893
  %t1894 = getelementptr [1024 x float], ptr %o3, i64 0, i64 870
  store float 0x4042800000000000, ptr %t1894
  %t1895 = getelementptr [1024 x float], ptr %o3, i64 0, i64 871
  store float 0x4046000000000000, ptr %t1895
  %t1896 = getelementptr [1024 x float], ptr %o3, i64 0, i64 872
  store float 0xC049000000000000, ptr %t1896
  %t1897 = getelementptr [1024 x float], ptr %o3, i64 0, i64 873
  store float 0xC045800000000000, ptr %t1897
  %t1898 = getelementptr [1024 x float], ptr %o3, i64 0, i64 874
  store float 0xC042000000000000, ptr %t1898
  %t1899 = getelementptr [1024 x float], ptr %o3, i64 0, i64 875
  store float 0xC03D000000000000, ptr %t1899
  %t1900 = getelementptr [1024 x float], ptr %o3, i64 0, i64 876
  store float 0xC036000000000000, ptr %t1900
  %t1901 = getelementptr [1024 x float], ptr %o3, i64 0, i64 877
  store float 0xC02E000000000000, ptr %t1901
  %t1902 = getelementptr [1024 x float], ptr %o3, i64 0, i64 878
  store float 0xC020000000000000, ptr %t1902
  %t1903 = getelementptr [1024 x float], ptr %o3, i64 0, i64 879
  store float 0xBFF0000000000000, ptr %t1903
  %t1904 = getelementptr [1024 x float], ptr %o3, i64 0, i64 880
  store float 0x4018000000000000, ptr %t1904
  %t1905 = getelementptr [1024 x float], ptr %o3, i64 0, i64 881
  store float 0x402A000000000000, ptr %t1905
  %t1906 = getelementptr [1024 x float], ptr %o3, i64 0, i64 882
  store float 0x4034000000000000, ptr %t1906
  %t1907 = getelementptr [1024 x float], ptr %o3, i64 0, i64 883
  store float 0x403B000000000000, ptr %t1907
  %t1908 = getelementptr [1024 x float], ptr %o3, i64 0, i64 884
  store float 0x4041000000000000, ptr %t1908
  %t1909 = getelementptr [1024 x float], ptr %o3, i64 0, i64 885
  store float 0x4044800000000000, ptr %t1909
  %t1910 = getelementptr [1024 x float], ptr %o3, i64 0, i64 886
  store float 0x4048000000000000, ptr %t1910
  %t1911 = getelementptr [1024 x float], ptr %o3, i64 0, i64 887
  store float 0xC047000000000000, ptr %t1911
  %t1912 = getelementptr [1024 x float], ptr %o3, i64 0, i64 888
  store float 0xC043800000000000, ptr %t1912
  %t1913 = getelementptr [1024 x float], ptr %o3, i64 0, i64 889
  store float 0xC040000000000000, ptr %t1913
  %t1914 = getelementptr [1024 x float], ptr %o3, i64 0, i64 890
  store float 0xC039000000000000, ptr %t1914
  %t1915 = getelementptr [1024 x float], ptr %o3, i64 0, i64 891
  store float 0xC032000000000000, ptr %t1915
  %t1916 = getelementptr [1024 x float], ptr %o3, i64 0, i64 892
  store float 0xC026000000000000, ptr %t1916
  %t1917 = getelementptr [1024 x float], ptr %o3, i64 0, i64 893
  store float 0xC010000000000000, ptr %t1917
  %t1918 = getelementptr [1024 x float], ptr %o3, i64 0, i64 894
  store float 0x4008000000000000, ptr %t1918
  %t1919 = getelementptr [1024 x float], ptr %o3, i64 0, i64 895
  store float 0x4024000000000000, ptr %t1919
  %t1920 = getelementptr [1024 x float], ptr %o3, i64 0, i64 896
  store float 0x4031000000000000, ptr %t1920
  %t1921 = getelementptr [1024 x float], ptr %o3, i64 0, i64 897
  store float 0x4038000000000000, ptr %t1921
  %t1922 = getelementptr [1024 x float], ptr %o3, i64 0, i64 898
  store float 0x403F000000000000, ptr %t1922
  %t1923 = getelementptr [1024 x float], ptr %o3, i64 0, i64 899
  store float 0x4043000000000000, ptr %t1923
  %t1924 = getelementptr [1024 x float], ptr %o3, i64 0, i64 900
  store float 0x4046800000000000, ptr %t1924
  %t1925 = getelementptr [1024 x float], ptr %o3, i64 0, i64 901
  store float 0xC048800000000000, ptr %t1925
  %t1926 = getelementptr [1024 x float], ptr %o3, i64 0, i64 902
  store float 0xC045000000000000, ptr %t1926
  %t1927 = getelementptr [1024 x float], ptr %o3, i64 0, i64 903
  store float 0xC041800000000000, ptr %t1927
  %t1928 = getelementptr [1024 x float], ptr %o3, i64 0, i64 904
  store float 0xC03C000000000000, ptr %t1928
  %t1929 = getelementptr [1024 x float], ptr %o3, i64 0, i64 905
  store float 0xC035000000000000, ptr %t1929
  %t1930 = getelementptr [1024 x float], ptr %o3, i64 0, i64 906
  store float 0xC02C000000000000, ptr %t1930
  %t1931 = getelementptr [1024 x float], ptr %o3, i64 0, i64 907
  store float 0xC01C000000000000, ptr %t1931
  %t1932 = getelementptr [1024 x float], ptr %o3, i64 0, i64 908
  store float 0x0000000000000000, ptr %t1932
  %t1933 = getelementptr [1024 x float], ptr %o3, i64 0, i64 909
  store float 0x401C000000000000, ptr %t1933
  %t1934 = getelementptr [1024 x float], ptr %o3, i64 0, i64 910
  store float 0x402C000000000000, ptr %t1934
  %t1935 = getelementptr [1024 x float], ptr %o3, i64 0, i64 911
  store float 0x4035000000000000, ptr %t1935
  %t1936 = getelementptr [1024 x float], ptr %o3, i64 0, i64 912
  store float 0x403C000000000000, ptr %t1936
  %t1937 = getelementptr [1024 x float], ptr %o3, i64 0, i64 913
  store float 0x4041800000000000, ptr %t1937
  %t1938 = getelementptr [1024 x float], ptr %o3, i64 0, i64 914
  store float 0x4045000000000000, ptr %t1938
  %t1939 = getelementptr [1024 x float], ptr %o3, i64 0, i64 915
  store float 0x4048800000000000, ptr %t1939
  %t1940 = getelementptr [1024 x float], ptr %o3, i64 0, i64 916
  store float 0xC046800000000000, ptr %t1940
  %t1941 = getelementptr [1024 x float], ptr %o3, i64 0, i64 917
  store float 0xC043000000000000, ptr %t1941
  %t1942 = getelementptr [1024 x float], ptr %o3, i64 0, i64 918
  store float 0xC03F000000000000, ptr %t1942
  %t1943 = getelementptr [1024 x float], ptr %o3, i64 0, i64 919
  store float 0xC038000000000000, ptr %t1943
  %t1944 = getelementptr [1024 x float], ptr %o3, i64 0, i64 920
  store float 0xC031000000000000, ptr %t1944
  %t1945 = getelementptr [1024 x float], ptr %o3, i64 0, i64 921
  store float 0xC024000000000000, ptr %t1945
  %t1946 = getelementptr [1024 x float], ptr %o3, i64 0, i64 922
  store float 0xC008000000000000, ptr %t1946
  %t1947 = getelementptr [1024 x float], ptr %o3, i64 0, i64 923
  store float 0x4010000000000000, ptr %t1947
  %t1948 = getelementptr [1024 x float], ptr %o3, i64 0, i64 924
  store float 0x4026000000000000, ptr %t1948
  %t1949 = getelementptr [1024 x float], ptr %o3, i64 0, i64 925
  store float 0x4032000000000000, ptr %t1949
  %t1950 = getelementptr [1024 x float], ptr %o3, i64 0, i64 926
  store float 0x4039000000000000, ptr %t1950
  %t1951 = getelementptr [1024 x float], ptr %o3, i64 0, i64 927
  store float 0x4040000000000000, ptr %t1951
  %t1952 = getelementptr [1024 x float], ptr %o3, i64 0, i64 928
  store float 0x4043800000000000, ptr %t1952
  %t1953 = getelementptr [1024 x float], ptr %o3, i64 0, i64 929
  store float 0x4047000000000000, ptr %t1953
  %t1954 = getelementptr [1024 x float], ptr %o3, i64 0, i64 930
  store float 0xC048000000000000, ptr %t1954
  %t1955 = getelementptr [1024 x float], ptr %o3, i64 0, i64 931
  store float 0xC044800000000000, ptr %t1955
  %t1956 = getelementptr [1024 x float], ptr %o3, i64 0, i64 932
  store float 0xC041000000000000, ptr %t1956
  %t1957 = getelementptr [1024 x float], ptr %o3, i64 0, i64 933
  store float 0xC03B000000000000, ptr %t1957
  %t1958 = getelementptr [1024 x float], ptr %o3, i64 0, i64 934
  store float 0xC034000000000000, ptr %t1958
  %t1959 = getelementptr [1024 x float], ptr %o3, i64 0, i64 935
  store float 0xC02A000000000000, ptr %t1959
  %t1960 = getelementptr [1024 x float], ptr %o3, i64 0, i64 936
  store float 0xC018000000000000, ptr %t1960
  %t1961 = getelementptr [1024 x float], ptr %o3, i64 0, i64 937
  store float 0x3FF0000000000000, ptr %t1961
  %t1962 = getelementptr [1024 x float], ptr %o3, i64 0, i64 938
  store float 0x4020000000000000, ptr %t1962
  %t1963 = getelementptr [1024 x float], ptr %o3, i64 0, i64 939
  store float 0x402E000000000000, ptr %t1963
  %t1964 = getelementptr [1024 x float], ptr %o3, i64 0, i64 940
  store float 0x4036000000000000, ptr %t1964
  %t1965 = getelementptr [1024 x float], ptr %o3, i64 0, i64 941
  store float 0x403D000000000000, ptr %t1965
  %t1966 = getelementptr [1024 x float], ptr %o3, i64 0, i64 942
  store float 0x4042000000000000, ptr %t1966
  %t1967 = getelementptr [1024 x float], ptr %o3, i64 0, i64 943
  store float 0x4045800000000000, ptr %t1967
  %t1968 = getelementptr [1024 x float], ptr %o3, i64 0, i64 944
  store float 0x4049000000000000, ptr %t1968
  %t1969 = getelementptr [1024 x float], ptr %o3, i64 0, i64 945
  store float 0xC046000000000000, ptr %t1969
  %t1970 = getelementptr [1024 x float], ptr %o3, i64 0, i64 946
  store float 0xC042800000000000, ptr %t1970
  %t1971 = getelementptr [1024 x float], ptr %o3, i64 0, i64 947
  store float 0xC03E000000000000, ptr %t1971
  %t1972 = getelementptr [1024 x float], ptr %o3, i64 0, i64 948
  store float 0xC037000000000000, ptr %t1972
  %t1973 = getelementptr [1024 x float], ptr %o3, i64 0, i64 949
  store float 0xC030000000000000, ptr %t1973
  %t1974 = getelementptr [1024 x float], ptr %o3, i64 0, i64 950
  store float 0xC022000000000000, ptr %t1974
  %t1975 = getelementptr [1024 x float], ptr %o3, i64 0, i64 951
  store float 0xC000000000000000, ptr %t1975
  %t1976 = getelementptr [1024 x float], ptr %o3, i64 0, i64 952
  store float 0x4014000000000000, ptr %t1976
  %t1977 = getelementptr [1024 x float], ptr %o3, i64 0, i64 953
  store float 0x4028000000000000, ptr %t1977
  %t1978 = getelementptr [1024 x float], ptr %o3, i64 0, i64 954
  store float 0x4033000000000000, ptr %t1978
  %t1979 = getelementptr [1024 x float], ptr %o3, i64 0, i64 955
  store float 0x403A000000000000, ptr %t1979
  %t1980 = getelementptr [1024 x float], ptr %o3, i64 0, i64 956
  store float 0x4040800000000000, ptr %t1980
  %t1981 = getelementptr [1024 x float], ptr %o3, i64 0, i64 957
  store float 0x4044000000000000, ptr %t1981
  %t1982 = getelementptr [1024 x float], ptr %o3, i64 0, i64 958
  store float 0x4047800000000000, ptr %t1982
  %t1983 = getelementptr [1024 x float], ptr %o3, i64 0, i64 959
  store float 0xC047800000000000, ptr %t1983
  %t1984 = getelementptr [1024 x float], ptr %o3, i64 0, i64 960
  store float 0xC044000000000000, ptr %t1984
  %t1985 = getelementptr [1024 x float], ptr %o3, i64 0, i64 961
  store float 0xC040800000000000, ptr %t1985
  %t1986 = getelementptr [1024 x float], ptr %o3, i64 0, i64 962
  store float 0xC03A000000000000, ptr %t1986
  %t1987 = getelementptr [1024 x float], ptr %o3, i64 0, i64 963
  store float 0xC033000000000000, ptr %t1987
  %t1988 = getelementptr [1024 x float], ptr %o3, i64 0, i64 964
  store float 0xC028000000000000, ptr %t1988
  %t1989 = getelementptr [1024 x float], ptr %o3, i64 0, i64 965
  store float 0xC014000000000000, ptr %t1989
  %t1990 = getelementptr [1024 x float], ptr %o3, i64 0, i64 966
  store float 0x4000000000000000, ptr %t1990
  %t1991 = getelementptr [1024 x float], ptr %o3, i64 0, i64 967
  store float 0x4022000000000000, ptr %t1991
  %t1992 = getelementptr [1024 x float], ptr %o3, i64 0, i64 968
  store float 0x4030000000000000, ptr %t1992
  %t1993 = getelementptr [1024 x float], ptr %o3, i64 0, i64 969
  store float 0x4037000000000000, ptr %t1993
  %t1994 = getelementptr [1024 x float], ptr %o3, i64 0, i64 970
  store float 0x403E000000000000, ptr %t1994
  %t1995 = getelementptr [1024 x float], ptr %o3, i64 0, i64 971
  store float 0x4042800000000000, ptr %t1995
  %t1996 = getelementptr [1024 x float], ptr %o3, i64 0, i64 972
  store float 0x4046000000000000, ptr %t1996
  %t1997 = getelementptr [1024 x float], ptr %o3, i64 0, i64 973
  store float 0xC049000000000000, ptr %t1997
  %t1998 = getelementptr [1024 x float], ptr %o3, i64 0, i64 974
  store float 0xC045800000000000, ptr %t1998
  %t1999 = getelementptr [1024 x float], ptr %o3, i64 0, i64 975
  store float 0xC042000000000000, ptr %t1999
  %t2000 = getelementptr [1024 x float], ptr %o3, i64 0, i64 976
  store float 0xC03D000000000000, ptr %t2000
  %t2001 = getelementptr [1024 x float], ptr %o3, i64 0, i64 977
  store float 0xC036000000000000, ptr %t2001
  %t2002 = getelementptr [1024 x float], ptr %o3, i64 0, i64 978
  store float 0xC02E000000000000, ptr %t2002
  %t2003 = getelementptr [1024 x float], ptr %o3, i64 0, i64 979
  store float 0xC020000000000000, ptr %t2003
  %t2004 = getelementptr [1024 x float], ptr %o3, i64 0, i64 980
  store float 0xBFF0000000000000, ptr %t2004
  %t2005 = getelementptr [1024 x float], ptr %o3, i64 0, i64 981
  store float 0x4018000000000000, ptr %t2005
  %t2006 = getelementptr [1024 x float], ptr %o3, i64 0, i64 982
  store float 0x402A000000000000, ptr %t2006
  %t2007 = getelementptr [1024 x float], ptr %o3, i64 0, i64 983
  store float 0x4034000000000000, ptr %t2007
  %t2008 = getelementptr [1024 x float], ptr %o3, i64 0, i64 984
  store float 0x403B000000000000, ptr %t2008
  %t2009 = getelementptr [1024 x float], ptr %o3, i64 0, i64 985
  store float 0x4041000000000000, ptr %t2009
  %t2010 = getelementptr [1024 x float], ptr %o3, i64 0, i64 986
  store float 0x4044800000000000, ptr %t2010
  %t2011 = getelementptr [1024 x float], ptr %o3, i64 0, i64 987
  store float 0x4048000000000000, ptr %t2011
  %t2012 = getelementptr [1024 x float], ptr %o3, i64 0, i64 988
  store float 0xC047000000000000, ptr %t2012
  %t2013 = getelementptr [1024 x float], ptr %o3, i64 0, i64 989
  store float 0xC043800000000000, ptr %t2013
  %t2014 = getelementptr [1024 x float], ptr %o3, i64 0, i64 990
  store float 0xC040000000000000, ptr %t2014
  %t2015 = getelementptr [1024 x float], ptr %o3, i64 0, i64 991
  store float 0xC039000000000000, ptr %t2015
  %t2016 = getelementptr [1024 x float], ptr %o3, i64 0, i64 992
  store float 0xC032000000000000, ptr %t2016
  %t2017 = getelementptr [1024 x float], ptr %o3, i64 0, i64 993
  store float 0xC026000000000000, ptr %t2017
  %t2018 = getelementptr [1024 x float], ptr %o3, i64 0, i64 994
  store float 0xC010000000000000, ptr %t2018
  %t2019 = getelementptr [1024 x float], ptr %o3, i64 0, i64 995
  store float 0x4008000000000000, ptr %t2019
  %t2020 = getelementptr [1024 x float], ptr %o3, i64 0, i64 996
  store float 0x4024000000000000, ptr %t2020
  %t2021 = getelementptr [1024 x float], ptr %o3, i64 0, i64 997
  store float 0x4031000000000000, ptr %t2021
  %t2022 = getelementptr [1024 x float], ptr %o3, i64 0, i64 998
  store float 0x4038000000000000, ptr %t2022
  %t2023 = getelementptr [1024 x float], ptr %o3, i64 0, i64 999
  store float 0x403F000000000000, ptr %t2023
  %t2024 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1000
  store float 0x4043000000000000, ptr %t2024
  %t2025 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1001
  store float 0x4046800000000000, ptr %t2025
  %t2026 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1002
  store float 0xC048800000000000, ptr %t2026
  %t2027 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1003
  store float 0xC045000000000000, ptr %t2027
  %t2028 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1004
  store float 0xC041800000000000, ptr %t2028
  %t2029 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1005
  store float 0xC03C000000000000, ptr %t2029
  %t2030 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1006
  store float 0xC035000000000000, ptr %t2030
  %t2031 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1007
  store float 0xC02C000000000000, ptr %t2031
  %t2032 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1008
  store float 0xC01C000000000000, ptr %t2032
  %t2033 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1009
  store float 0x0000000000000000, ptr %t2033
  %t2034 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1010
  store float 0x401C000000000000, ptr %t2034
  %t2035 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1011
  store float 0x402C000000000000, ptr %t2035
  %t2036 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1012
  store float 0x4035000000000000, ptr %t2036
  %t2037 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1013
  store float 0x403C000000000000, ptr %t2037
  %t2038 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1014
  store float 0x4041800000000000, ptr %t2038
  %t2039 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1015
  store float 0x4045000000000000, ptr %t2039
  %t2040 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1016
  store float 0x4048800000000000, ptr %t2040
  %t2041 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1017
  store float 0xC046800000000000, ptr %t2041
  %t2042 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1018
  store float 0xC043000000000000, ptr %t2042
  %t2043 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1019
  store float 0xC03F000000000000, ptr %t2043
  %t2044 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1020
  store float 0xC038000000000000, ptr %t2044
  %t2045 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1021
  store float 0xC031000000000000, ptr %t2045
  %t2046 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1022
  store float 0xC024000000000000, ptr %t2046
  %t2047 = getelementptr [1024 x float], ptr %o3, i64 0, i64 1023
  store float 0xC008000000000000, ptr %t2047
  %t2048 = getelementptr { ptr, i32 }, ptr %o6, i32 0, i32 1
  store i32 0, ptr %t2048
  %t2049 = getelementptr { ptr, i32 }, ptr %o10, i32 0, i32 1
  store i32 1023, ptr %t2049
  %t2050 = getelementptr { ptr, ptr }, ptr %o4, i32 0, i32 0
  store ptr %o2, ptr %t2050
  %t2051 = getelementptr { ptr, ptr }, ptr %o4, i32 0, i32 1
  store ptr %o3, ptr %t2051
  %t2053 = getelementptr { ptr, ptr }, ptr %s2052, i32 0, i32 0
  store ptr %o2, ptr %t2053
  %t2054 = getelementptr { ptr, ptr }, ptr %s2052, i32 0, i32 1
  store ptr %o3, ptr %t2054
  %t2055 = load { ptr, ptr }, ptr %s2052
  %t2056 = call [1024 x float] @fn1({ ptr, ptr } %t2055)
  store [1024 x float] %t2056, ptr %o5
  %t2057 = getelementptr { ptr, i32 }, ptr %o6, i32 0, i32 0
  store ptr %o5, ptr %t2057
  %t2058 = getelementptr { ptr, i32 }, ptr %o10, i32 0, i32 0
  store ptr %o5, ptr %t2058
  %t2059 = getelementptr { ptr, i32 }, ptr %o6, i32 0, i32 1
  %t2060 = load i32, ptr %t2059
  %t2061 = sext i32 %t2060 to i64
  %t2062 = getelementptr [1024 x float], ptr %o5, i64 0, i64 %t2061
  %t2063 = load float, ptr %t2062
  store float %t2063, ptr %o7
  %t2064 = getelementptr { ptr, i32 }, ptr %o10, i32 0, i32 1
  %t2065 = load i32, ptr %t2064
  %t2066 = sext i32 %t2065 to i64
  %t2067 = getelementptr [1024 x float], ptr %o5, i64 0, i64 %t2066
  %t2068 = load float, ptr %t2067
  store float %t2068, ptr %o11
  %t2069 = load float, ptr %o7
  store float %t2069, ptr %o8
  %t2070 = load float, ptr %o11
  store float %t2070, ptr %o12
  %t2071 = load float, ptr %o8
  call void @flow_print_f32(float %t2071, i1 zeroext true)
  %t2072 = load float, ptr %o12
  call void @flow_print_f32(float %t2072, i1 zeroext true)
  ret void
}

define i32 @main() {
entry:
  call void @flow_main()
  ret i32 0
}

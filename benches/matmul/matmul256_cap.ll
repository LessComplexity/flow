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

define internal void @flow_main() {
entry:
  %o2 = alloca [65536 x i32]
  %o3 = alloca [65536 x double]
  %o4 = alloca [65536 x double]
  %o5 = alloca [256 x i32]
  %o6 = alloca { ptr, ptr, ptr, ptr }
  %o7 = alloca [65536 x double]
  %o8 = alloca { ptr, i32 }
  %o9 = alloca double
  %o10 = alloca double
  %o12 = alloca { ptr, i32 }
  %o13 = alloca double
  %o14 = alloca double
  %s0 = alloca i64
  %s9 = alloca i64
  %s20 = alloca i64
  %s31 = alloca i64
  %s46 = alloca i64
  %s54 = alloca { ptr, ptr, ptr, i32 }
  store i64 0, ptr %s0
  br label %bb1
bb1:
  %t4 = load i64, ptr %s0
  %t5 = icmp uge i64 %t4, 65536
  br i1 %t5, label %bb3, label %bb2
bb2:
  %t6 = trunc i64 %t4 to i32
  %t7 = getelementptr [65536 x i32], ptr %o2, i64 0, i64 %t4
  store i32 %t6, ptr %t7
  %t8 = add i64 %t4, 1
  store i64 %t8, ptr %s0
  br label %bb1
bb3:
  store i64 0, ptr %s9
  br label %bb10
bb10:
  %t13 = load i64, ptr %s9
  %t14 = icmp uge i64 %t13, 256
  br i1 %t14, label %bb12, label %bb11
bb11:
  %t15 = trunc i64 %t13 to i32
  %t16 = getelementptr [256 x i32], ptr %o5, i64 0, i64 %t13
  store i32 %t15, ptr %t16
  %t17 = add i64 %t13, 1
  store i64 %t17, ptr %s9
  br label %bb10
bb12:
  %t18 = getelementptr { ptr, i32 }, ptr %o8, i32 0, i32 1
  store i32 0, ptr %t18
  %t19 = getelementptr { ptr, i32 }, ptr %o12, i32 0, i32 1
  store i32 65535, ptr %t19
  store i64 0, ptr %s20
  br label %bb21
bb21:
  %t24 = load i64, ptr %s20
  %t25 = icmp uge i64 %t24, 65536
  br i1 %t25, label %bb23, label %bb22
bb22:
  %t26 = getelementptr [65536 x i32], ptr %o2, i64 0, i64 %t24
  %t27 = load i32, ptr %t26
  %t28 = call double @fn1(i32 %t27)
  %t29 = getelementptr [65536 x double], ptr %o3, i64 0, i64 %t24
  store double %t28, ptr %t29
  %t30 = add i64 %t24, 1
  store i64 %t30, ptr %s20
  br label %bb21
bb23:
  store i64 0, ptr %s31
  br label %bb32
bb32:
  %t35 = load i64, ptr %s31
  %t36 = icmp uge i64 %t35, 65536
  br i1 %t36, label %bb34, label %bb33
bb33:
  %t37 = getelementptr [65536 x i32], ptr %o2, i64 0, i64 %t35
  %t38 = load i32, ptr %t37
  %t39 = call double @fn2(i32 %t38)
  %t40 = getelementptr [65536 x double], ptr %o4, i64 0, i64 %t35
  store double %t39, ptr %t40
  %t41 = add i64 %t35, 1
  store i64 %t41, ptr %s31
  br label %bb32
bb34:
  %t42 = getelementptr { ptr, ptr, ptr, ptr }, ptr %o6, i32 0, i32 3
  store ptr %o2, ptr %t42
  %t43 = getelementptr { ptr, ptr, ptr, ptr }, ptr %o6, i32 0, i32 0
  store ptr %o5, ptr %t43
  %t44 = getelementptr { ptr, ptr, ptr, ptr }, ptr %o6, i32 0, i32 1
  store ptr %o3, ptr %t44
  %t45 = getelementptr { ptr, ptr, ptr, ptr }, ptr %o6, i32 0, i32 2
  store ptr %o4, ptr %t45
  store i64 0, ptr %s46
  br label %bb47
bb47:
  %t50 = load i64, ptr %s46
  %t51 = icmp uge i64 %t50, 65536
  br i1 %t51, label %bb49, label %bb48
bb48:
  %t52 = getelementptr [65536 x i32], ptr %o2, i64 0, i64 %t50
  %t53 = load i32, ptr %t52
  %t55 = getelementptr { ptr, ptr, ptr, i32 }, ptr %s54, i32 0, i32 0
  store ptr %o5, ptr %t55
  %t56 = getelementptr { ptr, ptr, ptr, i32 }, ptr %s54, i32 0, i32 1
  store ptr %o3, ptr %t56
  %t57 = getelementptr { ptr, ptr, ptr, i32 }, ptr %s54, i32 0, i32 2
  store ptr %o4, ptr %t57
  %t58 = getelementptr { ptr, ptr, ptr, i32 }, ptr %s54, i32 0, i32 3
  store i32 %t53, ptr %t58
  %t59 = load { ptr, ptr, ptr, i32 }, ptr %s54
  %t60 = call double @fn4({ ptr, ptr, ptr, i32 } %t59)
  %t61 = getelementptr [65536 x double], ptr %o7, i64 0, i64 %t50
  store double %t60, ptr %t61
  %t62 = add i64 %t50, 1
  store i64 %t62, ptr %s46
  br label %bb47
bb49:
  %t63 = getelementptr { ptr, i32 }, ptr %o8, i32 0, i32 0
  store ptr %o7, ptr %t63
  %t64 = getelementptr { ptr, i32 }, ptr %o12, i32 0, i32 0
  store ptr %o7, ptr %t64
  %t65 = getelementptr { ptr, i32 }, ptr %o8, i32 0, i32 1
  %t66 = load i32, ptr %t65
  %t67 = sext i32 %t66 to i64
  %t68 = getelementptr [65536 x double], ptr %o7, i64 0, i64 %t67
  %t69 = load double, ptr %t68
  store double %t69, ptr %o9
  %t70 = getelementptr { ptr, i32 }, ptr %o12, i32 0, i32 1
  %t71 = load i32, ptr %t70
  %t72 = sext i32 %t71 to i64
  %t73 = getelementptr [65536 x double], ptr %o7, i64 0, i64 %t72
  %t74 = load double, ptr %t73
  store double %t74, ptr %o13
  %t75 = load double, ptr %o9
  store double %t75, ptr %o10
  %t76 = load double, ptr %o13
  store double %t76, ptr %o14
  %t77 = load double, ptr %o10
  call void @flow_print_f64(double %t77, i1 zeroext true)
  %t78 = load double, ptr %o14
  call void @flow_print_f64(double %t78, i1 zeroext true)
  ret void
}

define internal double @fn1(i32 %arg) readonly nounwind willreturn {
entry:
  %o0 = alloca i32
  %o1 = alloca double
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
  %t33 = sitofp i32 %t32 to double
  store double %t33, ptr %o1
  %t34 = load double, ptr %o1
  ret double %t34
}

define internal double @fn2(i32 %arg) readonly nounwind willreturn {
entry:
  %o0 = alloca i32
  %o1 = alloca double
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
  %t33 = sitofp i32 %t32 to double
  store double %t33, ptr %o1
  %t34 = load double, ptr %o1
  ret double %t34
}

define internal double @fn3({ ptr, i32, ptr, i32, double, i32 } %arg) readonly nounwind willreturn {
entry:
  %o0 = alloca { ptr, i32, ptr, i32, double, i32 }
  %o1 = alloca double
  %o2 = alloca ptr
  %o3 = alloca i32
  %o4 = alloca ptr
  %o5 = alloca i32
  %o6 = alloca double
  %o7 = alloca i32
  %o8 = alloca { i32, i32 }
  %o9 = alloca i32
  %o10 = alloca { i32, i32 }
  %o11 = alloca i32
  %o12 = alloca { ptr, i32 }
  %o13 = alloca double
  %o14 = alloca { i32, i32 }
  %o15 = alloca i32
  %o16 = alloca { i32, i32 }
  %o17 = alloca i32
  %o18 = alloca { ptr, i32 }
  %o19 = alloca double
  %o20 = alloca { double, double }
  %o21 = alloca double
  %o22 = alloca { double, double }
  store { ptr, i32, ptr, i32, double, i32 } %arg, ptr %o0
  %t0 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %o0, i32 0, i32 0
  %t1 = load ptr, ptr %t0
  store ptr %t1, ptr %o2
  %t2 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %o0, i32 0, i32 1
  %t3 = load i32, ptr %t2
  store i32 %t3, ptr %o3
  %t4 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %o0, i32 0, i32 2
  %t5 = load ptr, ptr %t4
  store ptr %t5, ptr %o4
  %t6 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %o0, i32 0, i32 3
  %t7 = load i32, ptr %t6
  store i32 %t7, ptr %o5
  %t8 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %o0, i32 0, i32 4
  %t9 = load double, ptr %t8
  store double %t9, ptr %o6
  %t10 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %o0, i32 0, i32 5
  %t11 = load i32, ptr %t10
  store i32 %t11, ptr %o7
  %t12 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  store i32 256, ptr %t12
  %t13 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 1
  store i32 256, ptr %t13
  %t14 = load ptr, ptr %o2
  %t15 = getelementptr { ptr, i32 }, ptr %o12, i32 0, i32 0
  store ptr %t14, ptr %t15
  %t16 = load i32, ptr %o3
  %t17 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  store i32 %t16, ptr %t17
  %t18 = load ptr, ptr %o4
  %t19 = getelementptr { ptr, i32 }, ptr %o18, i32 0, i32 0
  store ptr %t18, ptr %t19
  %t20 = load i32, ptr %o5
  %t21 = getelementptr { i32, i32 }, ptr %o16, i32 0, i32 1
  store i32 %t20, ptr %t21
  %t22 = load double, ptr %o6
  %t23 = getelementptr { double, double }, ptr %o22, i32 0, i32 0
  store double %t22, ptr %t23
  %t24 = load i32, ptr %o7
  %t25 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  store i32 %t24, ptr %t25
  %t26 = load i32, ptr %o7
  %t27 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 0
  store i32 %t26, ptr %t27
  %t28 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  %t29 = load i32, ptr %t28
  %t30 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  %t31 = load i32, ptr %t30
  %t32 = mul i32 %t29, %t31
  store i32 %t32, ptr %o9
  %t33 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 0
  %t34 = load i32, ptr %t33
  %t35 = getelementptr { i32, i32 }, ptr %o14, i32 0, i32 1
  %t36 = load i32, ptr %t35
  %t37 = mul i32 %t34, %t36
  store i32 %t37, ptr %o15
  %t38 = load i32, ptr %o9
  %t39 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 0
  store i32 %t38, ptr %t39
  %t40 = load i32, ptr %o15
  %t41 = getelementptr { i32, i32 }, ptr %o16, i32 0, i32 0
  store i32 %t40, ptr %t41
  %t42 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 0
  %t43 = load i32, ptr %t42
  %t44 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  %t45 = load i32, ptr %t44
  %t46 = add i32 %t43, %t45
  store i32 %t46, ptr %o11
  %t47 = getelementptr { i32, i32 }, ptr %o16, i32 0, i32 0
  %t48 = load i32, ptr %t47
  %t49 = getelementptr { i32, i32 }, ptr %o16, i32 0, i32 1
  %t50 = load i32, ptr %t49
  %t51 = add i32 %t48, %t50
  store i32 %t51, ptr %o17
  %t52 = load i32, ptr %o11
  %t53 = getelementptr { ptr, i32 }, ptr %o12, i32 0, i32 1
  store i32 %t52, ptr %t53
  %t54 = load i32, ptr %o17
  %t55 = getelementptr { ptr, i32 }, ptr %o18, i32 0, i32 1
  store i32 %t54, ptr %t55
  %t56 = load ptr, ptr %o2
  %t57 = getelementptr { ptr, i32 }, ptr %o12, i32 0, i32 1
  %t58 = load i32, ptr %t57
  %t59 = sext i32 %t58 to i64
  %t60 = getelementptr [65536 x double], ptr %t56, i64 0, i64 %t59
  %t61 = load double, ptr %t60
  store double %t61, ptr %o13
  %t62 = load ptr, ptr %o4
  %t63 = getelementptr { ptr, i32 }, ptr %o18, i32 0, i32 1
  %t64 = load i32, ptr %t63
  %t65 = sext i32 %t64 to i64
  %t66 = getelementptr [65536 x double], ptr %t62, i64 0, i64 %t65
  %t67 = load double, ptr %t66
  store double %t67, ptr %o19
  %t68 = load double, ptr %o13
  %t69 = getelementptr { double, double }, ptr %o20, i32 0, i32 0
  store double %t68, ptr %t69
  %t70 = load double, ptr %o19
  %t71 = getelementptr { double, double }, ptr %o20, i32 0, i32 1
  store double %t70, ptr %t71
  %t72 = getelementptr { double, double }, ptr %o20, i32 0, i32 0
  %t73 = load double, ptr %t72
  %t74 = getelementptr { double, double }, ptr %o20, i32 0, i32 1
  %t75 = load double, ptr %t74
  %t76 = fmul double %t73, %t75
  store double %t76, ptr %o21
  %t77 = load double, ptr %o21
  %t78 = getelementptr { double, double }, ptr %o22, i32 0, i32 1
  store double %t77, ptr %t78
  %t79 = getelementptr { double, double }, ptr %o22, i32 0, i32 0
  %t80 = load double, ptr %t79
  %t81 = getelementptr { double, double }, ptr %o22, i32 0, i32 1
  %t82 = load double, ptr %t81
  %t83 = fadd double %t80, %t82
  store double %t83, ptr %o1
  %t84 = load double, ptr %o1
  ret double %t84
}

define internal double @fn4({ ptr, ptr, ptr, i32 } %arg) readonly nounwind willreturn {
entry:
  %o0 = alloca { ptr, ptr, ptr, i32 }
  %o1 = alloca double
  %o2 = alloca ptr
  %o3 = alloca ptr
  %o4 = alloca ptr
  %o5 = alloca i32
  %o6 = alloca { i32, i32 }
  %o7 = alloca i32
  %o8 = alloca { i32, i32 }
  %o9 = alloca i32
  %o10 = alloca { double, [256 x i32] }
  %o11 = alloca double
  %o12 = alloca [256 x i32]
  %o13 = alloca { ptr, i32, ptr, i32, double, ptr }
  %s43 = alloca double
  %s44 = alloca i64
  %s53 = alloca { ptr, i32, ptr, i32, double, i32 }
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
  store i32 256, ptr %t8
  %t9 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  store i32 256, ptr %t9
  %t10 = getelementptr { double, [256 x i32] }, ptr %o10, i32 0, i32 0
  store double 0x0000000000000000, ptr %t10
  %t11 = load ptr, ptr %o2
  %t12 = getelementptr { double, [256 x i32] }, ptr %o10, i32 0, i32 1
  call void @llvm.memcpy.p0.p0.i64(ptr %t12, ptr %t11, i64 ptrtoint (ptr getelementptr ([256 x i32], ptr null, i64 1) to i64), i1 false)
  %t13 = load ptr, ptr %o3
  %t14 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o13, i32 0, i32 0
  store ptr %t13, ptr %t14
  %t15 = load ptr, ptr %o4
  %t16 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o13, i32 0, i32 2
  store ptr %t15, ptr %t16
  %t17 = load i32, ptr %o5
  %t18 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 0
  store i32 %t17, ptr %t18
  %t19 = load i32, ptr %o5
  %t20 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  store i32 %t19, ptr %t20
  %t21 = getelementptr { double, [256 x i32] }, ptr %o10, i32 0, i32 0
  %t22 = load double, ptr %t21
  store double %t22, ptr %o11
  %t23 = load ptr, ptr %o2
  call void @llvm.memcpy.p0.p0.i64(ptr %o12, ptr %t23, i64 ptrtoint (ptr getelementptr ([256 x i32], ptr null, i64 1) to i64), i1 false)
  %t24 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 0
  %t25 = load i32, ptr %t24
  %t26 = getelementptr { i32, i32 }, ptr %o6, i32 0, i32 1
  %t27 = load i32, ptr %t26
  %t28 = sdiv i32 %t25, %t27
  store i32 %t28, ptr %o7
  %t29 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 0
  %t30 = load i32, ptr %t29
  %t31 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  %t32 = load i32, ptr %t31
  %t33 = srem i32 %t30, %t32
  store i32 %t33, ptr %o9
  %t34 = load double, ptr %o11
  %t35 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o13, i32 0, i32 4
  store double %t34, ptr %t35
  %t36 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o13, i32 0, i32 5
  store ptr %o12, ptr %t36
  %t37 = load i32, ptr %o7
  %t38 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o13, i32 0, i32 1
  store i32 %t37, ptr %t38
  %t39 = load i32, ptr %o9
  %t40 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o13, i32 0, i32 3
  store i32 %t39, ptr %t40
  %t41 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o13, i32 0, i32 4
  %t42 = load double, ptr %t41
  store double %t42, ptr %s43
  store i64 0, ptr %s44
  br label %bb45
bb45:
  %t48 = load i64, ptr %s44
  %t49 = icmp uge i64 %t48, 256
  br i1 %t49, label %bb47, label %bb46
bb46:
  %t50 = getelementptr [256 x i32], ptr %o12, i64 0, i64 %t48
  %t51 = load i32, ptr %t50
  %t52 = load double, ptr %s43
  %t54 = load ptr, ptr %o3
  %t55 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %s53, i32 0, i32 0
  store ptr %t54, ptr %t55
  %t56 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o13, i32 0, i32 1
  %t57 = load i32, ptr %t56
  %t58 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %s53, i32 0, i32 1
  store i32 %t57, ptr %t58
  %t59 = load ptr, ptr %o4
  %t60 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %s53, i32 0, i32 2
  store ptr %t59, ptr %t60
  %t61 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o13, i32 0, i32 3
  %t62 = load i32, ptr %t61
  %t63 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %s53, i32 0, i32 3
  store i32 %t62, ptr %t63
  %t64 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %s53, i32 0, i32 4
  store double %t52, ptr %t64
  %t65 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %s53, i32 0, i32 5
  store i32 %t51, ptr %t65
  %t66 = load { ptr, i32, ptr, i32, double, i32 }, ptr %s53
  %t67 = call double @fn3({ ptr, i32, ptr, i32, double, i32 } %t66)
  store double %t67, ptr %s43
  %t68 = add i64 %t48, 1
  store i64 %t68, ptr %s44
  br label %bb45
bb47:
  %t69 = load double, ptr %s43
  store double %t69, ptr %o1
  %t70 = load double, ptr %o1
  ret double %t70
}

define i32 @main() {
entry:
  call void @flow_main()
  ret i32 0
}

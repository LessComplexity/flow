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

%Frame = type { [1048576 x i32], [1024 x i32], [1048576 x float], [1048576 x float], { ptr, ptr, ptr, ptr }, [1048576 x float], { ptr, i32 }, float, { ptr, i32 }, float, float, float }

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
  %t7 = getelementptr [1048576 x i32], ptr %o2, i64 0, i64 %t4
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
  %t7 = getelementptr [1024 x i32], ptr %o3, i64 0, i64 %t4
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
  store i32 1048575, ptr %t1
  %t2 = getelementptr { ptr, i32 }, ptr %o8, i32 0, i32 0
  store ptr %o7, ptr %t2
  %t3 = getelementptr { ptr, i32 }, ptr %o10, i32 0, i32 0
  store ptr %o7, ptr %t3
  %t4 = getelementptr { ptr, i32 }, ptr %o8, i32 0, i32 1
  %t5 = load i32, ptr %t4
  %t6 = sext i32 %t5 to i64
  %t7 = getelementptr [1048576 x float], ptr %o7, i64 0, i64 %t6
  %t8 = load float, ptr %t7
  store float %t8, ptr %o9
  %t9 = getelementptr { ptr, i32 }, ptr %o10, i32 0, i32 1
  %t10 = load i32, ptr %t9
  %t11 = sext i32 %t10 to i64
  %t12 = getelementptr [1048576 x float], ptr %o7, i64 0, i64 %t11
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
  %t6 = getelementptr [1048576 x i32], ptr %o2, i64 0, i64 %t4
  %t7 = load i32, ptr %t6
  %t8 = call float @fn1(i32 %t7)
  %t9 = getelementptr [1048576 x float], ptr %o4, i64 0, i64 %t4
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
  %t6 = getelementptr [1048576 x i32], ptr %o2, i64 0, i64 %t4
  %t7 = load i32, ptr %t6
  %t8 = call float @fn2(i32 %t7)
  %t9 = getelementptr [1048576 x float], ptr %o5, i64 0, i64 %t4
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
  %s0 = alloca [16 x float]
  %s1 = alloca i64
  %s2 = alloca i64
  %s3 = alloca i64
  %s4 = alloca i64
  %o4 = getelementptr %Frame, ptr %frame, i32 0, i32 2
  %o5 = getelementptr %Frame, ptr %frame, i32 0, i32 3
  %o7 = getelementptr %Frame, ptr %frame, i32 0, i32 5
  %t5 = udiv i64 %lo, 1024
  %t6 = add i64 %hi, 1023
  %t7 = udiv i64 %t6, 1024
  store i64 %t5, ptr %s1
  br label %bb8
bb8:
  %t26 = load i64, ptr %s1
  %t27 = icmp uge i64 %t26, %t7
  br i1 %t27, label %bb10, label %bb9
bb9:
  %t28 = mul i64 %t26, 1024
  %t29 = sub i64 %lo, %t28
  %t30 = icmp slt i64 %t29, 0
  %t31 = select i1 %t30, i64 0, i64 %t29
  %t32 = sub i64 %hi, %t28
  %t33 = icmp sgt i64 %t32, 1024
  %t34 = select i1 %t33, i64 1024, i64 %t32
  %t35 = mul i64 %t26, 1024
  store i64 %t31, ptr %s2
  br label %bb11
bb11:
  %t36 = load i64, ptr %s2
  %t37 = icmp uge i64 %t36, %t34
  br i1 %t37, label %bb13, label %bb12
bb12:
  %t38 = sub i64 %t34, %t36
  %t39 = icmp ult i64 %t38, 16
  %t40 = select i1 %t39, i64 %t38, i64 16
  store i64 0, ptr %s4
  br label %bb14
bb14:
  %t41 = load i64, ptr %s4
  %t42 = icmp uge i64 %t41, %t40
  br i1 %t42, label %bb16, label %bb15
bb15:
  %t43 = getelementptr [16 x float], ptr %s0, i64 0, i64 %t41
  store float 0x0000000000000000, ptr %t43
  %t44 = add i64 %t41, 1
  store i64 %t44, ptr %s4
  br label %bb14
bb16:
  store i64 0, ptr %s3
  br label %bb17
bb17:
  %t45 = load i64, ptr %s3
  %t46 = icmp uge i64 %t45, 1024
  br i1 %t46, label %bb19, label %bb18
bb18:
  %t47 = add i64 %t35, %t45
  %t48 = getelementptr [1048576 x float], ptr %o4, i64 0, i64 %t47
  %t49 = load float, ptr %t48
  %t50 = mul i64 %t45, 1024
  %t51 = add i64 %t50, %t36
  store i64 0, ptr %s4
  br label %bb20
bb20:
  %t52 = load i64, ptr %s4
  %t53 = icmp uge i64 %t52, %t40
  br i1 %t53, label %bb22, label %bb21
bb21:
  %t54 = add i64 %t51, %t52
  %t55 = getelementptr [1048576 x float], ptr %o5, i64 0, i64 %t54
  %t56 = load float, ptr %t55
  %t57 = fmul float %t49, %t56
  %t58 = getelementptr [16 x float], ptr %s0, i64 0, i64 %t52
  %t59 = load float, ptr %t58
  %t60 = fadd float %t59, %t57
  store float %t60, ptr %t58
  %t61 = add i64 %t52, 1
  store i64 %t61, ptr %s4
  br label %bb20
bb22:
  %t62 = add i64 %t45, 1
  store i64 %t62, ptr %s3
  br label %bb17
bb19:
  %t63 = add i64 %t28, %t36
  store i64 0, ptr %s4
  br label %bb23
bb23:
  %t64 = load i64, ptr %s4
  %t65 = icmp uge i64 %t64, %t40
  br i1 %t65, label %bb25, label %bb24
bb24:
  %t66 = getelementptr [16 x float], ptr %s0, i64 0, i64 %t64
  %t67 = load float, ptr %t66
  %t68 = add i64 %t63, %t64
  %t69 = getelementptr [1048576 x float], ptr %o7, i64 0, i64 %t68
  store float %t67, ptr %t69
  %t70 = add i64 %t64, 1
  store i64 %t70, ptr %s4
  br label %bb23
bb25:
  %t71 = add i64 %t36, 16
  store i64 %t71, ptr %s2
  br label %bb11
bb13:
  %t72 = add i64 %t26, 1
  store i64 %t72, ptr %s1
  br label %bb8
bb10:
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
  call void @flow_par_task(ptr %h, i32 0, i32 1, ptr @task0, i64 1048576, i32 3145730)
  call void @flow_par_task(ptr %h, i32 1, i32 1, ptr @task1, i64 1024, i32 1049602)
  call void @flow_par_task(ptr %h, i32 2, i32 0, ptr @task2, i64 6, i32 1)
  call void @flow_par_task(ptr %h, i32 3, i32 1, ptr @task3, i64 1048576, i32 2097154)
  call void @flow_par_task(ptr %h, i32 4, i32 1, ptr @task4, i64 1048576, i32 2097154)
  call void @flow_par_task(ptr %h, i32 5, i32 0, ptr @task5, i64 4, i32 1048578)
  call void @flow_par_task(ptr %h, i32 6, i32 1, ptr @task6, i64 1048576, i32 1048577)
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
  store i32 1024, ptr %t12
  %t13 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  store i32 1024, ptr %t13
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
  %t60 = getelementptr [1048576 x float], ptr %t56, i64 0, i64 %t59
  %t61 = load float, ptr %t60
  store float %t61, ptr %o17
  %t62 = load ptr, ptr %o4
  %t63 = getelementptr { ptr, i32 }, ptr %o18, i32 0, i32 1
  %t64 = load i32, ptr %t63
  %t65 = sext i32 %t64 to i64
  %t66 = getelementptr [1048576 x float], ptr %t62, i64 0, i64 %t65
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
  store i32 1024, ptr %t8
  %t9 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  store i32 1024, ptr %t9
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
  %t44 = icmp uge i64 %t43, 1024
  br i1 %t44, label %bb42, label %bb41
bb41:
  %t45 = getelementptr [1024 x i32], ptr %t37, i64 0, i64 %t43
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

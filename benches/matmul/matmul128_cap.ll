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

%Frame = type { [16384 x i32], [128 x i32], [16384 x double], [16384 x double], { ptr, ptr, ptr, ptr }, [16384 x double], { ptr, i32 }, double, { ptr, i32 }, double, double, double }

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
  %t7 = getelementptr [16384 x i32], ptr %o2, i64 0, i64 %t4
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
  %t7 = getelementptr [128 x i32], ptr %o3, i64 0, i64 %t4
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
  store i32 16383, ptr %t1
  %t2 = getelementptr { ptr, i32 }, ptr %o8, i32 0, i32 0
  store ptr %o7, ptr %t2
  %t3 = getelementptr { ptr, i32 }, ptr %o10, i32 0, i32 0
  store ptr %o7, ptr %t3
  %t4 = getelementptr { ptr, i32 }, ptr %o8, i32 0, i32 1
  %t5 = load i32, ptr %t4
  %t6 = sext i32 %t5 to i64
  %t7 = getelementptr [16384 x double], ptr %o7, i64 0, i64 %t6
  %t8 = load double, ptr %t7
  store double %t8, ptr %o9
  %t9 = getelementptr { ptr, i32 }, ptr %o10, i32 0, i32 1
  %t10 = load i32, ptr %t9
  %t11 = sext i32 %t10 to i64
  %t12 = getelementptr [16384 x double], ptr %o7, i64 0, i64 %t11
  %t13 = load double, ptr %t12
  store double %t13, ptr %o11
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
  %t6 = getelementptr [16384 x i32], ptr %o2, i64 0, i64 %t4
  %t7 = load i32, ptr %t6
  %t8 = call double @fn1(i32 %t7)
  %t9 = getelementptr [16384 x double], ptr %o4, i64 0, i64 %t4
  store double %t8, ptr %t9
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
  %t6 = getelementptr [16384 x i32], ptr %o2, i64 0, i64 %t4
  %t7 = load i32, ptr %t6
  %t8 = call double @fn2(i32 %t7)
  %t9 = getelementptr [16384 x double], ptr %o5, i64 0, i64 %t4
  store double %t8, ptr %t9
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
  %s0 = alloca i64
  %s8 = alloca { ptr, ptr, ptr, i32 }
  %o2 = getelementptr %Frame, ptr %frame, i32 0, i32 0
  %o7 = getelementptr %Frame, ptr %frame, i32 0, i32 5
  %o3 = getelementptr %Frame, ptr %frame, i32 0, i32 1
  %o4 = getelementptr %Frame, ptr %frame, i32 0, i32 2
  %o5 = getelementptr %Frame, ptr %frame, i32 0, i32 3
  store i64 %lo, ptr %s0
  br label %bb1
bb1:
  %t4 = load i64, ptr %s0
  %t5 = icmp uge i64 %t4, %hi
  br i1 %t5, label %bb3, label %bb2
bb2:
  %t6 = getelementptr [16384 x i32], ptr %o2, i64 0, i64 %t4
  %t7 = load i32, ptr %t6
  %t9 = getelementptr { ptr, ptr, ptr, i32 }, ptr %s8, i32 0, i32 0
  store ptr %o3, ptr %t9
  %t10 = getelementptr { ptr, ptr, ptr, i32 }, ptr %s8, i32 0, i32 1
  store ptr %o4, ptr %t10
  %t11 = getelementptr { ptr, ptr, ptr, i32 }, ptr %s8, i32 0, i32 2
  store ptr %o5, ptr %t11
  %t12 = getelementptr { ptr, ptr, ptr, i32 }, ptr %s8, i32 0, i32 3
  store i32 %t7, ptr %t12
  %t13 = load { ptr, ptr, ptr, i32 }, ptr %s8
  %t14 = call double @fn4({ ptr, ptr, ptr, i32 } %t13)
  %t15 = getelementptr [16384 x double], ptr %o7, i64 0, i64 %t4
  store double %t14, ptr %t15
  %t16 = add i64 %t4, 1
  store i64 %t16, ptr %s0
  br label %bb1
bb3:
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
  call void @flow_par_task(ptr %h, i32 0, i32 1, ptr @task0, i64 16384, i32 49154)
  call void @flow_par_task(ptr %h, i32 1, i32 1, ptr @task1, i64 128, i32 16514)
  call void @flow_par_task(ptr %h, i32 2, i32 0, ptr @task2, i64 6, i32 1)
  call void @flow_par_task(ptr %h, i32 3, i32 1, ptr @task3, i64 16384, i32 32770)
  call void @flow_par_task(ptr %h, i32 4, i32 1, ptr @task4, i64 16384, i32 32770)
  call void @flow_par_task(ptr %h, i32 5, i32 0, ptr @task5, i64 4, i32 16386)
  call void @flow_par_task(ptr %h, i32 6, i32 1, ptr @task6, i64 16384, i32 16385)
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
  %t0 = load double, ptr %o9
  store double %t0, ptr %o12
  call void @flow_par_wait(ptr %h, ptr @ckpt1_entries, i32 4)
  call void @flow_par_check(ptr %h, i64 20)
  %t1 = load double, ptr %o11
  store double %t1, ptr %o14
  %t2 = load double, ptr %o12
  call void @flow_print_f64(double %t2, i1 zeroext true)
  %t3 = load double, ptr %o14
  call void @flow_print_f64(double %t3, i1 zeroext true)
  call void @flow_par_finish(ptr %h)
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
  %o12 = alloca { i32, i32 }
  %o13 = alloca i32
  %o14 = alloca { i32, i32 }
  %o15 = alloca i32
  %o16 = alloca { ptr, i32 }
  %o17 = alloca double
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
  store i32 128, ptr %t12
  %t13 = getelementptr { i32, i32 }, ptr %o10, i32 0, i32 1
  store i32 128, ptr %t13
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
  %t22 = load double, ptr %o6
  %t23 = getelementptr { double, double }, ptr %o22, i32 0, i32 0
  store double %t22, ptr %t23
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
  %t60 = getelementptr [16384 x double], ptr %t56, i64 0, i64 %t59
  %t61 = load double, ptr %t60
  store double %t61, ptr %o17
  %t62 = load ptr, ptr %o4
  %t63 = getelementptr { ptr, i32 }, ptr %o18, i32 0, i32 1
  %t64 = load i32, ptr %t63
  %t65 = sext i32 %t64 to i64
  %t66 = getelementptr [16384 x double], ptr %t62, i64 0, i64 %t65
  %t67 = load double, ptr %t66
  store double %t67, ptr %o19
  %t68 = load double, ptr %o17
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
  %o10 = alloca { ptr, i32, ptr, i32, double, ptr }
  %s38 = alloca double
  %s39 = alloca i64
  %s48 = alloca { ptr, i32, ptr, i32, double, i32 }
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
  store i32 128, ptr %t8
  %t9 = getelementptr { i32, i32 }, ptr %o8, i32 0, i32 1
  store i32 128, ptr %t9
  %t10 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o10, i32 0, i32 4
  store double 0x0000000000000000, ptr %t10
  %t11 = load ptr, ptr %o2
  %t12 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o10, i32 0, i32 5
  store ptr %t11, ptr %t12
  %t13 = load ptr, ptr %o3
  %t14 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o10, i32 0, i32 0
  store ptr %t13, ptr %t14
  %t15 = load ptr, ptr %o4
  %t16 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o10, i32 0, i32 2
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
  %t32 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o10, i32 0, i32 1
  store i32 %t31, ptr %t32
  %t33 = load i32, ptr %o9
  %t34 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o10, i32 0, i32 3
  store i32 %t33, ptr %t34
  %t35 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o10, i32 0, i32 4
  %t36 = load double, ptr %t35
  %t37 = load ptr, ptr %o2
  store double %t36, ptr %s38
  store i64 0, ptr %s39
  br label %bb40
bb40:
  %t43 = load i64, ptr %s39
  %t44 = icmp uge i64 %t43, 128
  br i1 %t44, label %bb42, label %bb41
bb41:
  %t45 = getelementptr [128 x i32], ptr %t37, i64 0, i64 %t43
  %t46 = load i32, ptr %t45
  %t47 = load double, ptr %s38
  %t49 = load ptr, ptr %o3
  %t50 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %s48, i32 0, i32 0
  store ptr %t49, ptr %t50
  %t51 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o10, i32 0, i32 1
  %t52 = load i32, ptr %t51
  %t53 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %s48, i32 0, i32 1
  store i32 %t52, ptr %t53
  %t54 = load ptr, ptr %o4
  %t55 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %s48, i32 0, i32 2
  store ptr %t54, ptr %t55
  %t56 = getelementptr { ptr, i32, ptr, i32, double, ptr }, ptr %o10, i32 0, i32 3
  %t57 = load i32, ptr %t56
  %t58 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %s48, i32 0, i32 3
  store i32 %t57, ptr %t58
  %t59 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %s48, i32 0, i32 4
  store double %t47, ptr %t59
  %t60 = getelementptr { ptr, i32, ptr, i32, double, i32 }, ptr %s48, i32 0, i32 5
  store i32 %t46, ptr %t60
  %t61 = load { ptr, i32, ptr, i32, double, i32 }, ptr %s48
  %t62 = call double @fn3({ ptr, i32, ptr, i32, double, i32 } %t61)
  store double %t62, ptr %s38
  %t63 = add i64 %t43, 1
  store i64 %t63, ptr %s39
  br label %bb40
bb42:
  %t64 = load double, ptr %s38
  store double %t64, ptr %o1
  %t65 = load double, ptr %o1
  ret double %t65
}

define i32 @main() {
entry:
  call void @flow_main()
  ret i32 0
}

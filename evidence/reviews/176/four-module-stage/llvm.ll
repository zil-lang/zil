; ZIL verified LLVM module
; verified-fingerprint: a4c1f5c4f4ad8fcb904ca6fc8a888af5e81a8709b123ab65eb7b6c1632a0fd47
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32, { i32 }, i32, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  call void @zil.fn.2(ptr %payload.1)
  br label %done.1
done.1:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, align 8
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %package.live.1.2 = alloca i1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, align 8
  %local.4 = alloca i1, align 1
  %local.5 = alloca i1, align 1
  %local.6 = alloca ptr, align 8
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca %zil_env_0, align 4
  %local.8 = alloca ptr, align 8
  %index.slot.19 = alloca [3 x i64], align 8
  %index.slot.52 = alloca [3 x i64], align 8
  %index.slot.55 = alloca [3 x i64], align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.0, align 1
  %v.0 = add i32 0, 5
  %v.1 = add i32 0, 65
  %v.2 = call %zil_env_0 @zil.fn.3(i32 %v.0, i32 %v.1)
  store %zil_env_0 %v.2, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.3 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  %v.4 = call { [3 x i64], %zil_env_0, { i32 }, i16, i32 } @zil.fn.6(%zil_env_0 %v.3)
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  store { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.4, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.1, align 1
  store i1 true, ptr %package.live.1.2, align 1
  %v.5 = load { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  store { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.5, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.6 = add i32 0, 3
  %v.7 = load { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.8 = call { [3 x i64], %zil_env_0, { i32 }, i16, i32 } @zil.fn.7({ [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.7, i32 %v.6)
  store { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.8, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.1, align 1
  store i1 true, ptr %package.live.1.2, align 1
  %tmp.1 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.1, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.2 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.2)
  %tmp.3 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.2, i32 0, i32 2
  call void @zil.fn.2(ptr %tmp.3)
  br label %package.done.1
package.done.1:
  %v.9 = load { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  store { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.9, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.10 = add i32 0, 2
  %v.11 = load { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.12 = call { [3 x i64], %zil_env_0, { i32 }, i16, i32 } @zil.fn.8({ [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.11, i32 %v.10)
  store { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.12, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.1, align 1
  store i1 true, ptr %package.live.1.2, align 1
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.5 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.5)
  %tmp.6 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.3, i32 0, i32 2
  call void @zil.fn.2(ptr %tmp.6)
  br label %package.done.2
package.done.2:
  %tmp.7 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 4
  %v.13 = load i32, ptr %tmp.7, align 4
  %v.14 = add i32 0, 12
  %v.15 = icmp ne i32 %v.13, %v.14
  br i1 %v.15, label %bb.1, label %bb.2
bb.1:
  %v.16 = or i1 false, true
  store i1 %v.16, ptr %local.4, align 1
  br label %bb.3
bb.2:
  %tmp.8 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 0
  %v.17 = load [3 x i64], ptr %tmp.8, align 8
  %v.18 = add i32 0, 2
  %tmp.9 = sext i32 %v.18 to i64
  %tmp.10 = icmp ult i64 %tmp.9, 3
  br i1 %tmp.10, label %index.ok.3, label %zil.trap
index.ok.3:
  store [3 x i64] %v.17, ptr %index.slot.19, align 8
  %tmp.11 = getelementptr [3 x i64], ptr %index.slot.19, i64 0, i64 %tmp.9
  %v.19 = load i64, ptr %tmp.11, align 8
  %v.20 = add i32 0, 23
  %v.21 = sext i32 %v.20 to i64
  %v.22 = icmp ne i64 %v.19, %v.21
  store i1 %v.22, ptr %local.4, align 1
  br label %bb.3
bb.3:
  %v.23 = load i1, ptr %local.4, align 1
  br i1 %v.23, label %bb.4, label %bb.5
bb.4:
  %v.24 = or i1 false, true
  store i1 %v.24, ptr %local.5, align 1
  br label %bb.6
bb.5:
  %tmp.12 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 3
  %v.25 = load i16, ptr %tmp.12, align 2
  %v.26 = add i16 0, 513
  %v.27 = icmp ne i16 %v.25, %v.26
  store i1 %v.27, ptr %local.5, align 1
  br label %bb.6
bb.6:
  %v.28 = load i1, ptr %local.5, align 1
  br i1 %v.28, label %bb.7, label %bb.8
bb.7:
  %v.29 = add i32 0, 10
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.13 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.13, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.14 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.14, label %package.field.drop.5, label %package.field.done.5
package.field.drop.5:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.15 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.15)
  br label %package.field.done.5
package.field.done.5:
  %tmp.16 = load i1, ptr %package.live.1.2, align 1
  br i1 %tmp.16, label %package.field.drop.6, label %package.field.done.6
package.field.drop.6:
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.17 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 2
  call void @zil.fn.2(ptr %tmp.17)
  br label %package.field.done.6
package.field.done.6:
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.7, align 1
  ret i32 %v.29
bb.8:
  br label %bb.9
bb.9:
  %v.30 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 1
  %v.31 = getelementptr i8, ptr %v.30, i64 0
  store ptr %v.31, ptr %local.6, align 8
  %v.32 = load ptr, ptr %local.6, align 8
  %v.33 = add i32 0, 1
  %v.34 = call i32 @zil.fn.5(ptr %v.32, i32 %v.33)
  %v.35 = add i32 0, 13
  %v.36 = icmp ne i32 %v.34, %v.35
  br i1 %v.36, label %bb.10, label %bb.11
bb.10:
  %v.37 = add i32 0, 11
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.18 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.18, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.19 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.19, label %package.field.drop.8, label %package.field.done.8
package.field.drop.8:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.20 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.20)
  br label %package.field.done.8
package.field.done.8:
  %tmp.21 = load i1, ptr %package.live.1.2, align 1
  br i1 %tmp.21, label %package.field.drop.9, label %package.field.done.9
package.field.drop.9:
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.22 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 2
  call void @zil.fn.2(ptr %tmp.22)
  br label %package.field.done.9
package.field.done.9:
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.7, align 1
  ret i32 %v.37
bb.11:
  br label %bb.12
bb.12:
  %v.38 = add i32 0, 20
  %v.39 = add i32 0, 66
  %v.40 = call %zil_env_0 @zil.fn.3(i32 %v.38, i32 %v.39)
  store %zil_env_0 %v.40, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %tmp.23 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.23, label %package.field.drop.10, label %package.field.done.10
package.field.drop.10:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.24 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.24)
  br label %package.field.done.10
package.field.done.10:
  %v.41 = load %zil_env_0, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
  %tmp.25 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 1
  store %zil_env_0 %v.41, ptr %tmp.25, align 4
  store i1 true, ptr %package.live.1.1, align 1
  %tmp.26 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.26, label %closure.drop.11, label %closure.done.11
closure.drop.11:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.closure.drop.0(ptr %local.7)
  br label %closure.done.11
closure.done.11:
  %v.42 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 1
  %v.43 = getelementptr i8, ptr %v.42, i64 0
  store ptr %v.43, ptr %local.8, align 8
  %v.44 = load ptr, ptr %local.8, align 8
  %v.45 = add i32 0, 2
  %v.46 = call i32 @zil.fn.5(ptr %v.44, i32 %v.45)
  %v.47 = add i32 0, 22
  %v.48 = icmp ne i32 %v.46, %v.47
  br i1 %v.48, label %bb.13, label %bb.14
bb.13:
  %v.49 = add i32 0, 12
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.27 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.27, label %package.drop.12, label %package.done.12
package.drop.12:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.28 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.28, label %package.field.drop.13, label %package.field.done.13
package.field.drop.13:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.29 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.29)
  br label %package.field.done.13
package.field.done.13:
  %tmp.30 = load i1, ptr %package.live.1.2, align 1
  br i1 %tmp.30, label %package.field.drop.14, label %package.field.done.14
package.field.drop.14:
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.31 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 2
  call void @zil.fn.2(ptr %tmp.31)
  br label %package.field.done.14
package.field.done.14:
  br label %package.done.12
package.done.12:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.49
bb.14:
  br label %bb.15
bb.15:
  %tmp.32 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 0
  %v.50 = load [3 x i64], ptr %tmp.32, align 8
  %v.51 = add i32 0, 0
  %tmp.33 = sext i32 %v.51 to i64
  %tmp.34 = icmp ult i64 %tmp.33, 3
  br i1 %tmp.34, label %index.ok.15, label %zil.trap
index.ok.15:
  store [3 x i64] %v.50, ptr %index.slot.52, align 8
  %tmp.35 = getelementptr [3 x i64], ptr %index.slot.52, i64 0, i64 %tmp.33
  %v.52 = load i64, ptr %tmp.35, align 8
  %tmp.36 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 0
  %v.53 = load [3 x i64], ptr %tmp.36, align 8
  %v.54 = add i32 0, 1
  %tmp.37 = sext i32 %v.54 to i64
  %tmp.38 = icmp ult i64 %tmp.37, 3
  br i1 %tmp.38, label %index.ok.16, label %zil.trap
index.ok.16:
  store [3 x i64] %v.53, ptr %index.slot.55, align 8
  %tmp.39 = getelementptr [3 x i64], ptr %index.slot.55, i64 0, i64 %tmp.37
  %v.55 = load i64, ptr %tmp.39, align 8
  %tmp.40 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.52, i64 %v.55)
  %v.56 = extractvalue { i64, i1 } %tmp.40, 0
  %tmp.41 = extractvalue { i64, i1 } %tmp.40, 1
  br i1 %tmp.41, label %zil.trap, label %checked.17
checked.17:
  %v.57 = add i32 0, 36
  %v.58 = sext i32 %v.57 to i64
  %v.59 = icmp ne i64 %v.56, %v.58
  br i1 %v.59, label %bb.16, label %bb.17
bb.16:
  %v.60 = add i32 0, 13
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.42 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.42, label %package.drop.18, label %package.done.18
package.drop.18:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.43 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.43, label %package.field.drop.19, label %package.field.done.19
package.field.drop.19:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.44 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.44)
  br label %package.field.done.19
package.field.done.19:
  %tmp.45 = load i1, ptr %package.live.1.2, align 1
  br i1 %tmp.45, label %package.field.drop.20, label %package.field.done.20
package.field.drop.20:
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.46 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 2
  call void @zil.fn.2(ptr %tmp.46)
  br label %package.field.done.20
package.field.done.20:
  br label %package.done.18
package.done.18:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.60
bb.17:
  br label %bb.18
bb.18:
  %v.61 = add i32 0, 0
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.47 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.47, label %package.drop.21, label %package.done.21
package.drop.21:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.48 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.48, label %package.field.drop.22, label %package.field.done.22
package.field.drop.22:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.49 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.49)
  br label %package.field.done.22
package.field.done.22:
  %tmp.50 = load i1, ptr %package.live.1.2, align 1
  br i1 %tmp.50, label %package.field.drop.23, label %package.field.done.23
package.field.drop.23:
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.51 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.1, i32 0, i32 2
  call void @zil.fn.2(ptr %tmp.51)
  br label %package.field.done.23
package.field.done.23:
  br label %package.done.21
package.done.21:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.61
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.2(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  %v.2 = call i32 @putchar(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.3(i32 %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  store i32 %arg.0, ptr %local.0, align 4
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = load i32, ptr %local.1, align 4
  store i32 %v.1, ptr %local.3, align 4
  %v.2 = load i32, ptr %local.3, align 4
%v.3 = insertvalue { i32 } zeroinitializer, i32 %v.2, 0
  store i1 false, ptr %resource.live.4, align 1
  store { i32 } %v.3, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.4 = load i32, ptr %local.2, align 4
  %v.5 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  %v.6 = load i32, ptr %local.1, align 4
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, i32 %v.4, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, { i32 } %v.5, 1
  %tmp.2 = insertvalue %zil_env_0 %tmp.1, i32 %v.6, 2
  %v.7 = insertvalue %zil_env_0 %tmp.2, i1 true, 3
  %tmp.3 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.2(ptr %local.4)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.4, align 1
  ret %zil_env_0 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.4(i32 %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = call %zil_env_0 @zil.fn.3(i32 %v.0, i32 %v.1)
  ret %zil_env_0 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(ptr %env, i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca ptr, align 8
  %local.3 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.2, align 8
  %tmp.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %v.2 = load i32, ptr %tmp.1, align 4
  store i32 %v.2, ptr %local.3, align 4
  %v.3 = load i32, ptr %local.1, align 4
  %v.4 = load i32, ptr %local.0, align 4
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.4)
  %v.5 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.5, ptr %local.1, align 4
  %tmp.4 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store i32 %v.5, ptr %tmp.4, align 4
  %v.6 = load i32, ptr %local.1, align 4
  %v.7 = load ptr, ptr %local.2, align 8
  %tmp.5 = getelementptr { i32 }, ptr %v.7, i32 0, i32 0
  %v.8 = load i32, ptr %tmp.5, align 4
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.10 = load i32, ptr %local.3, align 4
  %tmp.8 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { [3 x i64], %zil_env_0, { i32 }, i16, i32 } @zil.fn.6(%zil_env_0 %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca [3 x i64], align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  %local.5 = alloca i16, align 2
  %local.6 = alloca i32, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca %zil_env_0, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { i32 }, align 4
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.7, align 1
  %v.0 = add i32 0, 17
  %v.1 = sext i32 %v.0 to i64
  %v.2 = add i32 0, 19
  %v.3 = sext i32 %v.2 to i64
  %v.4 = add i32 0, 23
  %v.5 = sext i32 %v.4 to i64
%tmp.0 = insertvalue [3 x i64] zeroinitializer, i64 %v.1, 0
%tmp.1 = insertvalue [3 x i64] %tmp.0, i64 %v.3, 1
%v.6 = insertvalue [3 x i64] %tmp.1, i64 %v.5, 2
  store [3 x i64] %v.6, ptr %local.1, align 8
  %v.7 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.7, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.8 = add i32 0, 84
  store i32 %v.8, ptr %local.3, align 4
  %v.9 = load i32, ptr %local.3, align 4
%v.10 = insertvalue { i32 } zeroinitializer, i32 %v.9, 0
  store i1 false, ptr %resource.live.4, align 1
  store { i32 } %v.10, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.11 = add i16 0, 513
  store i16 %v.11, ptr %local.5, align 2
  %v.12 = add i32 0, 0
  store i32 %v.12, ptr %local.6, align 4
  %v.13 = load [3 x i64], ptr %local.1, align 8
  %v.14 = load %zil_env_0, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.14, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.15 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  store { i32 } %v.15, ptr %local.8, align 4
  store i1 true, ptr %resource.live.8, align 1
  %v.16 = load i16, ptr %local.5, align 2
  %v.17 = load i32, ptr %local.6, align 4
  %v.18 = load %zil_env_0, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
  %v.19 = load { i32 }, ptr %local.8, align 4
  store i1 false, ptr %resource.live.8, align 1
%tmp.2 = insertvalue { [3 x i64], %zil_env_0, { i32 }, i16, i32 } zeroinitializer, [3 x i64] %v.13, 0
%tmp.3 = insertvalue { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %tmp.2, %zil_env_0 %v.18, 1
%tmp.4 = insertvalue { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %tmp.3, { i32 } %v.19, 2
%tmp.5 = insertvalue { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %tmp.4, i16 %v.16, 3
%v.20 = insertvalue { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %tmp.5, i32 %v.17, 4
  %tmp.6 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.6, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.2(ptr %local.8)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.7 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.7, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.closure.drop.0(ptr %local.7)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.8 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.8, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.2(ptr %local.4)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.9 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.9, label %closure.drop.3, label %closure.done.3
closure.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.3
closure.done.3:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.10 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.10, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.20
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { [3 x i64], %zil_env_0, { i32 }, i16, i32 } @zil.fn.7({ [3 x i64], %zil_env_0, { i32 }, i16, i32 } %arg.0, i32 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, align 8
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, align 8
  %package.live.2.1 = alloca i1, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %package.live.2.2 = alloca i1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  %local.3 = alloca ptr, align 8
  store { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  store { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  store i1 true, ptr %package.live.2.1, align 1
  store i1 true, ptr %package.live.2.2, align 1
  %v.1 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.2, i32 0, i32 1
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  store ptr %v.2, ptr %local.3, align 8
  %v.3 = load ptr, ptr %local.3, align 8
  %v.4 = load i32, ptr %local.1, align 4
  %v.5 = call i32 @zil.fn.5(ptr %v.3, i32 %v.4)
  %tmp.0 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.2, i32 0, i32 4
  store i32 %v.5, ptr %tmp.0, align 4
  %v.6 = load { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  %tmp.1 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.1, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.2 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.2, label %package.field.drop.1, label %package.field.done.1
package.field.drop.1:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.3 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.3)
  br label %package.field.done.1
package.field.done.1:
  %tmp.4 = load i1, ptr %package.live.2.2, align 1
  br i1 %tmp.4, label %package.field.drop.2, label %package.field.done.2
package.field.drop.2:
  store i1 false, ptr %package.live.2.2, align 1
  %tmp.5 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.2, i32 0, i32 2
  call void @zil.fn.2(ptr %tmp.5)
  br label %package.field.done.2
package.field.done.2:
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store i1 false, ptr %package.live.2.2, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.7 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.7)
  %tmp.8 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.0, i32 0, i32 2
  call void @zil.fn.2(ptr %tmp.8)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.0, align 1
  ret { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { [3 x i64], %zil_env_0, { i32 }, i16, i32 } @zil.fn.8({ [3 x i64], %zil_env_0, { i32 }, i16, i32 } %arg.0, i32 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, align 8
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, align 8
  store { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = load { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = load { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.3 = call { [3 x i64], %zil_env_0, { i32 }, i16, i32 } @zil.fn.7({ [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.2, i32 %v.1)
  store { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.3, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.4 = load i32, ptr %local.1, align 4
  %v.5 = load { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.6 = call { [3 x i64], %zil_env_0, { i32 }, i16, i32 } @zil.fn.7({ [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.5, i32 %v.4)
  %tmp.0 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.1 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.1)
  %tmp.2 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.3, i32 0, i32 2
  call void @zil.fn.2(ptr %tmp.2)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.4 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.4)
  %tmp.5 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.2, i32 0, i32 2
  call void @zil.fn.2(ptr %tmp.5)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.7 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.7)
  %tmp.8 = getelementptr { [3 x i64], %zil_env_0, { i32 }, i16, i32 }, ptr %local.0, i32 0, i32 2
  call void @zil.fn.2(ptr %tmp.8)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.0, align 1
  ret { [3 x i64], %zil_env_0, { i32 }, i16, i32 } %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

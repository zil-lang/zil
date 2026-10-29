; ZIL verified LLVM module
; verified-fingerprint: 2a06790f4d8df46ef25e12fe72a3f6d17aa7dddc52aeaeb237d818ecbe8151ae
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { ptr, ptr }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)

define { i1, i1, i32 } @zil.fn.0() {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 0
  %v.1 = call { i1, i1, i32 } @zil.fn.2(i64 %v.0)
  ret { i1, i1, i32 } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.1(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.4(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, i1, i32 } @zil.fn.2(i64 %arg.0) {
entry:
  %local.0 = alloca i64, align 8
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i64, align 8
  %local.4 = alloca %zil_env_0, align 8
  %local.5 = alloca i64, align 8
  store i64 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 84
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = add i64 0, 0
  store i64 %v.3, ptr %local.3, align 8
  %v.4 = getelementptr i8, ptr %local.0, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = getelementptr i8, ptr %local.3, i64 0
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, ptr %v.5, 0
  %v.8 = insertvalue %zil_env_0 %tmp.0, ptr %v.7, 1
  store %zil_env_0 %v.8, ptr %local.4, align 8
  %v.9 = getelementptr i8, ptr %local.4, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = call { i1, i1, i64 } @zil.fn.6(ptr %v.11)
  %v.13 = extractvalue { i1, i1, i64 } %v.12, 0
  br i1 %v.13, label %bb.1, label %bb.2
bb.1:
  %tmp.1 = extractvalue { i1, i1, i64 } %v.12, 0
  %tmp.2 = icmp ne i1 %tmp.1, true
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.14 = extractvalue { i1, i1, i64 } %v.12, 1
  %tmp.3 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.15 = insertvalue { i1, i1, i32 } %tmp.3, i1 %v.14, 1
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, i1, i32 } %v.15
bb.2:
  %tmp.5 = extractvalue { i1, i1, i64 } %v.12, 0
  %tmp.6 = icmp ne i1 %tmp.5, false
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  %v.16 = extractvalue { i1, i1, i64 } %v.12, 2
  store i64 %v.16, ptr %local.5, align 8
  %v.17 = load i64, ptr %local.5, align 8
  %v.18 = add i64 0, 20
  %v.19 = icmp ne i64 %v.17, %v.18
  br i1 %v.19, label %bb.3, label %bb.4
bb.3:
  %v.20 = or i1 false, true
  %tmp.7 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.21 = insertvalue { i1, i1, i32 } %tmp.7, i1 %v.20, 1
  %tmp.8 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.8, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, i1, i32 } %v.21
bb.4:
  br label %bb.5
bb.5:
  %v.22 = load i64, ptr %local.3, align 8
  %v.23 = add i64 0, 10
  %v.24 = icmp ne i64 %v.22, %v.23
  br i1 %v.24, label %bb.6, label %bb.7
bb.6:
  %v.25 = or i1 false, true
  %tmp.9 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.26 = insertvalue { i1, i1, i32 } %tmp.9, i1 %v.25, 1
  %tmp.10 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.10, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, i1, i32 } %v.26
bb.7:
  br label %bb.8
bb.8:
  %v.27 = add i32 0, 83
  call void @zil.fn.4(i32 %v.27)
  %v.29 = add i32 0, 0
  %tmp.11 = insertvalue { i1, i1, i32 } zeroinitializer, i1 false, 0
  %v.30 = insertvalue { i1, i1, i32 } %tmp.11, i32 %v.29, 2
  %tmp.12 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.12, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, i1, i32 } %v.30
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.4(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call i32 @putchar(i32 %v.0)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, i1, i64 } @zil.fn.5(ptr %env, i64 %arg.0) {
entry:
  %local.0 = alloca i64, align 8
  %local.1 = alloca ptr, align 8
  %local.2 = alloca ptr, align 8
  store i64 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load ptr, ptr %tmp.0, align 8
  store ptr %v.0, ptr %local.1, align 8
  %tmp.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %v.1 = load ptr, ptr %tmp.1, align 8
  store ptr %v.1, ptr %local.2, align 8
  %v.2 = add i32 0, 69
  call void @zil.fn.4(i32 %v.2)
  %v.4 = load i64, ptr %local.0, align 8
  %v.5 = load ptr, ptr %local.1, align 8
  %v.6 = load i64, ptr %v.5, align 8
  %v.7 = icmp eq i64 %v.4, %v.6
  br i1 %v.7, label %bb.1, label %bb.2
bb.1:
  %v.8 = or i1 false, true
  %tmp.2 = insertvalue { i1, i1, i64 } zeroinitializer, i1 true, 0
  %v.9 = insertvalue { i1, i1, i64 } %tmp.2, i1 %v.8, 1
  ret { i1, i1, i64 } %v.9
bb.2:
  br label %bb.3
bb.3:
  %v.10 = load ptr, ptr %local.2, align 8
  %v.11 = load ptr, ptr %local.2, align 8
  %v.12 = load i64, ptr %v.11, align 8
  %v.13 = load i64, ptr %local.0, align 8
  %tmp.3 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.12, i64 %v.13)
  %v.14 = extractvalue { i64, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i64, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.0
checked.0:
  store i64 %v.14, ptr %v.10, align 8
  %v.15 = load ptr, ptr %local.2, align 8
  %v.16 = load i64, ptr %v.15, align 8
  %tmp.5 = insertvalue { i1, i1, i64 } zeroinitializer, i1 false, 0
  %v.17 = insertvalue { i1, i1, i64 } %tmp.5, i64 %v.16, 2
  ret { i1, i1, i64 } %v.17
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, i1, i64 } @zil.fn.6(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  %local.2 = alloca i64, align 8
  %local.3 = alloca i64, align 8
  %local.4 = alloca i64, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = add i64 0, 1
  %v.2 = call { i1, i1, i64 } @zil.fn.5(ptr %v.0, i64 %v.1)
  %v.3 = extractvalue { i1, i1, i64 } %v.2, 0
  br i1 %v.3, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, i1, i64 } %v.2, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.4 = extractvalue { i1, i1, i64 } %v.2, 1
  %tmp.2 = insertvalue { i1, i1, i64 } zeroinitializer, i1 true, 0
  %v.5 = insertvalue { i1, i1, i64 } %tmp.2, i1 %v.4, 1
  ret { i1, i1, i64 } %v.5
bb.2:
  %tmp.3 = extractvalue { i1, i1, i64 } %v.2, 0
  %tmp.4 = icmp ne i1 %tmp.3, false
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.6 = extractvalue { i1, i1, i64 } %v.2, 2
  store i64 %v.6, ptr %local.1, align 8
  %v.7 = load ptr, ptr %local.0, align 8
  %v.8 = add i64 0, 2
  %v.9 = call { i1, i1, i64 } @zil.fn.5(ptr %v.7, i64 %v.8)
  %v.10 = extractvalue { i1, i1, i64 } %v.9, 0
  br i1 %v.10, label %bb.3, label %bb.4
bb.3:
  %tmp.5 = extractvalue { i1, i1, i64 } %v.9, 0
  %tmp.6 = icmp ne i1 %tmp.5, true
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  %v.11 = extractvalue { i1, i1, i64 } %v.9, 1
  %tmp.7 = insertvalue { i1, i1, i64 } zeroinitializer, i1 true, 0
  %v.12 = insertvalue { i1, i1, i64 } %tmp.7, i1 %v.11, 1
  ret { i1, i1, i64 } %v.12
bb.4:
  %tmp.8 = extractvalue { i1, i1, i64 } %v.9, 0
  %tmp.9 = icmp ne i1 %tmp.8, false
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  %v.13 = extractvalue { i1, i1, i64 } %v.9, 2
  store i64 %v.13, ptr %local.2, align 8
  %v.14 = load ptr, ptr %local.0, align 8
  %v.15 = add i64 0, 3
  %v.16 = call { i1, i1, i64 } @zil.fn.5(ptr %v.14, i64 %v.15)
  %v.17 = extractvalue { i1, i1, i64 } %v.16, 0
  br i1 %v.17, label %bb.5, label %bb.6
bb.5:
  %tmp.10 = extractvalue { i1, i1, i64 } %v.16, 0
  %tmp.11 = icmp ne i1 %tmp.10, true
  br i1 %tmp.11, label %zil.trap, label %checked.4
checked.4:
  %v.18 = extractvalue { i1, i1, i64 } %v.16, 1
  %tmp.12 = insertvalue { i1, i1, i64 } zeroinitializer, i1 true, 0
  %v.19 = insertvalue { i1, i1, i64 } %tmp.12, i1 %v.18, 1
  ret { i1, i1, i64 } %v.19
bb.6:
  %tmp.13 = extractvalue { i1, i1, i64 } %v.16, 0
  %tmp.14 = icmp ne i1 %tmp.13, false
  br i1 %tmp.14, label %zil.trap, label %checked.5
checked.5:
  %v.20 = extractvalue { i1, i1, i64 } %v.16, 2
  store i64 %v.20, ptr %local.3, align 8
  %v.21 = load ptr, ptr %local.0, align 8
  %v.22 = add i64 0, 4
  %v.23 = call { i1, i1, i64 } @zil.fn.5(ptr %v.21, i64 %v.22)
  %v.24 = extractvalue { i1, i1, i64 } %v.23, 0
  br i1 %v.24, label %bb.7, label %bb.8
bb.7:
  %tmp.15 = extractvalue { i1, i1, i64 } %v.23, 0
  %tmp.16 = icmp ne i1 %tmp.15, true
  br i1 %tmp.16, label %zil.trap, label %checked.6
checked.6:
  %v.25 = extractvalue { i1, i1, i64 } %v.23, 1
  %tmp.17 = insertvalue { i1, i1, i64 } zeroinitializer, i1 true, 0
  %v.26 = insertvalue { i1, i1, i64 } %tmp.17, i1 %v.25, 1
  ret { i1, i1, i64 } %v.26
bb.8:
  %tmp.18 = extractvalue { i1, i1, i64 } %v.23, 0
  %tmp.19 = icmp ne i1 %tmp.18, false
  br i1 %tmp.19, label %zil.trap, label %checked.7
checked.7:
  %v.27 = extractvalue { i1, i1, i64 } %v.23, 2
  store i64 %v.27, ptr %local.4, align 8
  %v.28 = load i64, ptr %local.1, align 8
  %v.29 = load i64, ptr %local.2, align 8
  %tmp.20 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.28, i64 %v.29)
  %v.30 = extractvalue { i64, i1 } %tmp.20, 0
  %tmp.21 = extractvalue { i64, i1 } %tmp.20, 1
  br i1 %tmp.21, label %zil.trap, label %checked.8
checked.8:
  %v.31 = load i64, ptr %local.3, align 8
  %tmp.22 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.30, i64 %v.31)
  %v.32 = extractvalue { i64, i1 } %tmp.22, 0
  %tmp.23 = extractvalue { i64, i1 } %tmp.22, 1
  br i1 %tmp.23, label %zil.trap, label %checked.9
checked.9:
  %v.33 = load i64, ptr %local.4, align 8
  %tmp.24 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.32, i64 %v.33)
  %v.34 = extractvalue { i64, i1 } %tmp.24, 0
  %tmp.25 = extractvalue { i64, i1 } %tmp.24, 1
  br i1 %tmp.25, label %zil.trap, label %checked.10
checked.10:
  %tmp.26 = insertvalue { i1, i1, i64 } zeroinitializer, i1 false, 0
  %v.35 = insertvalue { i1, i1, i64 } %tmp.26, i64 %v.34, 2
  ret { i1, i1, i64 } %v.35
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, i1, i32 } @zil.fn.0()
  %entry.error = extractvalue { i1, i1, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, i1, i32 } %entry.result, 2
  ret i32 %entry.success
}

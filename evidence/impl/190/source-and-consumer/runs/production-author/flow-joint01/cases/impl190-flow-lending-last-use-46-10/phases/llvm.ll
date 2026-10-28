; ZIL verified LLVM module
; verified-fingerprint: cae951327319bf3b6a0363ea0c9d31fc88495a7e44fad5b213e4d3392178ab49
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @getchar() nonlazybind
define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = call i32 @getchar()
  store i32 %v.1, ptr %local.0, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = add i32 0, 48
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.2() {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = insertvalue %zil_env_0 zeroinitializer, i32 %v.1, 0
  ret %zil_env_0 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca %zil_env_0, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = call i32 @zil.fn.1()
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = call %zil_env_0 @zil.fn.2()
  store %zil_env_0 %v.1, ptr %local.1, align 4
  %v.2 = getelementptr i8, ptr %local.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = load i32, ptr %local.0, align 4
  %v.6 = call i32 @zil.fn.5(ptr %v.4, i32 %v.5)
  store i32 %v.6, ptr %local.2, align 4
  %v.7 = call ptr @zil.fn.4(ptr %local.1)
  store ptr %v.7, ptr %local.3, align 8
  %v.8 = load i32, ptr %local.2, align 4
  %v.9 = load i32, ptr %local.0, align 4
  %v.10 = load i32, ptr %local.0, align 4
  %v.11 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.10, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %tmp.2 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.9, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %v.14 = add i32 0, 2
  %tmp.4 = icmp eq i32 %v.14, 0
  %tmp.5 = icmp eq i32 %v.13, -2147483648
  %tmp.6 = icmp eq i32 %v.14, -1
  %tmp.7 = and i1 %tmp.5, %tmp.6
  %tmp.8 = or i1 %tmp.4, %tmp.7
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  %v.15 = sdiv i32 %v.13, %v.14
  %tmp.9 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.8, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.9, 0
  %tmp.10 = extractvalue { i32, i1 } %tmp.9, 1
  br i1 %tmp.10, label %zil.trap, label %checked.3
checked.3:
  %v.17 = load ptr, ptr %local.3, align 8
  %v.18 = load i32, ptr %v.17, align 4
  %tmp.11 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.16, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i32, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.4
checked.4:
  %v.20 = load i32, ptr %local.0, align 4
  %tmp.13 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.19, i32 %v.20)
  %v.21 = extractvalue { i32, i1 } %tmp.13, 0
  %tmp.14 = extractvalue { i32, i1 } %tmp.13, 1
  br i1 %tmp.14, label %zil.trap, label %checked.5
checked.5:
  %v.22 = add i32 0, 1
  %tmp.15 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.21, i32 %v.22)
  %v.23 = extractvalue { i32, i1 } %tmp.15, 0
  %tmp.16 = extractvalue { i32, i1 } %tmp.15, 1
  br i1 %tmp.16, label %zil.trap, label %checked.6
checked.6:
  ret i32 %v.23
zil.trap:
  call void @llvm.trap()
  unreachable
}

define ptr @zil.fn.4(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %tmp.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.1, align 4
  %v.2 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.1, i32 %v.2)
  %v.3 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.3, ptr %local.0, align 4
  %tmp.4 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store i32 %v.3, ptr %tmp.4, align 4
  %v.4 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  ret ptr %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.3, align 4
  br label %bb.1
bb.1:
  %v.2 = load i32, ptr %local.3, align 4
  %v.3 = load i32, ptr %local.1, align 4
  %v.4 = icmp slt i32 %v.2, %v.3
  br i1 %v.4, label %bb.2, label %bb.3
bb.2:
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = call ptr @zil.fn.4(ptr %v.5)
  store ptr %v.6, ptr %local.4, align 8
  %v.7 = load i32, ptr %local.2, align 4
  %v.8 = load ptr, ptr %local.4, align 8
  %v.9 = load i32, ptr %v.8, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.7, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.10, ptr %local.2, align 4
  %v.11 = load i32, ptr %local.3, align 4
  %v.12 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.13, ptr %local.3, align 4
  br label %bb.1
bb.3:
  %v.14 = load i32, ptr %local.2, align 4
  ret i32 %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.3()
  ret i32 %entry.result
}

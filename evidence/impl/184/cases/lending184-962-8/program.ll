; ZIL verified LLVM module
; verified-fingerprint: d9cf5091fb48bfd7918c7377b8c7f82c535610587fa0d3200c793d38d42cc37a
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32, i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca ptr, align 8
  %local.2 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 7
  %v.1 = add i32 0, 11
  %v.2 = call %zil_env_0 @zil.fn.2(i32 %v.0, i32 %v.1)
  store %zil_env_0 %v.2, ptr %local.0, align 4
  %v.3 = or i1 false, true
  %v.4 = call ptr @zil.fn.3(ptr %local.0, i1 %v.3)
  store ptr %v.4, ptr %local.1, align 8
  %v.5 = or i1 false, false
  %v.6 = call ptr @zil.fn.3(ptr %local.0, i1 %v.5)
  store ptr %v.6, ptr %local.2, align 8
  %v.7 = load ptr, ptr %local.1, align 8
  %v.8 = load i32, ptr %v.7, align 4
  %v.9 = load ptr, ptr %local.2, align 8
  %v.10 = load i32, ptr %v.9, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.8, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.12 = add i32 0, 18
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.11, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.13
zil.trap:
  call void @llvm.trap()
  unreachable
}

define ptr @zil.fn.1(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  ret ptr %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.2(i32 %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.1, align 4
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, i32 %v.0, 0
  %v.2 = insertvalue %zil_env_0 %tmp.0, i32 %v.1, 1
  ret %zil_env_0 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define ptr @zil.fn.3(ptr %env, i1 %arg.0) {
entry:
  %local.0 = alloca i1, align 1
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  store i1 %arg.0, ptr %local.0, align 1
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %tmp.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %v.1 = load i32, ptr %tmp.1, align 4
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = load i1, ptr %local.0, align 1
  br i1 %v.2, label %bb.1, label %bb.2
bb.1:
  %v.3 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = call ptr @zil.fn.1(ptr %v.5)
  ret ptr %v.6
bb.2:
  br label %bb.3
bb.3:
  %v.7 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = call ptr @zil.fn.1(ptr %v.9)
  ret ptr %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

; ZIL verified LLVM module
; verified-fingerprint: f3fa570d6b0149985c6d65565f3b3d62349eae9d46085302fe412945ee503c9a
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.0, i32 %v.1)
  %v.2 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca %zil_env_0, align 1
  %local.2 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = freeze ptr @zil.fn.0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.1, ptr %local.1, align 1
  %v.2 = getelementptr i8, ptr %local.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = call ptr @zil.fn.3(ptr %v.4, ptr %v.5)
  store ptr %v.6, ptr %local.2, align 8
  %v.7 = load ptr, ptr %local.2, align 8
  %v.8 = add i32 0, 6
  %v.9 = call i32 %v.7(i32 %v.8)
  %v.10 = add i32 0, 7
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define ptr @zil.fn.2(%zil_env_0 %env, ptr %arg.0) {
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

define ptr @zil.fn.3(ptr %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.1, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.2 = call ptr @zil.fn.2(%zil_env_0 %tmp.0, ptr %v.1)
  ret ptr %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

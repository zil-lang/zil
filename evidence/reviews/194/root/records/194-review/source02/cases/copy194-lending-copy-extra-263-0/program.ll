; ZIL verified LLVM module
; verified-fingerprint: f7b8bde7456c96b8cb2223cfaaf8e3690a28d19d6cc72c94d96ade8c4f2c2aa8
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define %zil_env_0 @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 6
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = insertvalue %zil_env_0 zeroinitializer, i32 %v.1, 0
  ret %zil_env_0 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  %local.3 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = call %zil_env_0 @zil.fn.0()
  store %zil_env_0 %v.0, ptr %local.0, align 4
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = add i32 0, 0
  %v.5 = call ptr @zil.fn.3(ptr %v.3, i32 %v.4)
  store ptr %v.5, ptr %local.1, align 8
  %v.6 = load ptr, ptr %local.1, align 8
  %v.7 = load i32, ptr %v.6, align 4
  store i32 %v.7, ptr %local.2, align 4
  %v.8 = getelementptr i8, ptr %local.0, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = add i32 0, 0
  %v.12 = sext i32 %v.11 to i64
  %v.13 = call ptr @zil.fn.4(ptr %v.10, i64 %v.12)
  store ptr %v.13, ptr %local.3, align 8
  %v.14 = load i32, ptr %local.2, align 4
  %v.15 = load ptr, ptr %local.3, align 8
  %v.16 = load i32, ptr %v.15, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.14, i32 %v.16)
  %v.17 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.18 = add i32 0, 15
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.17, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}

define ptr @zil.fn.2(ptr %env) {
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

define ptr @zil.fn.3(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call ptr @zil.fn.2(ptr %v.0)
  ret ptr %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define ptr @zil.fn.4(ptr %arg.0, i64 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call ptr @zil.fn.2(ptr %v.0)
  ret ptr %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

; ZIL verified LLVM module
; verified-fingerprint: cf4db9fabd2f51d3e5b6d2ec95a87bdb533907b70a3dc440aa44ccc0b51653a9
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 41
  %v.1 = call %zil_env_0 @zil.fn.1(i32 %v.0)
  store %zil_env_0 %v.1, ptr %local.0, align 4
  %v.2 = getelementptr i8, ptr %local.0, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = call ptr @zil.fn.3(ptr %v.4)
  store ptr %v.5, ptr %local.1, align 8
  %v.6 = load ptr, ptr %local.1, align 8
  %v.7 = load i32, ptr %v.6, align 4
  %v.8 = add i32 0, 41
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.7, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.1(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = insertvalue %zil_env_0 zeroinitializer, i32 %v.0, 0
  ret %zil_env_0 %v.1
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
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  ret ptr %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define ptr @zil.fn.3(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
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
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

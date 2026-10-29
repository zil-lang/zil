; ZIL verified LLVM module
; verified-fingerprint: cfa22ea0c989ff2ac9770580520021d2f146c56e7d6bf379679c8ddcdd8be781
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define %zil_env_0 @zil.fn.0(i32 %arg.0) {
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

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca %zil_env_0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 42
  %v.1 = call %zil_env_0 @zil.fn.0(i32 %v.0)
  store %zil_env_0 %v.1, ptr %local.0, align 4
  %v.2 = call ptr @zil.fn.2(ptr %local.0)
  %v.3 = load i32, ptr %v.2, align 4
  %v.4 = add i32 0, 42
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.3, i32 %v.4)
  %v.5 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.5
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

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

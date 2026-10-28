; ZIL verified LLVM module
; verified-fingerprint: 0dee6b54d539a03a25c58e0a2613e80e1027904b931fe2fd439913f96a076729
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32, i64 } }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define %zil_env_0 @zil.fn.0({ i32, i64 } %arg.0) {
entry:
  %local.0 = alloca { i32, i64 }, align 8
  store { i32, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { i32, i64 }, ptr %local.0, align 8
  %v.1 = insertvalue %zil_env_0 zeroinitializer, { i32, i64 } %v.0, 0
  ret %zil_env_0 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i64, align 8
  %local.2 = alloca %zil_env_0, align 8
  %local.3 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 3
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i64 0, 41
  store i64 %v.1, ptr %local.1, align 8
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = load i64, ptr %local.1, align 8
%tmp.0 = insertvalue { i32, i64 } zeroinitializer, i32 %v.2, 0
%v.4 = insertvalue { i32, i64 } %tmp.0, i64 %v.3, 1
  %v.5 = call %zil_env_0 @zil.fn.0({ i32, i64 } %v.4)
  store %zil_env_0 %v.5, ptr %local.2, align 8
  %v.6 = call ptr @zil.fn.2(ptr %local.2)
  store ptr %v.6, ptr %local.3, align 8
  %v.7 = load ptr, ptr %local.3, align 8
  %v.8 = load i64, ptr %v.7, align 8
  %tmp.1 = icmp slt i64 %v.8, -2147483648
  %tmp.2 = icmp sgt i64 %v.8, 2147483647
  %tmp.3 = or i1 %tmp.1, %tmp.2
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.9 = trunc i64 %v.8 to i32
  %v.10 = add i32 0, 41
  %tmp.4 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define ptr @zil.fn.2(ptr %env) {
entry:
  %local.0 = alloca { i32, i64 }, align 8
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load { i32, i64 }, ptr %tmp.0, align 8
  store { i32, i64 } %v.0, ptr %local.0, align 8
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.2 = getelementptr { i32, i64 }, ptr %v.1, i32 0, i32 1
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  ret ptr %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

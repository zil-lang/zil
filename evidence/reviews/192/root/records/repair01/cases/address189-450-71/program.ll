; ZIL verified LLVM module
; verified-fingerprint: f99183cb2299e0cfa6b95c91279233391938ebd25d3bef68b2e01fea1f9778c2
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define ptr @zil.fn.0(ptr %arg.0) {
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

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca %zil_env_0, align 1
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 42
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.1, ptr %local.1, align 1
  %v.2 = load %zil_env_0, ptr %local.1, align 1
  %v.3 = getelementptr i8, ptr %local.0, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = call i32 @zil.fn.2(%zil_env_0 %v.2, ptr %v.5)
  %v.7 = add i32 0, 42
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.6, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(%zil_env_0 %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = call ptr @zil.fn.0(ptr %v.1)
  %v.3 = load i32, ptr %v.2, align 4
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

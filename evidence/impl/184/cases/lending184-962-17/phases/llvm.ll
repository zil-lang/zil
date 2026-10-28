; ZIL verified LLVM module
; verified-fingerprint: edd3f8b7fe054c20a9ec043564f861bd0674283b12ea618f1153c7175584621d
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define %zil_env_0 @zil.fn.0(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = insertvalue %zil_env_0 zeroinitializer, i32 %v.1, 0
  ret %zil_env_0 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca %zil_env_0, align 4
  %local.2 = alloca ptr, align 8
  %local.3 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 4
  %v.1 = call %zil_env_0 @zil.fn.0(i32 %v.0)
  store %zil_env_0 %v.1, ptr %local.0, align 4
  %v.2 = load %zil_env_0, ptr %local.0, align 4
  store %zil_env_0 %v.2, ptr %local.1, align 4
  %v.3 = call ptr @zil.fn.2(ptr %local.0)
  store ptr %v.3, ptr %local.2, align 8
  %v.4 = call ptr @zil.fn.2(ptr %local.1)
  store ptr %v.4, ptr %local.3, align 8
  %v.5 = load ptr, ptr %local.2, align 8
  %v.6 = load i32, ptr %v.5, align 4
  %v.7 = load ptr, ptr %local.3, align 8
  %v.8 = load i32, ptr %v.7, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.10 = add i32 0, 12
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define ptr @zil.fn.2(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  store ptr %v.2, ptr %local.1, align 8
  %v.3 = load ptr, ptr %local.1, align 8
  %v.4 = load ptr, ptr %local.1, align 8
  %v.5 = load i32, ptr %v.4, align 4
  %v.6 = add i32 0, 1
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.7, ptr %v.3, align 4
  %tmp.3 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.8 = load i32, ptr %tmp.3, align 4
  store i32 %v.8, ptr %local.2, align 4
  %v.9 = load i32, ptr %local.2, align 4
  %v.10 = add i32 0, 1
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.11, ptr %local.0, align 4
  %tmp.6 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store i32 %v.11, ptr %tmp.6, align 4
  %v.12 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.13 = getelementptr i8, ptr %v.12, i64 0
  ret ptr %v.13
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

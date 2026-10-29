; ZIL verified LLVM module
; verified-fingerprint: 0456ff457575d5adac5f86adf90436165edce3fafb944ab5a408d2a4c780efcc
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
  %v.0 = add i32 0, 0
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
  br label %bb.0
bb.0:
  %v.0 = call %zil_env_0 @zil.fn.0()
  store %zil_env_0 %v.0, ptr %local.0, align 4
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = add i32 0, 3
  %v.5 = call i32 @zil.fn.3(ptr %v.3, i32 %v.4)
  %v.6 = add i32 0, 3
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.7
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

define i32 @zil.fn.3(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  %local.5 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = add i32 0, 0
  %v.2 = load i32, ptr %local.1, align 4
  store i32 %v.1, ptr %local.3, align 4
  br label %bb.1
bb.1:
  %v.3 = load i32, ptr %local.3, align 4
  %v.4 = icmp slt i32 %v.3, %v.2
  br i1 %v.4, label %bb.2, label %bb.4
bb.2:
  store i32 %v.3, ptr %local.4, align 4
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = call ptr @zil.fn.2(ptr %v.5)
  store ptr %v.6, ptr %local.5, align 8
  %v.7 = load i32, ptr %local.2, align 4
  %v.8 = load ptr, ptr %local.5, align 8
  %v.9 = load i32, ptr %v.8, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.7, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.10, ptr %local.2, align 4
  %v.11 = load i32, ptr %local.4, align 4
  %v.12 = add i32 0, 1
  %v.13 = icmp eq i32 %v.11, %v.12
  br i1 %v.13, label %bb.5, label %bb.6
bb.3:
  %v.14 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.15, ptr %local.3, align 4
  br label %bb.1
bb.4:
  %v.16 = load i32, ptr %local.2, align 4
  ret i32 %v.16
bb.5:
  br label %bb.4
bb.6:
  br label %bb.7
bb.7:
  br label %bb.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

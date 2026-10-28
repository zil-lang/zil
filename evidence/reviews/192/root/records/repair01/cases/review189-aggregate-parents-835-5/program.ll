; ZIL verified LLVM module
; verified-fingerprint: 7e0530ba3a5a935c00eb0e17cb398d6c78cfe9cc86fe63eb49b8b3b624fdcb5c
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
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
  %local.1 = alloca i32, align 4
  %local.2 = alloca { i32, i32 }, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  %local.5 = alloca { i32, i32 }, align 4
  %local.6 = alloca ptr, align 8
  %local.7 = alloca ptr, align 8
  %local.8 = alloca %zil_env_0, align 1
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 2
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 3
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = load i32, ptr %local.1, align 4
%tmp.0 = insertvalue { i32, i32 } zeroinitializer, i32 %v.2, 0
%v.4 = insertvalue { i32, i32 } %tmp.0, i32 %v.3, 1
  store { i32, i32 } %v.4, ptr %local.2, align 4
  %v.5 = add i32 0, 5
  store i32 %v.5, ptr %local.3, align 4
  %v.6 = add i32 0, 7
  store i32 %v.6, ptr %local.4, align 4
  %v.7 = load i32, ptr %local.3, align 4
  %v.8 = load i32, ptr %local.4, align 4
%tmp.1 = insertvalue { i32, i32 } zeroinitializer, i32 %v.7, 0
%v.9 = insertvalue { i32, i32 } %tmp.1, i32 %v.8, 1
  store { i32, i32 } %v.9, ptr %local.5, align 4
  %v.10 = getelementptr i8, ptr %local.2, i64 0
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = getelementptr i8, ptr %v.11, i64 0
  %v.13 = call ptr @zil.fn.0(ptr %v.12)
  store ptr %v.13, ptr %local.6, align 8
  %v.14 = getelementptr i8, ptr %local.5, i64 0
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  %v.16 = getelementptr i8, ptr %v.15, i64 0
  %v.17 = call ptr @zil.fn.0(ptr %v.16)
  store ptr %v.17, ptr %local.7, align 8
  %v.18 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.18, ptr %local.8, align 1
  %v.19 = load %zil_env_0, ptr %local.8, align 1
  %v.20 = load ptr, ptr %local.6, align 8
  %v.21 = getelementptr i8, ptr %v.20, i64 0
  %v.22 = load ptr, ptr %local.7, align 8
  %v.23 = getelementptr i8, ptr %v.22, i64 0
  call void @zil.fn.2(%zil_env_0 %v.19, ptr %v.21, ptr %v.23)
  %v.25 = load %zil_env_0, ptr %local.8, align 1
  %v.26 = load ptr, ptr %local.6, align 8
  %v.27 = getelementptr i8, ptr %v.26, i64 0
  %v.28 = load ptr, ptr %local.7, align 8
  %v.29 = getelementptr i8, ptr %v.28, i64 0
  call void @zil.fn.2(%zil_env_0 %v.25, ptr %v.27, ptr %v.29)
  %v.31 = load { i32, i32 }, ptr %local.2, align 4
%v.32 = extractvalue { i32, i32 } %v.31, 0
  %v.33 = load { i32, i32 }, ptr %local.2, align 4
%v.34 = extractvalue { i32, i32 } %v.33, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.32, i32 %v.34)
  %v.35 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.36 = load { i32, i32 }, ptr %local.5, align 4
%v.37 = extractvalue { i32, i32 } %v.36, 0
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.35, i32 %v.37)
  %v.38 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  %v.39 = load { i32, i32 }, ptr %local.5, align 4
%v.40 = extractvalue { i32, i32 } %v.39, 1
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.38, i32 %v.40)
  %v.41 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.2
checked.2:
  %v.42 = add i32 0, 17
  %tmp.8 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.41, i32 %v.42)
  %v.43 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  ret i32 %v.43
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.2(%zil_env_0 %env, ptr %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32, i32 }, ptr %v.0, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = load ptr, ptr %local.0, align 8
  %v.5 = getelementptr { i32, i32 }, ptr %v.4, i32 0, i32 0
  %v.6 = load ptr, ptr %local.1, align 8
  %v.7 = getelementptr { i32, i32 }, ptr %v.6, i32 0, i32 1
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = load i32, ptr %v.8, align 4
  store i32 %v.9, ptr %v.5, align 4
  %v.10 = load ptr, ptr %local.1, align 8
  %v.11 = getelementptr { i32, i32 }, ptr %v.10, i32 0, i32 1
  %v.12 = load i32, ptr %local.2, align 4
  store i32 %v.12, ptr %v.11, align 4
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

; ZIL verified LLVM module
; verified-fingerprint: 6464443b01687c32110d438b146cc7f9570b4f6177d5c0df02d37ecb7b32945f
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
%zil_env_1 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca %zil_env_0, align 1
  %local.3 = alloca %zil_env_1, align 1
  %local.4 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 20
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 2
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.2, ptr %local.2, align 1
  %v.3 = freeze %zil_env_1 zeroinitializer
  store %zil_env_1 %v.3, ptr %local.3, align 1
  %v.4 = load %zil_env_0, ptr %local.2, align 1
  %v.5 = getelementptr i8, ptr %local.0, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = getelementptr i8, ptr %local.0, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = call i32 @zil.fn.1(%zil_env_0 %v.4, ptr %v.7, ptr %v.10)
  store i32 %v.11, ptr %local.4, align 4
  %v.12 = load %zil_env_1, ptr %local.3, align 1
  %v.13 = getelementptr i8, ptr %local.0, i64 0
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  %v.16 = getelementptr i8, ptr %local.1, i64 0
  %v.17 = getelementptr i8, ptr %v.16, i64 0
  %v.18 = getelementptr i8, ptr %v.17, i64 0
  call void @zil.fn.2(%zil_env_1 %v.12, ptr %v.15, ptr %v.18)
  %v.20 = load i32, ptr %local.4, align 4
  %v.21 = load i32, ptr %local.0, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.20, i32 %v.21)
  %v.22 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.23 = add i32 0, 62
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.22, i32 %v.23)
  %v.24 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.24
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(%zil_env_0 %env, ptr %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %v.0, align 4
  %v.2 = load ptr, ptr %local.1, align 8
  %v.3 = load i32, ptr %v.2, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.1, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.2(%zil_env_1 %env, ptr %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = load i32, ptr %v.1, align 4
  %v.3 = load ptr, ptr %local.1, align 8
  %v.4 = load i32, ptr %v.3, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.2, i32 %v.4)
  %v.5 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.5, ptr %v.0, align 4
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

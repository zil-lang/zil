; ZIL verified LLVM module
; verified-fingerprint: 39543fabbf0e39a51b313268e3a05ddd5964bd7ec1c27404ed0df610d3403288
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
  %local.1 = alloca ptr, align 8
  %local.2 = alloca %zil_env_0, align 1
  %local.3 = alloca %zil_env_1, align 1
  %local.4 = alloca i32, align 4
  %local.5 = alloca i32, align 4
  %local.6 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 20
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  store ptr %v.2, ptr %local.1, align 8
  %v.3 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.3, ptr %local.2, align 1
  %v.4 = freeze %zil_env_1 zeroinitializer
  store %zil_env_1 %v.4, ptr %local.3, align 1
  %v.5 = load %zil_env_0, ptr %local.2, align 1
  %v.6 = load ptr, ptr %local.1, align 8
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = call i32 @zil.fn.1(%zil_env_0 %v.5, ptr %v.7)
  store i32 %v.8, ptr %local.4, align 4
  %v.9 = load %zil_env_1, ptr %local.3, align 1
  %v.10 = load ptr, ptr %local.1, align 8
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = call i32 @zil.fn.2(%zil_env_1 %v.9, ptr %v.11)
  store i32 %v.12, ptr %local.5, align 4
  %v.13 = load %zil_env_1, ptr %local.3, align 1
  %v.14 = load ptr, ptr %local.1, align 8
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  %v.16 = call i32 @zil.fn.2(%zil_env_1 %v.13, ptr %v.15)
  store i32 %v.16, ptr %local.6, align 4
  %v.17 = load i32, ptr %local.4, align 4
  %v.18 = load i32, ptr %local.5, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.17, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.20 = load i32, ptr %local.6, align 4
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.19, i32 %v.20)
  %v.21 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %v.22 = load i32, ptr %local.0, align 4
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.21, i32 %v.22)
  %v.23 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.24 = add i32 0, 85
  %tmp.6 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.23, i32 %v.24)
  %v.25 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  ret i32 %v.25
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(%zil_env_0 %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %v.0, align 4
  ret i32 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(%zil_env_1 %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = load i32, ptr %v.1, align 4
  %v.3 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.4, ptr %v.0, align 4
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = load i32, ptr %v.5, align 4
  ret i32 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

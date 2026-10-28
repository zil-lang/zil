; ZIL verified LLVM module
; verified-fingerprint: f496277a1a18b2b11966906d8b9afcb950fd0e3e5caeac641e18bc2303ec7838
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca %zil_env_0, align 1
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = add i32 0, 1
  %v.5 = add i32 0, 0
  %v.6 = call i32 @zil.fn.3(ptr %v.3, i32 %v.4, i32 %v.5)
  %v.7 = getelementptr i8, ptr %local.0, i64 0
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = add i32 0, 2
  %v.11 = add i32 0, 1
  %v.12 = call i32 @zil.fn.3(ptr %v.9, i32 %v.10, i32 %v.11)
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.14 = getelementptr i8, ptr %local.0, i64 0
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  %v.16 = getelementptr i8, ptr %v.15, i64 0
  %v.17 = add i32 0, 3
  %v.18 = add i32 0, 7
  %v.19 = call i32 @zil.fn.3(ptr %v.16, i32 %v.17, i32 %v.18)
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.13, i32 %v.19)
  %v.20 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %v.21 = add i32 0, 14
  %tmp.4 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.20, i32 %v.21)
  %v.22 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.22
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(%zil_env_0 %env, i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.0, i32 %v.1)
  %v.2 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %local.1, align 4
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.2 = call i32 @zil.fn.1(%zil_env_0 %tmp.0, i32 %v.1)
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3(ptr %arg.0, i32 %arg.1, i32 %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  store i32 %arg.2, ptr %local.2, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.1, align 4
  store i32 %v.0, ptr %local.3, align 4
  %v.1 = load i32, ptr %local.2, align 4
  store i32 %v.1, ptr %local.4, align 4
  br label %bb.1
bb.1:
  %v.2 = load i32, ptr %local.4, align 4
  %v.3 = add i32 0, 0
  %v.4 = icmp sgt i32 %v.2, %v.3
  br i1 %v.4, label %bb.2, label %bb.3
bb.2:
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = load i32, ptr %local.3, align 4
  %v.8 = call i32 @zil.fn.2(ptr %v.6, i32 %v.7)
  store i32 %v.8, ptr %local.3, align 4
  %v.9 = load i32, ptr %local.4, align 4
  %v.10 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.11, ptr %local.4, align 4
  br label %bb.1
bb.3:
  %v.12 = load i32, ptr %local.3, align 4
  ret i32 %v.12
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

; ZIL verified LLVM module
; verified-fingerprint: 311ac7e17031470fcef5960c83a898bbe17c1d99b18be05b590d488b5e3ba182
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
  %v.4 = add i32 0, 3
  %v.5 = or i1 false, true
  %v.6 = call i32 @zil.fn.4(ptr %v.3, i32 %v.4, i1 %v.5)
  %v.7 = add i32 0, 4
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

define i32 @zil.fn.2(ptr %arg.0, i32 %arg.1, i1 %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i1, align 1
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  store i1 %arg.2, ptr %local.2, align 1
  br label %bb.0
bb.0:
  %v.0 = load i1, ptr %local.2, align 1
  br i1 %v.0, label %bb.1, label %bb.2
bb.1:
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = load i32, ptr %local.1, align 4
  %tmp.0 = load %zil_env_0, ptr %v.1, align 1
  %v.3 = call i32 @zil.fn.1(%zil_env_0 %tmp.0, i32 %v.2)
  ret i32 %v.3
bb.2:
  br label %bb.3
bb.3:
  %v.4 = add i32 0, 0
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3(ptr %arg.0, i32 %arg.1, i1 %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i1, align 1
  %local.3 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  store i1 %arg.2, ptr %local.2, align 1
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = load i32, ptr %local.1, align 4
  %v.3 = load i1, ptr %local.2, align 1
  %v.4 = call i32 @zil.fn.2(ptr %v.1, i32 %v.2, i1 %v.3)
  store i32 %v.4, ptr %local.3, align 4
  %v.5 = load i1, ptr %local.2, align 1
  br i1 %v.5, label %bb.1, label %bb.2
bb.1:
  %v.6 = load i32, ptr %local.3, align 4
  ret i32 %v.6
bb.2:
  br label %bb.3
bb.3:
  %v.7 = load ptr, ptr %local.0, align 8
  %v.8 = load i32, ptr %local.1, align 4
  %v.9 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.8, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %tmp.2 = load %zil_env_0, ptr %v.7, align 1
  %v.11 = call i32 @zil.fn.1(%zil_env_0 %tmp.2, i32 %v.10)
  %v.12 = load ptr, ptr %local.0, align 8
  %v.13 = load i32, ptr %local.1, align 4
  %v.14 = add i32 0, 2
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %tmp.5 = load %zil_env_0, ptr %v.12, align 1
  %v.16 = call i32 @zil.fn.1(%zil_env_0 %tmp.5, i32 %v.15)
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.16)
  %v.17 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.17
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4(ptr %arg.0, i32 %arg.1, i1 %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i1, align 1
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  store i1 %arg.2, ptr %local.2, align 1
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = load i32, ptr %local.1, align 4
  %v.3 = load i1, ptr %local.2, align 1
  %v.4 = call i32 @zil.fn.3(ptr %v.1, i32 %v.2, i1 %v.3)
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = load i32, ptr %local.1, align 4
  %v.8 = load i1, ptr %local.2, align 1
  %v.9 = xor i1 %v.8, true
  %v.10 = call i32 @zil.fn.2(ptr %v.6, i32 %v.7, i1 %v.9)
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

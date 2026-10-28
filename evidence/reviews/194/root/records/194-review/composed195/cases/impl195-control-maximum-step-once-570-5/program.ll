; ZIL verified LLVM module
; verified-fingerprint: 77a37c47e1275f02963dda807dc0cc0ff2fcc8ce3e8ef9289350e12f61c9d0b1
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.1(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call i32 @putchar(i32 %v.0)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.2(i64 %arg.0) {
entry:
  %local.0 = alloca i64, align 8
  store i64 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 76
  call void @zil.fn.1(i32 %v.0)
  %v.2 = load i64, ptr %local.0, align 8
  ret i64 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.3(i64 %arg.0) {
entry:
  %local.0 = alloca i64, align 8
  store i64 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 72
  call void @zil.fn.1(i32 %v.0)
  %v.2 = load i64, ptr %local.0, align 8
  ret i64 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4() {
entry:
  %local.0 = alloca %zil_env_0, align 1
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = call i32 @zil.fn.6(ptr %v.3)
  %v.5 = add i32 0, 2
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(%zil_env_0 %env, i64 %arg.0) {
entry:
  %local.0 = alloca i64, align 8
  store i64 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load i64, ptr %local.0, align 8
  %v.1 = add i64 0, 9223372036854775805
  %v.2 = icmp eq i64 %v.0, %v.1
  br i1 %v.2, label %bb.1, label %bb.2
bb.1:
  %v.3 = add i32 0, 65
  call void @zil.fn.1(i32 %v.3)
  br label %bb.3
bb.2:
  %v.5 = add i32 0, 66
  call void @zil.fn.1(i32 %v.5)
  br label %bb.3
bb.3:
  %v.7 = add i32 0, 1
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  %local.2 = alloca i32, align 4
  %local.3 = alloca i64, align 8
  %local.4 = alloca i64, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 7
  store i64 %v.0, ptr %local.1, align 8
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = add i64 0, 9223372036854775805
  %v.3 = call i64 @zil.fn.2(i64 %v.2)
  %v.4 = add i64 0, 9223372036854775807
  %v.5 = call i64 @zil.fn.3(i64 %v.4)
  store i64 %v.3, ptr %local.3, align 8
  br label %bb.1
bb.1:
  %v.6 = load i64, ptr %local.3, align 8
  %v.7 = icmp slt i64 %v.6, %v.5
  br i1 %v.7, label %bb.2, label %bb.4
bb.2:
  store i64 %v.6, ptr %local.4, align 8
  %v.8 = load i32, ptr %local.2, align 4
  %v.9 = load ptr, ptr %local.0, align 8
  %v.10 = load i64, ptr %local.4, align 8
  %tmp.0 = load %zil_env_0, ptr %v.9, align 1
  %v.11 = call i32 @zil.fn.5(%zil_env_0 %tmp.0, i64 %v.10)
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.8, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.12, ptr %local.2, align 4
  br label %bb.3
bb.3:
  %v.13 = add i64 0, 1
  %tmp.3 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.6, i64 %v.13)
  %v.14 = extractvalue { i64, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i64, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  store i64 %v.14, ptr %local.3, align 8
  br label %bb.1
bb.4:
  %v.15 = load i64, ptr %local.1, align 8
  %tmp.5 = icmp slt i64 %v.15, -2147483648
  %tmp.6 = icmp sgt i64 %v.15, 2147483647
  %tmp.7 = or i1 %tmp.5, %tmp.6
  br i1 %tmp.7, label %zil.trap, label %checked.2
checked.2:
  %v.16 = trunc i64 %v.15 to i32
  %v.17 = add i32 0, 48
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.16, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  call void @zil.fn.1(i32 %v.18)
  %v.20 = load i32, ptr %local.2, align 4
  ret i32 %v.20
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.4()
  ret i32 %entry.result
}

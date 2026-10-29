; ZIL verified LLVM module
; verified-fingerprint: f9f6ab625c64e67d32bbd509f72bf002e9a27650f256751f2c1dad75ddd803ce
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
  %v.4 = add i32 0, 5
  %v.5 = call i32 @zil.fn.2(ptr %v.3, i32 %v.4)
  %v.6 = add i32 0, 6
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

define i32 @zil.fn.1(%zil_env_0 %env) {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 3
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.3, align 4
  br label %bb.1
bb.1:
  %v.2 = load i32, ptr %local.2, align 4
  %v.3 = load i32, ptr %local.1, align 4
  %v.4 = icmp slt i32 %v.2, %v.3
  br i1 %v.4, label %bb.2, label %bb.3
bb.2:
  %v.5 = load i32, ptr %local.2, align 4
  %v.6 = add i32 0, 2
  %v.7 = icmp eq i32 %v.5, %v.6
  br i1 %v.7, label %bb.4, label %bb.5
bb.3:
  %v.15 = load i32, ptr %local.3, align 4
  ret i32 %v.15
bb.4:
  br label %bb.3
bb.5:
  br label %bb.6
bb.6:
  %v.8 = load i32, ptr %local.3, align 4
  %v.9 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.9, align 1
  %v.10 = call i32 @zil.fn.1(%zil_env_0 %tmp.0)
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.8, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.11, ptr %local.3, align 4
  %v.12 = load i32, ptr %local.2, align 4
  %v.13 = add i32 0, 1
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.12, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.14, ptr %local.2, align 4
  br label %bb.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

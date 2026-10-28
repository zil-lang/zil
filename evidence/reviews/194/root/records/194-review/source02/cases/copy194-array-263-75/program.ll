; ZIL verified LLVM module
; verified-fingerprint: e3a79ba36a9dbf053cd6e73dfb0b4582d1a5a7d0e333f0839dc80aad2269e0e4
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
  %local.1 = alloca [2 x i32], align 4
  %index.slot.10 = alloca [2 x i32], align 4
  %index.slot.13 = alloca [2 x i32], align 4
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = add i32 0, 3
  %v.5 = add i32 0, 4
%tmp.0 = insertvalue [2 x i32] zeroinitializer, i32 %v.4, 0
%v.6 = insertvalue [2 x i32] %tmp.0, i32 %v.5, 1
  %v.7 = call [2 x i32] @zil.fn.2(ptr %v.3, [2 x i32] %v.6)
  store [2 x i32] %v.7, ptr %local.1, align 4
  %v.8 = load [2 x i32], ptr %local.1, align 4
  %v.9 = add i32 0, 0
  %tmp.1 = sext i32 %v.9 to i64
  %tmp.2 = icmp ult i64 %tmp.1, 2
  br i1 %tmp.2, label %index.ok.0, label %zil.trap
index.ok.0:
  store [2 x i32] %v.8, ptr %index.slot.10, align 4
  %tmp.3 = getelementptr [2 x i32], ptr %index.slot.10, i64 0, i64 %tmp.1
  %v.10 = load i32, ptr %tmp.3, align 4
  %v.11 = load [2 x i32], ptr %local.1, align 4
  %v.12 = add i32 0, 1
  %tmp.4 = sext i32 %v.12 to i64
  %tmp.5 = icmp ult i64 %tmp.4, 2
  br i1 %tmp.5, label %index.ok.1, label %zil.trap
index.ok.1:
  store [2 x i32] %v.11, ptr %index.slot.13, align 4
  %tmp.6 = getelementptr [2 x i32], ptr %index.slot.13, i64 0, i64 %tmp.4
  %v.13 = load i32, ptr %tmp.6, align 4
  %tmp.7 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.10, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  %v.15 = add i32 0, 7
  %tmp.9 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.14, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.9, 0
  %tmp.10 = extractvalue { i32, i1 } %tmp.9, 1
  br i1 %tmp.10, label %zil.trap, label %checked.3
checked.3:
  ret i32 %v.16
zil.trap:
  call void @llvm.trap()
  unreachable
}

define [2 x i32] @zil.fn.1(%zil_env_0 %env, [2 x i32] %arg.0) {
entry:
  %local.0 = alloca [2 x i32], align 4
  store [2 x i32] %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load [2 x i32], ptr %local.0, align 4
  ret [2 x i32] %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define [2 x i32] @zil.fn.2(ptr %arg.0, [2 x i32] %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca [2 x i32], align 4
  store ptr %arg.0, ptr %local.0, align 8
  store [2 x i32] %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load [2 x i32], ptr %local.1, align 4
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.2 = call [2 x i32] @zil.fn.1(%zil_env_0 %tmp.0, [2 x i32] %v.1)
  ret [2 x i32] %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

; ZIL verified LLVM module
; verified-fingerprint: 37e4cf10c87c2296d147e250669a34b892db454d34ee20b5bad24d4f5c1bb086
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i64, i1 } @llvm.ssub.with.overflow.i64(i64, i64)

define ptr @zil.fn.0({ ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { ptr, i64 }, ptr %local.0, align 8
  %v.1 = add i32 0, 0
  %tmp.0 = extractvalue { ptr, i64 } %v.0, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.0, 1
  %tmp.2 = sext i32 %v.1 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.2 = getelementptr i64, ptr %tmp.0, i64 %tmp.2
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  ret ptr %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca [2 x i64], align 8
  %local.1 = alloca %zil_env_0, align 1
  %local.2 = alloca i64, align 8
  %index.slot.18 = alloca [2 x i64], align 8
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 5000000000
  %v.1 = add i64 0, 2
%tmp.0 = insertvalue [2 x i64] zeroinitializer, i64 %v.0, 0
%v.2 = insertvalue [2 x i64] %tmp.0, i64 %v.1, 1
  store [2 x i64] %v.2, ptr %local.0, align 8
  %v.3 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.3, ptr %local.1, align 1
  %v.4 = getelementptr i8, ptr %local.1, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = getelementptr i8, ptr %local.0, i64 0
  %tmp.1 = icmp ugt i64 0, 2
  %tmp.2 = icmp ugt i64 2, 2
  %tmp.3 = or i1 %tmp.1, %tmp.2
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %tmp.4 = getelementptr i64, ptr %v.7, i64 0
  %tmp.5 = sub i64 2, 0
  %tmp.6 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.4, 0
  %v.8 = insertvalue { ptr, i64 } %tmp.6, i64 %tmp.5, 1
  %tmp.7 = extractvalue { ptr, i64 } %v.8, 0
  %v.9 = insertvalue { ptr, i64 } %v.8, ptr %tmp.7, 0
  %tmp.8 = extractvalue { ptr, i64 } %v.9, 0
  %v.10 = insertvalue { ptr, i64 } %v.9, ptr %tmp.8, 0
  %v.11 = call i64 @zil.fn.4(ptr %v.6, { ptr, i64 } %v.10)
  store i64 %v.11, ptr %local.2, align 8
  %v.12 = load i64, ptr %local.2, align 8
  %v.13 = add i64 0, 10000000006
  %tmp.9 = call { i64, i1 } @llvm.ssub.with.overflow.i64(i64 %v.12, i64 %v.13)
  %v.14 = extractvalue { i64, i1 } %tmp.9, 0
  %tmp.10 = extractvalue { i64, i1 } %tmp.9, 1
  br i1 %tmp.10, label %zil.trap, label %checked.1
checked.1:
  %tmp.11 = icmp slt i64 %v.14, -2147483648
  %tmp.12 = icmp sgt i64 %v.14, 2147483647
  %tmp.13 = or i1 %tmp.11, %tmp.12
  br i1 %tmp.13, label %zil.trap, label %checked.2
checked.2:
  %v.15 = trunc i64 %v.14 to i32
  %v.16 = load [2 x i64], ptr %local.0, align 8
  %v.17 = add i32 0, 0
  %tmp.14 = sext i32 %v.17 to i64
  %tmp.15 = icmp ult i64 %tmp.14, 2
  br i1 %tmp.15, label %index.ok.3, label %zil.trap
index.ok.3:
  store [2 x i64] %v.16, ptr %index.slot.18, align 8
  %tmp.16 = getelementptr [2 x i64], ptr %index.slot.18, i64 0, i64 %tmp.14
  %v.18 = load i64, ptr %tmp.16, align 8
  %v.19 = add i64 0, 5000000004
  %tmp.17 = call { i64, i1 } @llvm.ssub.with.overflow.i64(i64 %v.18, i64 %v.19)
  %v.20 = extractvalue { i64, i1 } %tmp.17, 0
  %tmp.18 = extractvalue { i64, i1 } %tmp.17, 1
  br i1 %tmp.18, label %zil.trap, label %checked.4
checked.4:
  %tmp.19 = icmp slt i64 %v.20, -2147483648
  %tmp.20 = icmp sgt i64 %v.20, 2147483647
  %tmp.21 = or i1 %tmp.19, %tmp.20
  br i1 %tmp.21, label %zil.trap, label %checked.5
checked.5:
  %v.21 = trunc i64 %v.20 to i32
  %tmp.22 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.15, i32 %v.21)
  %v.22 = extractvalue { i32, i1 } %tmp.22, 0
  %tmp.23 = extractvalue { i32, i1 } %tmp.22, 1
  br i1 %tmp.23, label %zil.trap, label %checked.6
checked.6:
  ret i32 %v.22
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.2(%zil_env_0 %env, { ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %local.1 = alloca i64, align 8
  %local.2 = alloca ptr, align 8
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { ptr, i64 }, ptr %local.0, align 8
  %v.1 = add i32 0, 1
  %tmp.0 = extractvalue { ptr, i64 } %v.0, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.0, 1
  %tmp.2 = sext i32 %v.1 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.2 = getelementptr i64, ptr %tmp.0, i64 %tmp.2
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = load i64, ptr %v.3, align 8
  store i64 %v.4, ptr %local.1, align 8
  %v.5 = load { ptr, i64 }, ptr %local.0, align 8
  %tmp.4 = extractvalue { ptr, i64 } %v.5, 0
  %v.6 = insertvalue { ptr, i64 } %v.5, ptr %tmp.4, 0
  %v.7 = call ptr @zil.fn.0({ ptr, i64 } %v.6)
  store ptr %v.7, ptr %local.2, align 8
  %v.8 = load ptr, ptr %local.2, align 8
  %v.9 = load ptr, ptr %local.2, align 8
  %v.10 = load i64, ptr %v.9, align 8
  %v.11 = load i64, ptr %local.1, align 8
  %tmp.5 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.10, i64 %v.11)
  %v.12 = extractvalue { i64, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i64, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.1
checked.1:
  store i64 %v.12, ptr %v.8, align 8
  %v.13 = load ptr, ptr %local.2, align 8
  %v.14 = load i64, ptr %v.13, align 8
  ret i64 %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.3(ptr %arg.0, { ptr, i64 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { ptr, i64 } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.1, 0
  %v.2 = insertvalue { ptr, i64 } %v.1, ptr %tmp.0, 0
  %tmp.1 = load %zil_env_0, ptr %v.0, align 1
  %v.3 = call i64 @zil.fn.2(%zil_env_0 %tmp.1, { ptr, i64 } %v.2)
  ret i64 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.4(ptr %arg.0, { ptr, i64 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { ptr, i64 } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.2, 0
  %v.3 = insertvalue { ptr, i64 } %v.2, ptr %tmp.0, 0
  %v.4 = call i64 @zil.fn.3(ptr %v.1, { ptr, i64 } %v.3)
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.1 = extractvalue { ptr, i64 } %v.7, 0
  %v.8 = insertvalue { ptr, i64 } %v.7, ptr %tmp.1, 0
  %v.9 = call i64 @zil.fn.3(ptr %v.6, { ptr, i64 } %v.8)
  %tmp.2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.4, i64 %v.9)
  %v.10 = extractvalue { i64, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i64, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  ret i64 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

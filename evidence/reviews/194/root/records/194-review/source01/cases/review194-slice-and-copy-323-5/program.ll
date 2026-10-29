; ZIL verified LLVM module
; verified-fingerprint: 4c422175cac258a577de71f6466b733a92f08be262701843732322bf61d31b49
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca [2 x i64], align 8
  %local.1 = alloca %zil_env_0, align 1
  %local.2 = alloca i64, align 8
  %index.slot.17 = alloca [2 x i64], align 8
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 1
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
  %v.11 = add i32 0, 3
  %v.12 = sext i32 %v.11 to i64
  %v.13 = call i64 @zil.fn.3(ptr %v.6, { ptr, i64 } %v.10, i64 %v.12)
  store i64 %v.13, ptr %local.2, align 8
  %v.14 = load i64, ptr %local.2, align 8
  %v.15 = load [2 x i64], ptr %local.0, align 8
  %v.16 = add i32 0, 0
  %tmp.9 = sext i32 %v.16 to i64
  %tmp.10 = icmp ult i64 %tmp.9, 2
  br i1 %tmp.10, label %index.ok.1, label %zil.trap
index.ok.1:
  store [2 x i64] %v.15, ptr %index.slot.17, align 8
  %tmp.11 = getelementptr [2 x i64], ptr %index.slot.17, i64 0, i64 %tmp.9
  %v.17 = load i64, ptr %tmp.11, align 8
  %tmp.12 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.14, i64 %v.17)
  %v.18 = extractvalue { i64, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i64, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.2
checked.2:
  %tmp.14 = icmp slt i64 %v.18, -2147483648
  %tmp.15 = icmp sgt i64 %v.18, 2147483647
  %tmp.16 = or i1 %tmp.14, %tmp.15
  br i1 %tmp.16, label %zil.trap, label %checked.3
checked.3:
  %v.19 = trunc i64 %v.18 to i32
  %v.20 = add i32 0, 16
  %tmp.17 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.19, i32 %v.20)
  %v.21 = extractvalue { i32, i1 } %tmp.17, 0
  %tmp.18 = extractvalue { i32, i1 } %tmp.17, 1
  br i1 %tmp.18, label %zil.trap, label %checked.4
checked.4:
  ret i32 %v.21
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.1(%zil_env_0 %env, { ptr, i64 } %arg.0, i64 %arg.1) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %local.1 = alloca i64, align 8
  %local.2 = alloca i32, align 4
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = load { ptr, i64 }, ptr %local.0, align 8
  %v.2 = load i32, ptr %local.2, align 4
  %tmp.0 = extractvalue { ptr, i64 } %v.1, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.1, 1
  %tmp.2 = sext i32 %v.2 to i64
  %tmp.3 = icmp uge i64 %tmp.2, %tmp.1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.3 = getelementptr i64, ptr %tmp.0, i64 %tmp.2
  %v.4 = load { ptr, i64 }, ptr %local.0, align 8
  %v.5 = load i32, ptr %local.2, align 4
  %tmp.4 = extractvalue { ptr, i64 } %v.4, 0
  %tmp.5 = extractvalue { ptr, i64 } %v.4, 1
  %tmp.6 = sext i32 %v.5 to i64
  %tmp.7 = icmp uge i64 %tmp.6, %tmp.5
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.6 = getelementptr i64, ptr %tmp.4, i64 %tmp.6
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = load i64, ptr %v.7, align 8
  %v.9 = load i64, ptr %local.1, align 8
  %tmp.8 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.8, i64 %v.9)
  %v.10 = extractvalue { i64, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i64, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  store i64 %v.10, ptr %v.3, align 8
  %v.11 = load { ptr, i64 }, ptr %local.0, align 8
  %v.12 = add i32 0, 0
  %tmp.10 = extractvalue { ptr, i64 } %v.11, 0
  %tmp.11 = extractvalue { ptr, i64 } %v.11, 1
  %tmp.12 = sext i32 %v.12 to i64
  %tmp.13 = icmp uge i64 %tmp.12, %tmp.11
  br i1 %tmp.13, label %zil.trap, label %checked.3
checked.3:
  %v.13 = getelementptr i64, ptr %tmp.10, i64 %tmp.12
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  %v.15 = load i64, ptr %v.14, align 8
  ret i64 %v.15
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.2(ptr %arg.0, { ptr, i64 } %arg.1, i64 %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  %local.2 = alloca i64, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { ptr, i64 } %arg.1, ptr %local.1, align 8
  store i64 %arg.2, ptr %local.2, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.1, 0
  %v.2 = insertvalue { ptr, i64 } %v.1, ptr %tmp.0, 0
  %v.3 = load i64, ptr %local.2, align 8
  %tmp.1 = load %zil_env_0, ptr %v.0, align 1
  %v.4 = call i64 @zil.fn.1(%zil_env_0 %tmp.1, { ptr, i64 } %v.2, i64 %v.3)
  ret i64 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.3(ptr %arg.0, { ptr, i64 } %arg.1, i64 %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  %local.2 = alloca i64, align 8
  %local.3 = alloca i64, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { ptr, i64 } %arg.1, ptr %local.1, align 8
  store i64 %arg.2, ptr %local.2, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %v.0, i64 0
  %v.2 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.2, 0
  %v.3 = insertvalue { ptr, i64 } %v.2, ptr %tmp.0, 0
  %v.4 = load i64, ptr %local.2, align 8
  %v.5 = call i64 @zil.fn.2(ptr %v.1, { ptr, i64 } %v.3, i64 %v.4)
  store i64 %v.5, ptr %local.3, align 8
  %v.6 = load ptr, ptr %local.0, align 8
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.1 = extractvalue { ptr, i64 } %v.8, 0
  %v.9 = insertvalue { ptr, i64 } %v.8, ptr %tmp.1, 0
  %v.10 = load i64, ptr %local.3, align 8
  %v.11 = call i64 @zil.fn.2(ptr %v.7, { ptr, i64 } %v.9, i64 %v.10)
  ret i64 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

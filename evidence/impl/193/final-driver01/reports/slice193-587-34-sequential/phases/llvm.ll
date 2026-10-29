; ZIL verified LLVM module
; verified-fingerprint: f03f1528edf80ba1b775165f1b682d053f97f9ac2e59576f72ea4238df2800eb
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
  %local.0 = alloca [2 x i32], align 4
  %local.1 = alloca { ptr, i64 }, align 8
  %local.2 = alloca %zil_env_0, align 1
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  %local.5 = alloca %zil_env_1, align 1
  %index.slot.26 = alloca [2 x i32], align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 20
  %v.1 = add i32 0, 21
%tmp.0 = insertvalue [2 x i32] zeroinitializer, i32 %v.0, 0
%v.2 = insertvalue [2 x i32] %tmp.0, i32 %v.1, 1
  store [2 x i32] %v.2, ptr %local.0, align 4
  %v.3 = getelementptr i8, ptr %local.0, i64 0
  %tmp.1 = icmp ugt i64 0, 2
  %tmp.2 = icmp ugt i64 2, 2
  %tmp.3 = or i1 %tmp.1, %tmp.2
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %tmp.4 = getelementptr i32, ptr %v.3, i64 0
  %tmp.5 = sub i64 2, 0
  %tmp.6 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.4, 0
  %v.4 = insertvalue { ptr, i64 } %tmp.6, i64 %tmp.5, 1
  %tmp.7 = extractvalue { ptr, i64 } %v.4, 0
  %v.5 = insertvalue { ptr, i64 } %v.4, ptr %tmp.7, 0
  store { ptr, i64 } %v.5, ptr %local.1, align 8
  %v.6 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.6, ptr %local.2, align 1
  %v.7 = load %zil_env_0, ptr %local.2, align 1
  %v.8 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.8 = extractvalue { ptr, i64 } %v.8, 0
  %v.9 = insertvalue { ptr, i64 } %v.8, ptr %tmp.8, 0
  %v.10 = call i32 @zil.fn.1(%zil_env_0 %v.7, { ptr, i64 } %v.9)
  store i32 %v.10, ptr %local.3, align 4
  %v.11 = load %zil_env_0, ptr %local.2, align 1
  %v.12 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.9 = extractvalue { ptr, i64 } %v.12, 0
  %v.13 = insertvalue { ptr, i64 } %v.12, ptr %tmp.9, 0
  %v.14 = call i32 @zil.fn.1(%zil_env_0 %v.11, { ptr, i64 } %v.13)
  store i32 %v.14, ptr %local.4, align 4
  %v.15 = freeze %zil_env_1 zeroinitializer
  store %zil_env_1 %v.15, ptr %local.5, align 1
  %v.16 = load i32, ptr %local.3, align 4
  %v.17 = load i32, ptr %local.4, align 4
  %tmp.10 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.16, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.1
checked.1:
  %v.19 = load %zil_env_1, ptr %local.5, align 1
  %v.20 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.12 = extractvalue { ptr, i64 } %v.20, 0
  %v.21 = insertvalue { ptr, i64 } %v.20, ptr %tmp.12, 0
  %v.22 = call i32 @zil.fn.2(%zil_env_1 %v.19, { ptr, i64 } %v.21)
  %tmp.13 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.18, i32 %v.22)
  %v.23 = extractvalue { i32, i1 } %tmp.13, 0
  %tmp.14 = extractvalue { i32, i1 } %tmp.13, 1
  br i1 %tmp.14, label %zil.trap, label %checked.2
checked.2:
  %v.24 = load [2 x i32], ptr %local.0, align 4
  %v.25 = add i32 0, 1
  %tmp.15 = sext i32 %v.25 to i64
  %tmp.16 = icmp ult i64 %tmp.15, 2
  br i1 %tmp.16, label %index.ok.3, label %zil.trap
index.ok.3:
  store [2 x i32] %v.24, ptr %index.slot.26, align 4
  %tmp.17 = getelementptr [2 x i32], ptr %index.slot.26, i64 0, i64 %tmp.15
  %v.26 = load i32, ptr %tmp.17, align 4
  %tmp.18 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.23, i32 %v.26)
  %v.27 = extractvalue { i32, i1 } %tmp.18, 0
  %tmp.19 = extractvalue { i32, i1 } %tmp.18, 1
  br i1 %tmp.19, label %zil.trap, label %checked.4
checked.4:
  %v.28 = add i32 0, 111
  %tmp.20 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.27, i32 %v.28)
  %v.29 = extractvalue { i32, i1 } %tmp.20, 0
  %tmp.21 = extractvalue { i32, i1 } %tmp.20, 1
  br i1 %tmp.21, label %zil.trap, label %checked.5
checked.5:
  ret i32 %v.29
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(%zil_env_0 %env, { ptr, i64 } %arg.0) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  %local.2 = alloca i32, align 4
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { ptr, i64 }, ptr %local.0, align 8
  %v.1 = add i32 0, 1
  %tmp.0 = extractvalue { ptr, i64 } %v.0, 0
  %tmp.1 = extractvalue { ptr, i64 } %v.0, 1
  %tmp.2 = sext i32 %v.1 to i64
  %tmp.3 = icmp ugt i64 %tmp.2, %tmp.1
  %tmp.4 = icmp ugt i64 %tmp.1, %tmp.1
  %tmp.5 = or i1 %tmp.3, %tmp.4
  br i1 %tmp.5, label %zil.trap, label %checked.0
checked.0:
  %tmp.6 = getelementptr i32, ptr %tmp.0, i64 %tmp.2
  %tmp.7 = sub i64 %tmp.1, %tmp.2
  %tmp.8 = insertvalue { ptr, i64 } zeroinitializer, ptr %tmp.6, 0
  %v.2 = insertvalue { ptr, i64 } %tmp.8, i64 %tmp.7, 1
  %tmp.9 = extractvalue { ptr, i64 } %v.2, 0
  %v.3 = insertvalue { ptr, i64 } %v.2, ptr %tmp.9, 0
  store { ptr, i64 } %v.3, ptr %local.1, align 8
  %v.4 = add i32 0, 0
  store i32 %v.4, ptr %local.2, align 4
  %v.5 = load { ptr, i64 }, ptr %local.1, align 8
  %v.6 = load i32, ptr %local.2, align 4
  %tmp.10 = extractvalue { ptr, i64 } %v.5, 0
  %tmp.11 = extractvalue { ptr, i64 } %v.5, 1
  %tmp.12 = sext i32 %v.6 to i64
  %tmp.13 = icmp uge i64 %tmp.12, %tmp.11
  br i1 %tmp.13, label %zil.trap, label %checked.1
checked.1:
  %v.7 = getelementptr i32, ptr %tmp.10, i64 %tmp.12
  %v.8 = load { ptr, i64 }, ptr %local.1, align 8
  %v.9 = load i32, ptr %local.2, align 4
  %tmp.14 = extractvalue { ptr, i64 } %v.8, 0
  %tmp.15 = extractvalue { ptr, i64 } %v.8, 1
  %tmp.16 = sext i32 %v.9 to i64
  %tmp.17 = icmp uge i64 %tmp.16, %tmp.15
  br i1 %tmp.17, label %zil.trap, label %checked.2
checked.2:
  %v.10 = getelementptr i32, ptr %tmp.14, i64 %tmp.16
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = load i32, ptr %v.11, align 4
  %v.13 = add i32 0, 1
  %tmp.18 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.12, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.18, 0
  %tmp.19 = extractvalue { i32, i1 } %tmp.18, 1
  br i1 %tmp.19, label %zil.trap, label %checked.3
checked.3:
  store i32 %v.14, ptr %v.7, align 4
  %v.15 = load { ptr, i64 }, ptr %local.1, align 8
  %v.16 = add i32 0, 0
  %tmp.20 = extractvalue { ptr, i64 } %v.15, 0
  %tmp.21 = extractvalue { ptr, i64 } %v.15, 1
  %tmp.22 = sext i32 %v.16 to i64
  %tmp.23 = icmp uge i64 %tmp.22, %tmp.21
  br i1 %tmp.23, label %zil.trap, label %checked.4
checked.4:
  %v.17 = getelementptr i32, ptr %tmp.20, i64 %tmp.22
  %v.18 = getelementptr i8, ptr %v.17, i64 0
  %v.19 = load i32, ptr %v.18, align 4
  ret i32 %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(%zil_env_1 %env, { ptr, i64 } %arg.0) {
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
  %v.2 = getelementptr i32, ptr %tmp.0, i64 %tmp.2
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = load i32, ptr %v.3, align 4
  %v.5 = load { ptr, i64 }, ptr %local.0, align 8
  %v.6 = add i32 0, 1
  %tmp.4 = extractvalue { ptr, i64 } %v.5, 0
  %tmp.5 = extractvalue { ptr, i64 } %v.5, 1
  %tmp.6 = sext i32 %v.6 to i64
  %tmp.7 = icmp uge i64 %tmp.6, %tmp.5
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.7 = getelementptr i32, ptr %tmp.4, i64 %tmp.6
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = load i32, ptr %v.8, align 4
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

; ZIL verified LLVM module
; verified-fingerprint: 3355951df14b8b29820ca71d33fa29a2189449349d11fc308bed09493e916c2f
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
  %local.1 = alloca i32, align 4
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca { i32 }, align 4
  %local.5 = alloca i32, align 4
  %local.6 = alloca { i32 }, align 4
  %local.7 = alloca i32, align 4
  %local.8 = alloca i64, align 8
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = add i32 0, 3
  store i32 %v.4, ptr %local.1, align 4
  %v.5 = load i32, ptr %local.1, align 4
%v.6 = insertvalue { i32 } zeroinitializer, i32 %v.5, 0
  %v.7 = call { i32 } @zil.fn.2(ptr %v.3, { i32 } %v.6)
  store { i32 } %v.7, ptr %local.2, align 4
  %v.8 = getelementptr i8, ptr %local.0, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = add i32 0, 5
  store i32 %v.11, ptr %local.3, align 4
  %v.12 = load i32, ptr %local.3, align 4
%v.13 = insertvalue { i32 } zeroinitializer, i32 %v.12, 0
  %v.14 = call { i32 } @zil.fn.3(ptr %v.10, { i32 } %v.13)
  store { i32 } %v.14, ptr %local.4, align 4
  %v.15 = getelementptr i8, ptr %local.0, i64 0
  %v.16 = getelementptr i8, ptr %v.15, i64 0
  %v.17 = getelementptr i8, ptr %v.16, i64 0
  %v.18 = add i32 0, 7
  store i32 %v.18, ptr %local.5, align 4
  %v.19 = load i32, ptr %local.5, align 4
%v.20 = insertvalue { i32 } zeroinitializer, i32 %v.19, 0
  %v.21 = call { i32 } @zil.fn.2(ptr %v.17, { i32 } %v.20)
  store { i32 } %v.21, ptr %local.6, align 4
  %v.22 = getelementptr i8, ptr %local.0, i64 0
  %v.23 = getelementptr i8, ptr %v.22, i64 0
  %v.24 = getelementptr i8, ptr %v.23, i64 0
  %v.25 = add i32 0, 11
  %v.26 = add i32 %v.25, 0
  %v.27 = call i32 @zil.fn.4(ptr %v.24, i32 %v.26)
  store i32 %v.27, ptr %local.7, align 4
  %v.28 = getelementptr i8, ptr %local.0, i64 0
  %v.29 = getelementptr i8, ptr %v.28, i64 0
  %v.30 = getelementptr i8, ptr %v.29, i64 0
  %v.31 = add i32 0, 13
  %v.32 = sext i32 %v.31 to i64
  %v.33 = call i64 @zil.fn.5(ptr %v.30, i64 %v.32)
  store i64 %v.33, ptr %local.8, align 8
  %v.34 = load { i32 }, ptr %local.2, align 4
%v.35 = extractvalue { i32 } %v.34, 0
  %v.36 = load { i32 }, ptr %local.4, align 4
%v.37 = extractvalue { i32 } %v.36, 0
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.35, i32 %v.37)
  %v.38 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.39 = load { i32 }, ptr %local.6, align 4
%v.40 = extractvalue { i32 } %v.39, 0
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.38, i32 %v.40)
  %v.41 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %v.42 = load i32, ptr %local.7, align 4
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.41, i32 %v.42)
  %v.43 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.44 = load i64, ptr %local.8, align 8
  %tmp.6 = icmp slt i64 %v.44, -2147483648
  %tmp.7 = icmp sgt i64 %v.44, 2147483647
  %tmp.8 = or i1 %tmp.6, %tmp.7
  br i1 %tmp.8, label %zil.trap, label %checked.3
checked.3:
  %v.45 = trunc i64 %v.44 to i32
  %tmp.9 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.43, i32 %v.45)
  %v.46 = extractvalue { i32, i1 } %tmp.9, 0
  %tmp.10 = extractvalue { i32, i1 } %tmp.9, 1
  br i1 %tmp.10, label %zil.trap, label %checked.4
checked.4:
  %v.47 = add i32 0, 39
  %tmp.11 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.46, i32 %v.47)
  %v.48 = extractvalue { i32, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i32, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.5
checked.5:
  ret i32 %v.48
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(%zil_env_0 %env) {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 1
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i32 } @zil.fn.2(ptr %arg.0, { i32 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { i32 } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.1(%zil_env_0 %tmp.0)
  %v.2 = load { i32 }, ptr %local.1, align 4
  ret { i32 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i32 } @zil.fn.3(ptr %arg.0, { i32 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { i32 } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.1(%zil_env_0 %tmp.0)
  %v.2 = load { i32 }, ptr %local.1, align 4
  ret { i32 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.1(%zil_env_0 %tmp.0)
  %v.2 = load i32, ptr %local.1, align 4
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.5(ptr %arg.0, i64 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.1(%zil_env_0 %tmp.0)
  %v.2 = load i64, ptr %local.1, align 8
  ret i64 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

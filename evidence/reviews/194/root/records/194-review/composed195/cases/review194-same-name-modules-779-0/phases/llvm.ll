; ZIL verified LLVM module
; verified-fingerprint: b7c3c0e32a55cbc5b08d9a6431789e826dce986f70acfde442937b097d918ed4
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
  %local.0 = alloca %zil_env_0, align 1
  %local.1 = alloca i64, align 8
  %local.2 = alloca { i64 }, align 8
  %local.3 = alloca i64, align 8
  %local.4 = alloca { i64 }, align 8
  %local.5 = alloca i64, align 8
  %local.6 = alloca { i64 }, align 8
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = add i64 0, 3
  store i64 %v.4, ptr %local.1, align 8
  %v.5 = load i64, ptr %local.1, align 8
%v.6 = insertvalue { i64 } zeroinitializer, i64 %v.5, 0
  %v.7 = call { i64 } @zil.fn.2(ptr %v.3, { i64 } %v.6)
  store { i64 } %v.7, ptr %local.2, align 8
  %v.8 = getelementptr i8, ptr %local.0, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = add i64 0, 4
  store i64 %v.11, ptr %local.3, align 8
  %v.12 = load i64, ptr %local.3, align 8
%v.13 = insertvalue { i64 } zeroinitializer, i64 %v.12, 0
  %v.14 = call { i64 } @zil.fn.3(ptr %v.10, { i64 } %v.13)
  store { i64 } %v.14, ptr %local.4, align 8
  %v.15 = getelementptr i8, ptr %local.0, i64 0
  %v.16 = getelementptr i8, ptr %v.15, i64 0
  %v.17 = getelementptr i8, ptr %v.16, i64 0
  %v.18 = add i64 0, 5
  store i64 %v.18, ptr %local.5, align 8
  %v.19 = load i64, ptr %local.5, align 8
%v.20 = insertvalue { i64 } zeroinitializer, i64 %v.19, 0
  %v.21 = call { i64 } @zil.fn.2(ptr %v.17, { i64 } %v.20)
  store { i64 } %v.21, ptr %local.6, align 8
  %v.22 = load { i64 }, ptr %local.2, align 8
%v.23 = extractvalue { i64 } %v.22, 0
  %v.24 = load { i64 }, ptr %local.4, align 8
%v.25 = extractvalue { i64 } %v.24, 0
  %tmp.0 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.23, i64 %v.25)
  %v.26 = extractvalue { i64, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i64, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.27 = load { i64 }, ptr %local.6, align 8
%v.28 = extractvalue { i64 } %v.27, 0
  %tmp.2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.26, i64 %v.28)
  %v.29 = extractvalue { i64, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i64, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %tmp.4 = icmp slt i64 %v.29, -2147483648
  %tmp.5 = icmp sgt i64 %v.29, 2147483647
  %tmp.6 = or i1 %tmp.4, %tmp.5
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  %v.30 = trunc i64 %v.29 to i32
  %v.31 = add i32 0, 12
  %tmp.7 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.30, i32 %v.31)
  %v.32 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.3
checked.3:
  ret i32 %v.32
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

define { i64 } @zil.fn.2(ptr %arg.0, { i64 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i64 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { i64 } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.1(%zil_env_0 %tmp.0)
  %v.2 = load { i64 }, ptr %local.1, align 8
  ret { i64 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i64 } @zil.fn.3(ptr %arg.0, { i64 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i64 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { i64 } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.1(%zil_env_0 %tmp.0)
  %v.2 = load { i64 }, ptr %local.1, align 8
  ret { i64 } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

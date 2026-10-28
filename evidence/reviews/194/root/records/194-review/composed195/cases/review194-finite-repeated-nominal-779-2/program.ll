; ZIL verified LLVM module
; verified-fingerprint: bd26927a96f69f00b79aaca8f9f2ea07180c3c31cb0dcce34eeb7078b88acc40
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
  %local.3 = alloca { { i64 } }, align 8
  %local.4 = alloca { { { i64 } }, { { i64 } } }, align 8
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = add i64 0, 7
  store i64 %v.1, ptr %local.1, align 8
  %v.2 = load i64, ptr %local.1, align 8
%v.3 = insertvalue { i64 } zeroinitializer, i64 %v.2, 0
  store { i64 } %v.3, ptr %local.2, align 8
  %v.4 = load { i64 }, ptr %local.2, align 8
%v.5 = insertvalue { { i64 } } zeroinitializer, { i64 } %v.4, 0
  store { { i64 } } %v.5, ptr %local.3, align 8
  %v.6 = getelementptr i8, ptr %local.0, i64 0
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = load { { i64 } }, ptr %local.3, align 8
  %v.10 = call { { i64 } } @zil.fn.3(ptr %v.8, { { i64 } } %v.9)
  %v.11 = call { { { i64 } }, { { i64 } } } @zil.fn.1({ { i64 } } %v.10)
  store { { { i64 } }, { { i64 } } } %v.11, ptr %local.4, align 8
  %v.12 = load { { { i64 } }, { { i64 } } }, ptr %local.4, align 8
%v.13 = extractvalue { { { i64 } }, { { i64 } } } %v.12, 0
%v.14 = extractvalue { { i64 } } %v.13, 0
%v.15 = extractvalue { i64 } %v.14, 0
  %v.16 = load { { { i64 } }, { { i64 } } }, ptr %local.4, align 8
%v.17 = extractvalue { { { i64 } }, { { i64 } } } %v.16, 1
%v.18 = extractvalue { { i64 } } %v.17, 0
%v.19 = extractvalue { i64 } %v.18, 0
  %tmp.0 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.15, i64 %v.19)
  %v.20 = extractvalue { i64, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i64, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %tmp.2 = icmp slt i64 %v.20, -2147483648
  %tmp.3 = icmp sgt i64 %v.20, 2147483647
  %tmp.4 = or i1 %tmp.2, %tmp.3
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.21 = trunc i64 %v.20 to i32
  %v.22 = add i32 0, 14
  %tmp.5 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.21, i32 %v.22)
  %v.23 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.23
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { { i64 } }, { { i64 } } } @zil.fn.1({ { i64 } } %arg.0) {
entry:
  %local.0 = alloca { { i64 } }, align 8
  store { { i64 } } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { i64 } }, ptr %local.0, align 8
  %v.1 = load { { i64 } }, ptr %local.0, align 8
%tmp.0 = insertvalue { { { i64 } }, { { i64 } } } zeroinitializer, { { i64 } } %v.0, 0
%v.2 = insertvalue { { { i64 } }, { { i64 } } } %tmp.0, { { i64 } } %v.1, 1
  ret { { { i64 } }, { { i64 } } } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(%zil_env_0 %env) {
entry:
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 1
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i64 } } @zil.fn.3(ptr %arg.0, { { i64 } } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { { i64 } }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { { i64 } } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.2(%zil_env_0 %tmp.0)
  %v.2 = load { { i64 } }, ptr %local.1, align 8
  ret { { i64 } } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

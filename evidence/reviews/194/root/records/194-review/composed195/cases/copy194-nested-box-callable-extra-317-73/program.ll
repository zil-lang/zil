; ZIL verified LLVM module
; verified-fingerprint: 28198c0c75aed022693d1983eb35bf721527a36703882f44efbb7f3bb0466427
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca %zil_env_0, align 1
  %local.1 = alloca i32, align 4
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca { { i32 } }, align 4
  %local.4 = alloca { { i32 } }, align 4
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = add i32 0, 7
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.1, align 4
%v.3 = insertvalue { i32 } zeroinitializer, i32 %v.2, 0
  store { i32 } %v.3, ptr %local.2, align 4
  %v.4 = load { i32 }, ptr %local.2, align 4
%v.5 = insertvalue { { i32 } } zeroinitializer, { i32 } %v.4, 0
  store { { i32 } } %v.5, ptr %local.3, align 4
  %v.6 = getelementptr i8, ptr %local.0, i64 0
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = load { { i32 } }, ptr %local.3, align 4
  %v.10 = call { { i32 } } @zil.fn.2(ptr %v.8, { { i32 } } %v.9)
  store { { i32 } } %v.10, ptr %local.4, align 4
  %v.11 = load { { i32 } }, ptr %local.4, align 4
%v.12 = extractvalue { { i32 } } %v.11, 0
%v.13 = extractvalue { i32 } %v.12, 0
  %v.14 = add i32 0, 7
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.15
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

define { { i32 } } @zil.fn.2(ptr %arg.0, { { i32 } } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { { i32 } }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { { i32 } } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.1 = call i32 @zil.fn.1(%zil_env_0 %tmp.0)
  %v.2 = load { { i32 } }, ptr %local.1, align 4
  ret { { i32 } } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

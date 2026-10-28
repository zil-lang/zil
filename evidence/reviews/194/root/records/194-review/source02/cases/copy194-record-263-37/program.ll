; ZIL verified LLVM module
; verified-fingerprint: 133c16a5fdb5305a052e02137f6ae21c65aa8942d18fb07329f61f1eb828df48
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
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = add i64 0, 6
  store i64 %v.4, ptr %local.1, align 8
  %v.5 = load i64, ptr %local.1, align 8
%v.6 = insertvalue { i64 } zeroinitializer, i64 %v.5, 0
  %v.7 = call { i64 } @zil.fn.2(ptr %v.3, { i64 } %v.6)
%v.8 = extractvalue { i64 } %v.7, 0
  %tmp.0 = icmp slt i64 %v.8, -2147483648
  %tmp.1 = icmp sgt i64 %v.8, 2147483647
  %tmp.2 = or i1 %tmp.0, %tmp.1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.9 = trunc i64 %v.8 to i32
  %v.10 = add i32 0, 7
  %tmp.3 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i64 } @zil.fn.1(%zil_env_0 %env, { i64 } %arg.0) {
entry:
  %local.0 = alloca { i64 }, align 8
  %local.1 = alloca i64, align 8
  store { i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { i64 }, ptr %local.0, align 8
%v.1 = extractvalue { i64 } %v.0, 0
  %v.2 = add i64 0, 1
  %tmp.0 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.1, i64 %v.2)
  %v.3 = extractvalue { i64, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i64, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i64 %v.3, ptr %local.1, align 8
  %v.4 = load i64, ptr %local.1, align 8
%v.5 = insertvalue { i64 } zeroinitializer, i64 %v.4, 0
  ret { i64 } %v.5
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
  %v.1 = load { i64 }, ptr %local.1, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.2 = call { i64 } @zil.fn.1(%zil_env_0 %tmp.0, { i64 } %v.1)
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

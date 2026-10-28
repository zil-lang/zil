; ZIL verified LLVM module
; verified-fingerprint: 52980a2b89e7fc1d01fcba3ef0c9fda8497a6eeab6df664a62fe1710f36ac3b3
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { ptr }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
@zil.return.format = private unnamed_addr constant [6 x i8] c"%lld\0A\00"
declare i32 @printf(ptr, ...)

define i64 @zil.fn.0() {
entry:
  %local.0 = alloca i64, align 8
  %local.1 = alloca %zil_env_0, align 8
  %local.2 = alloca i64, align 8
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 0
  store i64 %v.0, ptr %local.0, align 8
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = insertvalue %zil_env_0 zeroinitializer, ptr %v.2, 0
  store %zil_env_0 %v.3, ptr %local.1, align 8
  %v.4 = getelementptr i8, ptr %local.1, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = call i64 @zil.fn.2(ptr %v.6)
  store i64 %v.7, ptr %local.2, align 8
  %v.8 = load i64, ptr %local.2, align 8
  %v.9 = load i64, ptr %local.0, align 8
  %tmp.0 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.8, i64 %v.9)
  %v.10 = extractvalue { i64, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i64, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i64 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.1(ptr %env, i64 %arg.0) {
entry:
  %local.0 = alloca i64, align 8
  %local.1 = alloca ptr, align 8
  store i64 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load ptr, ptr %tmp.0, align 8
  store ptr %v.0, ptr %local.1, align 8
  %v.1 = load ptr, ptr %local.1, align 8
  %v.2 = load ptr, ptr %local.1, align 8
  %v.3 = load i64, ptr %v.2, align 8
  %v.4 = load i64, ptr %local.0, align 8
  %tmp.1 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.3, i64 %v.4)
  %v.5 = extractvalue { i64, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i64, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i64 %v.5, ptr %v.1, align 8
  %v.6 = load ptr, ptr %local.1, align 8
  %v.7 = load i64, ptr %v.6, align 8
  ret i64 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.2(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = add i64 0, 1
  %v.2 = call i64 @zil.fn.1(ptr %v.0, i64 %v.1)
  %v.3 = load ptr, ptr %local.0, align 8
  %v.4 = add i64 0, 2
  %v.5 = call i64 @zil.fn.1(ptr %v.3, i64 %v.4)
  %tmp.0 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.2, i64 %v.5)
  %v.6 = extractvalue { i64, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i64, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.7 = load ptr, ptr %local.0, align 8
  %v.8 = add i64 0, 3
  %v.9 = call i64 @zil.fn.1(ptr %v.7, i64 %v.8)
  %tmp.2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.6, i64 %v.9)
  %v.10 = extractvalue { i64, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i64, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  ret i64 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.wide = call i64 @zil.fn.0()
  %entry.printed = call i32 (ptr, ...) @printf(ptr @zil.return.format, i64 %entry.wide)
  ret i32 0
}

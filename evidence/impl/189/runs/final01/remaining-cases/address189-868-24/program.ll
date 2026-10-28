; ZIL verified LLVM module
; verified-fingerprint: 58dca2866d979ee4e6da246669f71ce6e15902a9ff20e74b4cf240081ce55802
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { ptr }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca ptr, align 8
  %local.3 = alloca %zil_env_0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 3
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 4
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = getelementptr i8, ptr %local.0, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  store ptr %v.3, ptr %local.2, align 8
  %v.4 = load ptr, ptr %local.2, align 8
  %v.5 = insertvalue %zil_env_0 zeroinitializer, ptr %v.4, 0
  store %zil_env_0 %v.5, ptr %local.3, align 8
  %v.6 = load %zil_env_0, ptr %local.3, align 8
  %v.7 = getelementptr i8, ptr %local.1, i64 0
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = call i32 @zil.fn.1(%zil_env_0 %v.6, ptr %v.9)
  %v.11 = add i32 0, 3
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.10, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.12
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1(%zil_env_0 %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = extractvalue %zil_env_0 %env, 0
  store ptr %v.0, ptr %local.1, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = load ptr, ptr %local.0, align 8
  %v.3 = load i32, ptr %v.2, align 4
  %v.4 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.4)
  %v.5 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.5, ptr %v.1, align 4
  %v.6 = load ptr, ptr %local.1, align 8
  %v.7 = load i32, ptr %v.6, align 4
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

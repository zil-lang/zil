; ZIL verified LLVM module
; verified-fingerprint: ab2664c5abf3e8cc5733e4b6627a13499e581b03f9773c57d426414e418dd956
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
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca ptr, align 8
  %local.3 = alloca ptr, align 8
  %local.4 = alloca ptr, align 8
  %local.5 = alloca %zil_env_0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 42
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = getelementptr i8, ptr %local.1, i64 0
  store ptr %v.3, ptr %local.2, align 8
  %v.4 = getelementptr i8, ptr %local.1, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  store ptr %v.5, ptr %local.3, align 8
  %v.6 = load ptr, ptr %local.3, align 8
  %v.7 = getelementptr { i32 }, ptr %v.6, i32 0, i32 0
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  store ptr %v.8, ptr %local.4, align 8
  %v.9 = load ptr, ptr %local.2, align 8
  %v.10 = insertvalue %zil_env_0 zeroinitializer, ptr %v.9, 0
  store %zil_env_0 %v.10, ptr %local.5, align 8
  %v.11 = getelementptr i8, ptr %local.5, i64 0
  %v.12 = getelementptr i8, ptr %v.11, i64 0
  %v.13 = getelementptr i8, ptr %v.12, i64 0
  %v.14 = load ptr, ptr %local.4, align 8
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  %v.16 = call i32 @zil.fn.2(ptr %v.13, ptr %v.15)
  %v.17 = add i32 0, 84
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.16, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.18
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
  %v.1 = load ptr, ptr %local.1, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.1, i32 0, i32 0
  %v.2 = load i32, ptr %tmp.0, align 4
  %v.3 = load ptr, ptr %local.0, align 8
  %v.4 = load i32, ptr %v.3, align 4
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.2, i32 %v.4)
  %v.5 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(ptr %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.1, align 8
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %tmp.0 = load %zil_env_0, ptr %v.0, align 8
  %v.3 = call i32 @zil.fn.1(%zil_env_0 %tmp.0, ptr %v.2)
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

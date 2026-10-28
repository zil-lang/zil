; ZIL verified LLVM module
; verified-fingerprint: a71014e23c730ec5aadc883de0b994ba3d526d178bb3971035464e6cc86007aa
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32 }, ptr, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define void @zil.fn.0({ i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i32 }, align 4
  store { i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca ptr, align 8
  %local.3 = alloca i32, align 4
  %local.4 = alloca ptr, align 8
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca %zil_env_0, align 8
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
  %v.4 = add i32 0, 42
  store i32 %v.4, ptr %local.3, align 4
  %v.5 = getelementptr i8, ptr %local.3, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  store ptr %v.6, ptr %local.4, align 8
  %v.7 = add i32 0, 1
  store i32 %v.7, ptr %local.5, align 4
  %v.8 = load i32, ptr %local.5, align 4
%v.9 = insertvalue { i32 } zeroinitializer, i32 %v.8, 0
  store i1 false, ptr %resource.live.6, align 1
  store { i32 } %v.9, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.10 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %v.11 = load ptr, ptr %local.2, align 8
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { i32 } %v.10, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, ptr %v.11, 1
  %v.12 = insertvalue %zil_env_0 %tmp.1, i1 true, 2
  store i1 false, ptr %resource.live.7, align 1
  store %zil_env_0 %v.12, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.13 = load ptr, ptr %local.4, align 8
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  %v.15 = call i32 @zil.fn.2(ptr %local.7, ptr %v.14)
  %v.16 = add i32 0, 42
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.15, i32 %v.16)
  %v.17 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %tmp.4 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.4, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.closure.drop.0(ptr %local.7)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.5 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.5, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.6, align 1
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.6 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.6, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.1, align 1
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.17
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(ptr %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  %local.2 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.1, align 8
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %v.1 = load ptr, ptr %tmp.0, align 8
  store ptr %v.1, ptr %local.2, align 8
  %v.2 = load ptr, ptr %local.1, align 8
  %tmp.1 = getelementptr { i32 }, ptr %v.2, i32 0, i32 0
  %v.3 = load i32, ptr %tmp.1, align 4
  %v.4 = load ptr, ptr %local.2, align 8
  %v.5 = add i32 0, 43
  %tmp.2 = getelementptr { i32 }, ptr %v.4, i32 0, i32 0
  store i32 %v.5, ptr %tmp.2, align 4
  %v.6 = load ptr, ptr %local.0, align 8
  %v.7 = load i32, ptr %v.6, align 4
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

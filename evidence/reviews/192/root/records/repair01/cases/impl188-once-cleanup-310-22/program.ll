; ZIL verified LLVM module
; verified-fingerprint: 81c02b7bdefc7e0977f2eafab060201809c5c7965a4d515fb8f1e9d5af23966e
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32 }, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  call void @zil.fn.1(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.1(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  %v.2 = call i32 @putchar(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2({ i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i32 }, align 4
  store { i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 7
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.fn.1(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca %zil_env_0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = add i32 0, 65
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { i32 } %v.3, 0
  %v.4 = insertvalue %zil_env_0 %tmp.0, i1 true, 1
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.4, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.5 = load %zil_env_0, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.5, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.6 = or i1 false, true
  %v.7 = load %zil_env_0, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  %v.8 = call i32 @zil.fn.7(%zil_env_0 %v.7, i1 %v.6)
  %v.9 = add i32 0, 7
  %tmp.1 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.8, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %tmp.3 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.3, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.0(ptr %local.3)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.5 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  store i1 false, ptr %resource.live.1, align 1
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store i1 false, ptr %tmp.0, align 1
  %tmp.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.1 = load { i32 }, ptr %tmp.1, align 4
  store { i32 } %v.1, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.2 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.3 = call i32 @zil.fn.2({ i32 } %v.2)
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(%zil_env_0 %arg.0, i1 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca i1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  %closure.once.2 = alloca %zil_env_0, align 4
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  store i1 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  %v.0 = load i1, ptr %local.1, align 1
  br i1 %v.0, label %bb.1, label %bb.2
bb.1:
  store i1 false, ptr %resource.live.2, align 1
  %v.1 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_0 %v.1, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = load %zil_env_0, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.2, ptr %closure.once.2, align 4
  %v.3 = call i32 @zil.fn.4(ptr %closure.once.2)
  call void @zil.closure.drop.0(ptr %closure.once.2)
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.3
bb.2:
  br label %bb.3
bb.3:
  %v.4 = add i32 0, 0
  %tmp.2 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.2, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6(%zil_env_0 %arg.0, i1 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca i1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  store i1 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_0 %v.0, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.1 = load i1, ptr %local.1, align 1
  %v.2 = load %zil_env_0, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.3 = call i32 @zil.fn.5(%zil_env_0 %v.2, i1 %v.1)
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.7(%zil_env_0 %arg.0, i1 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca i1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  store i1 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_0 %v.0, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.1 = load i1, ptr %local.1, align 1
  %v.2 = load %zil_env_0, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.3 = call i32 @zil.fn.6(%zil_env_0 %v.2, i1 %v.1)
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.3()
  ret i32 %entry.result
}

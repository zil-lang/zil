; ZIL verified LLVM module
; verified-fingerprint: e171c1971cdf333caed6fb88f977a731b351ff0c237b8287663545f7c8e2228e
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
%zil_env_1 = type { i32, { i32 }, i32, i1 }
%zil_env_2 = type { %zil_env_1, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
entry:
  %flag.ptr.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 1
  call void @zil.fn.1(ptr %payload.1)
  br label %done.1
done.1:
  ret void
}
define internal void @zil.closure.drop.2(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_2, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %child.0 = getelementptr %zil_env_2, ptr %env, i32 0, i32 0
  call void @zil.closure.drop.1(ptr %child.0)
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
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

define %zil_env_0 @zil.fn.2(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = insertvalue %zil_env_0 zeroinitializer, i32 %v.1, 0
  ret %zil_env_0 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_1 @zil.fn.3(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load i32, ptr %local.0, align 4
  store i32 %v.3, ptr %local.3, align 4
  %v.4 = load i32, ptr %local.3, align 4
  %v.5 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.6 = load i32, ptr %local.0, align 4
  %tmp.0 = insertvalue %zil_env_1 zeroinitializer, i32 %v.4, 0
  %tmp.1 = insertvalue %zil_env_1 %tmp.0, { i32 } %v.5, 1
  %tmp.2 = insertvalue %zil_env_1 %tmp.1, i32 %v.6, 2
  %v.7 = insertvalue %zil_env_1 %tmp.2, i1 true, 3
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  ret %zil_env_1 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_1 @zil.fn.4(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call %zil_env_1 @zil.fn.3(i32 %v.0)
  ret %zil_env_1 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_2 @zil.fn.5(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca %zil_env_1, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call %zil_env_1 @zil.fn.3(i32 %v.0)
  store i1 false, ptr %resource.live.1, align 1
  store %zil_env_1 %v.1, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.2 = load %zil_env_1, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %tmp.0 = insertvalue %zil_env_2 zeroinitializer, %zil_env_1 %v.2, 0
  %v.3 = insertvalue %zil_env_2 %tmp.0, i1 true, 1
  %tmp.1 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.1, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.closure.drop.1(ptr %local.1)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_2 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_1, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, %zil_env_1, i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { i32 }, %zil_env_1, i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { i32 }, %zil_env_1, i32 }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.0, align 1
  %v.0 = add i32 0, 65
  %v.1 = call %zil_env_1 @zil.fn.3(i32 %v.0)
  store %zil_env_1 %v.1, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.2 = add i32 0, 84
  %v.3 = load %zil_env_1, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  %v.4 = call { { i32 }, %zil_env_1, i32 } @zil.fn.10(%zil_env_1 %v.3, i32 %v.2)
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.1(ptr %local.0)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.1, align 1
  store { { i32 }, %zil_env_1, i32 } %v.4, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.5 = load { { i32 }, %zil_env_1, i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { { i32 }, %zil_env_1, i32 } %v.5, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.6 = load { { i32 }, %zil_env_1, i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.7 = call { { i32 }, %zil_env_1, i32 } @zil.fn.11({ { i32 }, %zil_env_1, i32 } %v.6)
  store { { i32 }, %zil_env_1, i32 } %v.7, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %tmp.1 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.1, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.2 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.2)
  %tmp.3 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.3)
  br label %package.done.1
package.done.1:
  %v.8 = load { { i32 }, %zil_env_1, i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { { i32 }, %zil_env_1, i32 } %v.8, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.9 = load { { i32 }, %zil_env_1, i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  %v.10 = call { { i32 }, %zil_env_1, i32 } @zil.fn.12({ { i32 }, %zil_env_1, i32 } %v.9)
  store { { i32 }, %zil_env_1, i32 } %v.10, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.5 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.5)
  %tmp.6 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.6)
  br label %package.done.2
package.done.2:
  %tmp.7 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 2
  %v.11 = load i32, ptr %tmp.7, align 4
  %v.12 = add i32 0, 67
  %tmp.8 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.11, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.10 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.10, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.11 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.11)
  %tmp.12 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.12)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.13
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.7(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = add i32 0, 1
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.1, i32 %v.2)
  %v.3 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.3, ptr %local.0, align 4
  %tmp.3 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store i32 %v.3, ptr %tmp.3, align 4
  %v.4 = load i32, ptr %local.0, align 4
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.8(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.1, align 8
  %tmp.1 = getelementptr %zil_env_1, ptr %env, i32 0, i32 2
  %v.2 = load i32, ptr %tmp.1, align 4
  store i32 %v.2, ptr %local.2, align 4
  %v.3 = load i32, ptr %local.0, align 4
  %v.4 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.4)
  %v.5 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.5, ptr %local.0, align 4
  %tmp.4 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  store i32 %v.5, ptr %tmp.4, align 4
  %v.6 = load i32, ptr %local.0, align 4
  %v.7 = load ptr, ptr %local.1, align 8
  %tmp.5 = getelementptr { i32 }, ptr %v.7, i32 0, i32 0
  %v.8 = load i32, ptr %tmp.5, align 4
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.10 = load i32, ptr %local.2, align 4
  %tmp.8 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.9(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_2, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = call i32 @zil.fn.8(ptr %v.1)
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, %zil_env_1, i32 } @zil.fn.10(%zil_env_1 %arg.0, i32 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_1, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_1, align 4
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca %zil_env_1, align 4
  store %zil_env_1 %arg.0, ptr %local.0, align 4
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  %v.0 = load i32, ptr %local.1, align 4
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = load i32, ptr %local.2, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.2, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = load %zil_env_1, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_1 %v.3, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.4 = add i32 0, 0
  store i32 %v.4, ptr %local.5, align 4
  %v.5 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.5, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.6 = load %zil_env_1, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_1 %v.6, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.7 = load i32, ptr %local.5, align 4
  %v.8 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %v.9 = load %zil_env_1, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
%tmp.0 = insertvalue { { i32 }, %zil_env_1, i32 } zeroinitializer, { i32 } %v.8, 0
%tmp.1 = insertvalue { { i32 }, %zil_env_1, i32 } %tmp.0, %zil_env_1 %v.9, 1
%v.10 = insertvalue { { i32 }, %zil_env_1, i32 } %tmp.1, i32 %v.7, 2
  %tmp.2 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.2, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.closure.drop.1(ptr %local.7)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.3 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.1(ptr %local.6)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.4 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.4, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.1(ptr %local.4)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.5 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.1(ptr %local.3)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.1(ptr %local.0)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, %zil_env_1, i32 } %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, %zil_env_1, i32 } @zil.fn.11({ { i32 }, %zil_env_1, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, %zil_env_1, i32 }, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, %zil_env_1, i32 }, align 4
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_1, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca %zil_env_1, align 4
  store { { i32 }, %zil_env_1, i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = load { { i32 }, %zil_env_1, i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store { { i32 }, %zil_env_1, i32 } %v.0, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 true, ptr %package.live.1.1, align 1
  %tmp.0 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 1
  %v.1 = load %zil_env_1, ptr %tmp.0, align 4
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_1 %v.1, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = call i32 @zil.fn.8(ptr %local.2)
  %tmp.1 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 2
  store i32 %v.2, ptr %tmp.1, align 4
  %v.3 = load %zil_env_1, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_1 %v.3, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %tmp.2 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.2, label %package.field.drop.0, label %package.field.done.0
package.field.drop.0:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.3 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.3)
  br label %package.field.done.0
package.field.done.0:
  %v.4 = load %zil_env_1, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  %tmp.4 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 1
  store %zil_env_1 %v.4, ptr %tmp.4, align 4
  store i1 true, ptr %package.live.1.1, align 1
  %tmp.5 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.5, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.1(ptr %local.3)
  br label %closure.done.1
closure.done.1:
  %v.5 = load { { i32 }, %zil_env_1, i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %tmp.6 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.6, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.1(ptr %local.2)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.7 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.7, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.8 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.8, label %package.field.drop.4, label %package.field.done.4
package.field.drop.4:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.9 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.9)
  br label %package.field.done.4
package.field.done.4:
  %tmp.10 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.10, label %package.field.drop.5, label %package.field.done.5
package.field.drop.5:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.11 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.11)
  br label %package.field.done.5
package.field.done.5:
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.12 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.12, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.13 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.13)
  %tmp.14 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.14)
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, %zil_env_1, i32 } %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, %zil_env_1, i32 } @zil.fn.12({ { i32 }, %zil_env_1, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, %zil_env_1, i32 }, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, %zil_env_1, i32 }, align 4
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %local.2 = alloca ptr, align 8
  store { { i32 }, %zil_env_1, i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { { i32 }, %zil_env_1, i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store { { i32 }, %zil_env_1, i32 } %v.0, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 true, ptr %package.live.1.1, align 1
  %v.1 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 1
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  store ptr %v.2, ptr %local.2, align 8
  %v.3 = load ptr, ptr %local.2, align 8
  %v.4 = call i32 @zil.fn.8(ptr %v.3)
  %tmp.0 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 2
  store i32 %v.4, ptr %tmp.0, align 4
  %v.5 = load { { i32 }, %zil_env_1, i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.1 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.1, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.2 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.2, label %package.field.drop.1, label %package.field.done.1
package.field.drop.1:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.3 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.3)
  br label %package.field.done.1
package.field.done.1:
  %tmp.4 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.4, label %package.field.drop.2, label %package.field.done.2
package.field.drop.2:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.5 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.5)
  br label %package.field.done.2
package.field.done.2:
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.7 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.7)
  %tmp.8 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.8)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, %zil_env_1, i32 } %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.6()
  ret i32 %entry.result
}

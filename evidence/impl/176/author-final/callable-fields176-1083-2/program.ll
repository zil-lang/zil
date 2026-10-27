; ZIL verified LLVM module
; verified-fingerprint: 887e87d31214851d1e9b1ff2f83aef77a4aa417044a4fb36a31fb0da28a7687c
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
%zil_env_1 = type { { ptr, i64, i64 }, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  call void @zil.bytes.drop(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
declare ptr @realloc(ptr, i64)
declare void @free(ptr)

define internal ptr @zil.bytes.reallocate(ptr %old, i64 %size) {
entry:
  %result = call ptr @realloc(ptr %old, i64 %size)
  ret ptr %result
}
define internal void @zil.bytes.release(ptr %payload) {
entry:
  call void @free(ptr %payload)
  ret void
}

define internal i1 @zil.bytes.reserve(ptr %head, i64 %requested) {
entry:
  %negative = icmp slt i64 %requested, 0
  br i1 %negative, label %failed, label %capacity
capacity:
  %cap.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 2
  %cap = load i64, ptr %cap.field, align 8
  %fits = icmp ule i64 %requested, %cap
  br i1 %fits, label %done, label %prepare
prepare:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %old = load ptr, ptr %ptr.field, align 8
  %next = call ptr @zil.bytes.reallocate(ptr %old, i64 %requested)
  %ok = icmp ne ptr %next, null
  br i1 %ok, label %publish, label %failed
publish:
  store ptr %next, ptr %ptr.field, align 8
  store i64 %requested, ptr %cap.field, align 8
  br label %done
done:
  ret i1 true
failed:
  ret i1 false
}
define internal i1 @zil.bytes.truncate(ptr %head, i64 %requested) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %len = load i64, ptr %len.field, align 8
  %nonnegative = icmp sge i64 %requested, 0
  %within = icmp ule i64 %requested, %len
  %valid = and i1 %nonnegative, %within
  br i1 %valid, label %publish, label %failed
publish:
  store i64 %requested, ptr %len.field, align 8
  ret i1 true
failed:
  ret i1 false
}
define internal i1 @zil.bytes.push(ptr %head, i8 %byte) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %cap.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 2
  %len = load i64, ptr %len.field, align 8
  %cap = load i64, ptr %cap.field, align 8
  %room = icmp ult i64 %len, %cap
  br i1 %room, label %initialize, label %full
initialize:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %payload = load ptr, ptr %ptr.field, align 8
  %slot = getelementptr i8, ptr %payload, i64 %len
  store i8 %byte, ptr %slot, align 1
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret i1 true
full:
  ret i1 false
}
define internal void @zil.bytes.drop(ptr %head) {
entry:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %payload = load ptr, ptr %ptr.field, align 8
  call void @zil.bytes.release(ptr %payload)
  ret void
}
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

define %zil_env_1 @zil.fn.3() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = add i64 0, 4
  %v.3 = call i1 @zil.bytes.reserve(ptr %v.1, i64 %v.2)
  %v.4 = getelementptr i8, ptr %local.0, i64 0
  %v.5 = add i8 0, 7
  %v.6 = call i1 @zil.bytes.push(ptr %v.4, i8 %v.5)
  %v.7 = load { ptr, i64, i64 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  %tmp.0 = insertvalue %zil_env_1 zeroinitializer, { ptr, i64, i64 } %v.7, 0
  %v.8 = insertvalue %zil_env_1 %tmp.0, i1 true, 1
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.bytes.drop(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret %zil_env_1 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_1, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, %zil_env_1, i32 }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { i32 }, %zil_env_1, i32 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { i32 }, %zil_env_1, i32 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.0, align 1
  %v.0 = call %zil_env_1 @zil.fn.3()
  store %zil_env_1 %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = add i32 0, 84
  %v.2 = load %zil_env_1, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  %v.3 = call { { i32 }, %zil_env_1, i32 } @zil.fn.7(%zil_env_1 %v.2, i32 %v.1)
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.1(ptr %local.0)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.1, align 1
  store { { i32 }, %zil_env_1, i32 } %v.3, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.4 = load { { i32 }, %zil_env_1, i32 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store { { i32 }, %zil_env_1, i32 } %v.4, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.5 = load { { i32 }, %zil_env_1, i32 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.6 = call { { i32 }, %zil_env_1, i32 } @zil.fn.8({ { i32 }, %zil_env_1, i32 } %v.5)
  store { { i32 }, %zil_env_1, i32 } %v.6, ptr %local.1, align 8
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
  %tmp.4 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 2
  %v.7 = load i32, ptr %tmp.4, align 4
  %v.8 = add i32 0, 1
  %v.9 = icmp ne i32 %v.7, %v.8
  br i1 %v.9, label %bb.1, label %bb.2
bb.1:
  %v.10 = add i32 0, 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.5 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.5, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.6 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.6)
  %tmp.7 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.7)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.3, align 1
  ret i32 %v.10
bb.2:
  br label %bb.3
bb.3:
  %v.11 = load { { i32 }, %zil_env_1, i32 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store { { i32 }, %zil_env_1, i32 } %v.11, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.12 = load { { i32 }, %zil_env_1, i32 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.13 = call { { i32 }, %zil_env_1, i32 } @zil.fn.8({ { i32 }, %zil_env_1, i32 } %v.12)
  store { { i32 }, %zil_env_1, i32 } %v.13, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %tmp.8 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.8, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.9 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.9)
  %tmp.10 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.10)
  br label %package.done.3
package.done.3:
  %tmp.11 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 2
  %v.14 = load i32, ptr %tmp.11, align 4
  %v.15 = add i32 0, 2
  %tmp.12 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.14, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i32, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.4
checked.4:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %tmp.14 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.14, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.15 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.15)
  %tmp.16 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.closure.drop.1(ptr %tmp.16)
  br label %package.done.5
package.done.5:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.16
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(ptr %env) {
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

define i32 @zil.fn.6(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.1, i32 0, i32 1
  %v.2 = load i64, ptr %tmp.0, align 8
  store i64 %v.2, ptr %local.1, align 8
  %v.3 = load ptr, ptr %local.0, align 8
  %v.4 = add i8 0, 9
  %v.5 = call i1 @zil.bytes.push(ptr %v.3, i8 %v.4)
  %v.6 = load i64, ptr %local.1, align 8
  %tmp.1 = icmp slt i64 %v.6, -2147483648
  %tmp.2 = icmp sgt i64 %v.6, 2147483647
  %tmp.3 = or i1 %tmp.1, %tmp.2
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.7 = trunc i64 %v.6 to i32
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, %zil_env_1, i32 } @zil.fn.7(%zil_env_1 %arg.0, i32 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_1, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_1, align 8
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca %zil_env_1, align 8
  store %zil_env_1 %arg.0, ptr %local.0, align 8
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
  %v.3 = load %zil_env_1, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_1 %v.3, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.4 = add i32 0, 0
  store i32 %v.4, ptr %local.5, align 4
  %v.5 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.5, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.6 = load %zil_env_1, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_1 %v.6, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.7 = load i32, ptr %local.5, align 4
  %v.8 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %v.9 = load %zil_env_1, ptr %local.7, align 8
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

define { { i32 }, %zil_env_1, i32 } @zil.fn.8({ { i32 }, %zil_env_1, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, %zil_env_1, i32 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, %zil_env_1, i32 }, align 8
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %local.2 = alloca ptr, align 8
  store { { i32 }, %zil_env_1, i32 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { i32 }, %zil_env_1, i32 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store { { i32 }, %zil_env_1, i32 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 true, ptr %package.live.1.1, align 1
  %v.1 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 1
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  store ptr %v.2, ptr %local.2, align 8
  %v.3 = load ptr, ptr %local.2, align 8
  %v.4 = call i32 @zil.fn.6(ptr %v.3)
  %tmp.0 = getelementptr { { i32 }, %zil_env_1, i32 }, ptr %local.1, i32 0, i32 2
  store i32 %v.4, ptr %tmp.0, align 4
  %v.5 = load { { i32 }, %zil_env_1, i32 }, ptr %local.1, align 8
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
  %entry.result = call i32 @zil.fn.4()
  ret i32 %entry.result
}

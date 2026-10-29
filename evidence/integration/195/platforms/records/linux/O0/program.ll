; ZIL verified LLVM module
; verified-fingerprint: 7219be91eeedf9f9f816d0a4795fbddb2ff6259e13e6d3ba60372ad86f2be16b
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32 }, { ptr, i64, i64 }, i1, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  call void @zil.fn.2(ptr %payload.0)
  br label %done.0
done.0:
  %flag.ptr.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  call void @zil.bytes.drop(ptr %payload.1)
  br label %done.1
done.1:
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
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @getchar() nonlazybind
define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = call i32 @getchar()
  store i32 %v.1, ptr %local.0, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = add i32 0, 48
  %v.4 = icmp slt i32 %v.2, %v.3
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %v.5 = or i1 false, true
  store i1 %v.5, ptr %local.1, align 1
  br label %bb.3
bb.2:
  %v.6 = load i32, ptr %local.0, align 4
  %v.7 = add i32 0, 55
  %v.8 = icmp sgt i32 %v.6, %v.7
  store i1 %v.8, ptr %local.1, align 1
  br label %bb.3
bb.3:
  %v.9 = load i1, ptr %local.1, align 1
  br i1 %v.9, label %bb.4, label %bb.5
bb.4:
  %v.10 = add i32 0, 9
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.4, align 1
  ret i32 %v.10
bb.5:
  br label %bb.6
bb.6:
  %v.11 = call %zil_env_0 @zil.fn.3()
  store %zil_env_0 %v.11, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.12 = load %zil_env_0, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.13 = call { { ptr, i64, i64 }, %zil_env_0 } @zil.fn.8(%zil_env_0 %v.12)
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.3, align 1
  store { { ptr, i64, i64 }, %zil_env_0 } %v.13, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.14 = load { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  store { { ptr, i64, i64 }, %zil_env_0 } %v.14, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.15 = load i32, ptr %local.0, align 4
  %v.16 = add i32 0, 48
  %tmp.1 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.15, i32 %v.16)
  %v.17 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.1
checked.1:
  %v.18 = sext i32 %v.17 to i64
  %v.19 = load { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  %v.20 = call i32 @zil.fn.11({ { ptr, i64, i64 }, %zil_env_0 } %v.19, i64 %v.18)
  %tmp.3 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.3, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.4 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.4, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.4)
  %tmp.5 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.4, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.5)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.6 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.6, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.7 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.7)
  %tmp.8 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.8)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.20
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.2(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.5(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.3() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 72
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.3, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.4 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.5 = load { ptr, i64, i64 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { i32 } %v.4, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, { ptr, i64, i64 } %v.5, 1
  %tmp.2 = insertvalue %zil_env_0 %tmp.1, i1 true, 2
  %v.6 = insertvalue %zil_env_0 %tmp.2, i1 true, 3
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.bytes.drop(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.4 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.4, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.2(ptr %local.1)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_0 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.5(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call i32 @putchar(i32 %v.0)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.6(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.5(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.7(ptr %env, ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  %local.2 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.1, align 8
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.2, align 8
  %v.2 = load ptr, ptr %local.1, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.2, i32 0, i32 0
  %v.3 = load i32, ptr %tmp.0, align 4
  %v.4 = load ptr, ptr %local.2, align 8
  %v.5 = load ptr, ptr %local.2, align 8
  %tmp.1 = getelementptr { ptr, i64, i64 }, ptr %v.5, i32 0, i32 1
  %v.6 = load i64, ptr %tmp.1, align 8
  %v.7 = add i64 0, 1
  %tmp.2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.6, i64 %v.7)
  %v.8 = extractvalue { i64, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i64, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.9 = call i1 @zil.bytes.reserve(ptr %v.4, i64 %v.8)
  %v.10 = xor i1 %v.9, true
  br i1 %v.10, label %bb.1, label %bb.2
bb.1:
  %v.11 = add i32 0, -1
  ret i32 %v.11
bb.2:
  br label %bb.3
bb.3:
  %v.12 = load ptr, ptr %local.2, align 8
  %v.13 = load ptr, ptr %local.0, align 8
  %v.14 = load i32, ptr %v.13, align 4
  %tmp.4 = icmp ugt i32 %v.14, 255
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.15 = trunc i32 %v.14 to i8
  %v.16 = call i1 @zil.bytes.push(ptr %v.12, i8 %v.15)
  %v.17 = xor i1 %v.16, true
  br i1 %v.17, label %bb.4, label %bb.5
bb.4:
  %v.18 = add i32 0, -2
  ret i32 %v.18
bb.5:
  br label %bb.6
bb.6:
  %v.19 = load ptr, ptr %local.0, align 8
  %v.20 = load ptr, ptr %local.0, align 8
  %v.21 = load i32, ptr %v.20, align 4
  %v.22 = add i32 0, 1
  %tmp.5 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.21, i32 %v.22)
  %v.23 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.23, ptr %v.19, align 4
  %v.24 = load ptr, ptr %local.2, align 8
  %tmp.7 = getelementptr { ptr, i64, i64 }, ptr %v.24, i32 0, i32 1
  %v.25 = load i64, ptr %tmp.7, align 8
  %tmp.8 = icmp slt i64 %v.25, -2147483648
  %tmp.9 = icmp sgt i64 %v.25, 2147483647
  %tmp.10 = or i1 %tmp.8, %tmp.9
  br i1 %tmp.10, label %zil.trap, label %checked.3
checked.3:
  %v.26 = trunc i64 %v.25 to i32
  ret i32 %v.26
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { ptr, i64, i64 }, %zil_env_0 } @zil.fn.8(%zil_env_0 %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_0, align 8
  store %zil_env_0 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = load %zil_env_0, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.1, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.2, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = load %zil_env_0, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.3, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.4 = load { ptr, i64, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.5 = load %zil_env_0, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
%tmp.0 = insertvalue { { ptr, i64, i64 }, %zil_env_0 } zeroinitializer, { ptr, i64, i64 } %v.4, 0
%v.6 = insertvalue { { ptr, i64, i64 }, %zil_env_0 } %tmp.0, %zil_env_0 %v.5, 1
  %tmp.1 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.1, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.2 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.bytes.drop(ptr %local.3)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.4 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.4, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.5 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.5, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { { ptr, i64, i64 }, %zil_env_0 } %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.9(ptr %arg.0, ptr %arg.1) {
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
  %v.3 = call i32 @zil.fn.7(ptr %v.0, ptr %v.2)
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.10(ptr %arg.0, ptr %arg.1, ptr %arg.2, i64 %arg.3) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  %local.2 = alloca ptr, align 8
  %local.3 = alloca i64, align 8
  %local.4 = alloca i64, align 8
  %local.5 = alloca i64, align 8
  %local.6 = alloca i64, align 8
  %local.7 = alloca i32, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { i32 }, align 4
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { ptr, i64, i64 }, align 8
  %local.10 = alloca { ptr, i64 }, align 8
  %local.11 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  store ptr %arg.2, ptr %local.2, align 8
  store i64 %arg.3, ptr %local.3, align 8
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 0
  store i64 %v.0, ptr %local.4, align 8
  %v.1 = add i64 0, 0
  %v.2 = load i64, ptr %local.3, align 8
  store i64 %v.1, ptr %local.5, align 8
  br label %bb.1
bb.1:
  %v.3 = load i64, ptr %local.5, align 8
  %v.4 = icmp slt i64 %v.3, %v.2
  br i1 %v.4, label %bb.2, label %bb.4
bb.2:
  store i64 %v.3, ptr %local.6, align 8
  %v.5 = add i32 0, 48
  %v.6 = load i64, ptr %local.6, align 8
  %tmp.0 = icmp slt i64 %v.6, -2147483648
  %tmp.1 = icmp sgt i64 %v.6, 2147483647
  %tmp.2 = or i1 %tmp.0, %tmp.1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.7 = trunc i64 %v.6 to i32
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.8, ptr %local.7, align 4
  %v.9 = load i32, ptr %local.7, align 4
%v.10 = insertvalue { i32 } zeroinitializer, i32 %v.9, 0
  store i1 false, ptr %resource.live.8, align 1
  store { i32 } %v.10, ptr %local.8, align 4
  store i1 true, ptr %resource.live.8, align 1
  %v.11 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.9, align 1
  store { ptr, i64, i64 } %v.11, ptr %local.9, align 8
  store i1 true, ptr %resource.live.9, align 1
  %v.12 = getelementptr i8, ptr %local.9, i64 0
  %v.13 = add i64 0, 1
  %v.14 = call i1 @zil.bytes.reserve(ptr %v.12, i64 %v.13)
  %v.15 = xor i1 %v.14, true
  br i1 %v.15, label %bb.5, label %bb.6
bb.3:
  %v.80 = add i64 0, 1
  %tmp.5 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.3, i64 %v.80)
  %v.81 = extractvalue { i64, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i64, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  store i64 %v.81, ptr %local.5, align 8
  br label %bb.1
bb.4:
  %v.82 = add i32 0, 0
  ret i32 %v.82
bb.5:
  %v.16 = add i32 0, 1
  %tmp.7 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.7, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.bytes.drop(ptr %local.9)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.8 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.8, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.6(ptr %local.8)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.8, align 1
  ret i32 %v.16
bb.6:
  br label %bb.7
bb.7:
  %v.17 = getelementptr i8, ptr %local.9, i64 0
  %v.18 = load i64, ptr %local.6, align 8
  %tmp.9 = icmp ugt i64 %v.18, 255
  br i1 %tmp.9, label %zil.trap, label %checked.5
checked.5:
  %v.19 = trunc i64 %v.18 to i8
  %v.20 = call i1 @zil.bytes.push(ptr %v.17, i8 %v.19)
  %v.21 = xor i1 %v.20, true
  br i1 %v.21, label %bb.8, label %bb.9
bb.8:
  %v.22 = add i32 0, 2
  %tmp.10 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.10, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.bytes.drop(ptr %local.9)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.11 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.11, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.6(ptr %local.8)
  br label %resource.done.7
resource.done.7:
  store i1 false, ptr %resource.live.8, align 1
  ret i32 %v.22
bb.9:
  br label %bb.10
bb.10:
  %v.23 = getelementptr i8, ptr %local.9, i64 0
  %tmp.12 = load ptr, ptr %v.23, align 8
  %tmp.13 = getelementptr { ptr, i64, i64 }, ptr %v.23, i32 0, i32 1
  %tmp.14 = load i64, ptr %tmp.13, align 8
  %tmp.15 = insertvalue { ptr, i64 } poison, ptr %tmp.12, 0
  %v.24 = insertvalue { ptr, i64 } %tmp.15, i64 %tmp.14, 1
  store { ptr, i64 } %v.24, ptr %local.10, align 8
  %v.25 = load { ptr, i64 }, ptr %local.10, align 8
  %v.26 = add i32 0, 0
  %tmp.16 = extractvalue { ptr, i64 } %v.25, 0
  %tmp.17 = extractvalue { ptr, i64 } %v.25, 1
  %tmp.18 = sext i32 %v.26 to i64
  %tmp.19 = icmp uge i64 %tmp.18, %tmp.17
  br i1 %tmp.19, label %zil.trap, label %checked.8
checked.8:
  %v.27 = getelementptr i8, ptr %tmp.16, i64 %tmp.18
  %v.28 = getelementptr i8, ptr %v.27, i64 0
  %v.29 = load i8, ptr %v.28, align 1
  %v.30 = zext i8 %v.29 to i64
  %v.31 = load i64, ptr %local.6, align 8
  %v.32 = icmp ne i64 %v.30, %v.31
  br i1 %v.32, label %bb.11, label %bb.12
bb.11:
  %v.33 = add i32 0, 8
  %tmp.20 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.20, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.bytes.drop(ptr %local.9)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.21 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.21, label %resource.drop.10, label %resource.done.10
resource.drop.10:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.6(ptr %local.8)
  br label %resource.done.10
resource.done.10:
  store i1 false, ptr %resource.live.8, align 1
  ret i32 %v.33
bb.12:
  br label %bb.13
bb.13:
  %v.34 = load i64, ptr %local.6, align 8
  %v.35 = add i64 0, 1
  %v.36 = icmp eq i64 %v.34, %v.35
  br i1 %v.36, label %bb.14, label %bb.15
bb.14:
  %tmp.22 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.22, label %resource.drop.11, label %resource.done.11
resource.drop.11:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.bytes.drop(ptr %local.9)
  br label %resource.done.11
resource.done.11:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.23 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.23, label %resource.drop.12, label %resource.done.12
resource.drop.12:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.6(ptr %local.8)
  br label %resource.done.12
resource.done.12:
  store i1 false, ptr %resource.live.8, align 1
  br label %bb.3
bb.15:
  br label %bb.16
bb.16:
  %v.37 = load i64, ptr %local.6, align 8
  %v.38 = add i64 0, 4
  %v.39 = icmp eq i64 %v.37, %v.38
  br i1 %v.39, label %bb.17, label %bb.18
bb.17:
  %tmp.24 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.24, label %resource.drop.13, label %resource.done.13
resource.drop.13:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.bytes.drop(ptr %local.9)
  br label %resource.done.13
resource.done.13:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.25 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.25, label %resource.drop.14, label %resource.done.14
resource.drop.14:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.6(ptr %local.8)
  br label %resource.done.14
resource.done.14:
  store i1 false, ptr %resource.live.8, align 1
  br label %bb.4
bb.18:
  br label %bb.19
bb.19:
  %v.40 = load ptr, ptr %local.0, align 8
  %v.41 = getelementptr i8, ptr %v.40, i64 0
  %v.42 = load ptr, ptr %local.1, align 8
  %v.43 = getelementptr i8, ptr %v.42, i64 0
  %v.44 = call i32 @zil.fn.9(ptr %v.41, ptr %v.43)
  store i32 %v.44, ptr %local.11, align 4
  %v.45 = load i32, ptr %local.11, align 4
  %v.46 = add i32 0, 0
  %v.47 = icmp slt i32 %v.45, %v.46
  br i1 %v.47, label %bb.20, label %bb.21
bb.20:
  %v.48 = add i32 0, 1
  %tmp.26 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.26, label %resource.drop.15, label %resource.done.15
resource.drop.15:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.bytes.drop(ptr %local.9)
  br label %resource.done.15
resource.done.15:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.27 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.27, label %resource.drop.16, label %resource.done.16
resource.drop.16:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.6(ptr %local.8)
  br label %resource.done.16
resource.done.16:
  store i1 false, ptr %resource.live.8, align 1
  ret i32 %v.48
bb.21:
  br label %bb.22
bb.22:
  %v.49 = load i32, ptr %local.11, align 4
  %v.50 = sext i32 %v.49 to i64
  %v.51 = load i64, ptr %local.4, align 8
  %v.52 = add i64 0, 1
  %tmp.28 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.51, i64 %v.52)
  %v.53 = extractvalue { i64, i1 } %tmp.28, 0
  %tmp.29 = extractvalue { i64, i1 } %tmp.28, 1
  br i1 %tmp.29, label %zil.trap, label %checked.17
checked.17:
  %v.54 = icmp ne i64 %v.50, %v.53
  br i1 %v.54, label %bb.23, label %bb.24
bb.23:
  %v.55 = add i32 0, 7
  %tmp.30 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.30, label %resource.drop.18, label %resource.done.18
resource.drop.18:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.bytes.drop(ptr %local.9)
  br label %resource.done.18
resource.done.18:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.31 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.31, label %resource.drop.19, label %resource.done.19
resource.drop.19:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.6(ptr %local.8)
  br label %resource.done.19
resource.done.19:
  store i1 false, ptr %resource.live.8, align 1
  ret i32 %v.55
bb.24:
  br label %bb.25
bb.25:
  %v.56 = load ptr, ptr %local.2, align 8
  %v.57 = load i64, ptr %local.4, align 8
  %v.58 = add i64 0, 1
  %tmp.32 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.57, i64 %v.58)
  %v.59 = extractvalue { i64, i1 } %tmp.32, 0
  %tmp.33 = extractvalue { i64, i1 } %tmp.32, 1
  br i1 %tmp.33, label %zil.trap, label %checked.20
checked.20:
  %v.60 = call i1 @zil.bytes.reserve(ptr %v.56, i64 %v.59)
  %v.61 = xor i1 %v.60, true
  br i1 %v.61, label %bb.26, label %bb.27
bb.26:
  %v.62 = add i32 0, 1
  %tmp.34 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.34, label %resource.drop.21, label %resource.done.21
resource.drop.21:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.bytes.drop(ptr %local.9)
  br label %resource.done.21
resource.done.21:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.35 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.35, label %resource.drop.22, label %resource.done.22
resource.drop.22:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.6(ptr %local.8)
  br label %resource.done.22
resource.done.22:
  store i1 false, ptr %resource.live.8, align 1
  ret i32 %v.62
bb.27:
  br label %bb.28
bb.28:
  %v.63 = load ptr, ptr %local.2, align 8
  %v.64 = load ptr, ptr %local.1, align 8
  %v.65 = load i32, ptr %v.64, align 4
  %v.66 = add i32 0, 1
  %tmp.36 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.65, i32 %v.66)
  %v.67 = extractvalue { i32, i1 } %tmp.36, 0
  %tmp.37 = extractvalue { i32, i1 } %tmp.36, 1
  br i1 %tmp.37, label %zil.trap, label %checked.23
checked.23:
  %tmp.38 = icmp ugt i32 %v.67, 255
  br i1 %tmp.38, label %zil.trap, label %checked.24
checked.24:
  %v.68 = trunc i32 %v.67 to i8
  %v.69 = call i1 @zil.bytes.push(ptr %v.63, i8 %v.68)
  %v.70 = xor i1 %v.69, true
  br i1 %v.70, label %bb.29, label %bb.30
bb.29:
  %v.71 = add i32 0, 2
  %tmp.39 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.39, label %resource.drop.25, label %resource.done.25
resource.drop.25:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.bytes.drop(ptr %local.9)
  br label %resource.done.25
resource.done.25:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.40 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.40, label %resource.drop.26, label %resource.done.26
resource.drop.26:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.6(ptr %local.8)
  br label %resource.done.26
resource.done.26:
  store i1 false, ptr %resource.live.8, align 1
  ret i32 %v.71
bb.30:
  br label %bb.31
bb.31:
  %v.72 = load ptr, ptr %local.1, align 8
  %v.73 = load i32, ptr %v.72, align 4
  %v.74 = add i32 0, 1
  %tmp.41 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.73, i32 %v.74)
  %v.75 = extractvalue { i32, i1 } %tmp.41, 0
  %tmp.42 = extractvalue { i32, i1 } %tmp.41, 1
  br i1 %tmp.42, label %zil.trap, label %checked.27
checked.27:
  call void @zil.fn.5(i32 %v.75)
  %v.77 = load i64, ptr %local.4, align 8
  %v.78 = add i64 0, 1
  %tmp.43 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.77, i64 %v.78)
  %v.79 = extractvalue { i64, i1 } %tmp.43, 0
  %tmp.44 = extractvalue { i64, i1 } %tmp.43, 1
  br i1 %tmp.44, label %zil.trap, label %checked.28
checked.28:
  store i64 %v.79, ptr %local.4, align 8
  %tmp.45 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.45, label %resource.drop.29, label %resource.done.29
resource.drop.29:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.bytes.drop(ptr %local.9)
  br label %resource.done.29
resource.done.29:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.46 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.46, label %resource.drop.30, label %resource.done.30
resource.drop.30:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.6(ptr %local.8)
  br label %resource.done.30
resource.done.30:
  store i1 false, ptr %resource.live.8, align 1
  br label %bb.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.11({ { ptr, i64, i64 }, %zil_env_0 } %arg.0, i64 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %local.1 = alloca i64, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %local.3 = alloca i32, align 4
  store { { ptr, i64, i64 }, %zil_env_0 } %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store { { ptr, i64, i64 }, %zil_env_0 } %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.1 = add i32 0, 65
  store i32 %v.1, ptr %local.3, align 4
  %v.2 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 1
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = getelementptr i8, ptr %local.3, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 0
  %v.9 = load i64, ptr %local.1, align 8
  %v.10 = call i32 @zil.fn.10(ptr %v.4, ptr %v.7, ptr %v.8, i64 %v.9)
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.1 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.1)
  %tmp.2 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.2)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.3 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.3, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.4 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.4)
  %tmp.5 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.5)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

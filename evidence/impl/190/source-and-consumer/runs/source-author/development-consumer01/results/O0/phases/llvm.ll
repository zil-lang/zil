; ZIL verified LLVM module
; verified-fingerprint: c29a36cf7dd3c04c07a0dc3435c24fa86e8f56713de76f1fd3ed5c19b9b45a4a
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
declare { i64, i1 } @llvm.ssub.with.overflow.i64(i64, i64)

declare i32 @getchar() nonlazybind
define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca %zil_env_0, align 8
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca %zil_env_0, align 8
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.5, align 1
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
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.7, align 1
  ret i32 %v.10
bb.5:
  br label %bb.6
bb.6:
  %v.11 = add i32 0, 65
  store i32 %v.11, ptr %local.2, align 4
  %v.12 = call %zil_env_0 @zil.fn.3()
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_0 %v.12, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.13 = getelementptr i8, ptr %local.3, i64 0
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  %v.16 = getelementptr i8, ptr %local.2, i64 0
  %v.17 = getelementptr i8, ptr %v.16, i64 0
  %v.18 = getelementptr i8, ptr %v.17, i64 0
  %v.19 = call i32 @zil.fn.7(ptr %v.15, ptr %v.18)
  store i32 %v.19, ptr %local.4, align 4
  %v.20 = load i32, ptr %local.4, align 4
  %v.21 = add i32 0, 0
  %v.22 = icmp slt i32 %v.20, %v.21
  br i1 %v.22, label %bb.7, label %bb.8
bb.7:
  %v.23 = add i32 0, 1
  %tmp.0 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.0(ptr %local.3)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.7, align 1
  ret i32 %v.23
bb.8:
  br label %bb.9
bb.9:
  %v.24 = load i32, ptr %local.4, align 4
  %v.25 = add i32 0, 1
  %v.26 = icmp ne i32 %v.24, %v.25
  br i1 %v.26, label %bb.10, label %bb.11
bb.10:
  %v.27 = add i32 0, 8
  %tmp.1 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.1, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.0(ptr %local.3)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.7, align 1
  ret i32 %v.27
bb.11:
  br label %bb.12
bb.12:
  %v.28 = load i32, ptr %local.2, align 4
  %v.29 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.28, i32 %v.29)
  %v.30 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.2
checked.2:
  call void @zil.fn.5(i32 %v.30)
  %v.32 = load %zil_env_0, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_0 %v.32, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.33 = load %zil_env_0, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  %v.34 = call { { ptr, i64, i64 }, %zil_env_0 } @zil.fn.8(%zil_env_0 %v.33)
  %tmp.4 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.4, label %closure.drop.3, label %closure.done.3
closure.drop.3:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.closure.drop.0(ptr %local.5)
  br label %closure.done.3
closure.done.3:
  store i1 false, ptr %resource.live.6, align 1
  store { { ptr, i64, i64 }, %zil_env_0 } %v.34, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  %v.35 = load { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store { { ptr, i64, i64 }, %zil_env_0 } %v.35, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.36 = load i32, ptr %local.2, align 4
  %v.37 = load i32, ptr %local.0, align 4
  %v.38 = add i32 0, 48
  %tmp.5 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.37, i32 %v.38)
  %v.39 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.4
checked.4:
  %v.40 = sext i32 %v.39 to i64
  %v.41 = load { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
  %v.42 = call i32 @zil.fn.10({ { ptr, i64, i64 }, %zil_env_0 } %v.41, i32 %v.36, i64 %v.40)
  %tmp.7 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.7, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.8 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.7, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.8)
  %tmp.9 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.7, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.9)
  br label %package.done.5
package.done.5:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.10 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.10, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.11 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.6, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.11)
  %tmp.12 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.6, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.12)
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.13 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.13, label %closure.drop.7, label %closure.done.7
closure.drop.7:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.0(ptr %local.3)
  br label %closure.done.7
closure.done.7:
  store i1 false, ptr %resource.live.3, align 1
  ret i32 %v.42
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

define i32 @zil.fn.6(ptr %env, ptr %arg.0) {
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

define i32 @zil.fn.7(ptr %arg.0, ptr %arg.1) {
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
  %v.3 = call i32 @zil.fn.6(ptr %v.0, ptr %v.2)
  ret i32 %v.3
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

define i32 @zil.fn.9(ptr %arg.0, ptr %arg.1, ptr %arg.2, i64 %arg.3) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  %local.2 = alloca ptr, align 8
  %local.3 = alloca i64, align 8
  %local.4 = alloca i64, align 8
  %local.5 = alloca i32, align 4
  %local.6 = alloca { ptr, i64 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  store ptr %arg.2, ptr %local.2, align 8
  store i64 %arg.3, ptr %local.3, align 8
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 0
  store i64 %v.0, ptr %local.4, align 8
  br label %bb.1
bb.1:
  %v.1 = load i64, ptr %local.4, align 8
  %v.2 = load i64, ptr %local.3, align 8
  %v.3 = icmp slt i64 %v.1, %v.2
  br i1 %v.3, label %bb.2, label %bb.3
bb.2:
  %v.4 = load ptr, ptr %local.0, align 8
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = load ptr, ptr %local.1, align 8
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = call i32 @zil.fn.7(ptr %v.5, ptr %v.7)
  store i32 %v.8, ptr %local.5, align 4
  %v.9 = load i32, ptr %local.5, align 4
  %v.10 = add i32 0, 0
  %v.11 = icmp slt i32 %v.9, %v.10
  br i1 %v.11, label %bb.4, label %bb.5
bb.3:
  %v.48 = add i32 0, 0
  ret i32 %v.48
bb.4:
  %v.12 = add i32 0, 1
  ret i32 %v.12
bb.5:
  br label %bb.6
bb.6:
  %v.13 = load i64, ptr %local.4, align 8
  %v.14 = add i64 0, 1
  %tmp.0 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.13, i64 %v.14)
  %v.15 = extractvalue { i64, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i64, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i64 %v.15, ptr %local.4, align 8
  %v.16 = load i32, ptr %local.5, align 4
  %v.17 = sext i32 %v.16 to i64
  %v.18 = load i64, ptr %local.4, align 8
  %v.19 = add i64 0, 1
  %tmp.2 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.18, i64 %v.19)
  %v.20 = extractvalue { i64, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i64, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %v.21 = icmp ne i64 %v.17, %v.20
  br i1 %v.21, label %bb.7, label %bb.8
bb.7:
  %v.22 = add i32 0, 7
  ret i32 %v.22
bb.8:
  br label %bb.9
bb.9:
  %v.23 = load ptr, ptr %local.2, align 8
  %v.24 = load i64, ptr %local.4, align 8
  %v.25 = call i1 @zil.bytes.reserve(ptr %v.23, i64 %v.24)
  %v.26 = xor i1 %v.25, true
  br i1 %v.26, label %bb.10, label %bb.11
bb.10:
  %v.27 = add i32 0, 1
  ret i32 %v.27
bb.11:
  br label %bb.12
bb.12:
  %v.28 = load ptr, ptr %local.2, align 8
  %v.29 = load ptr, ptr %local.1, align 8
  %v.30 = load i32, ptr %v.29, align 4
  %v.31 = add i32 0, 1
  %tmp.4 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.30, i32 %v.31)
  %v.32 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %tmp.6 = icmp ugt i32 %v.32, 255
  br i1 %tmp.6, label %zil.trap, label %checked.3
checked.3:
  %v.33 = trunc i32 %v.32 to i8
  %v.34 = call i1 @zil.bytes.push(ptr %v.28, i8 %v.33)
  %v.35 = xor i1 %v.34, true
  br i1 %v.35, label %bb.13, label %bb.14
bb.13:
  %v.36 = add i32 0, 2
  ret i32 %v.36
bb.14:
  br label %bb.15
bb.15:
  %v.37 = load ptr, ptr %local.2, align 8
  %tmp.7 = load ptr, ptr %v.37, align 8
  %tmp.8 = getelementptr { ptr, i64, i64 }, ptr %v.37, i32 0, i32 1
  %tmp.9 = load i64, ptr %tmp.8, align 8
  %tmp.10 = insertvalue { ptr, i64 } poison, ptr %tmp.7, 0
  %v.38 = insertvalue { ptr, i64 } %tmp.10, i64 %tmp.9, 1
  store { ptr, i64 } %v.38, ptr %local.6, align 8
  %v.39 = load { ptr, i64 }, ptr %local.6, align 8
  %v.40 = load i64, ptr %local.4, align 8
  %v.41 = add i64 0, 1
  %tmp.11 = call { i64, i1 } @llvm.ssub.with.overflow.i64(i64 %v.40, i64 %v.41)
  %v.42 = extractvalue { i64, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i64, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.4
checked.4:
  %tmp.13 = extractvalue { ptr, i64 } %v.39, 0
  %tmp.14 = extractvalue { ptr, i64 } %v.39, 1
  %tmp.15 = icmp uge i64 %v.42, %tmp.14
  br i1 %tmp.15, label %zil.trap, label %checked.5
checked.5:
  %v.43 = getelementptr i8, ptr %tmp.13, i64 %v.42
  %v.44 = getelementptr i8, ptr %v.43, i64 0
  %v.45 = load i8, ptr %v.44, align 1
  %v.46 = zext i8 %v.45 to i32
  call void @zil.fn.5(i32 %v.46)
  br label %bb.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.10({ { ptr, i64, i64 }, %zil_env_0 } %arg.0, i32 %arg.1, i64 %arg.2) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i64, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %local.4 = alloca i32, align 4
  store { { ptr, i64, i64 }, %zil_env_0 } %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  store i64 %arg.2, ptr %local.2, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.3, align 1
  store { { ptr, i64, i64 }, %zil_env_0 } %v.0, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.1 = load i32, ptr %local.1, align 4
  store i32 %v.1, ptr %local.4, align 4
  %v.2 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 1
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = getelementptr i8, ptr %local.4, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 0
  %v.9 = load i64, ptr %local.2, align 8
  %v.10 = call i32 @zil.fn.9(ptr %v.4, ptr %v.7, ptr %v.8, i64 %v.9)
  %tmp.0 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.1 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.1)
  %tmp.2 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.2)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.3, align 1
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

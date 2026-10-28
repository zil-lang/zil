; ZIL verified LLVM module
; verified-fingerprint: f70637b0f02c2b643e9b2105de8c0a0675e47d466e48f7212ed86b932f855486
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
  %local.3 = alloca { { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
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
  store i1 false, ptr %resource.live.3, align 1
  ret i32 %v.10
bb.5:
  br label %bb.6
bb.6:
  %v.11 = call %zil_env_0 @zil.fn.3()
  store %zil_env_0 %v.11, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.12 = load %zil_env_0, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.13 = call { { ptr, i64, i64 }, %zil_env_0, i32 } @zil.fn.7(%zil_env_0 %v.12)
  store { { ptr, i64, i64 }, %zil_env_0, i32 } %v.13, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.14 = load i32, ptr %local.0, align 4
  %v.15 = add i32 0, 48
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.14, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.17 = sext i32 %v.16 to i64
  %v.18 = load { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.19 = call i32 @zil.fn.10({ { ptr, i64, i64 }, %zil_env_0, i32 } %v.18, i64 %v.17)
  %tmp.2 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.2, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.3 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.3)
  %tmp.4 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.4)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.5 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.5, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.19
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

define i32 @zil.fn.6(ptr %env, { ptr, i64 } %arg.0, ptr %arg.1) {
entry:
  %local.0 = alloca { ptr, i64 }, align 8
  %local.1 = alloca ptr, align 8
  %local.2 = alloca ptr, align 8
  %local.3 = alloca ptr, align 8
  store { ptr, i64 } %arg.0, ptr %local.0, align 8
  store ptr %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.2, align 8
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.3, align 8
  %v.2 = load ptr, ptr %local.2, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.2, i32 0, i32 0
  %v.3 = load i32, ptr %tmp.0, align 4
  %v.4 = load ptr, ptr %local.3, align 8
  %v.5 = load ptr, ptr %local.3, align 8
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
  %v.12 = load ptr, ptr %local.3, align 8
  %v.13 = load ptr, ptr %local.1, align 8
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
  %v.19 = load { ptr, i64 }, ptr %local.0, align 8
  %v.20 = add i32 0, 0
  %tmp.5 = extractvalue { ptr, i64 } %v.19, 0
  %tmp.6 = extractvalue { ptr, i64 } %v.19, 1
  %tmp.7 = sext i32 %v.20 to i64
  %tmp.8 = icmp uge i64 %tmp.7, %tmp.6
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  %v.21 = getelementptr i8, ptr %tmp.5, i64 %tmp.7
  %v.22 = load ptr, ptr %local.1, align 8
  %v.23 = load i32, ptr %v.22, align 4
  %tmp.9 = icmp ugt i32 %v.23, 255
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  %v.24 = trunc i32 %v.23 to i8
  store i8 %v.24, ptr %v.21, align 1
  %v.25 = load ptr, ptr %local.1, align 8
  %v.26 = load ptr, ptr %local.1, align 8
  %v.27 = load i32, ptr %v.26, align 4
  %v.28 = add i32 0, 1
  %tmp.10 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.27, i32 %v.28)
  %v.29 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.4
checked.4:
  store i32 %v.29, ptr %v.25, align 4
  %v.30 = load ptr, ptr %local.3, align 8
  %tmp.12 = getelementptr { ptr, i64, i64 }, ptr %v.30, i32 0, i32 1
  %v.31 = load i64, ptr %tmp.12, align 8
  %tmp.13 = icmp slt i64 %v.31, -2147483648
  %tmp.14 = icmp sgt i64 %v.31, 2147483647
  %tmp.15 = or i1 %tmp.13, %tmp.14
  br i1 %tmp.15, label %zil.trap, label %checked.5
checked.5:
  %v.32 = trunc i64 %v.31 to i32
  ret i32 %v.32
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { ptr, i64, i64 }, %zil_env_0, i32 } @zil.fn.7(%zil_env_0 %arg.0) {
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
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca %zil_env_0, align 8
  store %zil_env_0 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = load %zil_env_0, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.1, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = add i32 0, 65
  store i32 %v.2, ptr %local.3, align 4
  %v.3 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.3, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.4 = load %zil_env_0, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.4, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.5 = load i32, ptr %local.3, align 4
  %v.6 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  %v.7 = load %zil_env_0, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
%tmp.0 = insertvalue { { ptr, i64, i64 }, %zil_env_0, i32 } zeroinitializer, { ptr, i64, i64 } %v.6, 0
%tmp.1 = insertvalue { { ptr, i64, i64 }, %zil_env_0, i32 } %tmp.0, %zil_env_0 %v.7, 1
%v.8 = insertvalue { { ptr, i64, i64 }, %zil_env_0, i32 } %tmp.1, i32 %v.5, 2
  %tmp.2 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.2, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.closure.drop.0(ptr %local.5)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.3 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.4, align 1
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
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { { ptr, i64, i64 }, %zil_env_0, i32 } %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.8(ptr %arg.0, { ptr, i64 } %arg.1, ptr %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  %local.2 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { ptr, i64 } %arg.1, ptr %local.1, align 8
  store ptr %arg.2, ptr %local.2, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.1, 0
  %v.2 = insertvalue { ptr, i64 } %v.1, ptr %tmp.0, 0
  %v.3 = load ptr, ptr %local.2, align 8
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = call i32 @zil.fn.6(ptr %v.0, { ptr, i64 } %v.2, ptr %v.4)
  ret i32 %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.9(ptr %arg.0, { ptr, i64 } %arg.1, ptr %arg.2, i64 %arg.3) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  %local.2 = alloca ptr, align 8
  %local.3 = alloca i64, align 8
  %local.4 = alloca i64, align 8
  %local.5 = alloca i64, align 8
  %local.6 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { ptr, i64 } %arg.1, ptr %local.1, align 8
  store ptr %arg.2, ptr %local.2, align 8
  store i64 %arg.3, ptr %local.3, align 8
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 0
  %v.1 = load i64, ptr %local.3, align 8
  store i64 %v.0, ptr %local.4, align 8
  br label %bb.1
bb.1:
  %v.2 = load i64, ptr %local.4, align 8
  %v.3 = icmp slt i64 %v.2, %v.1
  br i1 %v.3, label %bb.2, label %bb.4
bb.2:
  store i64 %v.2, ptr %local.5, align 8
  %v.4 = load ptr, ptr %local.0, align 8
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = load { ptr, i64 }, ptr %local.1, align 8
  %tmp.0 = extractvalue { ptr, i64 } %v.6, 0
  %v.7 = insertvalue { ptr, i64 } %v.6, ptr %tmp.0, 0
  %v.8 = load ptr, ptr %local.2, align 8
  %v.9 = getelementptr i8, ptr %v.8, i64 0
  %v.10 = call i32 @zil.fn.8(ptr %v.5, { ptr, i64 } %v.7, ptr %v.9)
  store i32 %v.10, ptr %local.6, align 4
  %v.11 = load i32, ptr %local.6, align 4
  %v.12 = add i32 0, 0
  %v.13 = icmp slt i32 %v.11, %v.12
  br i1 %v.13, label %bb.5, label %bb.6
bb.3:
  %v.41 = add i64 0, 1
  %tmp.1 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.2, i64 %v.41)
  %v.42 = extractvalue { i64, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i64, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i64 %v.42, ptr %local.4, align 8
  br label %bb.1
bb.4:
  %v.43 = add i32 0, 0
  ret i32 %v.43
bb.5:
  %v.14 = add i32 0, 1
  ret i32 %v.14
bb.6:
  br label %bb.7
bb.7:
  %v.15 = load i32, ptr %local.6, align 4
  %v.16 = sext i32 %v.15 to i64
  %v.17 = load i64, ptr %local.5, align 8
  %v.18 = add i64 0, 1
  %tmp.3 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.17, i64 %v.18)
  %v.19 = extractvalue { i64, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i64, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.20 = icmp ne i64 %v.16, %v.19
  br i1 %v.20, label %bb.8, label %bb.9
bb.8:
  %v.21 = add i32 0, 7
  ret i32 %v.21
bb.9:
  br label %bb.10
bb.10:
  %v.22 = load { ptr, i64 }, ptr %local.1, align 8
  %v.23 = add i32 0, 0
  %tmp.5 = extractvalue { ptr, i64 } %v.22, 0
  %tmp.6 = extractvalue { ptr, i64 } %v.22, 1
  %tmp.7 = sext i32 %v.23 to i64
  %tmp.8 = icmp uge i64 %tmp.7, %tmp.6
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  %v.24 = getelementptr i8, ptr %tmp.5, i64 %tmp.7
  %v.25 = getelementptr i8, ptr %v.24, i64 0
  %v.26 = load i8, ptr %v.25, align 1
  %v.27 = zext i8 %v.26 to i32
  %v.28 = load ptr, ptr %local.2, align 8
  %v.29 = load i32, ptr %v.28, align 4
  %v.30 = add i32 0, 1
  %tmp.9 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.29, i32 %v.30)
  %v.31 = extractvalue { i32, i1 } %tmp.9, 0
  %tmp.10 = extractvalue { i32, i1 } %tmp.9, 1
  br i1 %tmp.10, label %zil.trap, label %checked.3
checked.3:
  %v.32 = icmp ne i32 %v.27, %v.31
  br i1 %v.32, label %bb.11, label %bb.12
bb.11:
  %v.33 = add i32 0, 8
  ret i32 %v.33
bb.12:
  br label %bb.13
bb.13:
  %v.34 = load { ptr, i64 }, ptr %local.1, align 8
  %v.35 = add i32 0, 0
  %tmp.11 = extractvalue { ptr, i64 } %v.34, 0
  %tmp.12 = extractvalue { ptr, i64 } %v.34, 1
  %tmp.13 = sext i32 %v.35 to i64
  %tmp.14 = icmp uge i64 %tmp.13, %tmp.12
  br i1 %tmp.14, label %zil.trap, label %checked.4
checked.4:
  %v.36 = getelementptr i8, ptr %tmp.11, i64 %tmp.13
  %v.37 = getelementptr i8, ptr %v.36, i64 0
  %v.38 = load i8, ptr %v.37, align 1
  %v.39 = zext i8 %v.38 to i32
  call void @zil.fn.5(i32 %v.39)
  br label %bb.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.10({ { ptr, i64, i64 }, %zil_env_0, i32 } %arg.0, i64 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  %local.1 = alloca i64, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  %local.3 = alloca { ptr, i64 }, align 8
  %local.4 = alloca i32, align 4
  store { { ptr, i64, i64 }, %zil_env_0, i32 } %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store { { ptr, i64, i64 }, %zil_env_0, i32 } %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.1 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  %v.2 = add i64 0, 1
  %v.3 = call i1 @zil.bytes.reserve(ptr %v.1, i64 %v.2)
  %v.4 = xor i1 %v.3, true
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %v.5 = add i32 0, 1
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.1 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.1)
  %tmp.2 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.2)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.3 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.3, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.4 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.4)
  %tmp.5 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.5)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.5
bb.2:
  br label %bb.3
bb.3:
  %v.6 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  %v.7 = add i8 0, 0
  %v.8 = call i1 @zil.bytes.push(ptr %v.6, i8 %v.7)
  %v.9 = xor i1 %v.8, true
  br i1 %v.9, label %bb.4, label %bb.5
bb.4:
  %v.10 = add i32 0, 2
  %tmp.6 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.6, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.7 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.7)
  %tmp.8 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.8)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.9 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.9, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.10 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.10)
  %tmp.11 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.11)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.10
bb.5:
  br label %bb.6
bb.6:
  %v.11 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  %tmp.12 = load ptr, ptr %v.11, align 8
  %tmp.13 = getelementptr { ptr, i64, i64 }, ptr %v.11, i32 0, i32 1
  %tmp.14 = load i64, ptr %tmp.13, align 8
  %tmp.15 = insertvalue { ptr, i64 } poison, ptr %tmp.12, 0
  %v.12 = insertvalue { ptr, i64 } %tmp.15, i64 %tmp.14, 1
  store { ptr, i64 } %v.12, ptr %local.3, align 8
  %v.13 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  %v.16 = load { ptr, i64 }, ptr %local.3, align 8
  %tmp.16 = extractvalue { ptr, i64 } %v.16, 0
  %v.17 = insertvalue { ptr, i64 } %v.16, ptr %tmp.16, 0
  %v.18 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 2
  %v.19 = getelementptr i8, ptr %v.18, i64 0
  %v.20 = getelementptr i8, ptr %v.19, i64 0
  %v.21 = add i64 0, 0
  %v.22 = call i32 @zil.fn.9(ptr %v.15, { ptr, i64 } %v.17, ptr %v.20, i64 %v.21)
  store i32 %v.22, ptr %local.4, align 4
  %v.23 = load i32, ptr %local.4, align 4
  %v.24 = add i32 0, 0
  %v.25 = icmp ne i32 %v.23, %v.24
  br i1 %v.25, label %bb.7, label %bb.8
bb.7:
  %v.26 = add i32 0, 6
  %tmp.17 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.17, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.18 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.18)
  %tmp.19 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.19)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.20 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.20, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.21 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.21)
  %tmp.22 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.22)
  br label %package.done.5
package.done.5:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.26
bb.8:
  br label %bb.9
bb.9:
  %v.27 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  %v.28 = getelementptr i8, ptr %v.27, i64 0
  %v.29 = getelementptr i8, ptr %v.28, i64 0
  %v.30 = load { ptr, i64 }, ptr %local.3, align 8
  %tmp.23 = extractvalue { ptr, i64 } %v.30, 0
  %v.31 = insertvalue { ptr, i64 } %v.30, ptr %tmp.23, 0
  %v.32 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 2
  %v.33 = getelementptr i8, ptr %v.32, i64 0
  %v.34 = getelementptr i8, ptr %v.33, i64 0
  %v.35 = load i64, ptr %local.1, align 8
  %v.36 = call i32 @zil.fn.9(ptr %v.29, { ptr, i64 } %v.31, ptr %v.34, i64 %v.35)
  %tmp.24 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.24, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.25 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.25)
  %tmp.26 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.26)
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.27 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.27, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.28 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.bytes.drop(ptr %tmp.28)
  %tmp.29 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.29)
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.36
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

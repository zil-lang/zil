; ZIL verified LLVM module
; verified-fingerprint: f86cd55d64c900b4824d04e9acc3da59c17f86fe822968fea9c44b95f6a14aaf
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32, { i32 }, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  call void @zil.fn.1(ptr %payload.1)
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

define internal i1 @zil.store.1.reserve(ptr %head, i64 %requested) {
entry:
  %stride = ptrtoint ptr getelementptr ({ { i32 }, i32 }, ptr null, i64 1) to i64
  %maximum = udiv i64 9223372036854775807, %stride
  %positive = icmp sge i64 %requested, 0
  %bounded = icmp ule i64 %requested, %maximum
  %valid = and i1 %positive, %bounded
  br i1 %valid, label %capacity, label %failed
capacity:
  %cap.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 2
  %cap = load i64, ptr %cap.field, align 8
  %fits = icmp ule i64 %requested, %cap
  br i1 %fits, label %done, label %prepare
prepare:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %old = load ptr, ptr %ptr.field, align 8
  %size = mul i64 %requested, %stride
  %next = call ptr @zil.bytes.reallocate(ptr %old, i64 %size)
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
define internal { i1, { { i32 }, i32 }, {} } @zil.store.1.push(ptr %head, { { i32 }, i32 } %item) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %cap.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 2
  %len = load i64, ptr %len.field, align 8
  %cap = load i64, ptr %cap.field, align 8
  %room = icmp ult i64 %len, %cap
  br i1 %room, label %initialize, label %full
initialize:
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %data = load ptr, ptr %ptr.field, align 8
  %slot = getelementptr { { i32 }, i32 }, ptr %data, i64 %len
  store { { i32 }, i32 } %item, ptr %slot, align 4
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret { i1, { { i32 }, i32 }, {} } zeroinitializer
full:
  %tag = insertvalue { i1, { { i32 }, i32 }, {} } zeroinitializer, i1 true, 0
  %error = insertvalue { i1, { { i32 }, i32 }, {} } %tag, { { i32 }, i32 } %item, 1
  ret { i1, { { i32 }, i32 }, {} } %error
}
define internal { i1, {}, { { i32 }, i32 } } @zil.store.1.pop(ptr %head) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %len = load i64, ptr %len.field, align 8
  %live = icmp ne i64 %len, 0
  br i1 %live, label %take, label %empty
take:
  %next.len = sub i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %data = load ptr, ptr %ptr.field, align 8
  %slot = getelementptr { { i32 }, i32 }, ptr %data, i64 %next.len
  %item = load { { i32 }, i32 }, ptr %slot, align 4
  %result = insertvalue { i1, {}, { { i32 }, i32 } } zeroinitializer, { { i32 }, i32 } %item, 2
  ret { i1, {}, { { i32 }, i32 } } %result
empty:
  %error = insertvalue { i1, {}, { { i32 }, i32 } } zeroinitializer, i1 true, 0
  ret { i1, {}, { { i32 }, i32 } } %error
}
define internal void @zil.store.1.drop(ptr %head) {
entry:
  %len.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 1
  %ptr.field = getelementptr { ptr, i64, i64 }, ptr %head, i32 0, i32 0
  %len = load i64, ptr %len.field, align 8
  %data = load ptr, ptr %ptr.field, align 8
  store i64 0, ptr %len.field, align 8
  br label %loop
loop:
  %index = phi i64 [0, %entry], [%next, %element]
  %remaining = icmp ult i64 %index, %len
  br i1 %remaining, label %element, label %release
element:
  %slot = getelementptr { { i32 }, i32 }, ptr %data, i64 %index
  %field.0 = getelementptr { { i32 }, i32 }, ptr %slot, i32 0, i32 0
  call void @zil.fn.1(ptr %field.0)
  %next = add i64 %index, 1
  br label %loop
release:
  call void @zil.bytes.release(ptr %data)
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

define { { ptr, i64, i64 }, i32 } @zil.fn.2(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32 }, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i1, { { i32 }, i32 }, {} }, align 4
  %local.7 = alloca i32, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { ptr, i64, i64 }, align 8
  %local.9 = alloca i32, align 4
  %resource.live.10 = alloca i1, align 1
  store i1 false, ptr %resource.live.10, align 1
  %local.10 = alloca { ptr, i64, i64 }, align 8
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = getelementptr i8, ptr %local.1, i64 0
  %v.2 = add i64 0, 1
  %v.3 = call i1 @zil.store.1.reserve(ptr %v.1, i64 %v.2)
  br i1 %v.3, label %bb.1, label %bb.2
bb.1:
  store i1 false, ptr %resource.live.6, align 1
  %v.4 = getelementptr i8, ptr %local.1, i64 0
  store i1 false, ptr %resource.live.5, align 1
  %v.5 = load i32, ptr %local.0, align 4
  store i32 %v.5, ptr %local.2, align 4
  %v.6 = load i32, ptr %local.2, align 4
%v.7 = insertvalue { i32 } zeroinitializer, i32 %v.6, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.7, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.8 = load i32, ptr %local.0, align 4
  store i32 %v.8, ptr %local.4, align 4
  %v.9 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.9, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.10 = load i32, ptr %local.4, align 4
  %v.11 = load { i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
%tmp.0 = insertvalue { { i32 }, i32 } zeroinitializer, { i32 } %v.11, 0
%v.12 = insertvalue { { i32 }, i32 } %tmp.0, i32 %v.10, 1
  %tmp.1 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.1(ptr %local.5)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.2 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.1(ptr %local.3)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.3, align 1
  %v.13 = call { i1, { { i32 }, i32 }, {} } @zil.store.1.push(ptr %v.4, { { i32 }, i32 } %v.12)
  store { i1, { { i32 }, i32 }, {} } %v.13, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %tmp.3 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.3, label %error.drop.2, label %error.done.2
error.drop.2:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.4 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.6, i32 0, i32 0
  %tmp.5 = load i1, ptr %tmp.4, align 1
  br i1 %tmp.5, label %error.payload.2, label %success.payload.2
error.payload.2:
  %tmp.6 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.6, i32 0, i32 1
  %tmp.7 = getelementptr { { i32 }, i32 }, ptr %tmp.6, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.7)
  br label %error.done.2
success.payload.2:
  br label %error.done.2
error.done.2:
  store i1 false, ptr %resource.live.6, align 1
  br label %bb.3
bb.2:
  br label %bb.3
bb.3:
  %v.14 = getelementptr i8, ptr %local.1, i64 0
  %tmp.8 = getelementptr { ptr, i64, i64 }, ptr %v.14, i32 0, i32 1
  %v.15 = load i64, ptr %tmp.8, align 8
  %tmp.9 = icmp slt i64 %v.15, -2147483648
  %tmp.10 = icmp sgt i64 %v.15, 2147483647
  %tmp.11 = or i1 %tmp.9, %tmp.10
  br i1 %tmp.11, label %zil.trap, label %checked.3
checked.3:
  %v.16 = trunc i64 %v.15 to i32
  store i32 %v.16, ptr %local.7, align 4
  store i1 false, ptr %resource.live.10, align 1
  %v.17 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.8, align 1
  store { ptr, i64, i64 } %v.17, ptr %local.8, align 8
  store i1 true, ptr %resource.live.8, align 1
  %v.18 = load i32, ptr %local.7, align 4
  store i32 %v.18, ptr %local.9, align 4
  %v.19 = load { ptr, i64, i64 }, ptr %local.8, align 8
  store i1 false, ptr %resource.live.8, align 1
  store { ptr, i64, i64 } %v.19, ptr %local.10, align 8
  store i1 true, ptr %resource.live.10, align 1
  %v.20 = load i32, ptr %local.9, align 4
  %v.21 = load { ptr, i64, i64 }, ptr %local.10, align 8
  store i1 false, ptr %resource.live.10, align 1
%tmp.12 = insertvalue { { ptr, i64, i64 }, i32 } zeroinitializer, { ptr, i64, i64 } %v.21, 0
%v.22 = insertvalue { { ptr, i64, i64 }, i32 } %tmp.12, i32 %v.20, 1
  %tmp.13 = load i1, ptr %resource.live.10, align 1
  br i1 %tmp.13, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.10, align 1
  call void @zil.store.1.drop(ptr %local.10)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.10, align 1
  %tmp.14 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.14, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.store.1.drop(ptr %local.8)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.15 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.15, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.store.1.drop(ptr %local.1)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.1, align 1
  ret { { ptr, i64, i64 }, i32 } %v.22
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.0, i32 0, i32 1
  %v.1 = load i64, ptr %tmp.0, align 8
  %tmp.1 = icmp slt i64 %v.1, -2147483648
  %tmp.2 = icmp sgt i64 %v.1, 2147483647
  %tmp.3 = or i1 %tmp.1, %tmp.2
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.2 = trunc i64 %v.1 to i32
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4({ { ptr, i64, i64 }, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { ptr, i64, i64 }, i32 }, align 8
  store { { ptr, i64, i64 }, i32 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 1
  %v.0 = load i32, ptr %tmp.0, align 4
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.2 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.store.1.drop(ptr %tmp.2)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.5() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 87
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i32 0, 0
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = load i32, ptr %local.2, align 4
  %v.5 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, i32 %v.4, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, { i32 } %v.5, 1
  %v.6 = insertvalue %zil_env_0 %tmp.1, i1 true, 2
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_0 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %v.0 = call %zil_env_0 @zil.fn.5()
  store %zil_env_0 %v.0, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  %v.2 = call { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } @zil.fn.8(%zil_env_0 %v.1)
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } %v.2, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  %v.4 = call { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } @zil.fn.9({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } %v.3)
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.1 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.1)
  %tmp.2 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.store.1.drop(ptr %tmp.2)
  %tmp.3 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.3)
  br label %package.done.0
package.done.0:
  %tmp.4 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.4, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } %v.4, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %tmp.5 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 3
  %v.5 = load i32, ptr %tmp.5, align 4
  %v.6 = add i32 0, 2
  %tmp.6 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.2
checked.2:
  %tmp.8 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.8, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.9 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.9)
  %tmp.10 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.store.1.drop(ptr %tmp.10)
  %tmp.11 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.11)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.7(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.1, align 8
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = add i32 0, 1
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.4, ptr %local.0, align 4
  %tmp.3 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store i32 %v.4, ptr %tmp.3, align 4
  %v.5 = load i32, ptr %local.0, align 4
  %v.6 = load ptr, ptr %local.1, align 8
  %tmp.4 = getelementptr { i32 }, ptr %v.6, i32 0, i32 0
  %v.7 = load i32, ptr %tmp.4, align 4
  %tmp.5 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.1
checked.1:
  %v.9 = add i32 0, 87
  %tmp.7 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.8, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  ret i32 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } @zil.fn.8(%zil_env_0 %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { ptr, i64, i64 }, i32 }, align 8
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca %zil_env_0, align 4
  %local.6 = alloca i32, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { i32 }, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { ptr, i64, i64 }, align 8
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca %zil_env_0, align 4
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 65
  %v.1 = call { { ptr, i64, i64 }, i32 } @zil.fn.2(i32 %v.0)
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store { { ptr, i64, i64 }, i32 } %v.1, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.7, align 1
  %v.2 = add i32 0, 88
  store i32 %v.2, ptr %local.2, align 4
  %v.3 = load i32, ptr %local.2, align 4
%v.4 = insertvalue { i32 } zeroinitializer, i32 %v.3, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.4, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %tmp.0 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 0
  %v.5 = load { ptr, i64, i64 }, ptr %tmp.0, align 8
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.5, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.6 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.5, align 1
  store %zil_env_0 %v.6, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.7 = add i32 0, 0
  store i32 %v.7, ptr %local.6, align 4
  %v.8 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.8, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.9 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.9, ptr %local.8, align 8
  store i1 true, ptr %resource.live.8, align 1
  %v.10 = load %zil_env_0, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  store %zil_env_0 %v.10, ptr %local.9, align 4
  store i1 true, ptr %resource.live.9, align 1
  %v.11 = load i32, ptr %local.6, align 4
  %v.12 = load { i32 }, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
  %v.13 = load { ptr, i64, i64 }, ptr %local.8, align 8
  store i1 false, ptr %resource.live.8, align 1
  %v.14 = load %zil_env_0, ptr %local.9, align 4
  store i1 false, ptr %resource.live.9, align 1
%tmp.1 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } zeroinitializer, { i32 } %v.12, 0
%tmp.2 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } %tmp.1, { ptr, i64, i64 } %v.13, 1
%tmp.3 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } %tmp.2, %zil_env_0 %v.14, 2
%v.15 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } %tmp.3, i32 %v.11, 3
  %tmp.4 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.4, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.closure.drop.0(ptr %local.9)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.5 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.5, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.store.1.drop(ptr %local.8)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.6 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.6, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.fn.1(ptr %local.7)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.7 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.7, label %closure.drop.3, label %closure.done.3
closure.drop.3:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.closure.drop.0(ptr %local.5)
  br label %closure.done.3
closure.done.3:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.8 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.8, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.store.1.drop(ptr %local.4)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.9 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.9, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.1(ptr %local.3)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.10 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.10, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.11 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.11, label %package.field.drop.7, label %package.field.done.7
package.field.drop.7:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.12 = getelementptr { { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.store.1.drop(ptr %tmp.12)
  br label %package.field.done.7
package.field.done.7:
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.13 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.13, label %closure.drop.8, label %closure.done.8
closure.drop.8:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.8
closure.done.8:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } %v.15
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } @zil.fn.9({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %package.live.1.2 = alloca i1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  %local.5 = alloca ptr, align 8
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 true, ptr %package.live.1.1, align 1
  store i1 true, ptr %package.live.1.2, align 1
  %tmp.0 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  %v.1 = load { i32 }, ptr %tmp.0, align 4
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.1, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = add i32 0, 89
  store i32 %v.2, ptr %local.3, align 4
  %v.3 = load i32, ptr %local.3, align 4
%v.4 = insertvalue { i32 } zeroinitializer, i32 %v.3, 0
  store { i32 } %v.4, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %tmp.1 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.1, label %package.field.drop.0, label %package.field.done.0
package.field.drop.0:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.2 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.2)
  br label %package.field.done.0
package.field.done.0:
  %v.5 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  %tmp.3 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  store { i32 } %v.5, ptr %tmp.3, align 4
  store i1 true, ptr %package.live.1.0, align 1
  %tmp.4 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.4, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.1(ptr %local.4)
  br label %resource.done.1
resource.done.1:
  %v.6 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 2
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  store ptr %v.7, ptr %local.5, align 8
  %v.8 = load ptr, ptr %local.5, align 8
  %v.9 = call i32 @zil.fn.7(ptr %v.8)
  %tmp.5 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 3
  store i32 %v.9, ptr %tmp.5, align 4
  %v.10 = load ptr, ptr %local.5, align 8
  %v.11 = call i32 @zil.fn.7(ptr %v.10)
  %tmp.6 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 3
  store i32 %v.11, ptr %tmp.6, align 4
  %v.12 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  store i1 false, ptr %resource.live.4, align 1
  %tmp.7 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.7, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.8 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.8, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.9 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.9, label %package.field.drop.4, label %package.field.done.4
package.field.drop.4:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.10 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.10)
  br label %package.field.done.4
package.field.done.4:
  %tmp.11 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.11, label %package.field.drop.5, label %package.field.done.5
package.field.drop.5:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.12 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.store.1.drop(ptr %tmp.12)
  br label %package.field.done.5
package.field.done.5:
  %tmp.13 = load i1, ptr %package.live.1.2, align 1
  br i1 %tmp.13, label %package.field.drop.6, label %package.field.done.6
package.field.drop.6:
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.14 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.1, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.14)
  br label %package.field.done.6
package.field.done.6:
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.15 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.15, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.16 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.16)
  %tmp.17 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.store.1.drop(ptr %tmp.17)
  %tmp.18 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.18)
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32 } %v.12
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.6()
  ret i32 %entry.result
}

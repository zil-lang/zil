; ZIL verified LLVM module
; verified-fingerprint: 01f8f97230f946f5e162956c91877118beea36f550a56b47e4bba66372089c8e
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
  call void @zil.fn.3(ptr %payload.1)
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

define internal i1 @zil.store.2.reserve(ptr %head, i64 %requested) {
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
define internal { i1, { { i32 }, i32 }, {} } @zil.store.2.push(ptr %head, { { i32 }, i32 } %item) {
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
define internal { i1, {}, { { i32 }, i32 } } @zil.store.2.pop(ptr %head) {
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
define internal void @zil.store.2.drop(ptr %head) {
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
  call void @zil.fn.3(ptr %field.0)
  %next = add i64 %index, 1
  br label %loop
release:
  call void @zil.bytes.release(ptr %data)
  ret void
}

define internal i1 @zil.store.3.reserve(ptr %head, i64 %requested) {
entry:
  %stride = ptrtoint ptr getelementptr ({ { i32 }, { ptr, i64, i64 }, i64 }, ptr null, i64 1) to i64
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
define internal { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} } @zil.store.3.push(ptr %head, { { i32 }, { ptr, i64, i64 }, i64 } %item) {
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
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %data, i64 %len
  store { { i32 }, { ptr, i64, i64 }, i64 } %item, ptr %slot, align 8
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} } zeroinitializer
full:
  %tag = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} } zeroinitializer, i1 true, 0
  %error = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} } %tag, { { i32 }, { ptr, i64, i64 }, i64 } %item, 1
  ret { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} } %error
}
define internal { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } @zil.store.3.pop(ptr %head) {
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
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %data, i64 %next.len
  %item = load { { i32 }, { ptr, i64, i64 }, i64 }, ptr %slot, align 8
  %result = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } zeroinitializer, { { i32 }, { ptr, i64, i64 }, i64 } %item, 2
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %result
empty:
  %error = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } zeroinitializer, i1 true, 0
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %error
}
define internal void @zil.store.3.drop(ptr %head) {
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
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %data, i64 %index
  %field.0 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %slot, i32 0, i32 0
  call void @zil.fn.3(ptr %field.0)
  %field.1 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %slot, i32 0, i32 1
  call void @zil.bytes.drop(ptr %field.1)
  %next = add i64 %index, 1
  br label %loop
release:
  call void @zil.bytes.release(ptr %data)
  ret void
}
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @getchar() nonlazybind
define { i1, {}, i32 } @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { { i32 }, { ptr, i64, i64 }, i64 }, align 8
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} }, align 8
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { ptr, i64, i64 }, align 8
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { ptr, i64, i64 }, align 8
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %local.10 = alloca i32, align 4
  %resource.live.11 = alloca i1, align 1
  store i1 false, ptr %resource.live.11, align 1
  %local.11 = alloca { { i32 }, { ptr, i64, i64 }, i64 }, align 8
  %local.12 = alloca i1, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
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
  %v.7 = add i32 0, 51
  %v.8 = icmp sgt i32 %v.6, %v.7
  store i1 %v.8, ptr %local.1, align 1
  br label %bb.3
bb.3:
  %v.9 = load i1, ptr %local.1, align 1
  br i1 %v.9, label %bb.4, label %bb.5
bb.4:
  %v.10 = add i32 0, 9
  %tmp.0 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.11 = insertvalue { i1, {}, i32 } %tmp.0, i32 %v.10, 2
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.9, align 1
  ret { i1, {}, i32 } %v.11
bb.5:
  br label %bb.6
bb.6:
  %v.12 = call %zil_env_0 @zil.fn.6()
  store %zil_env_0 %v.12, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.13 = load %zil_env_0, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.14 = call { { ptr, i64, i64 }, %zil_env_0 } @zil.fn.18(%zil_env_0 %v.13)
  %tmp.1 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.1, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.3, align 1
  store { { ptr, i64, i64 }, %zil_env_0 } %v.14, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.15 = call { ptr, i64, i64 } @zil.fn.7()
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.15, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.16 = getelementptr i8, ptr %local.4, i64 0
  %v.17 = add i64 0, 1
  %v.18 = call i1 @zil.fn.8(ptr %v.16, i64 %v.17)
  %v.19 = xor i1 %v.18, true
  br i1 %v.19, label %bb.7, label %bb.8
bb.7:
  %tmp.2 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.21 = insertvalue { i1, {}, i32 } %tmp.2, {} zeroinitializer, 1
  %tmp.3 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.store.3.drop(ptr %local.4)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.5 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.5)
  %tmp.6 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.6)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.9, align 1
  ret { i1, {}, i32 } %v.21
bb.8:
  br label %bb.9
bb.9:
  %v.22 = getelementptr i8, ptr %local.4, i64 0
  %v.23 = add i32 0, 90
  %v.24 = call { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } @zil.fn.5(i32 %v.23)
  %v.25 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %v.24, 0
  br i1 %v.25, label %bb.10, label %bb.11
bb.10:
  %tmp.7 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %v.24, 0
  %tmp.8 = icmp ne i1 %tmp.7, true
  br i1 %tmp.8, label %zil.trap, label %checked.3
checked.3:
  %tmp.9 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.27 = insertvalue { i1, {}, i32 } %tmp.9, {} zeroinitializer, 1
  %tmp.10 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.10, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.store.3.drop(ptr %local.4)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.11 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.11, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.12 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.12)
  %tmp.13 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.13)
  br label %package.done.5
package.done.5:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.9, align 1
  ret { i1, {}, i32 } %v.27
bb.11:
  %tmp.14 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %v.24, 0
  %tmp.15 = icmp ne i1 %tmp.14, false
  br i1 %tmp.15, label %zil.trap, label %checked.6
checked.6:
  %v.28 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %v.24, 2
  store { { i32 }, { ptr, i64, i64 }, i64 } %v.28, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.29 = load { { i32 }, { ptr, i64, i64 }, i64 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  %v.30 = call { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} } @zil.fn.9(ptr %v.22, { { i32 }, { ptr, i64, i64 }, i64 } %v.29)
  store { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} } %v.30, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  %tmp.16 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.16, label %error.drop.7, label %error.done.7
error.drop.7:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.17 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} }, ptr %local.6, i32 0, i32 0
  %tmp.18 = load i1, ptr %tmp.17, align 1
  br i1 %tmp.18, label %error.payload.7, label %success.payload.7
error.payload.7:
  %tmp.19 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} }, ptr %local.6, i32 0, i32 1
  %tmp.20 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %tmp.19, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.20)
  %tmp.21 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %tmp.19, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.21)
  br label %error.done.7
success.payload.7:
  br label %error.done.7
error.done.7:
  %tmp.22 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.22, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.23 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.23)
  %tmp.24 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %local.5, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.24)
  br label %package.done.8
package.done.8:
  %v.31 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.31, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.32 = load { ptr, i64, i64 }, ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
  %v.33 = call { ptr, i64, i64 } @zil.fn.10({ ptr, i64, i64 } %v.32)
  %tmp.25 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.25, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.store.3.drop(ptr %local.7)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.8, align 1
  store { ptr, i64, i64 } %v.33, ptr %local.8, align 8
  store i1 true, ptr %resource.live.8, align 1
  %v.34 = load { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  store { { ptr, i64, i64 }, %zil_env_0 } %v.34, ptr %local.9, align 8
  store i1 true, ptr %resource.live.9, align 1
  %v.35 = load i32, ptr %local.0, align 4
  %v.36 = add i32 0, 48
  %tmp.26 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.35, i32 %v.36)
  %v.37 = extractvalue { i32, i1 } %tmp.26, 0
  %tmp.27 = extractvalue { i32, i1 } %tmp.26, 1
  br i1 %tmp.27, label %zil.trap, label %checked.10
checked.10:
  %v.38 = sext i32 %v.37 to i64
  %v.39 = load { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.9, align 8
  store i1 false, ptr %resource.live.9, align 1
  %v.40 = call { i1, {}, i32 } @zil.fn.19({ { ptr, i64, i64 }, %zil_env_0 } %v.39, i64 %v.38)
  %v.41 = extractvalue { i1, {}, i32 } %v.40, 0
  br i1 %v.41, label %bb.12, label %bb.13
bb.12:
  %tmp.28 = extractvalue { i1, {}, i32 } %v.40, 0
  %tmp.29 = icmp ne i1 %tmp.28, true
  br i1 %tmp.29, label %zil.trap, label %checked.11
checked.11:
  %tmp.30 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.43 = insertvalue { i1, {}, i32 } %tmp.30, {} zeroinitializer, 1
  %tmp.31 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.31, label %package.drop.12, label %package.done.12
package.drop.12:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.32 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.9, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.32)
  %tmp.33 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.9, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.33)
  br label %package.done.12
package.done.12:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.34 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.34, label %resource.drop.13, label %resource.done.13
resource.drop.13:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.store.3.drop(ptr %local.8)
  br label %resource.done.13
resource.done.13:
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.35 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.35, label %resource.drop.14, label %resource.done.14
resource.drop.14:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.store.3.drop(ptr %local.4)
  br label %resource.done.14
resource.done.14:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.36 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.36, label %package.drop.15, label %package.done.15
package.drop.15:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.37 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.37)
  %tmp.38 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.38)
  br label %package.done.15
package.done.15:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, {}, i32 } %v.43
bb.13:
  %tmp.39 = extractvalue { i1, {}, i32 } %v.40, 0
  %tmp.40 = icmp ne i1 %tmp.39, false
  br i1 %tmp.40, label %zil.trap, label %checked.16
checked.16:
  %v.44 = extractvalue { i1, {}, i32 } %v.40, 2
  store i32 %v.44, ptr %local.10, align 4
  %tmp.41 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.41, label %package.drop.17, label %package.done.17
package.drop.17:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.42 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.9, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.42)
  %tmp.43 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.9, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.43)
  br label %package.done.17
package.done.17:
  %v.45 = getelementptr i8, ptr %local.8, i64 0
  %v.46 = call { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } @zil.fn.11(ptr %v.45)
  %v.47 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %v.46, 0
  br i1 %v.47, label %bb.14, label %bb.15
bb.14:
  %tmp.44 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %v.46, 0
  %tmp.45 = icmp ne i1 %tmp.44, true
  br i1 %tmp.45, label %zil.trap, label %checked.18
checked.18:
  %tmp.46 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.49 = insertvalue { i1, {}, i32 } %tmp.46, {} zeroinitializer, 1
  store i1 false, ptr %resource.live.9, align 1
  %tmp.47 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.47, label %resource.drop.19, label %resource.done.19
resource.drop.19:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.store.3.drop(ptr %local.8)
  br label %resource.done.19
resource.done.19:
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.48 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.48, label %resource.drop.20, label %resource.done.20
resource.drop.20:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.store.3.drop(ptr %local.4)
  br label %resource.done.20
resource.done.20:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.49 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.49, label %package.drop.21, label %package.done.21
package.drop.21:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.50 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.50)
  %tmp.51 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.51)
  br label %package.done.21
package.done.21:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, {}, i32 } %v.49
bb.15:
  %tmp.52 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %v.46, 0
  %tmp.53 = icmp ne i1 %tmp.52, false
  br i1 %tmp.53, label %zil.trap, label %checked.22
checked.22:
  %v.50 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %v.46, 2
  store i1 false, ptr %resource.live.11, align 1
  store { { i32 }, { ptr, i64, i64 }, i64 } %v.50, ptr %local.11, align 8
  store i1 true, ptr %resource.live.11, align 1
  %tmp.54 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %local.11, i32 0, i32 2
  %v.51 = load i64, ptr %tmp.54, align 8
  %v.52 = add i64 0, 90
  %v.53 = icmp ne i64 %v.51, %v.52
  br i1 %v.53, label %bb.16, label %bb.17
bb.16:
  %v.54 = or i1 false, true
  store i1 %v.54, ptr %local.12, align 1
  br label %bb.18
bb.17:
  %v.55 = getelementptr i8, ptr %local.8, i64 0
  %v.56 = call i64 @zil.fn.12(ptr %v.55)
  %v.57 = add i64 0, 0
  %v.58 = icmp ne i64 %v.56, %v.57
  store i1 %v.58, ptr %local.12, align 1
  br label %bb.18
bb.18:
  %v.59 = load i1, ptr %local.12, align 1
  br i1 %v.59, label %bb.19, label %bb.20
bb.19:
  %v.60 = add i32 0, 7
  %tmp.55 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.61 = insertvalue { i1, {}, i32 } %tmp.55, i32 %v.60, 2
  %tmp.56 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.56, label %package.drop.23, label %package.done.23
package.drop.23:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.57 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %local.11, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.57)
  %tmp.58 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %local.11, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.58)
  br label %package.done.23
package.done.23:
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.9, align 1
  %tmp.59 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.59, label %resource.drop.24, label %resource.done.24
resource.drop.24:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.store.3.drop(ptr %local.8)
  br label %resource.done.24
resource.done.24:
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.60 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.60, label %resource.drop.25, label %resource.done.25
resource.drop.25:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.store.3.drop(ptr %local.4)
  br label %resource.done.25
resource.done.25:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.61 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.61, label %package.drop.26, label %package.done.26
package.drop.26:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.62 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.62)
  %tmp.63 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.63)
  br label %package.done.26
package.done.26:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, {}, i32 } %v.61
bb.20:
  br label %bb.21
bb.21:
  %v.62 = load i32, ptr %local.10, align 4
  %tmp.64 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.63 = insertvalue { i1, {}, i32 } %tmp.64, i32 %v.62, 2
  %tmp.65 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.65, label %package.drop.27, label %package.done.27
package.drop.27:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.66 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %local.11, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.66)
  %tmp.67 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %local.11, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.67)
  br label %package.done.27
package.done.27:
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.9, align 1
  %tmp.68 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.68, label %resource.drop.28, label %resource.done.28
resource.drop.28:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.store.3.drop(ptr %local.8)
  br label %resource.done.28
resource.done.28:
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  %tmp.69 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.69, label %resource.drop.29, label %resource.done.29
resource.drop.29:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.store.3.drop(ptr %local.4)
  br label %resource.done.29
resource.done.29:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.70 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.70, label %package.drop.30, label %package.done.30
package.drop.30:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.71 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.71)
  %tmp.72 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.3, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.72)
  br label %package.done.30
package.done.30:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, {}, i32 } %v.63
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.3(ptr %arg.0) {
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

define { { i32 }, i32 } @zil.fn.4(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load i32, ptr %local.0, align 4
  store i32 %v.3, ptr %local.3, align 4
  %v.4 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.4, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.5 = load i32, ptr %local.3, align 4
  %v.6 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
%tmp.0 = insertvalue { { i32 }, i32 } zeroinitializer, { i32 } %v.6, 0
%v.7 = insertvalue { { i32 }, i32 } %tmp.0, i32 %v.5, 1
  %tmp.1 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.3(ptr %local.4)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.3(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, i32 } %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } @zil.fn.5(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %local.5 = alloca i64, align 8
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { ptr, i64, i64 }, align 8
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = getelementptr i8, ptr %local.1, i64 0
  %v.2 = add i64 0, 1
  %v.3 = call i1 @zil.bytes.reserve(ptr %v.1, i64 %v.2)
  %v.4 = xor i1 %v.3, true
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } zeroinitializer, i1 true, 0
  %v.6 = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %tmp.0, {} zeroinitializer, 1
  %tmp.1 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %v.6
bb.2:
  br label %bb.3
bb.3:
  %v.7 = getelementptr i8, ptr %local.1, i64 0
  %v.8 = load i32, ptr %local.0, align 4
  %tmp.2 = icmp ugt i32 %v.8, 255
  br i1 %tmp.2, label %zil.trap, label %checked.1
checked.1:
  %v.9 = trunc i32 %v.8 to i8
  %v.10 = call i1 @zil.bytes.push(ptr %v.7, i8 %v.9)
  %v.11 = xor i1 %v.10, true
  br i1 %v.11, label %bb.4, label %bb.5
bb.4:
  %tmp.3 = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } zeroinitializer, i1 true, 0
  %v.13 = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %tmp.3, {} zeroinitializer, 1
  %tmp.4 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %v.13
bb.5:
  br label %bb.6
bb.6:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  %v.14 = load i32, ptr %local.0, align 4
  store i32 %v.14, ptr %local.2, align 4
  %v.15 = load i32, ptr %local.2, align 4
%v.16 = insertvalue { i32 } zeroinitializer, i32 %v.15, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.16, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.17 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.17, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.18 = load i32, ptr %local.0, align 4
  %v.19 = sext i32 %v.18 to i64
  store i64 %v.19, ptr %local.5, align 8
  %v.20 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.20, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.21 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.21, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.22 = load i64, ptr %local.5, align 8
  %v.23 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %v.24 = load { ptr, i64, i64 }, ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
%tmp.5 = insertvalue { { i32 }, { ptr, i64, i64 }, i64 } zeroinitializer, { i32 } %v.23, 0
%tmp.6 = insertvalue { { i32 }, { ptr, i64, i64 }, i64 } %tmp.5, { ptr, i64, i64 } %v.24, 1
%v.25 = insertvalue { { i32 }, { ptr, i64, i64 }, i64 } %tmp.6, i64 %v.22, 2
  %tmp.7 = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } zeroinitializer, i1 false, 0
  %v.26 = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %tmp.7, { { i32 }, { ptr, i64, i64 }, i64 } %v.25, 2
  %tmp.8 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.8, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.bytes.drop(ptr %local.7)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.9 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.9, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.3(ptr %local.6)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.10 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.10, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.11 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.11, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.3(ptr %local.3)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.12 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.12, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.7
resource.done.7:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %v.26
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.6() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 72
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
  call void @zil.fn.3(ptr %local.1)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_0 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.7() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.8(ptr %arg.0, i64 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i64, ptr %local.1, align 8
  %v.2 = call i1 @zil.store.3.reserve(ptr %v.0, i64 %v.1)
  ret i1 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} } @zil.fn.9(ptr %arg.0, { { i32 }, { ptr, i64, i64 }, i64 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 true, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, { ptr, i64, i64 }, i64 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { { i32 }, { ptr, i64, i64 }, i64 } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { { i32 }, { ptr, i64, i64 }, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  %v.2 = call { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} } @zil.store.3.push(ptr %v.0, { { i32 }, { ptr, i64, i64 }, i64 } %v.1)
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.1 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.1)
  %tmp.2 = getelementptr { { i32 }, { ptr, i64, i64 }, i64 }, ptr %local.1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.2)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { { i32 }, { ptr, i64, i64 }, i64 }, {} } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.10({ ptr, i64, i64 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  store { ptr, i64, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { ptr, i64, i64 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.3.drop(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } @zil.fn.11(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } @zil.store.3.pop(ptr %v.0)
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i64 } } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.12(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.0, i32 0, i32 1
  %v.1 = load i64, ptr %tmp.0, align 8
  ret i64 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.13() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.14(ptr %arg.0, i64 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i64, ptr %local.1, align 8
  %v.2 = call i1 @zil.store.2.reserve(ptr %v.0, i64 %v.1)
  ret i1 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { { i32 }, i32 }, {} } @zil.fn.15(ptr %arg.0, { { i32 }, i32 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 true, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { { i32 }, i32 } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { { i32 }, i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.2 = call { i1, { { i32 }, i32 }, {} } @zil.store.2.push(ptr %v.0, { { i32 }, i32 } %v.1)
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.1 = getelementptr { { i32 }, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.1)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { { i32 }, i32 }, {} } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.16(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.0, i32 0, i32 1
  %v.1 = load i64, ptr %tmp.0, align 8
  ret i64 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.17(ptr %env) {
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
  %v.9 = add i32 0, 72
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

define { { ptr, i64, i64 }, %zil_env_0 } @zil.fn.18(%zil_env_0 %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_0, align 4
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = call { ptr, i64, i64 } @zil.fn.13()
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.1, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.2, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = load %zil_env_0, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.3, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.4 = load { ptr, i64, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.5 = load %zil_env_0, ptr %local.4, align 4
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
  call void @zil.store.2.drop(ptr %local.3)
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
  call void @zil.store.2.drop(ptr %local.1)
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

define { i1, {}, i32 } @zil.fn.19({ { ptr, i64, i64 }, %zil_env_0 } %arg.0, i64 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %local.1 = alloca i64, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { ptr, i64, i64 }, %zil_env_0 }, align 8
  %local.3 = alloca i64, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { i32 }, i32 }, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i1, { { i32 }, i32 }, {} }, align 4
  %local.6 = alloca ptr, align 8
  %local.7 = alloca i32, align 4
  %local.8 = alloca i32, align 4
  %local.9 = alloca i1, align 1
  store { { ptr, i64, i64 }, %zil_env_0 } %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store { { ptr, i64, i64 }, %zil_env_0 } %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.1 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 0
  %v.2 = load i64, ptr %local.1, align 8
  %v.3 = call i1 @zil.fn.14(ptr %v.1, i64 %v.2)
  %v.4 = xor i1 %v.3, true
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.6 = insertvalue { i1, {}, i32 } %tmp.0, {} zeroinitializer, 1
  %tmp.1 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.1, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.2 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.2)
  %tmp.3 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.3)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.4 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.4, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.5 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.5)
  %tmp.6 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.6)
  br label %package.done.1
package.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.6
bb.2:
  br label %bb.3
bb.3:
  %v.7 = add i64 0, 0
  store i64 %v.7, ptr %local.3, align 8
  br label %bb.4
bb.4:
  %v.8 = load i64, ptr %local.3, align 8
  %v.9 = load i64, ptr %local.1, align 8
  %v.10 = icmp slt i64 %v.8, %v.9
  br i1 %v.10, label %bb.5, label %bb.6
bb.5:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  %v.11 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 0
  %v.12 = load i64, ptr %local.3, align 8
  %tmp.7 = icmp slt i64 %v.12, -2147483648
  %tmp.8 = icmp sgt i64 %v.12, 2147483647
  %tmp.9 = or i1 %tmp.7, %tmp.8
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  %v.13 = trunc i64 %v.12 to i32
  %v.14 = add i32 0, 65
  %tmp.10 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.3
checked.3:
  %v.16 = call { { i32 }, i32 } @zil.fn.4(i32 %v.15)
  store { { i32 }, i32 } %v.16, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.17 = load { { i32 }, i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  %v.18 = call { i1, { { i32 }, i32 }, {} } @zil.fn.15(ptr %v.11, { { i32 }, i32 } %v.17)
  store { i1, { { i32 }, i32 }, {} } %v.18, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %tmp.12 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.12, label %error.drop.4, label %error.done.4
error.drop.4:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.13 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.5, i32 0, i32 0
  %tmp.14 = load i1, ptr %tmp.13, align 1
  br i1 %tmp.14, label %error.payload.4, label %success.payload.4
error.payload.4:
  %tmp.15 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.5, i32 0, i32 1
  %tmp.16 = getelementptr { { i32 }, i32 }, ptr %tmp.15, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.16)
  br label %error.done.4
success.payload.4:
  br label %error.done.4
error.done.4:
  %tmp.17 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.17, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.18 = getelementptr { { i32 }, i32 }, ptr %local.4, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.18)
  br label %package.done.5
package.done.5:
  %v.19 = load i64, ptr %local.3, align 8
  %v.20 = add i64 0, 1
  %tmp.19 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.19, i64 %v.20)
  %v.21 = extractvalue { i64, i1 } %tmp.19, 0
  %tmp.20 = extractvalue { i64, i1 } %tmp.19, 1
  br i1 %tmp.20, label %zil.trap, label %checked.6
checked.6:
  store i64 %v.21, ptr %local.3, align 8
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  br label %bb.4
bb.6:
  %v.22 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 1
  %v.23 = getelementptr i8, ptr %v.22, i64 0
  store ptr %v.23, ptr %local.6, align 8
  %v.24 = load ptr, ptr %local.6, align 8
  %v.25 = call i32 @zil.fn.17(ptr %v.24)
  store i32 %v.25, ptr %local.7, align 4
  %v.26 = load ptr, ptr %local.6, align 8
  %v.27 = call i32 @zil.fn.17(ptr %v.26)
  store i32 %v.27, ptr %local.8, align 4
  %v.28 = load i32, ptr %local.7, align 4
  %v.29 = add i32 0, 1
  %v.30 = icmp ne i32 %v.28, %v.29
  br i1 %v.30, label %bb.7, label %bb.8
bb.7:
  %v.31 = or i1 false, true
  store i1 %v.31, ptr %local.9, align 1
  br label %bb.9
bb.8:
  %v.32 = load i32, ptr %local.8, align 4
  %v.33 = add i32 0, 2
  %v.34 = icmp ne i32 %v.32, %v.33
  store i1 %v.34, ptr %local.9, align 1
  br label %bb.9
bb.9:
  %v.35 = load i1, ptr %local.9, align 1
  br i1 %v.35, label %bb.10, label %bb.11
bb.10:
  %v.36 = add i32 0, 8
  %tmp.21 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.37 = insertvalue { i1, {}, i32 } %tmp.21, i32 %v.36, 2
  %tmp.22 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.22, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.23 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.23)
  %tmp.24 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.24)
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.25 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.25, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.26 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.26)
  %tmp.27 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.27)
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.37
bb.11:
  br label %bb.12
bb.12:
  %v.38 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 0
  %v.39 = call i64 @zil.fn.16(ptr %v.38)
  %tmp.28 = icmp slt i64 %v.39, -2147483648
  %tmp.29 = icmp sgt i64 %v.39, 2147483647
  %tmp.30 = or i1 %tmp.28, %tmp.29
  br i1 %tmp.30, label %zil.trap, label %checked.9
checked.9:
  %v.40 = trunc i64 %v.39 to i32
  %v.41 = load i64, ptr %local.1, align 8
  %tmp.31 = icmp slt i64 %v.41, -2147483648
  %tmp.32 = icmp sgt i64 %v.41, 2147483647
  %tmp.33 = or i1 %tmp.31, %tmp.32
  br i1 %tmp.33, label %zil.trap, label %checked.10
checked.10:
  %v.42 = trunc i64 %v.41 to i32
  %tmp.34 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.40, i32 %v.42)
  %v.43 = extractvalue { i32, i1 } %tmp.34, 0
  %tmp.35 = extractvalue { i32, i1 } %tmp.34, 1
  br i1 %tmp.35, label %zil.trap, label %checked.11
checked.11:
  %tmp.36 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.44 = insertvalue { i1, {}, i32 } %tmp.36, i32 %v.43, 2
  %tmp.37 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.37, label %package.drop.12, label %package.done.12
package.drop.12:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.38 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.38)
  %tmp.39 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.39)
  br label %package.done.12
package.done.12:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.40 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.40, label %package.drop.13, label %package.done.13
package.drop.13:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.41 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.41)
  %tmp.42 = getelementptr { { ptr, i64, i64 }, %zil_env_0 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.42)
  br label %package.done.13
package.done.13:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.44
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, {}, i32 } @zil.fn.1()
  %entry.error = extractvalue { i1, {}, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, {}, i32 } %entry.result, 2
  ret i32 %entry.success
}

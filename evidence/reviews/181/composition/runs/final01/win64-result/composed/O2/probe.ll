; ZIL verified LLVM module
; verified-fingerprint: 6c82fb4b660aa4ad3d5669dfa76265af2e245fd670c0c90d33cb0c51806c93b3
target triple = "x86_64-pc-windows-msvc"

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
declare ptr @GetProcessHeap()
declare ptr @zil_probe_HeapAlloc(ptr, i32, i64)
declare ptr @zil_probe_HeapReAlloc(ptr, i32, ptr, i64)
declare i32 @zil_probe_HeapFree(ptr, i32, ptr)

define internal ptr @zil.bytes.reallocate(ptr %old, i64 %size) {
entry:
  %heap = call ptr @GetProcessHeap()
  %heap.ok = icmp ne ptr %heap, null
  br i1 %heap.ok, label %choose, label %failed
choose:
  %empty = icmp eq ptr %old, null
  br i1 %empty, label %allocate, label %reallocate
allocate:
  %fresh = call ptr @zil_probe_HeapAlloc(ptr %heap, i32 0, i64 %size)
  ret ptr %fresh
reallocate:
  %grown = call ptr @zil_probe_HeapReAlloc(ptr %heap, i32 0, ptr %old, i64 %size)
  ret ptr %grown
failed:
  ret ptr null
}
define internal void @zil.bytes.release(ptr %payload) {
entry:
  %empty = icmp eq ptr %payload, null
  br i1 %empty, label %done, label %release
release:
  %heap = call ptr @GetProcessHeap()
  %released = call i32 @zil_probe_HeapFree(ptr %heap, i32 0, ptr %payload)
  %ok = icmp ne i32 %released, 0
  br i1 %ok, label %done, label %failed
failed:
  call void @llvm.trap()
  unreachable
done:
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
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.smul.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @getchar()
define { i1, {}, i32 } @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
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
  %local.9 = alloca { { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
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
  %v.14 = call { { ptr, i64, i64 }, %zil_env_0, i32 } @zil.fn.18(%zil_env_0 %v.13)
  %tmp.1 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.1, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.3, align 1
  store { { ptr, i64, i64 }, %zil_env_0, i32 } %v.14, ptr %local.3, align 8
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
  %tmp.5 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.5)
  %tmp.6 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 1
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
  %tmp.12 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.12)
  %tmp.13 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 1
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
  %v.34 = load { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  store { { ptr, i64, i64 }, %zil_env_0, i32 } %v.34, ptr %local.9, align 8
  store i1 true, ptr %resource.live.9, align 1
  %v.35 = load i32, ptr %local.0, align 4
  %v.36 = add i32 0, 48
  %tmp.26 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.35, i32 %v.36)
  %v.37 = extractvalue { i32, i1 } %tmp.26, 0
  %tmp.27 = extractvalue { i32, i1 } %tmp.26, 1
  br i1 %tmp.27, label %zil.trap, label %checked.10
checked.10:
  %v.38 = sext i32 %v.37 to i64
  %v.39 = load { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.9, align 8
  store i1 false, ptr %resource.live.9, align 1
  %v.40 = call { i1, {}, i32 } @zil.fn.19({ { ptr, i64, i64 }, %zil_env_0, i32 } %v.39, i64 %v.38)
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
  %tmp.32 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.9, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.32)
  %tmp.33 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.9, i32 0, i32 1
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
  %tmp.37 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.37)
  %tmp.38 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 1
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
  %tmp.42 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.9, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.42)
  %tmp.43 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.9, i32 0, i32 1
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
  %tmp.50 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.50)
  %tmp.51 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 1
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
  %tmp.62 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.62)
  %tmp.63 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 1
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
  %tmp.71 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.71)
  %tmp.72 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.3, i32 0, i32 1
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

declare i32 @putchar(i32)
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

define { { ptr, i64, i64 }, %zil_env_0, i32 } @zil.fn.18(%zil_env_0 %arg.0) {
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
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca %zil_env_0, align 4
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = call { ptr, i64, i64 } @zil.fn.13()
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.1, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = add i32 0, 0
  store i32 %v.2, ptr %local.3, align 4
  %v.3 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.3, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.4 = load %zil_env_0, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.4, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.5 = load i32, ptr %local.3, align 4
  %v.6 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  %v.7 = load %zil_env_0, ptr %local.5, align 4
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
  call void @zil.store.2.drop(ptr %local.4)
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
  call void @zil.store.2.drop(ptr %local.1)
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

define { i1, {}, i32 } @zil.fn.19({ { ptr, i64, i64 }, %zil_env_0, i32 } %arg.0, i64 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  %local.1 = alloca i64, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { ptr, i64, i64 }, %zil_env_0, i32 }, align 8
  %package.live.2.0 = alloca i1, align 1
  store i1 false, ptr %package.live.2.0, align 1
  %package.live.2.1 = alloca i1, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %local.3 = alloca i64, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { i32 }, i32 }, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i1, { { i32 }, i32 }, {} }, align 4
  %local.6 = alloca ptr, align 8
  %local.7 = alloca ptr, align 8
  %local.8 = alloca i32, align 4
  %local.9 = alloca i32, align 4
  %local.10 = alloca i1, align 1
  %local.11 = alloca i64, align 8
  %local.12 = alloca i1, align 1
  %local.13 = alloca i1, align 1
  %local.14 = alloca ptr, align 8
  store { { ptr, i64, i64 }, %zil_env_0, i32 } %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  store { { ptr, i64, i64 }, %zil_env_0, i32 } %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  store i1 true, ptr %package.live.2.0, align 1
  store i1 true, ptr %package.live.2.1, align 1
  %v.1 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
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
  %tmp.2 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.2, label %package.field.drop.1, label %package.field.done.1
package.field.drop.1:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.3 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.3)
  br label %package.field.done.1
package.field.done.1:
  %tmp.4 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.4, label %package.field.drop.2, label %package.field.done.2
package.field.drop.2:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.5 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.5)
  br label %package.field.done.2
package.field.done.2:
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.6 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.6, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.7 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.7)
  %tmp.8 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.8)
  br label %package.done.3
package.done.3:
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
  %v.11 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  %v.12 = load i64, ptr %local.3, align 8
  %tmp.9 = icmp slt i64 %v.12, -2147483648
  %tmp.10 = icmp sgt i64 %v.12, 2147483647
  %tmp.11 = or i1 %tmp.9, %tmp.10
  br i1 %tmp.11, label %zil.trap, label %checked.4
checked.4:
  %v.13 = trunc i64 %v.12 to i32
  %v.14 = add i32 0, 65
  %tmp.12 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i32, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.5
checked.5:
  %v.16 = call { { i32 }, i32 } @zil.fn.4(i32 %v.15)
  store { { i32 }, i32 } %v.16, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.17 = load { { i32 }, i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  %v.18 = call { i1, { { i32 }, i32 }, {} } @zil.fn.15(ptr %v.11, { { i32 }, i32 } %v.17)
  store { i1, { { i32 }, i32 }, {} } %v.18, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %tmp.14 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.14, label %error.drop.6, label %error.done.6
error.drop.6:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.15 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.5, i32 0, i32 0
  %tmp.16 = load i1, ptr %tmp.15, align 1
  br i1 %tmp.16, label %error.payload.6, label %success.payload.6
error.payload.6:
  %tmp.17 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.5, i32 0, i32 1
  %tmp.18 = getelementptr { { i32 }, i32 }, ptr %tmp.17, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.18)
  br label %error.done.6
success.payload.6:
  br label %error.done.6
error.done.6:
  %tmp.19 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.19, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.20 = getelementptr { { i32 }, i32 }, ptr %local.4, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.20)
  br label %package.done.7
package.done.7:
  %v.19 = load i64, ptr %local.3, align 8
  %v.20 = add i64 0, 1
  %tmp.21 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.19, i64 %v.20)
  %v.21 = extractvalue { i64, i1 } %tmp.21, 0
  %tmp.22 = extractvalue { i64, i1 } %tmp.21, 1
  br i1 %tmp.22, label %zil.trap, label %checked.8
checked.8:
  store i64 %v.21, ptr %local.3, align 8
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  br label %bb.4
bb.6:
  %v.22 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  %v.23 = getelementptr i8, ptr %v.22, i64 0
  store ptr %v.23, ptr %local.6, align 8
  %v.24 = add i64 0, 0
  store i64 %v.24, ptr %local.3, align 8
  br label %bb.7
bb.7:
  %v.25 = load i64, ptr %local.3, align 8
  %v.26 = load i64, ptr %local.1, align 8
  %v.27 = icmp slt i64 %v.25, %v.26
  br i1 %v.27, label %bb.8, label %bb.9
bb.8:
  %v.28 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  %v.29 = load i64, ptr %local.3, align 8
  %tmp.23 = getelementptr { ptr, i64, i64 }, ptr %v.28, i32 0, i32 1
  %tmp.24 = load i64, ptr %tmp.23, align 8
  %tmp.25 = icmp ult i64 %v.29, %tmp.24
  br i1 %tmp.25, label %store.element.ok.9, label %zil.trap
store.element.ok.9:
  %tmp.26 = getelementptr { ptr, i64, i64 }, ptr %v.28, i32 0, i32 0
  %tmp.27 = load ptr, ptr %tmp.26, align 8
  %v.30 = getelementptr { { i32 }, i32 }, ptr %tmp.27, i64 %v.29
  store ptr %v.30, ptr %local.7, align 8
  %v.31 = load ptr, ptr %local.6, align 8
  %v.32 = call i32 @zil.fn.17(ptr %v.31)
  store i32 %v.32, ptr %local.8, align 4
  %v.33 = load ptr, ptr %local.7, align 8
  %v.34 = getelementptr { { i32 }, i32 }, ptr %v.33, i32 0, i32 1
  %v.35 = load ptr, ptr %local.7, align 8
  %v.36 = getelementptr { { i32 }, i32 }, ptr %v.35, i32 0, i32 1
  %v.37 = getelementptr i8, ptr %v.36, i64 0
  %v.38 = load i32, ptr %v.37, align 4
  %v.39 = load i32, ptr %local.8, align 4
  %tmp.28 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.38, i32 %v.39)
  %v.40 = extractvalue { i32, i1 } %tmp.28, 0
  %tmp.29 = extractvalue { i32, i1 } %tmp.28, 1
  br i1 %tmp.29, label %zil.trap, label %checked.10
checked.10:
  store i32 %v.40, ptr %v.34, align 4
  %tmp.30 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 2
  %v.41 = load i32, ptr %tmp.30, align 4
  %v.42 = load i32, ptr %local.8, align 4
  %tmp.31 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.41, i32 %v.42)
  %v.43 = extractvalue { i32, i1 } %tmp.31, 0
  %tmp.32 = extractvalue { i32, i1 } %tmp.31, 1
  br i1 %tmp.32, label %zil.trap, label %checked.11
checked.11:
  %tmp.33 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 2
  store i32 %v.43, ptr %tmp.33, align 4
  %v.44 = load ptr, ptr %local.6, align 8
  %v.45 = call i32 @zil.fn.17(ptr %v.44)
  store i32 %v.45, ptr %local.9, align 4
  %v.46 = load ptr, ptr %local.7, align 8
  %v.47 = getelementptr { { i32 }, i32 }, ptr %v.46, i32 0, i32 1
  %v.48 = getelementptr i8, ptr %v.47, i64 0
  %v.49 = load i32, ptr %v.48, align 4
  %v.50 = add i32 0, 66
  %v.51 = add i32 0, 3
  %v.52 = load i64, ptr %local.3, align 8
  %tmp.34 = icmp slt i64 %v.52, -2147483648
  %tmp.35 = icmp sgt i64 %v.52, 2147483647
  %tmp.36 = or i1 %tmp.34, %tmp.35
  br i1 %tmp.36, label %zil.trap, label %checked.12
checked.12:
  %v.53 = trunc i64 %v.52 to i32
  %tmp.37 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.51, i32 %v.53)
  %v.54 = extractvalue { i32, i1 } %tmp.37, 0
  %tmp.38 = extractvalue { i32, i1 } %tmp.37, 1
  br i1 %tmp.38, label %zil.trap, label %checked.13
checked.13:
  %tmp.39 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.50, i32 %v.54)
  %v.55 = extractvalue { i32, i1 } %tmp.39, 0
  %tmp.40 = extractvalue { i32, i1 } %tmp.39, 1
  br i1 %tmp.40, label %zil.trap, label %checked.14
checked.14:
  %v.56 = icmp ne i32 %v.49, %v.55
  br i1 %v.56, label %bb.10, label %bb.11
bb.9:
  %tmp.41 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 2
  %v.72 = load i32, ptr %tmp.41, align 4
  %v.73 = load i64, ptr %local.1, align 8
  %v.74 = load i64, ptr %local.1, align 8
  %tmp.42 = call { i64, i1 } @llvm.smul.with.overflow.i64(i64 %v.73, i64 %v.74)
  %v.75 = extractvalue { i64, i1 } %tmp.42, 0
  %tmp.43 = extractvalue { i64, i1 } %tmp.42, 1
  br i1 %tmp.43, label %zil.trap, label %checked.15
checked.15:
  %tmp.44 = icmp slt i64 %v.75, -2147483648
  %tmp.45 = icmp sgt i64 %v.75, 2147483647
  %tmp.46 = or i1 %tmp.44, %tmp.45
  br i1 %tmp.46, label %zil.trap, label %checked.16
checked.16:
  %v.76 = trunc i64 %v.75 to i32
  %v.77 = icmp ne i32 %v.72, %v.76
  br i1 %v.77, label %bb.16, label %bb.17
bb.10:
  %v.57 = or i1 false, true
  store i1 %v.57, ptr %local.10, align 1
  br label %bb.12
bb.11:
  %v.58 = load i32, ptr %local.9, align 4
  %v.59 = add i32 0, 2
  %v.60 = load i64, ptr %local.3, align 8
  %tmp.47 = icmp slt i64 %v.60, -2147483648
  %tmp.48 = icmp sgt i64 %v.60, 2147483647
  %tmp.49 = or i1 %tmp.47, %tmp.48
  br i1 %tmp.49, label %zil.trap, label %checked.17
checked.17:
  %v.61 = trunc i64 %v.60 to i32
  %tmp.50 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.59, i32 %v.61)
  %v.62 = extractvalue { i32, i1 } %tmp.50, 0
  %tmp.51 = extractvalue { i32, i1 } %tmp.50, 1
  br i1 %tmp.51, label %zil.trap, label %checked.18
checked.18:
  %v.63 = add i32 0, 2
  %tmp.52 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.62, i32 %v.63)
  %v.64 = extractvalue { i32, i1 } %tmp.52, 0
  %tmp.53 = extractvalue { i32, i1 } %tmp.52, 1
  br i1 %tmp.53, label %zil.trap, label %checked.19
checked.19:
  %v.65 = icmp ne i32 %v.58, %v.64
  store i1 %v.65, ptr %local.10, align 1
  br label %bb.12
bb.12:
  %v.66 = load i1, ptr %local.10, align 1
  br i1 %v.66, label %bb.13, label %bb.14
bb.13:
  %v.67 = add i32 0, 10
  %tmp.54 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.68 = insertvalue { i1, {}, i32 } %tmp.54, i32 %v.67, 2
  %tmp.55 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.55, label %package.drop.20, label %package.done.20
package.drop.20:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.56 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.56, label %package.field.drop.21, label %package.field.done.21
package.field.drop.21:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.57 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.57)
  br label %package.field.done.21
package.field.done.21:
  %tmp.58 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.58, label %package.field.drop.22, label %package.field.done.22
package.field.drop.22:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.59 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.59)
  br label %package.field.done.22
package.field.done.22:
  br label %package.done.20
package.done.20:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.60 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.60, label %package.drop.23, label %package.done.23
package.drop.23:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.61 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.61)
  %tmp.62 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.62)
  br label %package.done.23
package.done.23:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.68
bb.14:
  br label %bb.15
bb.15:
  %v.69 = load i64, ptr %local.3, align 8
  %v.70 = add i64 0, 1
  %tmp.63 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.69, i64 %v.70)
  %v.71 = extractvalue { i64, i1 } %tmp.63, 0
  %tmp.64 = extractvalue { i64, i1 } %tmp.63, 1
  br i1 %tmp.64, label %zil.trap, label %checked.24
checked.24:
  store i64 %v.71, ptr %local.3, align 8
  br label %bb.7
bb.16:
  %v.78 = add i32 0, 11
  %tmp.65 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.79 = insertvalue { i1, {}, i32 } %tmp.65, i32 %v.78, 2
  %tmp.66 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.66, label %package.drop.25, label %package.done.25
package.drop.25:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.67 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.67, label %package.field.drop.26, label %package.field.done.26
package.field.drop.26:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.68 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.68)
  br label %package.field.done.26
package.field.done.26:
  %tmp.69 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.69, label %package.field.drop.27, label %package.field.done.27
package.field.drop.27:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.70 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.70)
  br label %package.field.done.27
package.field.done.27:
  br label %package.done.25
package.done.25:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.71 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.71, label %package.drop.28, label %package.done.28
package.drop.28:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.72 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.72)
  %tmp.73 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.73)
  br label %package.done.28
package.done.28:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.79
bb.17:
  br label %bb.18
bb.18:
  %v.80 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  %tmp.74 = getelementptr { ptr, i64, i64 }, ptr %v.80, i32 0, i32 2
  %v.81 = load i64, ptr %tmp.74, align 8
  store i64 %v.81, ptr %local.11, align 8
  %v.82 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  %v.83 = load i64, ptr %local.1, align 8
  %v.84 = add i64 0, 2
  %tmp.75 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.83, i64 %v.84)
  %v.85 = extractvalue { i64, i1 } %tmp.75, 0
  %tmp.76 = extractvalue { i64, i1 } %tmp.75, 1
  br i1 %tmp.76, label %zil.trap, label %checked.29
checked.29:
  %v.86 = call i1 @zil.fn.14(ptr %v.82, i64 %v.85)
  store i1 %v.86, ptr %local.12, align 1
  %v.87 = load i1, ptr %local.12, align 1
  %v.88 = xor i1 %v.87, true
  br i1 %v.88, label %bb.20, label %bb.19
bb.19:
  %v.89 = or i1 false, false
  store i1 %v.89, ptr %local.13, align 1
  br label %bb.21
bb.20:
  %v.90 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  %tmp.77 = getelementptr { ptr, i64, i64 }, ptr %v.90, i32 0, i32 2
  %v.91 = load i64, ptr %tmp.77, align 8
  %v.92 = load i64, ptr %local.11, align 8
  %v.93 = icmp ne i64 %v.91, %v.92
  store i1 %v.93, ptr %local.13, align 1
  br label %bb.21
bb.21:
  %v.94 = load i1, ptr %local.13, align 1
  br i1 %v.94, label %bb.22, label %bb.23
bb.22:
  %v.95 = add i32 0, 12
  %tmp.78 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.96 = insertvalue { i1, {}, i32 } %tmp.78, i32 %v.95, 2
  %tmp.79 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.79, label %package.drop.30, label %package.done.30
package.drop.30:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.80 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.80, label %package.field.drop.31, label %package.field.done.31
package.field.drop.31:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.81 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.81)
  br label %package.field.done.31
package.field.done.31:
  %tmp.82 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.82, label %package.field.drop.32, label %package.field.done.32
package.field.drop.32:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.83 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.83)
  br label %package.field.done.32
package.field.done.32:
  br label %package.done.30
package.done.30:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.84 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.84, label %package.drop.33, label %package.done.33
package.drop.33:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.85 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.85)
  %tmp.86 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.86)
  br label %package.done.33
package.done.33:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.96
bb.23:
  br label %bb.24
bb.24:
  %v.97 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  %v.98 = call i64 @zil.fn.16(ptr %v.97)
  %v.99 = load i64, ptr %local.1, align 8
  %v.100 = icmp ne i64 %v.98, %v.99
  br i1 %v.100, label %bb.25, label %bb.26
bb.25:
  %v.101 = add i32 0, 13
  %tmp.87 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.102 = insertvalue { i1, {}, i32 } %tmp.87, i32 %v.101, 2
  %tmp.88 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.88, label %package.drop.34, label %package.done.34
package.drop.34:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.89 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.89, label %package.field.drop.35, label %package.field.done.35
package.field.drop.35:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.90 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.90)
  br label %package.field.done.35
package.field.done.35:
  %tmp.91 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.91, label %package.field.drop.36, label %package.field.done.36
package.field.drop.36:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.92 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.92)
  br label %package.field.done.36
package.field.done.36:
  br label %package.done.34
package.done.34:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.93 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.93, label %package.drop.37, label %package.done.37
package.drop.37:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.94 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.94)
  %tmp.95 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.95)
  br label %package.done.37
package.done.37:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.102
bb.26:
  br label %bb.27
bb.27:
  %v.103 = add i64 0, 0
  store i64 %v.103, ptr %local.3, align 8
  br label %bb.28
bb.28:
  %v.104 = load i64, ptr %local.3, align 8
  %v.105 = load i64, ptr %local.1, align 8
  %v.106 = icmp slt i64 %v.104, %v.105
  br i1 %v.106, label %bb.29, label %bb.30
bb.29:
  %v.107 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  %v.108 = load i64, ptr %local.3, align 8
  %tmp.96 = getelementptr { ptr, i64, i64 }, ptr %v.107, i32 0, i32 1
  %tmp.97 = load i64, ptr %tmp.96, align 8
  %tmp.98 = icmp ult i64 %v.108, %tmp.97
  br i1 %tmp.98, label %store.element.ok.38, label %zil.trap
store.element.ok.38:
  %tmp.99 = getelementptr { ptr, i64, i64 }, ptr %v.107, i32 0, i32 0
  %tmp.100 = load ptr, ptr %tmp.99, align 8
  %v.109 = getelementptr { { i32 }, i32 }, ptr %tmp.100, i64 %v.108
  store ptr %v.109, ptr %local.14, align 8
  %v.110 = load ptr, ptr %local.14, align 8
  %v.111 = getelementptr { { i32 }, i32 }, ptr %v.110, i32 0, i32 1
  %v.112 = getelementptr i8, ptr %v.111, i64 0
  %v.113 = load i32, ptr %v.112, align 4
  %v.114 = add i32 0, 66
  %v.115 = add i32 0, 3
  %v.116 = load i64, ptr %local.3, align 8
  %tmp.101 = icmp slt i64 %v.116, -2147483648
  %tmp.102 = icmp sgt i64 %v.116, 2147483647
  %tmp.103 = or i1 %tmp.101, %tmp.102
  br i1 %tmp.103, label %zil.trap, label %checked.39
checked.39:
  %v.117 = trunc i64 %v.116 to i32
  %tmp.104 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.115, i32 %v.117)
  %v.118 = extractvalue { i32, i1 } %tmp.104, 0
  %tmp.105 = extractvalue { i32, i1 } %tmp.104, 1
  br i1 %tmp.105, label %zil.trap, label %checked.40
checked.40:
  %tmp.106 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.114, i32 %v.118)
  %v.119 = extractvalue { i32, i1 } %tmp.106, 0
  %tmp.107 = extractvalue { i32, i1 } %tmp.106, 1
  br i1 %tmp.107, label %zil.trap, label %checked.41
checked.41:
  %v.120 = icmp ne i32 %v.113, %v.119
  br i1 %v.120, label %bb.31, label %bb.32
bb.30:
  %v.126 = add i32 0, 0
  %tmp.108 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.127 = insertvalue { i1, {}, i32 } %tmp.108, i32 %v.126, 2
  %tmp.109 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.109, label %package.drop.42, label %package.done.42
package.drop.42:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.110 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.110, label %package.field.drop.43, label %package.field.done.43
package.field.drop.43:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.111 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.111)
  br label %package.field.done.43
package.field.done.43:
  %tmp.112 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.112, label %package.field.drop.44, label %package.field.done.44
package.field.drop.44:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.113 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.113)
  br label %package.field.done.44
package.field.done.44:
  br label %package.done.42
package.done.42:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.114 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.114, label %package.drop.45, label %package.done.45
package.drop.45:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.115 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.115)
  %tmp.116 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.116)
  br label %package.done.45
package.done.45:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.127
bb.31:
  %v.121 = add i32 0, 14
  %tmp.117 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.122 = insertvalue { i1, {}, i32 } %tmp.117, i32 %v.121, 2
  %tmp.118 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.118, label %package.drop.46, label %package.done.46
package.drop.46:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.119 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.119, label %package.field.drop.47, label %package.field.done.47
package.field.drop.47:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.120 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.120)
  br label %package.field.done.47
package.field.done.47:
  %tmp.121 = load i1, ptr %package.live.2.1, align 1
  br i1 %tmp.121, label %package.field.drop.48, label %package.field.done.48
package.field.drop.48:
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.122 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.122)
  br label %package.field.done.48
package.field.done.48:
  br label %package.done.46
package.done.46:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %package.live.2.1, align 1
  %tmp.123 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.123, label %package.drop.49, label %package.done.49
package.drop.49:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.124 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.store.2.drop(ptr %tmp.124)
  %tmp.125 = getelementptr { { ptr, i64, i64 }, %zil_env_0, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.closure.drop.0(ptr %tmp.125)
  br label %package.done.49
package.done.49:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.122
bb.32:
  br label %bb.33
bb.33:
  %v.123 = load i64, ptr %local.3, align 8
  %v.124 = add i64 0, 1
  %tmp.126 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.123, i64 %v.124)
  %v.125 = extractvalue { i64, i1 } %tmp.126, 0
  %tmp.127 = extractvalue { i64, i1 } %tmp.126, 1
  br i1 %tmp.127, label %zil.trap, label %checked.50
checked.50:
  store i64 %v.125, ptr %local.3, align 8
  br label %bb.28
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
declare void @zil_probe_ExitProcess(i32)
define void @zil.pe.entry() {
entry:
  %code = call i32 @main()
  call void @zil_probe_ExitProcess(i32 %code)
  unreachable
}

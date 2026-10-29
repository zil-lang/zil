; ZIL verified LLVM module
; verified-fingerprint: 15f44746a1f20d437280f9cd9f6cb6d2af7613823507d89daf54bbf672c78a3b
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32, { i32 }, i32, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  call void @zil.fn.5(ptr %payload.1)
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

define internal i1 @zil.store.3.reserve(ptr %head, i64 %requested) {
entry:
  %stride = ptrtoint ptr getelementptr ({ { i32 }, { ptr, i64, i64 }, i32 }, ptr null, i64 1) to i64
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
define internal { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.store.3.push(ptr %head, { { i32 }, { ptr, i64, i64 }, i32 } %item) {
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
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %data, i64 %len
  store { { i32 }, { ptr, i64, i64 }, i32 } %item, ptr %slot, align 8
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } zeroinitializer
full:
  %tag = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } zeroinitializer, i1 true, 0
  %error = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %tag, { { i32 }, { ptr, i64, i64 }, i32 } %item, 1
  ret { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %error
}
define internal { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.store.3.pop(ptr %head) {
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
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %data, i64 %next.len
  %item = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %slot, align 8
  %result = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } zeroinitializer, { { i32 }, { ptr, i64, i64 }, i32 } %item, 2
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %result
empty:
  %error = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } zeroinitializer, i1 true, 0
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %error
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
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %data, i64 %index
  %field.0 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %slot, i32 0, i32 0
  call void @zil.fn.5(ptr %field.0)
  %field.1 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %slot, i32 0, i32 1
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
define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
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
  %v.7 = add i32 0, 51
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
  %v.11 = add i32 0, 72
  %v.12 = call %zil_env_0 @zil.fn.3(i32 %v.11)
  store %zil_env_0 %v.12, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.13 = load i32, ptr %local.0, align 4
  %v.14 = add i32 0, 48
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.16 = load %zil_env_0, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.17 = call { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.14(%zil_env_0 %v.16, i32 %v.15)
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.3, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.17, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %tmp.3 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.3, i32 0, i32 5
  %v.18 = load i1, ptr %tmp.3, align 1
  %v.19 = xor i1 %v.18, true
  br i1 %v.19, label %bb.7, label %bb.8
bb.7:
  %v.20 = add i32 0, 1
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.5 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.5(ptr %tmp.5)
  %tmp.6 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.3, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.6)
  %tmp.7 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.3, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.7)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.4, align 1
  ret i32 %v.20
bb.8:
  br label %bb.9
bb.9:
  %v.21 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.21, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.22 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  %v.23 = call i32 @zil.fn.15({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.22)
  %tmp.8 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.8, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.9 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.4, i32 0, i32 0
  call void @zil.fn.5(ptr %tmp.9)
  %tmp.10 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.4, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.10)
  %tmp.11 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.4, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.11)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.12 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.12, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.13 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.5(ptr %tmp.13)
  %tmp.14 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.3, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.14)
  %tmp.15 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.3, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.15)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.23
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { ptr, i64, i64 }, i1, i32 } @zil.fn.2(i32 %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  %local.3 = alloca i1, align 1
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { ptr, i64, i64 }, align 8
  %local.6 = alloca i1, align 1
  %local.7 = alloca i32, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { ptr, i64, i64 }, align 8
  store i32 %arg.0, ptr %local.0, align 4
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = call { ptr, i64, i64 } @zil.fn.11()
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.1 = getelementptr i8, ptr %local.2, i64 0
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = load i32, ptr %local.1, align 4
  %v.4 = call i1 @zil.fn.8(ptr %v.1, i32 %v.2, i32 %v.3)
  store i1 %v.4, ptr %local.3, align 1
  %v.5 = getelementptr i8, ptr %local.2, i64 0
  %v.6 = call i32 @zil.fn.10(ptr %v.5)
  store i32 %v.6, ptr %local.4, align 4
  store i1 false, ptr %resource.live.8, align 1
  %v.7 = load { ptr, i64, i64 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.7, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.8 = load i1, ptr %local.3, align 1
  store i1 %v.8, ptr %local.6, align 1
  %v.9 = load i32, ptr %local.4, align 4
  store i32 %v.9, ptr %local.7, align 4
  %v.10 = load { ptr, i64, i64 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.10, ptr %local.8, align 8
  store i1 true, ptr %resource.live.8, align 1
  %v.11 = load i1, ptr %local.6, align 1
  %v.12 = load i32, ptr %local.7, align 4
  %v.13 = load { ptr, i64, i64 }, ptr %local.8, align 8
  store i1 false, ptr %resource.live.8, align 1
%tmp.0 = insertvalue { { ptr, i64, i64 }, i1, i32 } zeroinitializer, { ptr, i64, i64 } %v.13, 0
%tmp.1 = insertvalue { { ptr, i64, i64 }, i1, i32 } %tmp.0, i1 %v.11, 1
%v.14 = insertvalue { { ptr, i64, i64 }, i1, i32 } %tmp.1, i32 %v.12, 2
  %tmp.2 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.store.3.drop(ptr %local.8)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.3 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.store.3.drop(ptr %local.5)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.3.drop(ptr %local.2)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.2, align 1
  ret { { ptr, i64, i64 }, i1, i32 } %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.3(i32 %arg.0) {
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
  %v.3 = add i32 0, 0
  store i32 %v.3, ptr %local.3, align 4
  %v.4 = load i32, ptr %local.3, align 4
  %v.5 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.6 = load i32, ptr %local.0, align 4
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, i32 %v.4, 0
  %tmp.1 = insertvalue %zil_env_0 %tmp.0, { i32 } %v.5, 1
  %tmp.2 = insertvalue %zil_env_0 %tmp.1, i32 %v.6, 2
  %v.7 = insertvalue %zil_env_0 %tmp.2, i1 true, 3
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.5(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  ret %zil_env_0 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.5(ptr %arg.0) {
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

define { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.6(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32 }, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { ptr, i64, i64 }, align 8
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.3, align 1
  store { ptr, i64, i64 } %v.3, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.4 = load i32, ptr %local.0, align 4
  store i32 %v.4, ptr %local.4, align 4
  %v.5 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.5, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.6 = load { ptr, i64, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  store { ptr, i64, i64 } %v.6, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  %v.7 = load i32, ptr %local.4, align 4
  %v.8 = load { i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  %v.9 = load { ptr, i64, i64 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
%tmp.0 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } zeroinitializer, { i32 } %v.8, 0
%tmp.1 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.0, { ptr, i64, i64 } %v.9, 1
%v.10 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.1, i32 %v.7, 2
  %tmp.2 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.bytes.drop(ptr %local.6)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.3 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.5(ptr %local.5)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.bytes.drop(ptr %local.3)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.5 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.5(ptr %local.2)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, { ptr, i64, i64 }, i32 } %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.7(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.8(ptr %arg.0, i32 %arg.1, i32 %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  store i32 %arg.2, ptr %local.2, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = sext i32 %v.1 to i64
  %v.3 = call i1 @zil.store.3.reserve(ptr %v.0, i64 %v.2)
  %v.4 = xor i1 %v.3, true
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %v.5 = or i1 false, false
  ret i1 %v.5
bb.2:
  br label %bb.3
bb.3:
  %v.6 = add i32 0, 0
  store i32 %v.6, ptr %local.3, align 4
  br label %bb.4
bb.4:
  %v.7 = load i32, ptr %local.3, align 4
  %v.8 = load i32, ptr %local.1, align 4
  %v.9 = icmp slt i32 %v.7, %v.8
  br i1 %v.9, label %bb.5, label %bb.6
bb.5:
  store i1 false, ptr %resource.live.4, align 1
  %v.10 = load ptr, ptr %local.0, align 8
  %v.11 = load i32, ptr %local.2, align 4
  %v.12 = load i32, ptr %local.3, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.14 = call { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.6(i32 %v.13)
  %v.15 = call { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.store.3.push(ptr %v.10, { { i32 }, { ptr, i64, i64 }, i32 } %v.14)
  store { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %v.15, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %tmp.2 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.2, label %error.drop.1, label %error.done.1
error.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.3 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} }, ptr %local.4, i32 0, i32 0
  %tmp.4 = load i1, ptr %tmp.3, align 1
  br i1 %tmp.4, label %error.payload.1, label %success.payload.1
error.payload.1:
  %tmp.5 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} }, ptr %local.4, i32 0, i32 1
  %tmp.6 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.5, i32 0, i32 0
  call void @zil.fn.5(ptr %tmp.6)
  %tmp.7 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.5, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.7)
  br label %error.done.1
success.payload.1:
  br label %error.done.1
error.done.1:
  %v.16 = load i32, ptr %local.3, align 4
  %v.17 = add i32 0, 1
  %tmp.8 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.16, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.18, ptr %local.3, align 4
  store i1 false, ptr %resource.live.4, align 1
  br label %bb.4
bb.6:
  %v.19 = load ptr, ptr %local.0, align 8
  %tmp.10 = getelementptr { ptr, i64, i64 }, ptr %v.19, i32 0, i32 1
  %v.20 = load i64, ptr %tmp.10, align 8
  %v.21 = load i32, ptr %local.1, align 4
  %v.22 = sext i32 %v.21 to i64
  %v.23 = icmp eq i64 %v.20, %v.22
  ret i1 %v.23
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.9(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %v.0, i32 0, i32 2
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.10(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  %local.2 = alloca i32, align 4
  %local.3 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 0
  store i64 %v.0, ptr %local.1, align 8
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.2, align 4
  br label %bb.1
bb.1:
  %v.2 = load i64, ptr %local.1, align 8
  %v.3 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.3, i32 0, i32 1
  %v.4 = load i64, ptr %tmp.0, align 8
  %v.5 = icmp slt i64 %v.2, %v.4
  br i1 %v.5, label %bb.2, label %bb.3
bb.2:
  %v.6 = load ptr, ptr %local.0, align 8
  %v.7 = load i64, ptr %local.1, align 8
  %tmp.1 = getelementptr { ptr, i64, i64 }, ptr %v.6, i32 0, i32 1
  %tmp.2 = load i64, ptr %tmp.1, align 8
  %tmp.3 = icmp ult i64 %v.7, %tmp.2
  br i1 %tmp.3, label %store.element.ok.0, label %zil.trap
store.element.ok.0:
  %tmp.4 = getelementptr { ptr, i64, i64 }, ptr %v.6, i32 0, i32 0
  %tmp.5 = load ptr, ptr %tmp.4, align 8
  %v.8 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.5, i64 %v.7
  store ptr %v.8, ptr %local.3, align 8
  %v.9 = load i32, ptr %local.2, align 4
  %v.10 = load ptr, ptr %local.3, align 8
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = call i32 @zil.fn.9(ptr %v.11)
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.9, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.13, ptr %local.2, align 4
  %v.14 = load i64, ptr %local.1, align 8
  %v.15 = add i64 0, 1
  %tmp.8 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.14, i64 %v.15)
  %v.16 = extractvalue { i64, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i64, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.2
checked.2:
  store i64 %v.16, ptr %local.1, align 8
  br label %bb.1
bb.3:
  %v.17 = load i32, ptr %local.2, align 4
  ret i32 %v.17
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.11() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.12(ptr %arg.0, i64 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  %local.2 = alloca i64, align 8
  %local.3 = alloca i64, align 8
  %local.4 = alloca i32, align 4
  %local.5 = alloca i1, align 1
  %local.6 = alloca i1, align 1
  store ptr %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.0, i32 0, i32 1
  %v.1 = load i64, ptr %tmp.0, align 8
  store i64 %v.1, ptr %local.2, align 8
  %v.2 = load ptr, ptr %local.0, align 8
  %tmp.1 = getelementptr { ptr, i64, i64 }, ptr %v.2, i32 0, i32 2
  %v.3 = load i64, ptr %tmp.1, align 8
  store i64 %v.3, ptr %local.3, align 8
  %v.4 = load ptr, ptr %local.0, align 8
  %v.5 = call i32 @zil.fn.10(ptr %v.4)
  store i32 %v.5, ptr %local.4, align 4
  %v.6 = load ptr, ptr %local.0, align 8
  %v.7 = load i64, ptr %local.1, align 8
  %v.8 = call i1 @zil.store.3.reserve(ptr %v.6, i64 %v.7)
  store i1 %v.8, ptr %local.5, align 1
  %v.9 = load ptr, ptr %local.0, align 8
  %tmp.2 = getelementptr { ptr, i64, i64 }, ptr %v.9, i32 0, i32 1
  %v.10 = load i64, ptr %tmp.2, align 8
  %v.11 = load i64, ptr %local.2, align 8
  %v.12 = icmp ne i64 %v.10, %v.11
  br i1 %v.12, label %bb.1, label %bb.2
bb.1:
  %v.13 = or i1 false, false
  ret i1 %v.13
bb.2:
  br label %bb.3
bb.3:
  %v.14 = load i1, ptr %local.5, align 1
  %v.15 = xor i1 %v.14, true
  br i1 %v.15, label %bb.5, label %bb.4
bb.4:
  %v.16 = or i1 false, false
  store i1 %v.16, ptr %local.6, align 1
  br label %bb.6
bb.5:
  %v.17 = load ptr, ptr %local.0, align 8
  %tmp.3 = getelementptr { ptr, i64, i64 }, ptr %v.17, i32 0, i32 2
  %v.18 = load i64, ptr %tmp.3, align 8
  %v.19 = load i64, ptr %local.3, align 8
  %v.20 = icmp ne i64 %v.18, %v.19
  store i1 %v.20, ptr %local.6, align 1
  br label %bb.6
bb.6:
  %v.21 = load i1, ptr %local.6, align 1
  br i1 %v.21, label %bb.7, label %bb.8
bb.7:
  %v.22 = or i1 false, false
  ret i1 %v.22
bb.8:
  br label %bb.9
bb.9:
  %v.23 = load ptr, ptr %local.0, align 8
  %v.24 = call i32 @zil.fn.10(ptr %v.23)
  %v.25 = load i32, ptr %local.4, align 4
  %v.26 = icmp eq i32 %v.24, %v.25
  ret i1 %v.26
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.13(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  %local.2 = alloca i32, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.1, align 8
  %tmp.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 2
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
  %tmp.4 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
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

define { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } @zil.fn.14(%zil_env_0 %arg.0, i32 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { ptr, i64, i64 }, i1, i32 }, align 8
  %package.live.2.0 = alloca i1, align 1
  store i1 false, ptr %package.live.2.0, align 1
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { ptr, i64, i64 }, align 8
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca %zil_env_0, align 4
  %local.7 = alloca i32, align 4
  %local.8 = alloca i32, align 4
  %local.9 = alloca i1, align 1
  %resource.live.10 = alloca i1, align 1
  store i1 false, ptr %resource.live.10, align 1
  %local.10 = alloca { i32 }, align 4
  %resource.live.11 = alloca i1, align 1
  store i1 false, ptr %resource.live.11, align 1
  %local.11 = alloca { ptr, i64, i64 }, align 8
  %resource.live.12 = alloca i1, align 1
  store i1 false, ptr %resource.live.12, align 1
  %local.12 = alloca %zil_env_0, align 4
  store %zil_env_0 %arg.0, ptr %local.0, align 4
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.1, align 4
  %v.1 = add i32 0, 68
  %v.2 = call { { ptr, i64, i64 }, i1, i32 } @zil.fn.2(i32 %v.0, i32 %v.1)
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  store { { ptr, i64, i64 }, i1, i32 } %v.2, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  store i1 true, ptr %package.live.2.0, align 1
  store i1 false, ptr %resource.live.12, align 1
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.10, align 1
  %v.3 = add i32 0, 90
  store i32 %v.3, ptr %local.3, align 4
  %v.4 = load i32, ptr %local.3, align 4
%v.5 = insertvalue { i32 } zeroinitializer, i32 %v.4, 0
  store i1 false, ptr %resource.live.4, align 1
  store { i32 } %v.5, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %tmp.0 = getelementptr { { ptr, i64, i64 }, i1, i32 }, ptr %local.2, i32 0, i32 0
  %v.6 = load { ptr, i64, i64 }, ptr %tmp.0, align 8
  store i1 false, ptr %package.live.2.0, align 1
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.6, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.7 = load %zil_env_0, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.6, align 1
  store %zil_env_0 %v.7, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.8 = add i32 0, 0
  store i32 %v.8, ptr %local.7, align 4
  %tmp.1 = getelementptr { { ptr, i64, i64 }, i1, i32 }, ptr %local.2, i32 0, i32 2
  %v.9 = load i32, ptr %tmp.1, align 4
  store i32 %v.9, ptr %local.8, align 4
  %tmp.2 = getelementptr { { ptr, i64, i64 }, i1, i32 }, ptr %local.2, i32 0, i32 1
  %v.10 = load i1, ptr %tmp.2, align 1
  store i1 %v.10, ptr %local.9, align 1
  %v.11 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  store { i32 } %v.11, ptr %local.10, align 4
  store i1 true, ptr %resource.live.10, align 1
  %v.12 = load { ptr, i64, i64 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.12, ptr %local.11, align 8
  store i1 true, ptr %resource.live.11, align 1
  %v.13 = load %zil_env_0, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  store %zil_env_0 %v.13, ptr %local.12, align 4
  store i1 true, ptr %resource.live.12, align 1
  %v.14 = load i32, ptr %local.7, align 4
  %v.15 = load i32, ptr %local.8, align 4
  %v.16 = load i1, ptr %local.9, align 1
  %v.17 = load { i32 }, ptr %local.10, align 4
  store i1 false, ptr %resource.live.10, align 1
  %v.18 = load { ptr, i64, i64 }, ptr %local.11, align 8
  store i1 false, ptr %resource.live.11, align 1
  %v.19 = load %zil_env_0, ptr %local.12, align 4
  store i1 false, ptr %resource.live.12, align 1
%tmp.3 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } zeroinitializer, { i32 } %v.17, 0
%tmp.4 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %tmp.3, { ptr, i64, i64 } %v.18, 1
%tmp.5 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %tmp.4, %zil_env_0 %v.19, 2
%tmp.6 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %tmp.5, i32 %v.14, 3
%tmp.7 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %tmp.6, i32 %v.15, 4
%v.20 = insertvalue { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %tmp.7, i1 %v.16, 5
  %tmp.8 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.8, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.12, align 1
  call void @zil.closure.drop.0(ptr %local.12)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.12, align 1
  %tmp.9 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.9, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.11, align 1
  call void @zil.store.3.drop(ptr %local.11)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.10 = load i1, ptr %resource.live.10, align 1
  br i1 %tmp.10, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.10, align 1
  call void @zil.fn.5(ptr %local.10)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.10, align 1
  %tmp.11 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.11, label %closure.drop.3, label %closure.done.3
closure.drop.3:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.closure.drop.0(ptr %local.6)
  br label %closure.done.3
closure.done.3:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.12 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.12, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.store.3.drop(ptr %local.5)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.13 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.13, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.5(ptr %local.4)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.14 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.14, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.15 = load i1, ptr %package.live.2.0, align 1
  br i1 %tmp.15, label %package.field.drop.7, label %package.field.done.7
package.field.drop.7:
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.16 = getelementptr { { ptr, i64, i64 }, i1, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.store.3.drop(ptr %tmp.16)
  br label %package.field.done.7
package.field.done.7:
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %package.live.2.0, align 1
  %tmp.17 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.17, label %closure.drop.8, label %closure.done.8
closure.drop.8:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.8
closure.done.8:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.20
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.15({ { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, align 8
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %package.live.1.2 = alloca i1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  %local.4 = alloca i1, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  store { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 true, ptr %package.live.1.1, align 1
  store i1 true, ptr %package.live.1.2, align 1
  %tmp.0 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 2
  %v.1 = load %zil_env_0, ptr %tmp.0, align 4
  store i1 false, ptr %package.live.1.2, align 1
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.1, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %tmp.1 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 1
  %v.2 = load { ptr, i64, i64 }, ptr %tmp.1, align 8
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.3, align 1
  store { ptr, i64, i64 } %v.2, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = getelementptr i8, ptr %local.3, i64 0
  %v.4 = call i32 @zil.fn.10(ptr %v.3)
  %tmp.2 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 4
  %v.5 = load i32, ptr %tmp.2, align 4
  %v.6 = icmp ne i32 %v.4, %v.5
  br i1 %v.6, label %bb.1, label %bb.2
bb.1:
  %v.7 = add i32 0, 2
  %tmp.3 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.store.3.drop(ptr %local.3)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.5 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.5, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.6 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.6, label %package.field.drop.3, label %package.field.done.3
package.field.drop.3:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.7 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.5(ptr %tmp.7)
  br label %package.field.done.3
package.field.done.3:
  %tmp.8 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.8, label %package.field.drop.4, label %package.field.done.4
package.field.drop.4:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.9 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.9)
  br label %package.field.done.4
package.field.done.4:
  %tmp.10 = load i1, ptr %package.live.1.2, align 1
  br i1 %tmp.10, label %package.field.drop.5, label %package.field.done.5
package.field.drop.5:
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.11 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.11)
  br label %package.field.done.5
package.field.done.5:
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.12 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.12, label %package.drop.6, label %package.done.6
package.drop.6:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.13 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.5(ptr %tmp.13)
  %tmp.14 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.14)
  %tmp.15 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.15)
  br label %package.done.6
package.done.6:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.7
bb.2:
  br label %bb.3
bb.3:
  %v.8 = call i32 @zil.fn.13(ptr %local.2)
  %v.9 = add i32 0, 1
  %v.10 = icmp ne i32 %v.8, %v.9
  br i1 %v.10, label %bb.4, label %bb.5
bb.4:
  %v.11 = or i1 false, true
  store i1 %v.11, ptr %local.4, align 1
  br label %bb.6
bb.5:
  %v.12 = call i32 @zil.fn.13(ptr %local.2)
  %v.13 = add i32 0, 2
  %v.14 = icmp ne i32 %v.12, %v.13
  store i1 %v.14, ptr %local.4, align 1
  br label %bb.6
bb.6:
  %v.15 = load i1, ptr %local.4, align 1
  br i1 %v.15, label %bb.7, label %bb.8
bb.7:
  %v.16 = add i32 0, 3
  %tmp.16 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.16, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.store.3.drop(ptr %local.3)
  br label %resource.done.7
resource.done.7:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.17 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.17, label %closure.drop.8, label %closure.done.8
closure.drop.8:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.8
closure.done.8:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.18 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.18, label %package.drop.9, label %package.done.9
package.drop.9:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.19 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.19, label %package.field.drop.10, label %package.field.done.10
package.field.drop.10:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.20 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.5(ptr %tmp.20)
  br label %package.field.done.10
package.field.done.10:
  %tmp.21 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.21, label %package.field.drop.11, label %package.field.done.11
package.field.drop.11:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.22 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.22)
  br label %package.field.done.11
package.field.done.11:
  %tmp.23 = load i1, ptr %package.live.1.2, align 1
  br i1 %tmp.23, label %package.field.drop.12, label %package.field.done.12
package.field.drop.12:
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.24 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.24)
  br label %package.field.done.12
package.field.done.12:
  br label %package.done.9
package.done.9:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.25 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.25, label %package.drop.13, label %package.done.13
package.drop.13:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.26 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.5(ptr %tmp.26)
  %tmp.27 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.27)
  %tmp.28 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.28)
  br label %package.done.13
package.done.13:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.16
bb.8:
  br label %bb.9
bb.9:
  %v.17 = add i32 0, 1
  %tmp.29 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.29, label %resource.drop.14, label %resource.done.14
resource.drop.14:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.store.3.drop(ptr %local.3)
  br label %resource.done.14
resource.done.14:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.30 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.30, label %closure.drop.15, label %closure.done.15
closure.drop.15:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.15
closure.done.15:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.31 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.31, label %package.drop.16, label %package.done.16
package.drop.16:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.32 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.32, label %package.field.drop.17, label %package.field.done.17
package.field.drop.17:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.33 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.5(ptr %tmp.33)
  br label %package.field.done.17
package.field.done.17:
  %tmp.34 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.34, label %package.field.drop.18, label %package.field.done.18
package.field.drop.18:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.35 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.35)
  br label %package.field.done.18
package.field.done.18:
  %tmp.36 = load i1, ptr %package.live.1.2, align 1
  br i1 %tmp.36, label %package.field.drop.19, label %package.field.done.19
package.field.drop.19:
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.37 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.1, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.37)
  br label %package.field.done.19
package.field.done.19:
  br label %package.done.16
package.done.16:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.2, align 1
  %tmp.38 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.38, label %package.drop.20, label %package.done.20
package.drop.20:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.39 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.5(ptr %tmp.39)
  %tmp.40 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 1
  call void @zil.store.3.drop(ptr %tmp.40)
  %tmp.41 = getelementptr { { i32 }, { ptr, i64, i64 }, %zil_env_0, i32, i32, i1 }, ptr %local.0, i32 0, i32 2
  call void @zil.closure.drop.0(ptr %tmp.41)
  br label %package.done.20
package.done.20:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.17
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

; ZIL verified LLVM module
; verified-fingerprint: 0064ecca36212358c21ccfa57ab28fbe24b3a82b0b914dabf22f9040e462a604
target triple = "x86_64-unknown-linux-gnu"

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

define internal i1 @zil.store.2.reserve(ptr %head, i64 %requested) {
entry:
  %stride = ptrtoint ptr getelementptr ({ { i32 }, i64 }, ptr null, i64 1) to i64
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
define internal { i1, { { i32 }, i64 }, {} } @zil.store.2.push(ptr %head, { { i32 }, i64 } %item) {
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
  %slot = getelementptr { { i32 }, i64 }, ptr %data, i64 %len
  store { { i32 }, i64 } %item, ptr %slot, align 8
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret { i1, { { i32 }, i64 }, {} } zeroinitializer
full:
  %tag = insertvalue { i1, { { i32 }, i64 }, {} } zeroinitializer, i1 true, 0
  %error = insertvalue { i1, { { i32 }, i64 }, {} } %tag, { { i32 }, i64 } %item, 1
  ret { i1, { { i32 }, i64 }, {} } %error
}
define internal { i1, {}, { { i32 }, i64 } } @zil.store.2.pop(ptr %head) {
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
  %slot = getelementptr { { i32 }, i64 }, ptr %data, i64 %next.len
  %item = load { { i32 }, i64 }, ptr %slot, align 8
  %result = insertvalue { i1, {}, { { i32 }, i64 } } zeroinitializer, { { i32 }, i64 } %item, 2
  ret { i1, {}, { { i32 }, i64 } } %result
empty:
  %error = insertvalue { i1, {}, { { i32 }, i64 } } zeroinitializer, i1 true, 0
  ret { i1, {}, { { i32 }, i64 } } %error
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
  %slot = getelementptr { { i32 }, i64 }, ptr %data, i64 %index
  %field.0 = getelementptr { { i32 }, i64 }, ptr %slot, i32 0, i32 0
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

define { { i32 }, i32 } @zil.fn.2(i32 %arg.0) {
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
  call void @zil.fn.1(ptr %local.4)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, i32 } %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, i64 } @zil.fn.3(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i64, align 8
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
  %v.4 = sext i32 %v.3 to i64
  store i64 %v.4, ptr %local.3, align 8
  %v.5 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.5, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.6 = load i64, ptr %local.3, align 8
  %v.7 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
%tmp.0 = insertvalue { { i32 }, i64 } zeroinitializer, { i32 } %v.7, 0
%v.8 = insertvalue { { i32 }, i64 } %tmp.0, i64 %v.6, 1
  %tmp.1 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.1(ptr %local.4)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, i64 } %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, {}, i32 } @zil.fn.4() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i1, { { i32 }, i32 }, {} }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { i32 }, i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i1, { { i32 }, i32 }, {} }, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { ptr, i64, i64 }, align 8
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { { i32 }, i64 }, align 8
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { i1, { { i32 }, i64 }, {} }, align 8
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { { i32 }, i32 }, align 4
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { ptr, i64, i64 }, align 8
  %resource.live.10 = alloca i1, align 1
  store i1 false, ptr %resource.live.10, align 1
  %local.10 = alloca { ptr, i64, i64 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %v.0 = call { ptr, i64, i64 } @zil.fn.5()
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = add i64 0, 2
  %v.3 = call i1 @zil.fn.6(ptr %v.1, i64 %v.2)
  %v.4 = getelementptr i8, ptr %local.0, i64 0
  %v.5 = add i32 0, 65
  %v.6 = call { { i32 }, i32 } @zil.fn.2(i32 %v.5)
  store { { i32 }, i32 } %v.6, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.7 = load { { i32 }, i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.8 = call { i1, { { i32 }, i32 }, {} } @zil.fn.7(ptr %v.4, { { i32 }, i32 } %v.7)
  store { i1, { { i32 }, i32 }, {} } %v.8, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %error.drop.0, label %error.done.0
error.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.1 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.2, i32 0, i32 0
  %tmp.2 = load i1, ptr %tmp.1, align 1
  br i1 %tmp.2, label %error.payload.0, label %success.payload.0
error.payload.0:
  %tmp.3 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.2, i32 0, i32 1
  %tmp.4 = getelementptr { { i32 }, i32 }, ptr %tmp.3, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.4)
  br label %error.done.0
success.payload.0:
  br label %error.done.0
error.done.0:
  %tmp.5 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.5, label %package.drop.1, label %package.done.1
package.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.6 = getelementptr { { i32 }, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.6)
  br label %package.done.1
package.done.1:
  %v.9 = getelementptr i8, ptr %local.0, i64 0
  %v.10 = add i32 0, 66
  %v.11 = call { { i32 }, i32 } @zil.fn.2(i32 %v.10)
  store { { i32 }, i32 } %v.11, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.12 = load { { i32 }, i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  %v.13 = call { i1, { { i32 }, i32 }, {} } @zil.fn.7(ptr %v.9, { { i32 }, i32 } %v.12)
  store { i1, { { i32 }, i32 }, {} } %v.13, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %tmp.7 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.7, label %error.drop.2, label %error.done.2
error.drop.2:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.8 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.4, i32 0, i32 0
  %tmp.9 = load i1, ptr %tmp.8, align 1
  br i1 %tmp.9, label %error.payload.2, label %success.payload.2
error.payload.2:
  %tmp.10 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.4, i32 0, i32 1
  %tmp.11 = getelementptr { { i32 }, i32 }, ptr %tmp.10, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.11)
  br label %error.done.2
success.payload.2:
  br label %error.done.2
error.done.2:
  %tmp.12 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.12, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.13 = getelementptr { { i32 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.13)
  br label %package.done.3
package.done.3:
  %v.14 = call { ptr, i64, i64 } @zil.fn.8()
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.14, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.15 = getelementptr i8, ptr %local.5, i64 0
  %v.16 = add i64 0, 1
  %v.17 = call i1 @zil.fn.9(ptr %v.15, i64 %v.16)
  %v.18 = getelementptr i8, ptr %local.5, i64 0
  %v.19 = add i32 0, 67
  %v.20 = call { { i32 }, i64 } @zil.fn.3(i32 %v.19)
  store { { i32 }, i64 } %v.20, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  %v.21 = load { { i32 }, i64 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  %v.22 = call { i1, { { i32 }, i64 }, {} } @zil.fn.10(ptr %v.18, { { i32 }, i64 } %v.21)
  store { i1, { { i32 }, i64 }, {} } %v.22, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %tmp.14 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.14, label %error.drop.4, label %error.done.4
error.drop.4:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.15 = getelementptr { i1, { { i32 }, i64 }, {} }, ptr %local.7, i32 0, i32 0
  %tmp.16 = load i1, ptr %tmp.15, align 1
  br i1 %tmp.16, label %error.payload.4, label %success.payload.4
error.payload.4:
  %tmp.17 = getelementptr { i1, { { i32 }, i64 }, {} }, ptr %local.7, i32 0, i32 1
  %tmp.18 = getelementptr { { i32 }, i64 }, ptr %tmp.17, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.18)
  br label %error.done.4
success.payload.4:
  br label %error.done.4
error.done.4:
  %tmp.19 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.19, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.20 = getelementptr { { i32 }, i64 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.20)
  br label %package.done.5
package.done.5:
  %v.23 = getelementptr i8, ptr %local.0, i64 0
  %v.24 = call { i1, {}, { { i32 }, i32 } } @zil.fn.11(ptr %v.23)
  %v.25 = extractvalue { i1, {}, { { i32 }, i32 } } %v.24, 0
  br i1 %v.25, label %bb.1, label %bb.2
bb.1:
  %tmp.21 = extractvalue { i1, {}, { { i32 }, i32 } } %v.24, 0
  %tmp.22 = icmp ne i1 %tmp.21, true
  br i1 %tmp.22, label %zil.trap, label %checked.6
checked.6:
  %tmp.23 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.27 = insertvalue { i1, {}, i32 } %tmp.23, {} zeroinitializer, 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  %tmp.24 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.24, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.store.2.drop(ptr %local.5)
  br label %resource.done.7
resource.done.7:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.25 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.25, label %resource.drop.8, label %resource.done.8
resource.drop.8:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.8
resource.done.8:
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.9, align 1
  ret { i1, {}, i32 } %v.27
bb.2:
  %tmp.26 = extractvalue { i1, {}, { { i32 }, i32 } } %v.24, 0
  %tmp.27 = icmp ne i1 %tmp.26, false
  br i1 %tmp.27, label %zil.trap, label %checked.9
checked.9:
  %v.28 = extractvalue { i1, {}, { { i32 }, i32 } } %v.24, 2
  store i1 false, ptr %resource.live.8, align 1
  store { { i32 }, i32 } %v.28, ptr %local.8, align 4
  store i1 true, ptr %resource.live.8, align 1
  %v.29 = load { ptr, i64, i64 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.29, ptr %local.9, align 8
  store i1 true, ptr %resource.live.9, align 1
  %v.30 = load { ptr, i64, i64 }, ptr %local.9, align 8
  store i1 false, ptr %resource.live.9, align 1
  %v.31 = call { ptr, i64, i64 } @zil.fn.12({ ptr, i64, i64 } %v.30)
  %tmp.28 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.28, label %resource.drop.10, label %resource.done.10
resource.drop.10:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.store.2.drop(ptr %local.9)
  br label %resource.done.10
resource.done.10:
  store i1 false, ptr %resource.live.10, align 1
  store { ptr, i64, i64 } %v.31, ptr %local.10, align 8
  store i1 true, ptr %resource.live.10, align 1
  %tmp.29 = getelementptr { { i32 }, i32 }, ptr %local.8, i32 0, i32 1
  %v.32 = load i32, ptr %tmp.29, align 4
  %v.33 = getelementptr i8, ptr %local.0, i64 0
  %v.34 = call i64 @zil.fn.13(ptr %v.33)
  %tmp.30 = icmp slt i64 %v.34, -2147483648
  %tmp.31 = icmp sgt i64 %v.34, 2147483647
  %tmp.32 = or i1 %tmp.30, %tmp.31
  br i1 %tmp.32, label %zil.trap, label %checked.11
checked.11:
  %v.35 = trunc i64 %v.34 to i32
  %tmp.33 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.32, i32 %v.35)
  %v.36 = extractvalue { i32, i1 } %tmp.33, 0
  %tmp.34 = extractvalue { i32, i1 } %tmp.33, 1
  br i1 %tmp.34, label %zil.trap, label %checked.12
checked.12:
  %v.37 = getelementptr i8, ptr %local.10, i64 0
  %v.38 = call i64 @zil.fn.14(ptr %v.37)
  %tmp.35 = icmp slt i64 %v.38, -2147483648
  %tmp.36 = icmp sgt i64 %v.38, 2147483647
  %tmp.37 = or i1 %tmp.35, %tmp.36
  br i1 %tmp.37, label %zil.trap, label %checked.13
checked.13:
  %v.39 = trunc i64 %v.38 to i32
  %tmp.38 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.36, i32 %v.39)
  %v.40 = extractvalue { i32, i1 } %tmp.38, 0
  %tmp.39 = extractvalue { i32, i1 } %tmp.38, 1
  br i1 %tmp.39, label %zil.trap, label %checked.14
checked.14:
  %v.41 = add i32 0, 68
  %tmp.40 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.40, i32 %v.41)
  %v.42 = extractvalue { i32, i1 } %tmp.40, 0
  %tmp.41 = extractvalue { i32, i1 } %tmp.40, 1
  br i1 %tmp.41, label %zil.trap, label %checked.15
checked.15:
  %tmp.42 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.43 = insertvalue { i1, {}, i32 } %tmp.42, i32 %v.42, 2
  %tmp.43 = load i1, ptr %resource.live.10, align 1
  br i1 %tmp.43, label %resource.drop.16, label %resource.done.16
resource.drop.16:
  store i1 false, ptr %resource.live.10, align 1
  call void @zil.store.2.drop(ptr %local.10)
  br label %resource.done.16
resource.done.16:
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  %tmp.44 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.44, label %package.drop.17, label %package.done.17
package.drop.17:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.45 = getelementptr { { i32 }, i32 }, ptr %local.8, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.45)
  br label %package.done.17
package.done.17:
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  %tmp.46 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.46, label %resource.drop.18, label %resource.done.18
resource.drop.18:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.store.2.drop(ptr %local.5)
  br label %resource.done.18
resource.done.18:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.47 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.47, label %resource.drop.19, label %resource.done.19
resource.drop.19:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.19
resource.done.19:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.43
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.5() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.6(ptr %arg.0, i64 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store i64 %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i64, ptr %local.1, align 8
  %v.2 = call i1 @zil.store.1.reserve(ptr %v.0, i64 %v.1)
  ret i1 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { { i32 }, i32 }, {} } @zil.fn.7(ptr %arg.0, { { i32 }, i32 } %arg.1) {
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
  %v.2 = call { i1, { { i32 }, i32 }, {} } @zil.store.1.push(ptr %v.0, { { i32 }, i32 } %v.1)
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.1 = getelementptr { { i32 }, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.1)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { { i32 }, i32 }, {} } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.8() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.9(ptr %arg.0, i64 %arg.1) {
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

define { i1, { { i32 }, i64 }, {} } @zil.fn.10(ptr %arg.0, { { i32 }, i64 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 true, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, i64 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { { i32 }, i64 } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { { i32 }, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  %v.2 = call { i1, { { i32 }, i64 }, {} } @zil.store.2.push(ptr %v.0, { { i32 }, i64 } %v.1)
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.1 = getelementptr { { i32 }, i64 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.1)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { { i32 }, i64 }, {} } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, {}, { { i32 }, i32 } } @zil.fn.11(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call { i1, {}, { { i32 }, i32 } } @zil.store.1.pop(ptr %v.0)
  ret { i1, {}, { { i32 }, i32 } } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.12({ ptr, i64, i64 } %arg.0) {
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
  call void @zil.store.2.drop(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.13(ptr %arg.0) {
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

define i64 @zil.fn.14(ptr %arg.0) {
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

define i32 @main() {
entry:
  %entry.result = call { i1, {}, i32 } @zil.fn.4()
  %entry.error = extractvalue { i1, {}, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, {}, i32 } %entry.result, 2
  ret i32 %entry.success
}

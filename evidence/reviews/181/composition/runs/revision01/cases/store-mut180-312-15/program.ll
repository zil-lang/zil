; ZIL verified LLVM module
; verified-fingerprint: d26267c39228b50db66a44d56e7161dac2f538e890d31ee1d4a6a753f5eaced2
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
  %stride = ptrtoint ptr getelementptr ({ { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr null, i64 1) to i64
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
define internal { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} } @zil.store.1.push(ptr %head, { { i32 }, [3 x i32], { i32, i32 }, i32 } %item) {
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
  %slot = getelementptr { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %data, i64 %len
  store { { i32 }, [3 x i32], { i32, i32 }, i32 } %item, ptr %slot, align 4
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} } zeroinitializer
full:
  %tag = insertvalue { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} } zeroinitializer, i1 true, 0
  %error = insertvalue { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} } %tag, { { i32 }, [3 x i32], { i32, i32 }, i32 } %item, 1
  ret { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} } %error
}
define internal { i1, {}, { { i32 }, [3 x i32], { i32, i32 }, i32 } } @zil.store.1.pop(ptr %head) {
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
  %slot = getelementptr { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %data, i64 %next.len
  %item = load { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %slot, align 4
  %result = insertvalue { i1, {}, { { i32 }, [3 x i32], { i32, i32 }, i32 } } zeroinitializer, { { i32 }, [3 x i32], { i32, i32 }, i32 } %item, 2
  ret { i1, {}, { { i32 }, [3 x i32], { i32, i32 }, i32 } } %result
empty:
  %error = insertvalue { i1, {}, { { i32 }, [3 x i32], { i32, i32 }, i32 } } zeroinitializer, i1 true, 0
  ret { i1, {}, { { i32 }, [3 x i32], { i32, i32 }, i32 } } %error
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
  %slot = getelementptr { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %data, i64 %index
  %field.0 = getelementptr { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %slot, i32 0, i32 0
  call void @zil.fn.1(ptr %field.0)
  %next = add i64 %index, 1
  br label %loop
release:
  call void @zil.bytes.release(ptr %data)
  ret void
}
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)

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

define { { i32 }, [3 x i32], { i32, i32 }, i32 } @zil.fn.2(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca [3 x i32], align 4
  %local.4 = alloca i32, align 4
  %local.5 = alloca i32, align 4
  %local.6 = alloca { i32, i32 }, align 4
  %local.7 = alloca i32, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { i32 }, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.8, align 1
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = add i32 0, 1
  %v.4 = add i32 0, 2
  %v.5 = add i32 0, 3
%tmp.0 = insertvalue [3 x i32] zeroinitializer, i32 %v.3, 0
%tmp.1 = insertvalue [3 x i32] %tmp.0, i32 %v.4, 1
%v.6 = insertvalue [3 x i32] %tmp.1, i32 %v.5, 2
  store [3 x i32] %v.6, ptr %local.3, align 4
  %v.7 = add i32 0, 4
  store i32 %v.7, ptr %local.4, align 4
  %v.8 = add i32 0, 5
  store i32 %v.8, ptr %local.5, align 4
  %v.9 = load i32, ptr %local.4, align 4
  %v.10 = load i32, ptr %local.5, align 4
%tmp.2 = insertvalue { i32, i32 } zeroinitializer, i32 %v.9, 0
%v.11 = insertvalue { i32, i32 } %tmp.2, i32 %v.10, 1
  store { i32, i32 } %v.11, ptr %local.6, align 4
  %v.12 = load i32, ptr %local.0, align 4
  store i32 %v.12, ptr %local.7, align 4
  %v.13 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.13, ptr %local.8, align 4
  store i1 true, ptr %resource.live.8, align 1
  %v.14 = load [3 x i32], ptr %local.3, align 4
  %v.15 = load { i32, i32 }, ptr %local.6, align 4
  %v.16 = load i32, ptr %local.7, align 4
  %v.17 = load { i32 }, ptr %local.8, align 4
  store i1 false, ptr %resource.live.8, align 1
%tmp.3 = insertvalue { { i32 }, [3 x i32], { i32, i32 }, i32 } zeroinitializer, { i32 } %v.17, 0
%tmp.4 = insertvalue { { i32 }, [3 x i32], { i32, i32 }, i32 } %tmp.3, [3 x i32] %v.14, 1
%tmp.5 = insertvalue { { i32 }, [3 x i32], { i32, i32 }, i32 } %tmp.4, { i32, i32 } %v.15, 2
%v.18 = insertvalue { { i32 }, [3 x i32], { i32, i32 }, i32 } %tmp.5, i32 %v.16, 3
  %tmp.6 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.6, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.1(ptr %local.8)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.7 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.7, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, [3 x i32], { i32, i32 }, i32 } %v.18
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.3() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.4(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %v.0, i32 0, i32 3
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %v.0, i32 0, i32 3
  %v.2 = load ptr, ptr %local.0, align 8
  %v.3 = getelementptr { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %v.2, i32 0, i32 3
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = load i32, ptr %v.4, align 4
  %v.6 = load i32, ptr %local.1, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.7, ptr %v.1, align 4
  %v.8 = load ptr, ptr %local.0, align 8
  %v.9 = getelementptr { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %v.8, i32 0, i32 3
  %v.10 = getelementptr i8, ptr %v.9, i64 0
  %v.11 = load i32, ptr %v.10, align 4
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.6() {
entry:
  %enum.slot.0 = alloca { i32, [0 x i8] }, align 4
  br label %bb.0
bb.0:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.0, align 4
  %tmp.0 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.0, i32 0, i32 0
  store i32 0, ptr %tmp.0, align 4
  %tmp.1 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.0, i32 0, i32 1
  %v.0 = load { i32, [0 x i8] }, ptr %enum.slot.0, align 4
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.1 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.2, { i32, [0 x i8] } %v.0, 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, {} } @zil.fn.7(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %v.0, i32 0, i32 3
  %v.2 = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.6()
  %v.3 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.2, 0
  br i1 %v.3, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.2, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.4 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.2, 1
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, {} } zeroinitializer, i1 true, 0
  %v.5 = insertvalue { i1, { i32, [0 x i8] }, {} } %tmp.2, { i32, [0 x i8] } %v.4, 1
  ret { i1, { i32, [0 x i8] }, {} } %v.5
bb.2:
  %tmp.3 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.2, 0
  %tmp.4 = icmp ne i1 %tmp.3, false
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.6 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.2, 2
  store i32 %v.6, ptr %v.1, align 4
  %tmp.5 = insertvalue { i1, { i32, [0 x i8] }, {} } zeroinitializer, i1 false, 0
  %v.8 = insertvalue { i1, { i32, [0 x i8] }, {} } %tmp.5, {} zeroinitializer, 2
  ret { i1, { i32, [0 x i8] }, {} } %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.8() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} }, align 4
  %local.3 = alloca ptr, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %v.0 = call { ptr, i64, i64 } @zil.fn.3()
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = add i64 0, 8
  %v.3 = call i1 @zil.store.1.reserve(ptr %v.1, i64 %v.2)
  %v.4 = getelementptr i8, ptr %local.0, i64 0
  %v.5 = add i32 0, 65
  %v.6 = call { { i32 }, [3 x i32], { i32, i32 }, i32 } @zil.fn.2(i32 %v.5)
  %v.7 = call { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} } @zil.store.1.push(ptr %v.4, { { i32 }, [3 x i32], { i32, i32 }, i32 } %v.6)
  store { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} } %v.7, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %error.drop.0, label %error.done.0
error.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.1 = getelementptr { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} }, ptr %local.1, i32 0, i32 0
  %tmp.2 = load i1, ptr %tmp.1, align 1
  br i1 %tmp.2, label %error.payload.0, label %success.payload.0
error.payload.0:
  %tmp.3 = getelementptr { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} }, ptr %local.1, i32 0, i32 1
  %tmp.4 = getelementptr { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %tmp.3, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.4)
  br label %error.done.0
success.payload.0:
  br label %error.done.0
error.done.0:
  %v.8 = getelementptr i8, ptr %local.0, i64 0
  %v.9 = add i32 0, 66
  %v.10 = call { { i32 }, [3 x i32], { i32, i32 }, i32 } @zil.fn.2(i32 %v.9)
  %v.11 = call { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} } @zil.store.1.push(ptr %v.8, { { i32 }, [3 x i32], { i32, i32 }, i32 } %v.10)
  store { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} } %v.11, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %tmp.5 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.5, label %error.drop.1, label %error.done.1
error.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.6 = getelementptr { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} }, ptr %local.2, i32 0, i32 0
  %tmp.7 = load i1, ptr %tmp.6, align 1
  br i1 %tmp.7, label %error.payload.1, label %success.payload.1
error.payload.1:
  %tmp.8 = getelementptr { i1, { { i32 }, [3 x i32], { i32, i32 }, i32 }, {} }, ptr %local.2, i32 0, i32 1
  %tmp.9 = getelementptr { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %tmp.8, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.9)
  br label %error.done.1
success.payload.1:
  br label %error.done.1
error.done.1:
  %v.12 = getelementptr i8, ptr %local.0, i64 0
  %v.13 = add i64 0, 0
  %tmp.10 = getelementptr { ptr, i64, i64 }, ptr %v.12, i32 0, i32 1
  %tmp.11 = load i64, ptr %tmp.10, align 8
  %tmp.12 = icmp ult i64 %v.13, %tmp.11
  br i1 %tmp.12, label %store.element.ok.2, label %zil.trap
store.element.ok.2:
  %tmp.13 = getelementptr { ptr, i64, i64 }, ptr %v.12, i32 0, i32 0
  %tmp.14 = load ptr, ptr %tmp.13, align 8
  %v.14 = getelementptr { { i32 }, [3 x i32], { i32, i32 }, i32 }, ptr %tmp.14, i64 %v.13
  store ptr %v.14, ptr %local.3, align 8
  %v.15 = load ptr, ptr %local.3, align 8
  %v.16 = getelementptr i8, ptr %v.15, i64 0
  %v.17 = call { i1, { i32, [0 x i8] }, {} } @zil.fn.7(ptr %v.16)
  %v.18 = extractvalue { i1, { i32, [0 x i8] }, {} } %v.17, 0
  br i1 %v.18, label %bb.1, label %bb.2
bb.1:
  %tmp.15 = extractvalue { i1, { i32, [0 x i8] }, {} } %v.17, 0
  %tmp.16 = icmp ne i1 %tmp.15, true
  br i1 %tmp.16, label %zil.trap, label %checked.3
checked.3:
  %v.19 = extractvalue { i1, { i32, [0 x i8] }, {} } %v.17, 1
  %tmp.17 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.20 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.17, { i32, [0 x i8] } %v.19, 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.18 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.18, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.20
bb.2:
  %tmp.19 = extractvalue { i1, { i32, [0 x i8] }, {} } %v.17, 0
  %tmp.20 = icmp ne i1 %tmp.19, false
  br i1 %tmp.20, label %zil.trap, label %checked.5
checked.5:
  %v.22 = add i32 0, 90
  %tmp.21 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.23 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.21, i32 %v.22, 2
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.22 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.22, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.23
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.8()
  %entry.error = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 2
  ret i32 %entry.success
}

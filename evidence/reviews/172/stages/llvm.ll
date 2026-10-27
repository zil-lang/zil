; ZIL verified LLVM module
; verified-fingerprint: 5987ed24fcf467ff024a85ac7559e2b16d0f505256c2cdb4731650620bd93645
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
  %stride = ptrtoint ptr getelementptr ({ i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr null, i64 1) to i64
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
define internal { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} } @zil.store.1.push(ptr %head, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %item) {
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
  %slot = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %data, i64 %len
  store { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %item, ptr %slot, align 8
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} } zeroinitializer
full:
  %tag = insertvalue { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} } zeroinitializer, i1 true, 0
  %error = insertvalue { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} } %tag, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %item, 1
  ret { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} } %error
}
define internal { i1, {}, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } } @zil.store.1.pop(ptr %head) {
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
  %slot = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %data, i64 %next.len
  %item = load { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %slot, align 8
  %result = insertvalue { i1, {}, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } } zeroinitializer, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %item, 2
  ret { i1, {}, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } } %result
empty:
  %error = insertvalue { i1, {}, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } } zeroinitializer, i1 true, 0
  ret { i1, {}, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } } %error
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
  %slot = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %data, i64 %index
  %field.1 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %slot, i32 0, i32 1
  call void @zil.fn.1(ptr %field.1)
  %field.5 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %slot, i32 0, i32 5
  call void @zil.fn.1(ptr %field.5)
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

define { i8, { i32 }, i64, i16, [3 x i32], { i32 } } @zil.fn.2(i32 %arg.0, i32 %arg.1, i32 %arg.2) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i8, align 1
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32 }, align 4
  %local.6 = alloca i64, align 8
  %local.7 = alloca i16, align 2
  %local.8 = alloca [3 x i32], align 4
  %local.9 = alloca i32, align 4
  %resource.live.10 = alloca i1, align 1
  store i1 false, ptr %resource.live.10, align 1
  %local.10 = alloca { i32 }, align 4
  %resource.live.11 = alloca i1, align 1
  store i1 false, ptr %resource.live.11, align 1
  %local.11 = alloca { i32 }, align 4
  %resource.live.12 = alloca i1, align 1
  store i1 false, ptr %resource.live.12, align 1
  %local.12 = alloca { i32 }, align 4
  store i32 %arg.0, ptr %local.0, align 4
  store i32 %arg.1, ptr %local.1, align 4
  store i32 %arg.2, ptr %local.2, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.12, align 1
  store i1 false, ptr %resource.live.11, align 1
  %v.0 = add i8 0, 7
  store i8 %v.0, ptr %local.3, align 1
  %v.1 = load i32, ptr %local.0, align 4
  store i32 %v.1, ptr %local.4, align 4
  %v.2 = load i32, ptr %local.4, align 4
%v.3 = insertvalue { i32 } zeroinitializer, i32 %v.2, 0
  store i1 false, ptr %resource.live.5, align 1
  store { i32 } %v.3, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.4 = add i64 0, 4294967297
  store i64 %v.4, ptr %local.6, align 8
  %v.5 = add i16 0, 513
  store i16 %v.5, ptr %local.7, align 2
  %v.6 = load i32, ptr %local.2, align 4
  %v.7 = load i32, ptr %local.2, align 4
  %v.8 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.7, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.10 = load i32, ptr %local.2, align 4
  %v.11 = add i32 0, 2
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.10, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
%tmp.4 = insertvalue [3 x i32] zeroinitializer, i32 %v.6, 0
%tmp.5 = insertvalue [3 x i32] %tmp.4, i32 %v.9, 1
%v.13 = insertvalue [3 x i32] %tmp.5, i32 %v.12, 2
  store [3 x i32] %v.13, ptr %local.8, align 4
  %v.14 = load i32, ptr %local.1, align 4
  store i32 %v.14, ptr %local.9, align 4
  %v.15 = load i32, ptr %local.9, align 4
%v.16 = insertvalue { i32 } zeroinitializer, i32 %v.15, 0
  store i1 false, ptr %resource.live.10, align 1
  store { i32 } %v.16, ptr %local.10, align 4
  store i1 true, ptr %resource.live.10, align 1
  %v.17 = load i8, ptr %local.3, align 1
  %v.18 = load { i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  store { i32 } %v.18, ptr %local.11, align 4
  store i1 true, ptr %resource.live.11, align 1
  %v.19 = load i64, ptr %local.6, align 8
  %v.20 = load i16, ptr %local.7, align 2
  %v.21 = load [3 x i32], ptr %local.8, align 4
  %v.22 = load { i32 }, ptr %local.10, align 4
  store i1 false, ptr %resource.live.10, align 1
  store { i32 } %v.22, ptr %local.12, align 4
  store i1 true, ptr %resource.live.12, align 1
  %v.23 = load { i32 }, ptr %local.11, align 4
  store i1 false, ptr %resource.live.11, align 1
  %v.24 = load { i32 }, ptr %local.12, align 4
  store i1 false, ptr %resource.live.12, align 1
%tmp.6 = insertvalue { i8, { i32 }, i64, i16, [3 x i32], { i32 } } zeroinitializer, i8 %v.17, 0
%tmp.7 = insertvalue { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %tmp.6, { i32 } %v.23, 1
%tmp.8 = insertvalue { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %tmp.7, i64 %v.19, 2
%tmp.9 = insertvalue { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %tmp.8, i16 %v.20, 3
%tmp.10 = insertvalue { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %tmp.9, [3 x i32] %v.21, 4
%v.25 = insertvalue { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %tmp.10, { i32 } %v.24, 5
  %tmp.11 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.11, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.12, align 1
  call void @zil.fn.1(ptr %local.12)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.12, align 1
  %tmp.12 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.12, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.11, align 1
  call void @zil.fn.1(ptr %local.11)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.13 = load i1, ptr %resource.live.10, align 1
  br i1 %tmp.13, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.10, align 1
  call void @zil.fn.1(ptr %local.10)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.10, align 1
  %tmp.14 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.14, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.1(ptr %local.5)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.5, align 1
  ret { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %v.25
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

define { i1, {}, i32 } @zil.fn.4() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} }, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} }, align 8
  %local.4 = alloca i1, align 1
  %local.5 = alloca i1, align 1
  %local.6 = alloca i1, align 1
  %local.7 = alloca i1, align 1
  %local.8 = alloca i1, align 1
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, align 8
  %local.10 = alloca i1, align 1
  %local.11 = alloca i1, align 1
  %local.12 = alloca i1, align 1
  %local.13 = alloca i1, align 1
  %index.slot.75 = alloca [3 x i32], align 4
  %index.slot.82 = alloca [3 x i32], align 4
  %index.slot.90 = alloca [3 x i32], align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %v.0 = call { ptr, i64, i64 } @zil.fn.3()
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = add i64 0, 2
  %v.3 = call i1 @zil.store.1.reserve(ptr %v.1, i64 %v.2)
  %v.4 = xor i1 %v.3, true
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.6 = insertvalue { i1, {}, i32 } %tmp.0, {} zeroinitializer, 1
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, {}, i32 } %v.6
bb.2:
  br label %bb.3
bb.3:
  %v.7 = getelementptr i8, ptr %local.0, i64 0
  %v.8 = add i32 0, 65
  %v.9 = add i32 0, 97
  %v.10 = add i32 0, 10
  %v.11 = call { i8, { i32 }, i64, i16, [3 x i32], { i32 } } @zil.fn.2(i32 %v.8, i32 %v.9, i32 %v.10)
  %v.12 = call { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} } @zil.store.1.push(ptr %v.7, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %v.11)
  store { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} } %v.12, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %error.drop.1, label %error.done.1
error.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.3 = getelementptr { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} }, ptr %local.1, i32 0, i32 0
  %tmp.4 = load i1, ptr %tmp.3, align 1
  br i1 %tmp.4, label %error.payload.1, label %success.payload.1
error.payload.1:
  %tmp.5 = getelementptr { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} }, ptr %local.1, i32 0, i32 1
  %tmp.6 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %tmp.5, i32 0, i32 1
  call void @zil.fn.1(ptr %tmp.6)
  %tmp.7 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %tmp.5, i32 0, i32 5
  call void @zil.fn.1(ptr %tmp.7)
  br label %error.done.1
success.payload.1:
  br label %error.done.1
error.done.1:
  %v.13 = getelementptr i8, ptr %local.0, i64 0
  %v.14 = add i32 0, 66
  %v.15 = add i32 0, 98
  %v.16 = add i32 0, 30
  %v.17 = call { i8, { i32 }, i64, i16, [3 x i32], { i32 } } @zil.fn.2(i32 %v.14, i32 %v.15, i32 %v.16)
  %v.18 = call { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} } @zil.store.1.push(ptr %v.13, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %v.17)
  store { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} } %v.18, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %tmp.8 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.8, label %error.drop.2, label %error.done.2
error.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.9 = getelementptr { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} }, ptr %local.2, i32 0, i32 0
  %tmp.10 = load i1, ptr %tmp.9, align 1
  br i1 %tmp.10, label %error.payload.2, label %success.payload.2
error.payload.2:
  %tmp.11 = getelementptr { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} }, ptr %local.2, i32 0, i32 1
  %tmp.12 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %tmp.11, i32 0, i32 1
  call void @zil.fn.1(ptr %tmp.12)
  %tmp.13 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %tmp.11, i32 0, i32 5
  call void @zil.fn.1(ptr %tmp.13)
  br label %error.done.2
success.payload.2:
  br label %error.done.2
error.done.2:
  %v.19 = getelementptr i8, ptr %local.0, i64 0
  %v.20 = add i32 0, 67
  %v.21 = add i32 0, 99
  %v.22 = add i32 0, 40
  %v.23 = call { i8, { i32 }, i64, i16, [3 x i32], { i32 } } @zil.fn.2(i32 %v.20, i32 %v.21, i32 %v.22)
  %v.24 = call { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} } @zil.store.1.push(ptr %v.19, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %v.23)
  store { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} } %v.24, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %tmp.14 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.14, label %error.drop.3, label %error.done.3
error.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.15 = getelementptr { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} }, ptr %local.3, i32 0, i32 0
  %tmp.16 = load i1, ptr %tmp.15, align 1
  br i1 %tmp.16, label %error.payload.3, label %success.payload.3
error.payload.3:
  %tmp.17 = getelementptr { i1, { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, {} }, ptr %local.3, i32 0, i32 1
  %tmp.18 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %tmp.17, i32 0, i32 1
  call void @zil.fn.1(ptr %tmp.18)
  %tmp.19 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %tmp.17, i32 0, i32 5
  call void @zil.fn.1(ptr %tmp.19)
  br label %error.done.3
success.payload.3:
  br label %error.done.3
error.done.3:
  %v.25 = getelementptr i8, ptr %local.0, i64 0
  %v.26 = add i64 0, 8
  %v.27 = call i1 @zil.store.1.reserve(ptr %v.25, i64 %v.26)
  store i1 %v.27, ptr %local.4, align 1
  %v.28 = getelementptr i8, ptr %local.0, i64 0
  %tmp.20 = getelementptr { ptr, i64, i64 }, ptr %v.28, i32 0, i32 1
  %v.29 = load i64, ptr %tmp.20, align 8
  %v.30 = add i64 0, 2
  %v.31 = icmp ne i64 %v.29, %v.30
  br i1 %v.31, label %bb.4, label %bb.5
bb.4:
  %v.32 = or i1 false, true
  store i1 %v.32, ptr %local.5, align 1
  br label %bb.6
bb.5:
  %v.33 = load i1, ptr %local.4, align 1
  br i1 %v.33, label %bb.8, label %bb.7
bb.6:
  %v.40 = load i1, ptr %local.5, align 1
  br i1 %v.40, label %bb.10, label %bb.11
bb.7:
  %v.34 = or i1 false, false
  store i1 %v.34, ptr %local.6, align 1
  br label %bb.9
bb.8:
  %v.35 = getelementptr i8, ptr %local.0, i64 0
  %tmp.21 = getelementptr { ptr, i64, i64 }, ptr %v.35, i32 0, i32 2
  %v.36 = load i64, ptr %tmp.21, align 8
  %v.37 = add i64 0, 8
  %v.38 = icmp ne i64 %v.36, %v.37
  store i1 %v.38, ptr %local.6, align 1
  br label %bb.9
bb.9:
  %v.39 = load i1, ptr %local.6, align 1
  store i1 %v.39, ptr %local.5, align 1
  br label %bb.6
bb.10:
  %v.41 = or i1 false, true
  store i1 %v.41, ptr %local.7, align 1
  br label %bb.12
bb.11:
  %v.42 = load i1, ptr %local.4, align 1
  %v.43 = xor i1 %v.42, true
  br i1 %v.43, label %bb.14, label %bb.13
bb.12:
  %v.50 = load i1, ptr %local.7, align 1
  br i1 %v.50, label %bb.16, label %bb.17
bb.13:
  %v.44 = or i1 false, false
  store i1 %v.44, ptr %local.8, align 1
  br label %bb.15
bb.14:
  %v.45 = getelementptr i8, ptr %local.0, i64 0
  %tmp.22 = getelementptr { ptr, i64, i64 }, ptr %v.45, i32 0, i32 2
  %v.46 = load i64, ptr %tmp.22, align 8
  %v.47 = add i64 0, 2
  %v.48 = icmp ne i64 %v.46, %v.47
  store i1 %v.48, ptr %local.8, align 1
  br label %bb.15
bb.15:
  %v.49 = load i1, ptr %local.8, align 1
  store i1 %v.49, ptr %local.7, align 1
  br label %bb.12
bb.16:
  %v.51 = add i32 0, 4
  %tmp.23 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.52 = insertvalue { i1, {}, i32 } %tmp.23, i32 %v.51, 2
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.24 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.24, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.52
bb.17:
  br label %bb.18
bb.18:
  %v.53 = getelementptr i8, ptr %local.0, i64 0
  %v.54 = call { i1, {}, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } } @zil.store.1.pop(ptr %v.53)
  %v.55 = extractvalue { i1, {}, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } } %v.54, 0
  br i1 %v.55, label %bb.19, label %bb.20
bb.19:
  %tmp.25 = extractvalue { i1, {}, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } } %v.54, 0
  %tmp.26 = icmp ne i1 %tmp.25, true
  br i1 %tmp.26, label %zil.trap, label %checked.5
checked.5:
  %tmp.27 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.57 = insertvalue { i1, {}, i32 } %tmp.27, {} zeroinitializer, 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.28 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.28, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.57
bb.20:
  %tmp.29 = extractvalue { i1, {}, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } } %v.54, 0
  %tmp.30 = icmp ne i1 %tmp.29, false
  br i1 %tmp.30, label %zil.trap, label %checked.7
checked.7:
  %v.58 = extractvalue { i1, {}, { i8, { i32 }, i64, i16, [3 x i32], { i32 } } } %v.54, 2
  store i1 false, ptr %resource.live.9, align 1
  store { i8, { i32 }, i64, i16, [3 x i32], { i32 } } %v.58, ptr %local.9, align 8
  store i1 true, ptr %resource.live.9, align 1
  %tmp.31 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %local.9, i32 0, i32 0
  %v.59 = load i8, ptr %tmp.31, align 1
  %v.60 = add i8 0, 7
  %v.61 = icmp ne i8 %v.59, %v.60
  br i1 %v.61, label %bb.21, label %bb.22
bb.21:
  %v.62 = or i1 false, true
  store i1 %v.62, ptr %local.10, align 1
  br label %bb.23
bb.22:
  %tmp.32 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %local.9, i32 0, i32 2
  %v.63 = load i64, ptr %tmp.32, align 8
  %v.64 = add i64 0, 4294967297
  %v.65 = icmp ne i64 %v.63, %v.64
  store i1 %v.65, ptr %local.10, align 1
  br label %bb.23
bb.23:
  %v.66 = load i1, ptr %local.10, align 1
  br i1 %v.66, label %bb.24, label %bb.25
bb.24:
  %v.67 = or i1 false, true
  store i1 %v.67, ptr %local.11, align 1
  br label %bb.26
bb.25:
  %tmp.33 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %local.9, i32 0, i32 3
  %v.68 = load i16, ptr %tmp.33, align 2
  %v.69 = add i16 0, 513
  %v.70 = icmp ne i16 %v.68, %v.69
  store i1 %v.70, ptr %local.11, align 1
  br label %bb.26
bb.26:
  %v.71 = load i1, ptr %local.11, align 1
  br i1 %v.71, label %bb.27, label %bb.28
bb.27:
  %v.72 = or i1 false, true
  store i1 %v.72, ptr %local.12, align 1
  br label %bb.29
bb.28:
  %tmp.34 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %local.9, i32 0, i32 4
  %v.73 = load [3 x i32], ptr %tmp.34, align 4
  %v.74 = add i32 0, 0
  %tmp.35 = sext i32 %v.74 to i64
  %tmp.36 = icmp ult i64 %tmp.35, 3
  br i1 %tmp.36, label %index.ok.8, label %zil.trap
index.ok.8:
  store [3 x i32] %v.73, ptr %index.slot.75, align 4
  %tmp.37 = getelementptr [3 x i32], ptr %index.slot.75, i64 0, i64 %tmp.35
  %v.75 = load i32, ptr %tmp.37, align 4
  %v.76 = add i32 0, 30
  %v.77 = icmp ne i32 %v.75, %v.76
  store i1 %v.77, ptr %local.12, align 1
  br label %bb.29
bb.29:
  %v.78 = load i1, ptr %local.12, align 1
  br i1 %v.78, label %bb.30, label %bb.31
bb.30:
  %v.79 = or i1 false, true
  store i1 %v.79, ptr %local.13, align 1
  br label %bb.32
bb.31:
  %tmp.38 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %local.9, i32 0, i32 4
  %v.80 = load [3 x i32], ptr %tmp.38, align 4
  %v.81 = add i32 0, 1
  %tmp.39 = sext i32 %v.81 to i64
  %tmp.40 = icmp ult i64 %tmp.39, 3
  br i1 %tmp.40, label %index.ok.9, label %zil.trap
index.ok.9:
  store [3 x i32] %v.80, ptr %index.slot.82, align 4
  %tmp.41 = getelementptr [3 x i32], ptr %index.slot.82, i64 0, i64 %tmp.39
  %v.82 = load i32, ptr %tmp.41, align 4
  %v.83 = add i32 0, 31
  %v.84 = icmp ne i32 %v.82, %v.83
  store i1 %v.84, ptr %local.13, align 1
  br label %bb.32
bb.32:
  %v.85 = load i1, ptr %local.13, align 1
  br i1 %v.85, label %bb.33, label %bb.34
bb.33:
  %v.86 = add i32 0, 5
  %tmp.42 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.87 = insertvalue { i1, {}, i32 } %tmp.42, i32 %v.86, 2
  %tmp.43 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.43, label %package.drop.10, label %package.done.10
package.drop.10:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.44 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %local.9, i32 0, i32 1
  call void @zil.fn.1(ptr %tmp.44)
  %tmp.45 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %local.9, i32 0, i32 5
  call void @zil.fn.1(ptr %tmp.45)
  br label %package.done.10
package.done.10:
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.46 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.46, label %resource.drop.11, label %resource.done.11
resource.drop.11:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.11
resource.done.11:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.87
bb.34:
  br label %bb.35
bb.35:
  %tmp.47 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %local.9, i32 0, i32 4
  %v.88 = load [3 x i32], ptr %tmp.47, align 4
  %v.89 = add i32 0, 2
  %tmp.48 = sext i32 %v.89 to i64
  %tmp.49 = icmp ult i64 %tmp.48, 3
  br i1 %tmp.49, label %index.ok.12, label %zil.trap
index.ok.12:
  store [3 x i32] %v.88, ptr %index.slot.90, align 4
  %tmp.50 = getelementptr [3 x i32], ptr %index.slot.90, i64 0, i64 %tmp.48
  %v.90 = load i32, ptr %tmp.50, align 4
  %v.91 = add i32 0, 32
  %tmp.51 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.90, i32 %v.91)
  %v.92 = extractvalue { i32, i1 } %tmp.51, 0
  %tmp.52 = extractvalue { i32, i1 } %tmp.51, 1
  br i1 %tmp.52, label %zil.trap, label %checked.13
checked.13:
  %tmp.53 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.93 = insertvalue { i1, {}, i32 } %tmp.53, i32 %v.92, 2
  %tmp.54 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.54, label %package.drop.14, label %package.done.14
package.drop.14:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.55 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %local.9, i32 0, i32 1
  call void @zil.fn.1(ptr %tmp.55)
  %tmp.56 = getelementptr { i8, { i32 }, i64, i16, [3 x i32], { i32 } }, ptr %local.9, i32 0, i32 5
  call void @zil.fn.1(ptr %tmp.56)
  br label %package.done.14
package.done.14:
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.57 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.57, label %resource.drop.15, label %resource.done.15
resource.drop.15:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.15
resource.done.15:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.93
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

; ZIL verified LLVM module
; verified-fingerprint: a7aabb07062b964c2a27ae2ef0acf519d1ee63d9d10f8691214f281466b585c4
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

define internal i1 @zil.store.2.reserve(ptr %head, i64 %requested) {
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
define internal { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.store.2.push(ptr %head, { { i32 }, { ptr, i64, i64 }, i32 } %item) {
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
define internal { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.store.2.pop(ptr %head) {
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
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %data, i64 %index
  %field.0 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %slot, i32 0, i32 0
  call void @zil.fn.2(ptr %field.0)
  %field.1 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %slot, i32 0, i32 1
  call void @zil.bytes.drop(ptr %field.1)
  %next = add i64 %index, 1
  br label %loop
release:
  call void @zil.bytes.release(ptr %data)
  ret void
}

define { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } @zil.fn.0() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  br label %bb.0
bb.0:
  %v.0 = call { ptr, i64, i64 } @zil.fn.6()
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = add i32 0, 90
  %v.3 = call { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.5(i32 %v.2)
  %v.4 = call { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.store.2.push(ptr %v.1, { { i32 }, { ptr, i64, i64 }, i32 } %v.3)
  %v.5 = extractvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %v.4, 0
  br i1 %v.5, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %v.4, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.6 = extractvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %v.4, 1
  %tmp.2 = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } zeroinitializer, i1 true, 0
  %v.7 = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %tmp.2, { { i32 }, { ptr, i64, i64 }, i32 } %v.6, 1
  %tmp.3 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.2.drop(ptr %local.0)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %v.7
bb.2:
  %tmp.4 = extractvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %v.4, 0
  %tmp.5 = icmp ne i1 %tmp.4, false
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.9 = add i32 0, 0
  %tmp.6 = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } zeroinitializer, i1 false, 0
  %v.10 = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %tmp.6, i32 %v.9, 2
  %tmp.7 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.7, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.2.drop(ptr %local.0)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.2(ptr %arg.0) {
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

define { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.3(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  %local.4 = alloca i1, align 1
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32 }, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { ptr, i64, i64 }, align 8
  %local.7 = alloca i32, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { i32 }, align 4
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { ptr, i64, i64 }, align 8
  %enum.slot.15 = alloca { i32, [0 x i8] }, align 4
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
  %v.3 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.3, align 1
  store { ptr, i64, i64 } %v.3, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.4 = getelementptr i8, ptr %local.3, i64 0
  %v.5 = add i64 0, 1
  %v.6 = call i1 @zil.bytes.reserve(ptr %v.4, i64 %v.5)
  %v.7 = xor i1 %v.6, true
  br i1 %v.7, label %bb.1, label %bb.2
bb.1:
  %v.8 = or i1 false, true
  store i1 %v.8, ptr %local.4, align 1
  br label %bb.3
bb.2:
  %v.9 = getelementptr i8, ptr %local.3, i64 0
  %v.10 = load i32, ptr %local.0, align 4
  %tmp.0 = icmp ugt i32 %v.10, 255
  br i1 %tmp.0, label %zil.trap, label %checked.0
checked.0:
  %v.11 = trunc i32 %v.10 to i8
  %v.12 = call i1 @zil.bytes.push(ptr %v.9, i8 %v.11)
  %v.13 = xor i1 %v.12, true
  store i1 %v.13, ptr %local.4, align 1
  br label %bb.3
bb.3:
  %v.14 = load i1, ptr %local.4, align 1
  br i1 %v.14, label %bb.4, label %bb.5
bb.4:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.15, align 4
  %tmp.1 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.15, i32 0, i32 0
  store i32 0, ptr %tmp.1, align 4
  %tmp.2 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.15, i32 0, i32 1
  %v.15 = load { i32, [0 x i8] }, ptr %enum.slot.15, align 4
  %tmp.3 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } zeroinitializer, i1 true, 0
  %v.16 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %tmp.3, { i32, [0 x i8] } %v.15, 1
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.bytes.drop(ptr %local.3)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.5 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.5, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.2(ptr %local.2)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.16
bb.5:
  br label %bb.6
bb.6:
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  %v.17 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.5, align 1
  store { i32 } %v.17, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.18 = load { ptr, i64, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.6, align 1
  store { ptr, i64, i64 } %v.18, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  %v.19 = load i32, ptr %local.0, align 4
  store i32 %v.19, ptr %local.7, align 4
  %v.20 = load { i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  store { i32 } %v.20, ptr %local.8, align 4
  store i1 true, ptr %resource.live.8, align 1
  %v.21 = load { ptr, i64, i64 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store { ptr, i64, i64 } %v.21, ptr %local.9, align 8
  store i1 true, ptr %resource.live.9, align 1
  %v.22 = load i32, ptr %local.7, align 4
  %v.23 = load { i32 }, ptr %local.8, align 4
  store i1 false, ptr %resource.live.8, align 1
  %v.24 = load { ptr, i64, i64 }, ptr %local.9, align 8
  store i1 false, ptr %resource.live.9, align 1
%tmp.6 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } zeroinitializer, { i32 } %v.23, 0
%tmp.7 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.6, { ptr, i64, i64 } %v.24, 1
%v.25 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.7, i32 %v.22, 2
  %tmp.8 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } zeroinitializer, i1 false, 0
  %v.26 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %tmp.8, { { i32 }, { ptr, i64, i64 }, i32 } %v.25, 2
  %tmp.9 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.9, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.bytes.drop(ptr %local.9)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.10 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.10, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.fn.2(ptr %local.8)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.11 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.11, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.bytes.drop(ptr %local.6)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.12 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.12, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.2(ptr %local.5)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.13 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.13, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.bytes.drop(ptr %local.3)
  br label %resource.done.7
resource.done.7:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.14 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.14, label %resource.drop.8, label %resource.done.8
resource.drop.8:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.2(ptr %local.2)
  br label %resource.done.8
resource.done.8:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.26
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, {}, i32 } @zil.fn.4({ { i32 }, { ptr, i64, i64 }, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %local.1 = alloca { ptr, i64 }, align 8
  store { { i32 }, { ptr, i64, i64 }, i32 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 1
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.0, i32 0, i32 1
  %v.1 = load i64, ptr %tmp.0, align 8
  %v.2 = add i64 0, 1
  %v.3 = icmp ne i64 %v.1, %v.2
  br i1 %v.3, label %bb.1, label %bb.2
bb.1:
  %tmp.1 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.5 = insertvalue { i1, {}, i32 } %tmp.1, {} zeroinitializer, 1
  %tmp.2 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.2, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.3 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.3)
  %tmp.4 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.4)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.5
bb.2:
  br label %bb.3
bb.3:
  %v.6 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 1
  %tmp.5 = load ptr, ptr %v.6, align 8
  %tmp.6 = getelementptr { ptr, i64, i64 }, ptr %v.6, i32 0, i32 1
  %tmp.7 = load i64, ptr %tmp.6, align 8
  %tmp.8 = insertvalue { ptr, i64 } poison, ptr %tmp.5, 0
  %v.7 = insertvalue { ptr, i64 } %tmp.8, i64 %tmp.7, 1
  store { ptr, i64 } %v.7, ptr %local.1, align 8
  %v.8 = load { ptr, i64 }, ptr %local.1, align 8
  %v.9 = add i32 0, 0
  %tmp.9 = extractvalue { ptr, i64 } %v.8, 0
  %tmp.10 = extractvalue { ptr, i64 } %v.8, 1
  %tmp.11 = sext i32 %v.9 to i64
  %tmp.12 = icmp uge i64 %tmp.11, %tmp.10
  br i1 %tmp.12, label %zil.trap, label %checked.1
checked.1:
  %v.10 = getelementptr i8, ptr %tmp.9, i64 %tmp.11
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = load i8, ptr %v.11, align 1
  %tmp.13 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 2
  %v.13 = load i32, ptr %tmp.13, align 4
  %tmp.14 = icmp ugt i32 %v.13, 255
  br i1 %tmp.14, label %zil.trap, label %checked.2
checked.2:
  %v.14 = trunc i32 %v.13 to i8
  %v.15 = icmp ne i8 %v.12, %v.14
  br i1 %v.15, label %bb.4, label %bb.5
bb.4:
  %tmp.15 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.17 = insertvalue { i1, {}, i32 } %tmp.15, {} zeroinitializer, 1
  %tmp.16 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.16, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.17 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.17)
  %tmp.18 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.18)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.17
bb.5:
  br label %bb.6
bb.6:
  %tmp.19 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 2
  %v.18 = load i32, ptr %tmp.19, align 4
  %tmp.20 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.19 = insertvalue { i1, {}, i32 } %tmp.20, i32 %v.18, 2
  %tmp.21 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.21, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.22 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.22)
  %tmp.23 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.23)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.5(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { ptr, i64, i64 }, align 8
  %local.6 = alloca i32, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { i32 }, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { ptr, i64, i64 }, align 8
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
  %v.3 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.3, align 1
  store { ptr, i64, i64 } %v.3, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.4 = getelementptr i8, ptr %local.3, i64 0
  %v.5 = add i64 0, 1
  %v.6 = call i1 @zil.bytes.reserve(ptr %v.4, i64 %v.5)
  %v.7 = getelementptr i8, ptr %local.3, i64 0
  %v.8 = load i32, ptr %local.0, align 4
  %tmp.0 = icmp ugt i32 %v.8, 255
  br i1 %tmp.0, label %zil.trap, label %checked.0
checked.0:
  %v.9 = trunc i32 %v.8 to i8
  %v.10 = call i1 @zil.bytes.push(ptr %v.7, i8 %v.9)
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.7, align 1
  %v.11 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.4, align 1
  store { i32 } %v.11, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.12 = load { ptr, i64, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.12, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  %v.13 = load i32, ptr %local.0, align 4
  store i32 %v.13, ptr %local.6, align 4
  %v.14 = load { i32 }, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  store { i32 } %v.14, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.15 = load { ptr, i64, i64 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  store { ptr, i64, i64 } %v.15, ptr %local.8, align 8
  store i1 true, ptr %resource.live.8, align 1
  %v.16 = load i32, ptr %local.6, align 4
  %v.17 = load { i32 }, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
  %v.18 = load { ptr, i64, i64 }, ptr %local.8, align 8
  store i1 false, ptr %resource.live.8, align 1
%tmp.1 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } zeroinitializer, { i32 } %v.17, 0
%tmp.2 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.1, { ptr, i64, i64 } %v.18, 1
%v.19 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.2, i32 %v.16, 2
  %tmp.3 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.bytes.drop(ptr %local.8)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.4 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.fn.2(ptr %local.7)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.5 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.bytes.drop(ptr %local.5)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.6 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.6, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.2(ptr %local.4)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.7 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.7, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.bytes.drop(ptr %local.3)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.8 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.8, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.2(ptr %local.2)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, { ptr, i64, i64 }, i32 } %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.6() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.7({ ptr, i64, i64 } %arg.0) {
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

define { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.fn.8(ptr %arg.0, { { i32 }, { ptr, i64, i64 }, i32 } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 true, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  store { { i32 }, { ptr, i64, i64 }, i32 } %arg.1, ptr %local.1, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  %v.2 = call { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.store.2.push(ptr %v.0, { { i32 }, { ptr, i64, i64 }, i32 } %v.1)
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.1 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.1)
  %tmp.2 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.2)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.9(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.store.2.pop(ptr %v.0)
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } @zil.fn.0()
  %entry.error = extractvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  %entry.error.value = extractvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %entry.result, 1
  %entry.error.storage = alloca { { i32 }, { ptr, i64, i64 }, i32 }
  store { { i32 }, { ptr, i64, i64 }, i32 } %entry.error.value, ptr %entry.error.storage
  %entry.error.field.0 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %entry.error.storage, i32 0, i32 0
  call void @zil.fn.2(ptr %entry.error.field.0)
  %entry.error.field.1 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %entry.error.storage, i32 0, i32 1
  call void @zil.bytes.drop(ptr %entry.error.field.1)
  ret i32 1
success:
  %entry.success = extractvalue { i1, { { i32 }, { ptr, i64, i64 }, i32 }, i32 } %entry.result, 2
  ret i32 %entry.success
}

; ZIL verified LLVM module
; verified-fingerprint: 2558837997903eaff19ae31c232b64da244908d33387bb497f1d860396ebfe3c
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
  call void @zil.fn.3(ptr %field.0)
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
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @getchar() nonlazybind
define { i1, {}, i32 } @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { ptr, i64, i64 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %local.5 = alloca i32, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = call i32 @getchar()
  %v.2 = add i32 0, 48
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.1, i32 %v.2)
  %v.3 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.3, ptr %local.0, align 4
  %v.4 = load i32, ptr %local.0, align 4
  %v.5 = add i32 0, 0
  %v.6 = icmp slt i32 %v.4, %v.5
  br i1 %v.6, label %bb.1, label %bb.2
bb.1:
  %v.7 = or i1 false, true
  store i1 %v.7, ptr %local.1, align 1
  br label %bb.3
bb.2:
  %v.8 = load i32, ptr %local.0, align 4
  %v.9 = add i32 0, 9
  %v.10 = icmp sgt i32 %v.8, %v.9
  store i1 %v.10, ptr %local.1, align 1
  br label %bb.3
bb.3:
  %v.11 = load i1, ptr %local.1, align 1
  br i1 %v.11, label %bb.4, label %bb.5
bb.4:
  %tmp.2 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.13 = insertvalue { i1, {}, i32 } %tmp.2, {} zeroinitializer, 1
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, {}, i32 } %v.13
bb.5:
  br label %bb.6
bb.6:
  %v.14 = call { ptr, i64, i64 } @zil.fn.7()
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.14, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.15 = getelementptr i8, ptr %local.2, i64 0
  %v.16 = load i32, ptr %local.0, align 4
  %v.17 = call { i1, { i32, [0 x i8] }, {} } @zil.fn.11(ptr %v.15, i32 %v.16)
  %v.18 = getelementptr i8, ptr %local.2, i64 0
  %tmp.3 = getelementptr { ptr, i64, i64 }, ptr %v.18, i32 0, i32 1
  %v.19 = load i64, ptr %tmp.3, align 8
  %v.20 = load i32, ptr %local.0, align 4
  %v.21 = sext i32 %v.20 to i64
  %v.22 = icmp ne i64 %v.19, %v.21
  br i1 %v.22, label %bb.7, label %bb.8
bb.7:
  %tmp.4 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.24 = insertvalue { i1, {}, i32 } %tmp.4, {} zeroinitializer, 1
  %tmp.5 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.5, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.2.drop(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, {}, i32 } %v.24
bb.8:
  br label %bb.9
bb.9:
  %v.25 = load { ptr, i64, i64 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.25, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.26 = load { ptr, i64, i64 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.27 = call { ptr, i64, i64 } @zil.fn.8({ ptr, i64, i64 } %v.26)
  %tmp.6 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.6, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.store.2.drop(ptr %local.3)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.27, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.28 = getelementptr i8, ptr %local.4, i64 0
  %v.29 = call { i1, {}, i32 } @zil.fn.12(ptr %v.28)
  %v.30 = extractvalue { i1, {}, i32 } %v.29, 0
  br i1 %v.30, label %bb.10, label %bb.11
bb.10:
  %tmp.7 = extractvalue { i1, {}, i32 } %v.29, 0
  %tmp.8 = icmp ne i1 %tmp.7, true
  br i1 %tmp.8, label %zil.trap, label %checked.3
checked.3:
  %tmp.9 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.32 = insertvalue { i1, {}, i32 } %tmp.9, {} zeroinitializer, 1
  %tmp.10 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.10, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.store.2.drop(ptr %local.4)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %tmp.11 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.11, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.2.drop(ptr %local.2)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, {}, i32 } %v.32
bb.11:
  %tmp.12 = extractvalue { i1, {}, i32 } %v.29, 0
  %tmp.13 = icmp ne i1 %tmp.12, false
  br i1 %tmp.13, label %zil.trap, label %checked.6
checked.6:
  %v.33 = extractvalue { i1, {}, i32 } %v.29, 2
  store i32 %v.33, ptr %local.5, align 4
  %v.34 = load i32, ptr %local.5, align 4
  %v.35 = load i32, ptr %local.0, align 4
  %v.36 = add i32 0, 65
  %tmp.14 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.35, i32 %v.36)
  %v.37 = extractvalue { i32, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i32, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.7
checked.7:
  %v.38 = load i32, ptr %local.0, align 4
  %v.39 = load i32, ptr %local.0, align 4
  %v.40 = add i32 0, 1
  %tmp.16 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.39, i32 %v.40)
  %v.41 = extractvalue { i32, i1 } %tmp.16, 0
  %tmp.17 = extractvalue { i32, i1 } %tmp.16, 1
  br i1 %tmp.17, label %zil.trap, label %checked.8
checked.8:
  %tmp.18 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.38, i32 %v.41)
  %v.42 = extractvalue { i32, i1 } %tmp.18, 0
  %tmp.19 = extractvalue { i32, i1 } %tmp.18, 1
  br i1 %tmp.19, label %zil.trap, label %checked.9
checked.9:
  %v.43 = add i32 0, 2
  %tmp.20 = icmp eq i32 %v.43, 0
  %tmp.21 = icmp eq i32 %v.42, -2147483648
  %tmp.22 = icmp eq i32 %v.43, -1
  %tmp.23 = and i1 %tmp.21, %tmp.22
  %tmp.24 = or i1 %tmp.20, %tmp.23
  br i1 %tmp.24, label %zil.trap, label %checked.10
checked.10:
  %v.44 = sdiv i32 %v.42, %v.43
  %tmp.25 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.37, i32 %v.44)
  %v.45 = extractvalue { i32, i1 } %tmp.25, 0
  %tmp.26 = extractvalue { i32, i1 } %tmp.25, 1
  br i1 %tmp.26, label %zil.trap, label %checked.11
checked.11:
  %tmp.27 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.34, i32 %v.45)
  %v.46 = extractvalue { i32, i1 } %tmp.27, 0
  %tmp.28 = extractvalue { i32, i1 } %tmp.27, 1
  br i1 %tmp.28, label %zil.trap, label %checked.12
checked.12:
  %tmp.29 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.47 = insertvalue { i1, {}, i32 } %tmp.29, i32 %v.46, 2
  %tmp.30 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.30, label %resource.drop.13, label %resource.done.13
resource.drop.13:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.store.2.drop(ptr %local.4)
  br label %resource.done.13
resource.done.13:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %tmp.31 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.31, label %resource.drop.14, label %resource.done.14
resource.drop.14:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.2.drop(ptr %local.2)
  br label %resource.done.14
resource.done.14:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, {}, i32 } %v.47
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

define { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.4(i32 %arg.0) {
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
  call void @zil.fn.3(ptr %local.2)
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
  call void @zil.fn.3(ptr %local.8)
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
  call void @zil.fn.3(ptr %local.5)
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
  call void @zil.fn.3(ptr %local.2)
  br label %resource.done.8
resource.done.8:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.26
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, {}, i32 } @zil.fn.5({ { i32 }, { ptr, i64, i64 }, i32 } %arg.0) {
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
  call void @zil.fn.3(ptr %tmp.3)
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
  call void @zil.fn.3(ptr %tmp.17)
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
  call void @zil.fn.3(ptr %tmp.22)
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
  call void @zil.fn.3(ptr %local.7)
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
  call void @zil.fn.3(ptr %local.4)
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
  call void @zil.fn.3(ptr %local.2)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, { ptr, i64, i64 }, i32 } %v.19
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

define { ptr, i64, i64 } @zil.fn.8({ ptr, i64, i64 } %arg.0) {
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

define { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.fn.9(ptr %arg.0, { { i32 }, { ptr, i64, i64 }, i32 } %arg.1) {
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
  call void @zil.fn.3(ptr %tmp.1)
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

define { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.10(ptr %arg.0) {
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

define { i1, { i32, [0 x i8] }, {} } @zil.fn.11(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} }, align 8
  %enum.slot.4 = alloca { i32, [0 x i8] }, align 4
  %enum.slot.22 = alloca { i32, [0 x i8] }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = add i64 0, 2
  %v.2 = call i1 @zil.store.2.reserve(ptr %v.0, i64 %v.1)
  %v.3 = xor i1 %v.2, true
  br i1 %v.3, label %bb.1, label %bb.2
bb.1:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.4, align 4
  %tmp.0 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.4, i32 0, i32 0
  store i32 0, ptr %tmp.0, align 4
  %tmp.1 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.4, i32 0, i32 1
  %v.4 = load { i32, [0 x i8] }, ptr %enum.slot.4, align 4
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, {} } zeroinitializer, i1 true, 0
  %v.5 = insertvalue { i1, { i32, [0 x i8] }, {} } %tmp.2, { i32, [0 x i8] } %v.4, 1
  ret { i1, { i32, [0 x i8] }, {} } %v.5
bb.2:
  br label %bb.3
bb.3:
  %v.6 = add i32 0, 0
  store i32 %v.6, ptr %local.2, align 4
  br label %bb.4
bb.4:
  %v.7 = load i32, ptr %local.2, align 4
  %v.8 = load i32, ptr %local.1, align 4
  %v.9 = icmp slt i32 %v.7, %v.8
  br i1 %v.9, label %bb.5, label %bb.6
bb.5:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %v.10 = load ptr, ptr %local.0, align 8
  %tmp.3 = getelementptr { ptr, i64, i64 }, ptr %v.10, i32 0, i32 1
  %v.11 = load i64, ptr %tmp.3, align 8
  %v.12 = load ptr, ptr %local.0, align 8
  %tmp.4 = getelementptr { ptr, i64, i64 }, ptr %v.12, i32 0, i32 2
  %v.13 = load i64, ptr %tmp.4, align 8
  %v.14 = icmp eq i64 %v.11, %v.13
  br i1 %v.14, label %bb.7, label %bb.8
bb.6:
  %tmp.5 = insertvalue { i1, { i32, [0 x i8] }, {} } zeroinitializer, i1 false, 0
  %v.39 = insertvalue { i1, { i32, [0 x i8] }, {} } %tmp.5, {} zeroinitializer, 2
  ret { i1, { i32, [0 x i8] }, {} } %v.39
bb.7:
  %v.15 = load ptr, ptr %local.0, align 8
  %v.16 = load ptr, ptr %local.0, align 8
  %tmp.6 = getelementptr { ptr, i64, i64 }, ptr %v.16, i32 0, i32 2
  %v.17 = load i64, ptr %tmp.6, align 8
  %v.18 = add i64 0, 2
  %tmp.7 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.17, i64 %v.18)
  %v.19 = extractvalue { i64, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i64, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.0
checked.0:
  %v.20 = call i1 @zil.store.2.reserve(ptr %v.15, i64 %v.19)
  %v.21 = xor i1 %v.20, true
  br i1 %v.21, label %bb.9, label %bb.10
bb.8:
  br label %bb.12
bb.9:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.22, align 4
  %tmp.9 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.22, i32 0, i32 0
  store i32 0, ptr %tmp.9, align 4
  %tmp.10 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.22, i32 0, i32 1
  %v.22 = load { i32, [0 x i8] }, ptr %enum.slot.22, align 4
  %tmp.11 = insertvalue { i1, { i32, [0 x i8] }, {} } zeroinitializer, i1 true, 0
  %v.23 = insertvalue { i1, { i32, [0 x i8] }, {} } %tmp.11, { i32, [0 x i8] } %v.22, 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.4, align 1
  ret { i1, { i32, [0 x i8] }, {} } %v.23
bb.10:
  br label %bb.11
bb.11:
  br label %bb.12
bb.12:
  %v.24 = load ptr, ptr %local.0, align 8
  %v.25 = add i32 0, 65
  %v.26 = load i32, ptr %local.2, align 4
  %tmp.12 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.25, i32 %v.26)
  %v.27 = extractvalue { i32, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i32, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.1
checked.1:
  %v.28 = call { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.4(i32 %v.27)
  %v.29 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.28, 0
  br i1 %v.29, label %bb.13, label %bb.14
bb.13:
  %tmp.14 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.28, 0
  %tmp.15 = icmp ne i1 %tmp.14, true
  br i1 %tmp.15, label %zil.trap, label %checked.2
checked.2:
  %v.30 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.28, 1
  %tmp.16 = insertvalue { i1, { i32, [0 x i8] }, {} } zeroinitializer, i1 true, 0
  %v.31 = insertvalue { i1, { i32, [0 x i8] }, {} } %tmp.16, { i32, [0 x i8] } %v.30, 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.4, align 1
  ret { i1, { i32, [0 x i8] }, {} } %v.31
bb.14:
  %tmp.17 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.28, 0
  %tmp.18 = icmp ne i1 %tmp.17, false
  br i1 %tmp.18, label %zil.trap, label %checked.3
checked.3:
  %v.32 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.28, 2
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.32, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.33 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.34 = call { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.fn.9(ptr %v.24, { { i32 }, { ptr, i64, i64 }, i32 } %v.33)
  store { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %v.34, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %tmp.19 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.19, label %error.drop.4, label %error.done.4
error.drop.4:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.20 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} }, ptr %local.4, i32 0, i32 0
  %tmp.21 = load i1, ptr %tmp.20, align 1
  br i1 %tmp.21, label %error.payload.4, label %success.payload.4
error.payload.4:
  %tmp.22 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} }, ptr %local.4, i32 0, i32 1
  %tmp.23 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.22, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.23)
  %tmp.24 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.22, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.24)
  br label %error.done.4
success.payload.4:
  br label %error.done.4
error.done.4:
  %tmp.25 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.25, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.26 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.26)
  %tmp.27 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.27)
  br label %package.done.5
package.done.5:
  %v.35 = load i32, ptr %local.2, align 4
  %v.36 = add i32 0, 1
  %tmp.28 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.35, i32 %v.36)
  %v.37 = extractvalue { i32, i1 } %tmp.28, 0
  %tmp.29 = extractvalue { i32, i1 } %tmp.28, 1
  br i1 %tmp.29, label %zil.trap, label %checked.6
checked.6:
  store i32 %v.37, ptr %local.2, align 4
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  br label %bb.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, {}, i32 } @zil.fn.12(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.1, align 4
  br label %bb.1
bb.1:
  %v.1 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.1, i32 0, i32 1
  %v.2 = load i64, ptr %tmp.0, align 8
  %v.3 = add i64 0, 0
  %v.4 = icmp sgt i64 %v.2, %v.3
  br i1 %v.4, label %bb.2, label %bb.3
bb.2:
  store i1 false, ptr %resource.live.3, align 1
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = call { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.10(ptr %v.5)
  %v.7 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.6, 0
  br i1 %v.7, label %bb.4, label %bb.5
bb.3:
  %v.20 = load i32, ptr %local.1, align 4
  %tmp.1 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.21 = insertvalue { i1, {}, i32 } %tmp.1, i32 %v.20, 2
  ret { i1, {}, i32 } %v.21
bb.4:
  %tmp.2 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.6, 0
  %tmp.3 = icmp ne i1 %tmp.2, true
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %tmp.4 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.9 = insertvalue { i1, {}, i32 } %tmp.4, {} zeroinitializer, 1
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, {}, i32 } %v.9
bb.5:
  %tmp.5 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.6, 0
  %tmp.6 = icmp ne i1 %tmp.5, false
  br i1 %tmp.6, label %zil.trap, label %checked.1
checked.1:
  %v.10 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.6, 2
  store i1 false, ptr %resource.live.2, align 1
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.10, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.11 = load i32, ptr %local.1, align 4
  %v.12 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.12, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.13 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.14 = call { i1, {}, i32 } @zil.fn.5({ { i32 }, { ptr, i64, i64 }, i32 } %v.13)
  %v.15 = extractvalue { i1, {}, i32 } %v.14, 0
  br i1 %v.15, label %bb.6, label %bb.7
bb.6:
  %tmp.7 = extractvalue { i1, {}, i32 } %v.14, 0
  %tmp.8 = icmp ne i1 %tmp.7, true
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  %tmp.9 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.17 = insertvalue { i1, {}, i32 } %tmp.9, {} zeroinitializer, 1
  %tmp.10 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.10, label %package.drop.3, label %package.done.3
package.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.11 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.11)
  %tmp.12 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.12)
  br label %package.done.3
package.done.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.13 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.13, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.14 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.14)
  %tmp.15 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.15)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, {}, i32 } %v.17
bb.7:
  %tmp.16 = extractvalue { i1, {}, i32 } %v.14, 0
  %tmp.17 = icmp ne i1 %tmp.16, false
  br i1 %tmp.17, label %zil.trap, label %checked.5
checked.5:
  %v.18 = extractvalue { i1, {}, i32 } %v.14, 2
  %tmp.18 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.18, 0
  %tmp.19 = extractvalue { i32, i1 } %tmp.18, 1
  br i1 %tmp.19, label %zil.trap, label %checked.6
checked.6:
  store i32 %v.19, ptr %local.1, align 4
  %tmp.20 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.20, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.21 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.21)
  %tmp.22 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.22)
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.23 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.23, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.24 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.24)
  %tmp.25 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.25)
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.2, align 1
  br label %bb.1
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

; ZIL verified LLVM module
; verified-fingerprint: 6f88afc7d2146b9603039ddfbd2ea3e05eabad2319d92802fac76f6b0f945b62
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
  %stride = ptrtoint ptr getelementptr ({ { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr null, i64 1) to i64
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
define internal { i1, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, {} } @zil.store.2.push(ptr %head, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } %item) {
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
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %data, i64 %len
  store { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } %item, ptr %slot, align 8
  %next.len = add i64 %len, 1
  store i64 %next.len, ptr %len.field, align 8
  ret { i1, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, {} } zeroinitializer
full:
  %tag = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, {} } zeroinitializer, i1 true, 0
  %error = insertvalue { i1, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, {} } %tag, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } %item, 1
  ret { i1, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, {} } %error
}
define internal { i1, {}, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } @zil.store.2.pop(ptr %head) {
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
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %data, i64 %next.len
  %item = load { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %slot, align 8
  %result = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } zeroinitializer, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } %item, 2
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } %result
empty:
  %error = insertvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } zeroinitializer, i1 true, 0
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } %error
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
  %slot = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %data, i64 %index
  %field.0 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %slot, i32 0, i32 0
  call void @zil.fn.3(ptr %field.0)
  %field.1 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %slot, i32 0, i32 1
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
define { i1, { i32, [0 x i8] }, i32 } @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { ptr, i64, i64 }, align 8
  %local.3 = alloca i32, align 4
  %local.4 = alloca i64, align 8
  %local.5 = alloca i1, align 1
  %enum.slot.61 = alloca { i32, [0 x i8] }, align 4
  br label %bb.0
bb.0:
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
  %v.9 = add i32 0, 4
  %v.10 = icmp sgt i32 %v.8, %v.9
  store i1 %v.10, ptr %local.1, align 1
  br label %bb.3
bb.3:
  %v.11 = load i1, ptr %local.1, align 1
  br i1 %v.11, label %bb.4, label %bb.5
bb.4:
  %v.12 = add i32 0, 9
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.13 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.2, i32 %v.12, 2
  ret { i1, { i32, [0 x i8] }, i32 } %v.13
bb.5:
  br label %bb.6
bb.6:
  %v.14 = call { ptr, i64, i64 } @zil.fn.6()
  store i1 false, ptr %resource.live.2, align 1
  store { ptr, i64, i64 } %v.14, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.15 = getelementptr i8, ptr %local.2, i64 0
  %v.16 = load i32, ptr %local.0, align 4
  %v.17 = call { i1, { i32, [0 x i8] }, {} } @zil.fn.7(ptr %v.15, i32 %v.16)
  %v.18 = extractvalue { i1, { i32, [0 x i8] }, {} } %v.17, 0
  br i1 %v.18, label %bb.7, label %bb.8
bb.7:
  %tmp.3 = extractvalue { i1, { i32, [0 x i8] }, {} } %v.17, 0
  %tmp.4 = icmp ne i1 %tmp.3, true
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.19 = extractvalue { i1, { i32, [0 x i8] }, {} } %v.17, 1
  %tmp.5 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.20 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.5, { i32, [0 x i8] } %v.19, 1
  %tmp.6 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.6, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.2.drop(ptr %local.2)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.20
bb.8:
  %tmp.7 = extractvalue { i1, { i32, [0 x i8] }, {} } %v.17, 0
  %tmp.8 = icmp ne i1 %tmp.7, false
  br i1 %tmp.8, label %zil.trap, label %checked.3
checked.3:
  %v.22 = load i32, ptr %local.0, align 4
  %v.23 = add i32 0, 97
  %tmp.9 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.22, i32 %v.23)
  %v.24 = extractvalue { i32, i1 } %tmp.9, 0
  %tmp.10 = extractvalue { i32, i1 } %tmp.9, 1
  br i1 %tmp.10, label %zil.trap, label %checked.4
checked.4:
  %v.25 = load i32, ptr %local.0, align 4
  %v.26 = load i32, ptr %local.0, align 4
  %v.27 = add i32 0, 1
  %tmp.11 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.26, i32 %v.27)
  %v.28 = extractvalue { i32, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i32, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.5
checked.5:
  %tmp.13 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.25, i32 %v.28)
  %v.29 = extractvalue { i32, i1 } %tmp.13, 0
  %tmp.14 = extractvalue { i32, i1 } %tmp.13, 1
  br i1 %tmp.14, label %zil.trap, label %checked.6
checked.6:
  %v.30 = add i32 0, 2
  %tmp.15 = icmp eq i32 %v.30, 0
  %tmp.16 = icmp eq i32 %v.29, -2147483648
  %tmp.17 = icmp eq i32 %v.30, -1
  %tmp.18 = and i1 %tmp.16, %tmp.17
  %tmp.19 = or i1 %tmp.15, %tmp.18
  br i1 %tmp.19, label %zil.trap, label %checked.7
checked.7:
  %v.31 = sdiv i32 %v.29, %v.30
  %tmp.20 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.24, i32 %v.31)
  %v.32 = extractvalue { i32, i1 } %tmp.20, 0
  %tmp.21 = extractvalue { i32, i1 } %tmp.20, 1
  br i1 %tmp.21, label %zil.trap, label %checked.8
checked.8:
  store i32 %v.32, ptr %local.3, align 4
  %v.33 = getelementptr i8, ptr %local.2, i64 0
  %v.34 = add i32 0, 32
  %v.35 = call i32 @zil.fn.9(ptr %v.33, i32 %v.34)
  %v.36 = load i32, ptr %local.3, align 4
  %v.37 = icmp ne i32 %v.35, %v.36
  br i1 %v.37, label %bb.9, label %bb.10
bb.9:
  %v.38 = add i32 0, 90
  %tmp.22 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.39 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.22, i32 %v.38, 2
  %tmp.23 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.23, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.2.drop(ptr %local.2)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.39
bb.10:
  br label %bb.11
bb.11:
  %v.40 = add i64 0, 0
  store i64 %v.40, ptr %local.4, align 8
  br label %bb.12
bb.12:
  %v.41 = load i64, ptr %local.4, align 8
  %v.42 = load i32, ptr %local.0, align 4
  %v.43 = sext i32 %v.42 to i64
  %v.44 = icmp slt i64 %v.41, %v.43
  br i1 %v.44, label %bb.13, label %bb.14
bb.13:
  %v.45 = getelementptr i8, ptr %local.2, i64 0
  %v.46 = load i64, ptr %local.4, align 8
  %tmp.24 = getelementptr { ptr, i64, i64 }, ptr %v.45, i32 0, i32 1
  %tmp.25 = load i64, ptr %tmp.24, align 8
  %tmp.26 = icmp ult i64 %v.46, %tmp.25
  br i1 %tmp.26, label %store.element.ok.10, label %zil.trap
store.element.ok.10:
  %tmp.27 = getelementptr { ptr, i64, i64 }, ptr %v.45, i32 0, i32 0
  %tmp.28 = load ptr, ptr %tmp.27, align 8
  %v.47 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %tmp.28, i64 %v.46
  %v.48 = getelementptr i8, ptr %v.47, i64 0
  %v.49 = call i32 @zil.fn.10(ptr %v.48)
  call void @zil.fn.5(i32 %v.49)
  %v.51 = load i64, ptr %local.4, align 8
  %v.52 = add i64 0, 1
  %tmp.29 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.51, i64 %v.52)
  %v.53 = extractvalue { i64, i1 } %tmp.29, 0
  %tmp.30 = extractvalue { i64, i1 } %tmp.29, 1
  br i1 %tmp.30, label %zil.trap, label %checked.11
checked.11:
  store i64 %v.53, ptr %local.4, align 8
  br label %bb.12
bb.14:
  %v.54 = getelementptr i8, ptr %local.2, i64 0
  %v.55 = load i32, ptr %local.0, align 4
  %v.56 = add i32 0, 3
  %tmp.31 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.55, i32 %v.56)
  %v.57 = extractvalue { i32, i1 } %tmp.31, 0
  %tmp.32 = extractvalue { i32, i1 } %tmp.31, 1
  br i1 %tmp.32, label %zil.trap, label %checked.12
checked.12:
  %v.58 = sext i32 %v.57 to i64
  %v.59 = call i1 @zil.store.2.reserve(ptr %v.54, i64 %v.58)
  %v.60 = xor i1 %v.59, true
  br i1 %v.60, label %bb.15, label %bb.16
bb.15:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.61, align 4
  %tmp.33 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.61, i32 0, i32 0
  store i32 0, ptr %tmp.33, align 4
  %tmp.34 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.61, i32 0, i32 1
  %v.61 = load { i32, [0 x i8] }, ptr %enum.slot.61, align 4
  %tmp.35 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.62 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.35, { i32, [0 x i8] } %v.61, 1
  %tmp.36 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.36, label %resource.drop.13, label %resource.done.13
resource.drop.13:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.2.drop(ptr %local.2)
  br label %resource.done.13
resource.done.13:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.62
bb.16:
  br label %bb.17
bb.17:
  %v.63 = load i32, ptr %local.0, align 4
  %v.64 = add i32 0, 0
  %v.65 = icmp sgt i32 %v.63, %v.64
  br i1 %v.65, label %bb.19, label %bb.18
bb.18:
  %v.66 = or i1 false, false
  store i1 %v.66, ptr %local.5, align 1
  br label %bb.20
bb.19:
  %v.67 = getelementptr i8, ptr %local.2, i64 0
  %v.68 = load i32, ptr %local.0, align 4
  %v.69 = add i32 0, 1
  %tmp.37 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.68, i32 %v.69)
  %v.70 = extractvalue { i32, i1 } %tmp.37, 0
  %tmp.38 = extractvalue { i32, i1 } %tmp.37, 1
  br i1 %tmp.38, label %zil.trap, label %checked.14
checked.14:
  %v.71 = sext i32 %v.70 to i64
  %tmp.39 = getelementptr { ptr, i64, i64 }, ptr %v.67, i32 0, i32 1
  %tmp.40 = load i64, ptr %tmp.39, align 8
  %tmp.41 = icmp ult i64 %v.71, %tmp.40
  br i1 %tmp.41, label %store.element.ok.15, label %zil.trap
store.element.ok.15:
  %tmp.42 = getelementptr { ptr, i64, i64 }, ptr %v.67, i32 0, i32 0
  %tmp.43 = load ptr, ptr %tmp.42, align 8
  %v.72 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %tmp.43, i64 %v.71
  %v.73 = getelementptr i8, ptr %v.72, i64 0
  %v.74 = call i32 @zil.fn.10(ptr %v.73)
  %v.75 = add i32 0, 96
  %v.76 = load i32, ptr %local.0, align 4
  %tmp.44 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.75, i32 %v.76)
  %v.77 = extractvalue { i32, i1 } %tmp.44, 0
  %tmp.45 = extractvalue { i32, i1 } %tmp.44, 1
  br i1 %tmp.45, label %zil.trap, label %checked.16
checked.16:
  %v.78 = icmp ne i32 %v.74, %v.77
  store i1 %v.78, ptr %local.5, align 1
  br label %bb.20
bb.20:
  %v.79 = load i1, ptr %local.5, align 1
  br i1 %v.79, label %bb.21, label %bb.22
bb.21:
  %v.80 = add i32 0, 91
  %tmp.46 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.81 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.46, i32 %v.80, 2
  %tmp.47 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.47, label %resource.drop.17, label %resource.done.17
resource.drop.17:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.2.drop(ptr %local.2)
  br label %resource.done.17
resource.done.17:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.81
bb.22:
  br label %bb.23
bb.23:
  %v.82 = add i32 0, 0
  %tmp.48 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.83 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.48, i32 %v.82, 2
  %tmp.49 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.49, label %resource.drop.18, label %resource.done.18
resource.drop.18:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.store.2.drop(ptr %local.2)
  br label %resource.done.18
resource.done.18:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.83
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

define { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } @zil.fn.4(i32 %arg.0) {
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
  %local.8 = alloca [2 x i32], align 4
  %local.9 = alloca { i32, [2 x i32] }, align 4
  %resource.live.10 = alloca i1, align 1
  store i1 false, ptr %resource.live.10, align 1
  %local.10 = alloca { i32 }, align 4
  %resource.live.11 = alloca i1, align 1
  store i1 false, ptr %resource.live.11, align 1
  %local.11 = alloca { ptr, i64, i64 }, align 8
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
  %tmp.3 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } zeroinitializer, i1 true, 0
  %v.16 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } %tmp.3, { i32, [0 x i8] } %v.15, 1
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
  ret { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } %v.16
bb.5:
  br label %bb.6
bb.6:
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.10, align 1
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
  %v.20 = load i32, ptr %local.0, align 4
  %v.21 = load i32, ptr %local.0, align 4
  %v.22 = add i32 0, 1
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.21, i32 %v.22)
  %v.23 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
%tmp.8 = insertvalue [2 x i32] zeroinitializer, i32 %v.20, 0
%v.24 = insertvalue [2 x i32] %tmp.8, i32 %v.23, 1
  store [2 x i32] %v.24, ptr %local.8, align 4
  %v.25 = load i32, ptr %local.7, align 4
  %v.26 = load [2 x i32], ptr %local.8, align 4
%tmp.9 = insertvalue { i32, [2 x i32] } zeroinitializer, i32 %v.25, 0
%v.27 = insertvalue { i32, [2 x i32] } %tmp.9, [2 x i32] %v.26, 1
  store { i32, [2 x i32] } %v.27, ptr %local.9, align 4
  %v.28 = load { i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  store { i32 } %v.28, ptr %local.10, align 4
  store i1 true, ptr %resource.live.10, align 1
  %v.29 = load { ptr, i64, i64 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store { ptr, i64, i64 } %v.29, ptr %local.11, align 8
  store i1 true, ptr %resource.live.11, align 1
  %v.30 = load { i32, [2 x i32] }, ptr %local.9, align 4
  %v.31 = load { i32 }, ptr %local.10, align 4
  store i1 false, ptr %resource.live.10, align 1
  %v.32 = load { ptr, i64, i64 }, ptr %local.11, align 8
  store i1 false, ptr %resource.live.11, align 1
%tmp.10 = insertvalue { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } zeroinitializer, { i32 } %v.31, 0
%tmp.11 = insertvalue { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } %tmp.10, { ptr, i64, i64 } %v.32, 1
%v.33 = insertvalue { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } %tmp.11, { i32, [2 x i32] } %v.30, 2
  %tmp.12 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } zeroinitializer, i1 false, 0
  %v.34 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } %tmp.12, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } %v.33, 2
  %tmp.13 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.13, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.11, align 1
  call void @zil.bytes.drop(ptr %local.11)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.14 = load i1, ptr %resource.live.10, align 1
  br i1 %tmp.14, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.10, align 1
  call void @zil.fn.3(ptr %local.10)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.10, align 1
  %tmp.15 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.15, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.bytes.drop(ptr %local.6)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.16 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.16, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.3(ptr %local.5)
  br label %resource.done.7
resource.done.7:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.17 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.17, label %resource.drop.8, label %resource.done.8
resource.drop.8:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.bytes.drop(ptr %local.3)
  br label %resource.done.8
resource.done.8:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.18 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.18, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.3(ptr %local.2)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } %v.34
zil.trap:
  call void @llvm.trap()
  unreachable
}

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

define { i1, { i32, [0 x i8] }, {} } @zil.fn.7(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i1, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, {} }, align 8
  %enum.slot.5 = alloca { i32, [0 x i8] }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = sext i32 %v.1 to i64
  %v.3 = call i1 @zil.store.2.reserve(ptr %v.0, i64 %v.2)
  %v.4 = xor i1 %v.3, true
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.5, align 4
  %tmp.0 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.5, i32 0, i32 0
  store i32 0, ptr %tmp.0, align 4
  %tmp.1 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.5, i32 0, i32 1
  %v.5 = load { i32, [0 x i8] }, ptr %enum.slot.5, align 4
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, {} } zeroinitializer, i1 true, 0
  %v.6 = insertvalue { i1, { i32, [0 x i8] }, {} } %tmp.2, { i32, [0 x i8] } %v.5, 1
  ret { i1, { i32, [0 x i8] }, {} } %v.6
bb.2:
  br label %bb.3
bb.3:
  %v.7 = add i32 0, 0
  store i32 %v.7, ptr %local.2, align 4
  br label %bb.4
bb.4:
  %v.8 = load i32, ptr %local.2, align 4
  %v.9 = load i32, ptr %local.1, align 4
  %v.10 = icmp slt i32 %v.8, %v.9
  br i1 %v.10, label %bb.5, label %bb.6
bb.5:
  store i1 false, ptr %resource.live.4, align 1
  %v.11 = add i32 0, 65
  %v.12 = load i32, ptr %local.2, align 4
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.0
checked.0:
  %v.14 = call { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } @zil.fn.4(i32 %v.13)
  %v.15 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } %v.14, 0
  br i1 %v.15, label %bb.7, label %bb.8
bb.6:
  %tmp.5 = insertvalue { i1, { i32, [0 x i8] }, {} } zeroinitializer, i1 false, 0
  %v.26 = insertvalue { i1, { i32, [0 x i8] }, {} } %tmp.5, {} zeroinitializer, 2
  ret { i1, { i32, [0 x i8] }, {} } %v.26
bb.7:
  %tmp.6 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } %v.14, 0
  %tmp.7 = icmp ne i1 %tmp.6, true
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  %v.16 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } %v.14, 1
  %tmp.8 = insertvalue { i1, { i32, [0 x i8] }, {} } zeroinitializer, i1 true, 0
  %v.17 = insertvalue { i1, { i32, [0 x i8] }, {} } %tmp.8, { i32, [0 x i8] } %v.16, 1
  store i1 false, ptr %resource.live.4, align 1
  ret { i1, { i32, [0 x i8] }, {} } %v.17
bb.8:
  %tmp.9 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } %v.14, 0
  %tmp.10 = icmp ne i1 %tmp.9, false
  br i1 %tmp.10, label %zil.trap, label %checked.2
checked.2:
  %v.18 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } } %v.14, 2
  store i1 false, ptr %resource.live.3, align 1
  store { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } %v.18, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.19 = load ptr, ptr %local.0, align 8
  %v.20 = load { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.21 = call { i1, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, {} } @zil.store.2.push(ptr %v.19, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } } %v.20)
  store { i1, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, {} } %v.21, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %tmp.11 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.11, label %error.drop.3, label %error.done.3
error.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.12 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, {} }, ptr %local.4, i32 0, i32 0
  %tmp.13 = load i1, ptr %tmp.12, align 1
  br i1 %tmp.13, label %error.payload.3, label %success.payload.3
error.payload.3:
  %tmp.14 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, {} }, ptr %local.4, i32 0, i32 1
  %tmp.15 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %tmp.14, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.15)
  %tmp.16 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %tmp.14, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.16)
  br label %error.done.3
success.payload.3:
  br label %error.done.3
error.done.3:
  %v.22 = load i32, ptr %local.2, align 4
  %v.23 = add i32 0, 1
  %tmp.17 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.22, i32 %v.23)
  %v.24 = extractvalue { i32, i1 } %tmp.17, 0
  %tmp.18 = extractvalue { i32, i1 } %tmp.17, 1
  br i1 %tmp.18, label %zil.trap, label %checked.4
checked.4:
  store i32 %v.24, ptr %local.2, align 4
  store i1 false, ptr %resource.live.4, align 1
  %tmp.19 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.19, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.20 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.20)
  %tmp.21 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %local.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.21)
  br label %package.done.5
package.done.5:
  store i1 false, ptr %resource.live.3, align 1
  br label %bb.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.8(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %index.slot.22 = alloca [2 x i32], align 4
  %index.slot.37 = alloca [2 x i32], align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.0, i32 0, i32 2
  %v.2 = getelementptr { i32, [2 x i32] }, ptr %v.1, i32 0, i32 0
  %v.3 = load ptr, ptr %local.0, align 8
  %v.4 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.3, i32 0, i32 2
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = load { i32, [2 x i32] }, ptr %v.5, align 4
%v.7 = extractvalue { i32, [2 x i32] } %v.6, 0
  %v.8 = load i32, ptr %local.1, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.7, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.9, ptr %v.2, align 4
  %v.10 = add i32 0, 0
  store i32 %v.10, ptr %local.2, align 4
  %v.11 = load ptr, ptr %local.0, align 8
  %v.12 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.11, i32 0, i32 2
  %v.13 = getelementptr { i32, [2 x i32] }, ptr %v.12, i32 0, i32 1
  %v.14 = load i32, ptr %local.2, align 4
  %tmp.2 = sext i32 %v.14 to i64
  %tmp.3 = icmp ult i64 %tmp.2, 2
  br i1 %tmp.3, label %address.ok.1, label %zil.trap
address.ok.1:
  %v.15 = getelementptr [2 x i32], ptr %v.13, i64 0, i64 %tmp.2
  %v.16 = load ptr, ptr %local.0, align 8
  %v.17 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.16, i32 0, i32 2
  %v.18 = getelementptr i8, ptr %v.17, i64 0
  %v.19 = load { i32, [2 x i32] }, ptr %v.18, align 4
%v.20 = extractvalue { i32, [2 x i32] } %v.19, 1
  %v.21 = load i32, ptr %local.2, align 4
  %tmp.4 = sext i32 %v.21 to i64
  %tmp.5 = icmp ult i64 %tmp.4, 2
  br i1 %tmp.5, label %index.ok.2, label %zil.trap
index.ok.2:
  store [2 x i32] %v.20, ptr %index.slot.22, align 4
  %tmp.6 = getelementptr [2 x i32], ptr %index.slot.22, i64 0, i64 %tmp.4
  %v.22 = load i32, ptr %tmp.6, align 4
  %v.23 = load i32, ptr %local.1, align 4
  %tmp.7 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.22, i32 %v.23)
  %v.24 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.3
checked.3:
  store i32 %v.24, ptr %v.15, align 4
  %v.25 = add i32 0, 1
  store i32 %v.25, ptr %local.3, align 4
  %v.26 = load ptr, ptr %local.0, align 8
  %v.27 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.26, i32 0, i32 2
  %v.28 = getelementptr { i32, [2 x i32] }, ptr %v.27, i32 0, i32 1
  %v.29 = load i32, ptr %local.3, align 4
  %tmp.9 = sext i32 %v.29 to i64
  %tmp.10 = icmp ult i64 %tmp.9, 2
  br i1 %tmp.10, label %address.ok.4, label %zil.trap
address.ok.4:
  %v.30 = getelementptr [2 x i32], ptr %v.28, i64 0, i64 %tmp.9
  %v.31 = load ptr, ptr %local.0, align 8
  %v.32 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.31, i32 0, i32 2
  %v.33 = getelementptr i8, ptr %v.32, i64 0
  %v.34 = load { i32, [2 x i32] }, ptr %v.33, align 4
%v.35 = extractvalue { i32, [2 x i32] } %v.34, 1
  %v.36 = load i32, ptr %local.3, align 4
  %tmp.11 = sext i32 %v.36 to i64
  %tmp.12 = icmp ult i64 %tmp.11, 2
  br i1 %tmp.12, label %index.ok.5, label %zil.trap
index.ok.5:
  store [2 x i32] %v.35, ptr %index.slot.37, align 4
  %tmp.13 = getelementptr [2 x i32], ptr %index.slot.37, i64 0, i64 %tmp.11
  %v.37 = load i32, ptr %tmp.13, align 4
  %v.38 = load i32, ptr %local.1, align 4
  %tmp.14 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.37, i32 %v.38)
  %v.39 = extractvalue { i32, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i32, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.6
checked.6:
  store i32 %v.39, ptr %v.30, align 4
  %v.40 = load ptr, ptr %local.0, align 8
  %v.41 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.40, i32 0, i32 2
  %v.42 = getelementptr i8, ptr %v.41, i64 0
  %v.43 = load { i32, [2 x i32] }, ptr %v.42, align 4
%v.44 = extractvalue { i32, [2 x i32] } %v.43, 0
  ret i32 %v.44
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.9(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i64, align 8
  %local.3 = alloca i64, align 8
  %local.4 = alloca i32, align 4
  %local.5 = alloca ptr, align 8
  %local.6 = alloca i1, align 1
  %index.slot.22 = alloca [2 x i32], align 4
  %index.slot.36 = alloca [2 x i32], align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.0, i32 0, i32 1
  %v.1 = load i64, ptr %tmp.0, align 8
  store i64 %v.1, ptr %local.2, align 8
  %v.2 = add i64 0, 0
  store i64 %v.2, ptr %local.3, align 8
  %v.3 = add i32 0, 0
  store i32 %v.3, ptr %local.4, align 4
  br label %bb.1
bb.1:
  %v.4 = load i64, ptr %local.3, align 8
  %v.5 = load i64, ptr %local.2, align 8
  %v.6 = icmp slt i64 %v.4, %v.5
  br i1 %v.6, label %bb.2, label %bb.3
bb.2:
  %v.7 = load ptr, ptr %local.0, align 8
  %v.8 = load i64, ptr %local.3, align 8
  %tmp.1 = getelementptr { ptr, i64, i64 }, ptr %v.7, i32 0, i32 1
  %tmp.2 = load i64, ptr %tmp.1, align 8
  %tmp.3 = icmp ult i64 %v.8, %tmp.2
  br i1 %tmp.3, label %store.element.ok.0, label %zil.trap
store.element.ok.0:
  %tmp.4 = getelementptr { ptr, i64, i64 }, ptr %v.7, i32 0, i32 0
  %tmp.5 = load ptr, ptr %tmp.4, align 8
  %v.9 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %tmp.5, i64 %v.8
  store ptr %v.9, ptr %local.5, align 8
  %v.10 = load i32, ptr %local.4, align 4
  %v.11 = load ptr, ptr %local.5, align 8
  %v.12 = getelementptr i8, ptr %v.11, i64 0
  %v.13 = load i32, ptr %local.1, align 4
  %v.14 = call i32 @zil.fn.8(ptr %v.12, i32 %v.13)
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.10, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.15, ptr %local.4, align 4
  %v.16 = load ptr, ptr %local.5, align 8
  %v.17 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.16, i32 0, i32 2
  %v.18 = getelementptr i8, ptr %v.17, i64 0
  %v.19 = load { i32, [2 x i32] }, ptr %v.18, align 4
%v.20 = extractvalue { i32, [2 x i32] } %v.19, 1
  %v.21 = add i32 0, 0
  %tmp.8 = sext i32 %v.21 to i64
  %tmp.9 = icmp ult i64 %tmp.8, 2
  br i1 %tmp.9, label %index.ok.2, label %zil.trap
index.ok.2:
  store [2 x i32] %v.20, ptr %index.slot.22, align 4
  %tmp.10 = getelementptr [2 x i32], ptr %index.slot.22, i64 0, i64 %tmp.8
  %v.22 = load i32, ptr %tmp.10, align 4
  %v.23 = load ptr, ptr %local.5, align 8
  %v.24 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.23, i32 0, i32 2
  %v.25 = getelementptr i8, ptr %v.24, i64 0
  %v.26 = load { i32, [2 x i32] }, ptr %v.25, align 4
%v.27 = extractvalue { i32, [2 x i32] } %v.26, 0
  %v.28 = icmp ne i32 %v.22, %v.27
  br i1 %v.28, label %bb.4, label %bb.5
bb.3:
  %v.50 = load i32, ptr %local.4, align 4
  ret i32 %v.50
bb.4:
  %v.29 = or i1 false, true
  store i1 %v.29, ptr %local.6, align 1
  br label %bb.6
bb.5:
  %v.30 = load ptr, ptr %local.5, align 8
  %v.31 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.30, i32 0, i32 2
  %v.32 = getelementptr i8, ptr %v.31, i64 0
  %v.33 = load { i32, [2 x i32] }, ptr %v.32, align 4
%v.34 = extractvalue { i32, [2 x i32] } %v.33, 1
  %v.35 = add i32 0, 1
  %tmp.11 = sext i32 %v.35 to i64
  %tmp.12 = icmp ult i64 %tmp.11, 2
  br i1 %tmp.12, label %index.ok.3, label %zil.trap
index.ok.3:
  store [2 x i32] %v.34, ptr %index.slot.36, align 4
  %tmp.13 = getelementptr [2 x i32], ptr %index.slot.36, i64 0, i64 %tmp.11
  %v.36 = load i32, ptr %tmp.13, align 4
  %v.37 = load ptr, ptr %local.5, align 8
  %v.38 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.37, i32 0, i32 2
  %v.39 = getelementptr i8, ptr %v.38, i64 0
  %v.40 = load { i32, [2 x i32] }, ptr %v.39, align 4
%v.41 = extractvalue { i32, [2 x i32] } %v.40, 0
  %v.42 = add i32 0, 1
  %tmp.14 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.41, i32 %v.42)
  %v.43 = extractvalue { i32, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i32, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.4
checked.4:
  %v.44 = icmp ne i32 %v.36, %v.43
  store i1 %v.44, ptr %local.6, align 1
  br label %bb.6
bb.6:
  %v.45 = load i1, ptr %local.6, align 1
  br i1 %v.45, label %bb.7, label %bb.8
bb.7:
  %v.46 = add i32 0, -1
  ret i32 %v.46
bb.8:
  br label %bb.9
bb.9:
  %v.47 = load i64, ptr %local.3, align 8
  %v.48 = add i64 0, 1
  %tmp.16 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.47, i64 %v.48)
  %v.49 = extractvalue { i64, i1 } %tmp.16, 0
  %tmp.17 = extractvalue { i64, i1 } %tmp.16, 1
  br i1 %tmp.17, label %zil.trap, label %checked.5
checked.5:
  store i64 %v.49, ptr %local.3, align 8
  br label %bb.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.10(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.0, i32 0, i32 2
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load { i32, [2 x i32] }, ptr %v.2, align 4
%v.4 = extractvalue { i32, [2 x i32] } %v.3, 0
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.1()
  %entry.error = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 2
  ret i32 %entry.success
}

; ZIL verified LLVM module
; verified-fingerprint: cd7663f68414e53796ed060d5206f3d3edab44ac6b10fb53417955693f97a3cc
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
  %local.1 = alloca i32, align 4
  %local.2 = alloca i1, align 1
  %local.3 = alloca i1, align 1
  %local.4 = alloca i1, align 1
  %local.5 = alloca i1, align 1
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { ptr, i64, i64 }, align 8
  %local.7 = alloca ptr, align 8
  %local.8 = alloca i32, align 4
  %local.9 = alloca i32, align 4
  %local.10 = alloca i1, align 1
  %local.11 = alloca i1, align 1
  %enum.slot.96 = alloca { i32, [0 x i8] }, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = call i32 @getchar()
  %v.3 = add i32 0, 48
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.4, ptr %local.0, align 4
  %v.5 = call i32 @getchar()
  %v.6 = add i32 0, 48
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.7, ptr %local.1, align 4
  %v.8 = load i32, ptr %local.0, align 4
  %v.9 = add i32 0, 0
  %v.10 = icmp slt i32 %v.8, %v.9
  br i1 %v.10, label %bb.1, label %bb.2
bb.1:
  %v.11 = or i1 false, true
  store i1 %v.11, ptr %local.2, align 1
  br label %bb.3
bb.2:
  %v.12 = load i32, ptr %local.0, align 4
  %v.13 = add i32 0, 4
  %v.14 = icmp sgt i32 %v.12, %v.13
  store i1 %v.14, ptr %local.2, align 1
  br label %bb.3
bb.3:
  %v.15 = load i1, ptr %local.2, align 1
  br i1 %v.15, label %bb.4, label %bb.5
bb.4:
  %v.16 = or i1 false, true
  store i1 %v.16, ptr %local.3, align 1
  br label %bb.6
bb.5:
  %v.17 = load i32, ptr %local.1, align 4
  %v.18 = add i32 0, 0
  %v.19 = icmp slt i32 %v.17, %v.18
  store i1 %v.19, ptr %local.3, align 1
  br label %bb.6
bb.6:
  %v.20 = load i1, ptr %local.3, align 1
  br i1 %v.20, label %bb.7, label %bb.8
bb.7:
  %v.21 = or i1 false, true
  store i1 %v.21, ptr %local.4, align 1
  br label %bb.9
bb.8:
  %v.22 = load i32, ptr %local.1, align 4
  %v.23 = load i32, ptr %local.0, align 4
  %v.24 = icmp sge i32 %v.22, %v.23
  store i1 %v.24, ptr %local.4, align 1
  br label %bb.9
bb.9:
  %v.25 = load i1, ptr %local.4, align 1
  br i1 %v.25, label %bb.10, label %bb.11
bb.10:
  %v.26 = load i32, ptr %local.0, align 4
  %v.27 = add i32 0, 0
  %v.28 = icmp eq i32 %v.26, %v.27
  br i1 %v.28, label %bb.13, label %bb.12
bb.11:
  br label %bb.18
bb.12:
  %v.29 = or i1 false, false
  store i1 %v.29, ptr %local.5, align 1
  br label %bb.14
bb.13:
  %v.30 = load i32, ptr %local.1, align 4
  %v.31 = add i32 0, 0
  %v.32 = icmp eq i32 %v.30, %v.31
  store i1 %v.32, ptr %local.5, align 1
  br label %bb.14
bb.14:
  %v.33 = load i1, ptr %local.5, align 1
  br i1 %v.33, label %bb.15, label %bb.16
bb.15:
  %v.34 = add i32 0, 45
  call void @zil.fn.5(i32 %v.34)
  %v.36 = add i32 0, 0
  %tmp.4 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.37 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.4, i32 %v.36, 2
  ret { i1, { i32, [0 x i8] }, i32 } %v.37
bb.16:
  br label %bb.17
bb.17:
  %v.38 = add i32 0, 9
  %tmp.5 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.39 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.5, i32 %v.38, 2
  ret { i1, { i32, [0 x i8] }, i32 } %v.39
bb.18:
  %v.40 = call { ptr, i64, i64 } @zil.fn.6()
  store i1 false, ptr %resource.live.6, align 1
  store { ptr, i64, i64 } %v.40, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  %v.41 = getelementptr i8, ptr %local.6, i64 0
  %v.42 = load i32, ptr %local.0, align 4
  %v.43 = call { i1, { i32, [0 x i8] }, {} } @zil.fn.7(ptr %v.41, i32 %v.42)
  %v.44 = extractvalue { i1, { i32, [0 x i8] }, {} } %v.43, 0
  br i1 %v.44, label %bb.19, label %bb.20
bb.19:
  %tmp.6 = extractvalue { i1, { i32, [0 x i8] }, {} } %v.43, 0
  %tmp.7 = icmp ne i1 %tmp.6, true
  br i1 %tmp.7, label %zil.trap, label %checked.2
checked.2:
  %v.45 = extractvalue { i1, { i32, [0 x i8] }, {} } %v.43, 1
  %tmp.8 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.46 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.8, { i32, [0 x i8] } %v.45, 1
  %tmp.9 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.9, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.store.2.drop(ptr %local.6)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.6, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.46
bb.20:
  %tmp.10 = extractvalue { i1, { i32, [0 x i8] }, {} } %v.43, 0
  %tmp.11 = icmp ne i1 %tmp.10, false
  br i1 %tmp.11, label %zil.trap, label %checked.4
checked.4:
  %v.48 = getelementptr i8, ptr %local.6, i64 0
  %v.49 = load i32, ptr %local.1, align 4
  %v.50 = sext i32 %v.49 to i64
  %tmp.12 = getelementptr { ptr, i64, i64 }, ptr %v.48, i32 0, i32 1
  %tmp.13 = load i64, ptr %tmp.12, align 8
  %tmp.14 = icmp ult i64 %v.50, %tmp.13
  br i1 %tmp.14, label %store.element.ok.5, label %zil.trap
store.element.ok.5:
  %tmp.15 = getelementptr { ptr, i64, i64 }, ptr %v.48, i32 0, i32 0
  %tmp.16 = load ptr, ptr %tmp.15, align 8
  %v.51 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %tmp.16, i64 %v.50
  store ptr %v.51, ptr %local.7, align 8
  %v.52 = load ptr, ptr %local.7, align 8
  %v.53 = getelementptr i8, ptr %v.52, i64 0
  %v.54 = call i32 @zil.fn.8(ptr %v.53)
  store i32 %v.54, ptr %local.8, align 4
  %v.55 = load i32, ptr %local.0, align 4
  %v.56 = add i32 0, 65
  %tmp.17 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.55, i32 %v.56)
  %v.57 = extractvalue { i32, i1 } %tmp.17, 0
  %tmp.18 = extractvalue { i32, i1 } %tmp.17, 1
  br i1 %tmp.18, label %zil.trap, label %checked.6
checked.6:
  %v.58 = load i32, ptr %local.0, align 4
  %v.59 = load i32, ptr %local.0, align 4
  %v.60 = add i32 0, 1
  %tmp.19 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.59, i32 %v.60)
  %v.61 = extractvalue { i32, i1 } %tmp.19, 0
  %tmp.20 = extractvalue { i32, i1 } %tmp.19, 1
  br i1 %tmp.20, label %zil.trap, label %checked.7
checked.7:
  %tmp.21 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.58, i32 %v.61)
  %v.62 = extractvalue { i32, i1 } %tmp.21, 0
  %tmp.22 = extractvalue { i32, i1 } %tmp.21, 1
  br i1 %tmp.22, label %zil.trap, label %checked.8
checked.8:
  %v.63 = add i32 0, 2
  %tmp.23 = icmp eq i32 %v.63, 0
  %tmp.24 = icmp eq i32 %v.62, -2147483648
  %tmp.25 = icmp eq i32 %v.63, -1
  %tmp.26 = and i1 %tmp.24, %tmp.25
  %tmp.27 = or i1 %tmp.23, %tmp.26
  br i1 %tmp.27, label %zil.trap, label %checked.9
checked.9:
  %v.64 = sdiv i32 %v.62, %v.63
  %tmp.28 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.57, i32 %v.64)
  %v.65 = extractvalue { i32, i1 } %tmp.28, 0
  %tmp.29 = extractvalue { i32, i1 } %tmp.28, 1
  br i1 %tmp.29, label %zil.trap, label %checked.10
checked.10:
  store i32 %v.65, ptr %local.9, align 4
  %v.66 = load i32, ptr %local.8, align 4
  %v.67 = add i32 0, 65
  %v.68 = load i32, ptr %local.1, align 4
  %tmp.30 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.67, i32 %v.68)
  %v.69 = extractvalue { i32, i1 } %tmp.30, 0
  %tmp.31 = extractvalue { i32, i1 } %tmp.30, 1
  br i1 %tmp.31, label %zil.trap, label %checked.11
checked.11:
  %v.70 = icmp ne i32 %v.66, %v.69
  br i1 %v.70, label %bb.21, label %bb.22
bb.21:
  %v.71 = or i1 false, true
  store i1 %v.71, ptr %local.10, align 1
  br label %bb.23
bb.22:
  %v.72 = getelementptr i8, ptr %local.6, i64 0
  %v.73 = call i32 @zil.fn.9(ptr %v.72)
  %v.74 = load i32, ptr %local.9, align 4
  %v.75 = icmp ne i32 %v.73, %v.74
  store i1 %v.75, ptr %local.10, align 1
  br label %bb.23
bb.23:
  %v.76 = load i1, ptr %local.10, align 1
  br i1 %v.76, label %bb.24, label %bb.25
bb.24:
  %v.77 = add i32 0, 90
  %tmp.32 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.78 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.32, i32 %v.77, 2
  %tmp.33 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.33, label %resource.drop.12, label %resource.done.12
resource.drop.12:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.store.2.drop(ptr %local.6)
  br label %resource.done.12
resource.done.12:
  store i1 false, ptr %resource.live.6, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.78
bb.25:
  br label %bb.26
bb.26:
  %v.79 = load i32, ptr %local.8, align 4
  %v.80 = add i32 0, 32
  %tmp.34 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.79, i32 %v.80)
  %v.81 = extractvalue { i32, i1 } %tmp.34, 0
  %tmp.35 = extractvalue { i32, i1 } %tmp.34, 1
  br i1 %tmp.35, label %zil.trap, label %checked.13
checked.13:
  call void @zil.fn.5(i32 %v.81)
  %v.83 = getelementptr i8, ptr %local.6, i64 0
  %v.84 = load i32, ptr %local.0, align 4
  %v.85 = add i32 0, 2
  %tmp.36 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.84, i32 %v.85)
  %v.86 = extractvalue { i32, i1 } %tmp.36, 0
  %tmp.37 = extractvalue { i32, i1 } %tmp.36, 1
  br i1 %tmp.37, label %zil.trap, label %checked.14
checked.14:
  %v.87 = sext i32 %v.86 to i64
  %v.88 = call i1 @zil.store.2.reserve(ptr %v.83, i64 %v.87)
  %v.89 = xor i1 %v.88, true
  br i1 %v.89, label %bb.27, label %bb.28
bb.27:
  %v.90 = getelementptr i8, ptr %local.6, i64 0
  %v.91 = call i32 @zil.fn.9(ptr %v.90)
  %v.92 = load i32, ptr %local.9, align 4
  %v.93 = icmp ne i32 %v.91, %v.92
  br i1 %v.93, label %bb.29, label %bb.30
bb.28:
  br label %bb.32
bb.29:
  %v.94 = add i32 0, 91
  %tmp.38 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.95 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.38, i32 %v.94, 2
  %tmp.39 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.39, label %resource.drop.15, label %resource.done.15
resource.drop.15:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.store.2.drop(ptr %local.6)
  br label %resource.done.15
resource.done.15:
  store i1 false, ptr %resource.live.6, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.95
bb.30:
  br label %bb.31
bb.31:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.96, align 4
  %tmp.40 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.96, i32 0, i32 0
  store i32 0, ptr %tmp.40, align 4
  %tmp.41 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.96, i32 0, i32 1
  %v.96 = load { i32, [0 x i8] }, ptr %enum.slot.96, align 4
  %tmp.42 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.97 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.42, { i32, [0 x i8] } %v.96, 1
  %tmp.43 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.43, label %resource.drop.16, label %resource.done.16
resource.drop.16:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.store.2.drop(ptr %local.6)
  br label %resource.done.16
resource.done.16:
  store i1 false, ptr %resource.live.6, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.97
bb.32:
  %v.98 = getelementptr i8, ptr %local.6, i64 0
  %v.99 = load i32, ptr %local.1, align 4
  %v.100 = sext i32 %v.99 to i64
  %tmp.44 = getelementptr { ptr, i64, i64 }, ptr %v.98, i32 0, i32 1
  %tmp.45 = load i64, ptr %tmp.44, align 8
  %tmp.46 = icmp ult i64 %v.100, %tmp.45
  br i1 %tmp.46, label %store.element.ok.17, label %zil.trap
store.element.ok.17:
  %tmp.47 = getelementptr { ptr, i64, i64 }, ptr %v.98, i32 0, i32 0
  %tmp.48 = load ptr, ptr %tmp.47, align 8
  %v.101 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %tmp.48, i64 %v.100
  %v.102 = getelementptr i8, ptr %v.101, i64 0
  %v.103 = call i32 @zil.fn.8(ptr %v.102)
  %v.104 = load i32, ptr %local.8, align 4
  %v.105 = icmp ne i32 %v.103, %v.104
  br i1 %v.105, label %bb.33, label %bb.34
bb.33:
  %v.106 = or i1 false, true
  store i1 %v.106, ptr %local.11, align 1
  br label %bb.35
bb.34:
  %v.107 = getelementptr i8, ptr %local.6, i64 0
  %v.108 = call i32 @zil.fn.9(ptr %v.107)
  %v.109 = load i32, ptr %local.9, align 4
  %v.110 = icmp ne i32 %v.108, %v.109
  store i1 %v.110, ptr %local.11, align 1
  br label %bb.35
bb.35:
  %v.111 = load i1, ptr %local.11, align 1
  br i1 %v.111, label %bb.36, label %bb.37
bb.36:
  %v.112 = add i32 0, 92
  %tmp.49 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.113 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.49, i32 %v.112, 2
  %tmp.50 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.50, label %resource.drop.18, label %resource.done.18
resource.drop.18:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.store.2.drop(ptr %local.6)
  br label %resource.done.18
resource.done.18:
  store i1 false, ptr %resource.live.6, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.113
bb.37:
  br label %bb.38
bb.38:
  %v.114 = add i32 0, 0
  %tmp.51 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.115 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.51, i32 %v.114, 2
  %tmp.52 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.52, label %resource.drop.19, label %resource.done.19
resource.drop.19:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.store.2.drop(ptr %local.6)
  br label %resource.done.19
resource.done.19:
  store i1 false, ptr %resource.live.6, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.115
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

define i32 @zil.fn.8(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i32, [2 x i32] }, align 4
  %local.2 = alloca i1, align 1
  %index.slot.7 = alloca [2 x i32], align 4
  %index.slot.15 = alloca [2 x i32], align 4
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %v.0, i32 0, i32 2
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load { i32, [2 x i32] }, ptr %v.2, align 4
  store { i32, [2 x i32] } %v.3, ptr %local.1, align 4
  %v.4 = load { i32, [2 x i32] }, ptr %local.1, align 4
%v.5 = extractvalue { i32, [2 x i32] } %v.4, 1
  %v.6 = add i32 0, 0
  %tmp.0 = sext i32 %v.6 to i64
  %tmp.1 = icmp ult i64 %tmp.0, 2
  br i1 %tmp.1, label %index.ok.0, label %zil.trap
index.ok.0:
  store [2 x i32] %v.5, ptr %index.slot.7, align 4
  %tmp.2 = getelementptr [2 x i32], ptr %index.slot.7, i64 0, i64 %tmp.0
  %v.7 = load i32, ptr %tmp.2, align 4
  %v.8 = load { i32, [2 x i32] }, ptr %local.1, align 4
%v.9 = extractvalue { i32, [2 x i32] } %v.8, 0
  %v.10 = icmp ne i32 %v.7, %v.9
  br i1 %v.10, label %bb.1, label %bb.2
bb.1:
  %v.11 = or i1 false, true
  store i1 %v.11, ptr %local.2, align 1
  br label %bb.3
bb.2:
  %v.12 = load { i32, [2 x i32] }, ptr %local.1, align 4
%v.13 = extractvalue { i32, [2 x i32] } %v.12, 1
  %v.14 = add i32 0, 1
  %tmp.3 = sext i32 %v.14 to i64
  %tmp.4 = icmp ult i64 %tmp.3, 2
  br i1 %tmp.4, label %index.ok.1, label %zil.trap
index.ok.1:
  store [2 x i32] %v.13, ptr %index.slot.15, align 4
  %tmp.5 = getelementptr [2 x i32], ptr %index.slot.15, i64 0, i64 %tmp.3
  %v.15 = load i32, ptr %tmp.5, align 4
  %v.16 = load { i32, [2 x i32] }, ptr %local.1, align 4
%v.17 = extractvalue { i32, [2 x i32] } %v.16, 0
  %v.18 = add i32 0, 1
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.17, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.2
checked.2:
  %v.20 = icmp ne i32 %v.15, %v.19
  store i1 %v.20, ptr %local.2, align 1
  br label %bb.3
bb.3:
  %v.21 = load i1, ptr %local.2, align 1
  br i1 %v.21, label %bb.4, label %bb.5
bb.4:
  %v.22 = add i32 0, -1000
  ret i32 %v.22
bb.5:
  br label %bb.6
bb.6:
  %v.23 = load { i32, [2 x i32] }, ptr %local.1, align 4
%v.24 = extractvalue { i32, [2 x i32] } %v.23, 0
  ret i32 %v.24
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.9(ptr %arg.0) {
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
  %v.8 = getelementptr { { i32 }, { ptr, i64, i64 }, { i32, [2 x i32] } }, ptr %tmp.5, i64 %v.7
  store ptr %v.8, ptr %local.3, align 8
  %v.9 = load i32, ptr %local.2, align 4
  %v.10 = load ptr, ptr %local.3, align 8
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = call i32 @zil.fn.8(ptr %v.11)
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

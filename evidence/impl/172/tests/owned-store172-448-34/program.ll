; ZIL verified LLVM module
; verified-fingerprint: 2447d46e348e573ab6b0d1f67f0078b61888fea6b53edb213e148e698fab8165
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
  %local.1 = alloca { i1, { { i32 }, i32 }, {} }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i1, { { i32 }, i32 }, {} }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { i32 }, i32 }, align 4
  br label %bb.0
bb.0:
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
  ret { i1, {}, i32 } %v.6
bb.2:
  br label %bb.3
bb.3:
  %v.7 = getelementptr i8, ptr %local.0, i64 0
  %v.8 = add i32 0, 65
  %v.9 = call { { i32 }, i32 } @zil.fn.2(i32 %v.8)
  %v.10 = call { i1, { { i32 }, i32 }, {} } @zil.store.1.push(ptr %v.7, { { i32 }, i32 } %v.9)
  store { i1, { { i32 }, i32 }, {} } %v.10, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %error.drop.1, label %error.done.1
error.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.3 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.1, i32 0, i32 0
  %tmp.4 = load i1, ptr %tmp.3, align 1
  br i1 %tmp.4, label %error.payload.1, label %success.payload.1
error.payload.1:
  %tmp.5 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.1, i32 0, i32 1
  %tmp.6 = getelementptr { { i32 }, i32 }, ptr %tmp.5, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.6)
  br label %error.done.1
success.payload.1:
  br label %error.done.1
error.done.1:
  %v.11 = getelementptr i8, ptr %local.0, i64 0
  %v.12 = add i32 0, 66
  %v.13 = call { { i32 }, i32 } @zil.fn.2(i32 %v.12)
  %v.14 = call { i1, { { i32 }, i32 }, {} } @zil.store.1.push(ptr %v.11, { { i32 }, i32 } %v.13)
  store { i1, { { i32 }, i32 }, {} } %v.14, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %tmp.7 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.7, label %error.drop.2, label %error.done.2
error.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.8 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.2, i32 0, i32 0
  %tmp.9 = load i1, ptr %tmp.8, align 1
  br i1 %tmp.9, label %error.payload.2, label %success.payload.2
error.payload.2:
  %tmp.10 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.2, i32 0, i32 1
  %tmp.11 = getelementptr { { i32 }, i32 }, ptr %tmp.10, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.11)
  br label %error.done.2
success.payload.2:
  br label %error.done.2
error.done.2:
  %v.15 = getelementptr i8, ptr %local.0, i64 0
  %v.16 = call { i1, {}, { { i32 }, i32 } } @zil.store.1.pop(ptr %v.15)
  %v.17 = extractvalue { i1, {}, { { i32 }, i32 } } %v.16, 0
  br i1 %v.17, label %bb.4, label %bb.5
bb.4:
  %tmp.12 = extractvalue { i1, {}, { { i32 }, i32 } } %v.16, 0
  %tmp.13 = icmp ne i1 %tmp.12, true
  br i1 %tmp.13, label %zil.trap, label %checked.3
checked.3:
  %tmp.14 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.19 = insertvalue { i1, {}, i32 } %tmp.14, {} zeroinitializer, 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.15 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.15, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.19
bb.5:
  %tmp.16 = extractvalue { i1, {}, { { i32 }, i32 } } %v.16, 0
  %tmp.17 = icmp ne i1 %tmp.16, false
  br i1 %tmp.17, label %zil.trap, label %checked.5
checked.5:
  %v.20 = extractvalue { i1, {}, { { i32 }, i32 } } %v.16, 2
  store i1 false, ptr %resource.live.3, align 1
  store { { i32 }, i32 } %v.20, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %tmp.18 = getelementptr { { i32 }, i32 }, ptr %local.3, i32 0, i32 1
  %v.21 = load i32, ptr %tmp.18, align 4
  %v.22 = add i32 0, 66
  %tmp.19 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.21, i32 %v.22)
  %v.23 = extractvalue { i32, i1 } %tmp.19, 0
  %tmp.20 = extractvalue { i32, i1 } %tmp.19, 1
  br i1 %tmp.20, label %zil.trap, label %checked.6
checked.6:
  %tmp.21 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.24 = insertvalue { i1, {}, i32 } %tmp.21, i32 %v.23, 2
  %tmp.22 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.22, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.23 = getelementptr { { i32 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.23)
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.24 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.24, label %resource.drop.8, label %resource.done.8
resource.drop.8:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.8
resource.done.8:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, {}, i32 } %v.24
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

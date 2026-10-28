; ZIL verified LLVM module
; verified-fingerprint: d2d99781aa412f700bb189df15f8ea6fa81b8e57f575054681fdd25790ccd346
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
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
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

define i32 @zil.fn.4() {
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
  %local.5 = alloca { { i32 }, i32 }, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i1, { { i32 }, i32 }, {} }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %v.0 = call { ptr, i64, i64 } @zil.fn.5()
  store i1 false, ptr %resource.live.0, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = add i64 0, 3
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
  %v.14 = getelementptr i8, ptr %local.0, i64 0
  %v.15 = add i32 0, 67
  %v.16 = call { { i32 }, i32 } @zil.fn.2(i32 %v.15)
  store { { i32 }, i32 } %v.16, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.17 = load { { i32 }, i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  %v.18 = call { i1, { { i32 }, i32 }, {} } @zil.fn.7(ptr %v.14, { { i32 }, i32 } %v.17)
  store { i1, { { i32 }, i32 }, {} } %v.18, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %tmp.14 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.14, label %error.drop.4, label %error.done.4
error.drop.4:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.15 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.6, i32 0, i32 0
  %tmp.16 = load i1, ptr %tmp.15, align 1
  br i1 %tmp.16, label %error.payload.4, label %success.payload.4
error.payload.4:
  %tmp.17 = getelementptr { i1, { { i32 }, i32 }, {} }, ptr %local.6, i32 0, i32 1
  %tmp.18 = getelementptr { { i32 }, i32 }, ptr %tmp.17, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.18)
  br label %error.done.4
success.payload.4:
  br label %error.done.4
error.done.4:
  %tmp.19 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.19, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.20 = getelementptr { { i32 }, i32 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.20)
  br label %package.done.5
package.done.5:
  %v.19 = getelementptr i8, ptr %local.0, i64 0
  %v.20 = call i64 @zil.fn.8(ptr %v.19)
  %tmp.21 = icmp slt i64 %v.20, -2147483648
  %tmp.22 = icmp sgt i64 %v.20, 2147483647
  %tmp.23 = or i1 %tmp.21, %tmp.22
  br i1 %tmp.23, label %zil.trap, label %checked.6
checked.6:
  %v.21 = trunc i64 %v.20 to i32
  %v.22 = getelementptr i8, ptr %local.0, i64 0
  %v.23 = call i64 @zil.fn.9(ptr %v.22)
  %tmp.24 = icmp slt i64 %v.23, -2147483648
  %tmp.25 = icmp sgt i64 %v.23, 2147483647
  %tmp.26 = or i1 %tmp.24, %tmp.25
  br i1 %tmp.26, label %zil.trap, label %checked.7
checked.7:
  %v.24 = trunc i64 %v.23 to i32
  %tmp.27 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.21, i32 %v.24)
  %v.25 = extractvalue { i32, i1 } %tmp.27, 0
  %tmp.28 = extractvalue { i32, i1 } %tmp.27, 1
  br i1 %tmp.28, label %zil.trap, label %checked.8
checked.8:
  %v.26 = add i32 0, 3
  %tmp.29 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.25, i32 %v.26)
  %v.27 = extractvalue { i32, i1 } %tmp.29, 0
  %tmp.30 = extractvalue { i32, i1 } %tmp.29, 1
  br i1 %tmp.30, label %zil.trap, label %checked.9
checked.9:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.31 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.31, label %resource.drop.10, label %resource.done.10
resource.drop.10:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.store.1.drop(ptr %local.0)
  br label %resource.done.10
resource.done.10:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.27
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

define i64 @zil.fn.8(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i1, {}, { { i32 }, i32 } }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i64 0, 0
  store i64 %v.0, ptr %local.1, align 8
  br label %bb.1
bb.1:
  %v.1 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.1, i32 0, i32 1
  %v.2 = load i64, ptr %tmp.0, align 8
  %v.3 = add i64 0, 0
  %v.4 = icmp sgt i64 %v.2, %v.3
  br i1 %v.4, label %bb.2, label %bb.3
bb.2:
  store i1 false, ptr %resource.live.2, align 1
  %v.5 = load ptr, ptr %local.0, align 8
  %v.6 = call { i1, {}, { { i32 }, i32 } } @zil.store.1.pop(ptr %v.5)
  store { i1, {}, { { i32 }, i32 } } %v.6, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %tmp.1 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.1, label %error.drop.0, label %error.done.0
error.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.2 = getelementptr { i1, {}, { { i32 }, i32 } }, ptr %local.2, i32 0, i32 0
  %tmp.3 = load i1, ptr %tmp.2, align 1
  br i1 %tmp.3, label %error.payload.0, label %success.payload.0
error.payload.0:
  br label %error.done.0
success.payload.0:
  %tmp.4 = getelementptr { i1, {}, { { i32 }, i32 } }, ptr %local.2, i32 0, i32 2
  %tmp.5 = getelementptr { { i32 }, i32 }, ptr %tmp.4, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.5)
  br label %error.done.0
error.done.0:
  %v.7 = load i64, ptr %local.1, align 8
  %v.8 = add i64 0, 1
  %tmp.6 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.7, i64 %v.8)
  %v.9 = extractvalue { i64, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i64, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.1
checked.1:
  store i64 %v.9, ptr %local.1, align 8
  store i1 false, ptr %resource.live.2, align 1
  br label %bb.1
bb.3:
  %v.10 = load i64, ptr %local.1, align 8
  ret i64 %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i64 @zil.fn.9(ptr %arg.0) {
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
  %entry.result = call i32 @zil.fn.4()
  ret i32 %entry.result
}

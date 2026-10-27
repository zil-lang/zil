; ZIL verified LLVM module
; verified-fingerprint: 05edeba45ef975ad84ab846b9eee2f070231f5b21710a440a99916b19ab7c5c3
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32, { i32 }, i32, i1 }
%zil_env_1 = type { %zil_env_0, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  call void @zil.fn.3(ptr %payload.1)
  br label %done.1
done.1:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %child.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  call void @zil.closure.drop.0(ptr %child.0)
  br label %done.0
done.0:
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
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @getchar() nonlazybind
define { i1, {}, i32 } @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, align 8
  %package.live.5.0 = alloca i1, align 1
  store i1 false, ptr %package.live.5.0, align 1
  %package.live.5.1 = alloca i1, align 1
  store i1 false, ptr %package.live.5.1, align 1
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca [2 x { { i32 }, { ptr, i64, i64 }, i32 }], align 8
  %array.live.6.0 = alloca i1, align 1
  store i1 false, ptr %array.live.6.0, align 1
  %array.live.6.1 = alloca i1, align 1
  store i1 false, ptr %array.live.6.1, align 1
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca [2 x { { i32 }, { ptr, i64, i64 }, i32 }], align 8
  %array.live.9.0 = alloca i1, align 1
  store i1 false, ptr %array.live.9.0, align 1
  %array.live.9.1 = alloca i1, align 1
  store i1 false, ptr %array.live.9.1, align 1
  %resource.live.10 = alloca i1, align 1
  store i1 false, ptr %resource.live.10, align 1
  %local.10 = alloca { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, align 8
  %resource.live.11 = alloca i1, align 1
  store i1 false, ptr %resource.live.11, align 1
  %local.11 = alloca { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, align 8
  %resource.live.12 = alloca i1, align 1
  store i1 false, ptr %resource.live.12, align 1
  %local.12 = alloca { ptr, i64, i64 }, align 8
  %resource.live.13 = alloca i1, align 1
  store i1 false, ptr %resource.live.13, align 1
  %local.13 = alloca { ptr, i64, i64 }, align 8
  %resource.live.14 = alloca i1, align 1
  store i1 false, ptr %resource.live.14, align 1
  %local.14 = alloca { ptr, i64, i64 }, align 8
  %local.15 = alloca i32, align 4
  %resource.live.16 = alloca i1, align 1
  store i1 false, ptr %resource.live.16, align 1
  %local.16 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.17 = alloca i1, align 1
  store i1 false, ptr %resource.live.17, align 1
  %local.17 = alloca { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} }, align 8
  %local.18 = alloca i32, align 4
  %resource.live.19 = alloca i1, align 1
  store i1 false, ptr %resource.live.19, align 1
  %local.19 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %local.20 = alloca i1, align 1
  %resource.live.21 = alloca i1, align 1
  store i1 false, ptr %resource.live.21, align 1
  %local.21 = alloca %zil_env_1, align 4
  %local.22 = alloca i1, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %array.live.9.0, align 1
  store i1 false, ptr %array.live.9.1, align 1
  store i1 false, ptr %resource.live.8, align 1
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
  %v.7 = add i32 0, 52
  %v.8 = icmp sgt i32 %v.6, %v.7
  store i1 %v.8, ptr %local.1, align 1
  br label %bb.3
bb.3:
  %v.9 = load i1, ptr %local.1, align 1
  br i1 %v.9, label %bb.4, label %bb.5
bb.4:
  %v.10 = add i32 0, 9
  %tmp.0 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.11 = insertvalue { i1, {}, i32 } %tmp.0, i32 %v.10, 2
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %array.live.9.0, align 1
  store i1 false, ptr %array.live.9.1, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.13, align 1
  ret { i1, {}, i32 } %v.11
bb.5:
  br label %bb.6
bb.6:
  %v.12 = load i32, ptr %local.0, align 4
  %v.13 = add i32 0, 48
  %tmp.1 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.12, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.14, ptr %local.2, align 4
  %v.15 = load i32, ptr %local.2, align 4
%v.16 = insertvalue { i32 } zeroinitializer, i32 %v.15, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.16, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.17 = getelementptr i8, ptr %local.3, i64 0
  %v.18 = getelementptr i8, ptr %v.17, i64 0
  %v.19 = call i32 @zil.fn.4(ptr %v.18)
  store i32 %v.19, ptr %local.4, align 4
  %v.20 = call { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } @zil.fn.8()
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %package.live.5.0, align 1
  store i1 false, ptr %package.live.5.1, align 1
  store { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %v.20, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  store i1 true, ptr %package.live.5.0, align 1
  store i1 true, ptr %package.live.5.1, align 1
  %tmp.3 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1
  %v.21 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %tmp.3, align 8
  store i1 false, ptr %package.live.5.1, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %array.live.6.1, align 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.21, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  store i1 true, ptr %array.live.6.0, align 1
  store i1 true, ptr %array.live.6.1, align 1
  %tmp.4 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0
  %v.22 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.4, align 8
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %resource.live.7, align 1
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.22, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.23 = add i32 0, 67
  %v.24 = call { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.5(i32 %v.23)
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.24, ptr %local.8, align 8
  store i1 true, ptr %resource.live.8, align 1
  %tmp.5 = load i1, ptr %array.live.6.0, align 1
  br i1 %tmp.5, label %array.element.drop.1, label %array.element.done.1
array.element.drop.1:
  store i1 false, ptr %array.live.6.0, align 1
  %tmp.6 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.6)
  %tmp.7 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.7)
  br label %array.element.done.1
array.element.done.1:
  %v.25 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.8, align 8
  store i1 false, ptr %resource.live.8, align 1
  %tmp.8 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.25, ptr %tmp.8, align 8
  store i1 true, ptr %array.live.6.0, align 1
  %tmp.9 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.9, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.10 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.8, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.10)
  %tmp.11 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.8, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.11)
  br label %package.done.2
package.done.2:
  %v.26 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %array.live.6.1, align 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.26, ptr %local.9, align 8
  store i1 true, ptr %resource.live.9, align 1
  store i1 true, ptr %array.live.9.0, align 1
  store i1 true, ptr %array.live.9.1, align 1
  %tmp.12 = load i1, ptr %package.live.5.1, align 1
  br i1 %tmp.12, label %package.field.drop.3, label %package.field.done.3
package.field.drop.3:
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.13 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.13)
  %tmp.14 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.14)
  %tmp.15 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.15)
  %tmp.16 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.16)
  br label %package.field.done.3
package.field.done.3:
  %v.27 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.9, align 8
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %array.live.9.0, align 1
  store i1 false, ptr %array.live.9.1, align 1
  %tmp.17 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.27, ptr %tmp.17, align 8
  store i1 true, ptr %package.live.5.1, align 1
  %tmp.18 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.18, label %array.drop.4, label %array.done.4
array.drop.4:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.19 = load i1, ptr %array.live.9.0, align 1
  br i1 %tmp.19, label %array.element.drop.5, label %array.element.done.5
array.element.drop.5:
  store i1 false, ptr %array.live.9.0, align 1
  %tmp.20 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.9, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.20)
  %tmp.21 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.9, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.21)
  br label %array.element.done.5
array.element.done.5:
  %tmp.22 = load i1, ptr %array.live.9.1, align 1
  br i1 %tmp.22, label %array.element.drop.6, label %array.element.done.6
array.element.drop.6:
  store i1 false, ptr %array.live.9.1, align 1
  %tmp.23 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.9, i32 0, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.23)
  %tmp.24 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.9, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.24)
  br label %array.element.done.6
array.element.done.6:
  br label %array.done.4
array.done.4:
  %v.28 = load { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %package.live.5.0, align 1
  store i1 false, ptr %package.live.5.1, align 1
  store { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %v.28, ptr %local.10, align 8
  store i1 true, ptr %resource.live.10, align 1
  %v.29 = load { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.10, align 8
  store i1 false, ptr %resource.live.10, align 1
  %v.30 = call { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } @zil.fn.7({ { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %v.29)
  %tmp.25 = load i1, ptr %resource.live.10, align 1
  br i1 %tmp.25, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.10, align 1
  %tmp.26 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.10, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.26)
  %tmp.27 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.10, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.27)
  %tmp.28 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.10, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.28)
  %tmp.29 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.10, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.29)
  %tmp.30 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.10, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.30)
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.11, align 1
  store { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %v.30, ptr %local.11, align 8
  store i1 true, ptr %resource.live.11, align 1
  %v.31 = call { ptr, i64, i64 } @zil.fn.10()
  store i1 false, ptr %resource.live.12, align 1
  store { ptr, i64, i64 } %v.31, ptr %local.12, align 8
  store i1 true, ptr %resource.live.12, align 1
  %v.32 = load { ptr, i64, i64 }, ptr %local.12, align 8
  store i1 false, ptr %resource.live.12, align 1
  store { ptr, i64, i64 } %v.32, ptr %local.13, align 8
  store i1 true, ptr %resource.live.13, align 1
  %v.33 = load { ptr, i64, i64 }, ptr %local.13, align 8
  store i1 false, ptr %resource.live.13, align 1
  %v.34 = call { ptr, i64, i64 } @zil.fn.11({ ptr, i64, i64 } %v.33)
  %tmp.31 = load i1, ptr %resource.live.13, align 1
  br i1 %tmp.31, label %resource.drop.8, label %resource.done.8
resource.drop.8:
  store i1 false, ptr %resource.live.13, align 1
  call void @zil.store.3.drop(ptr %local.13)
  br label %resource.done.8
resource.done.8:
  store i1 false, ptr %resource.live.14, align 1
  store { ptr, i64, i64 } %v.34, ptr %local.14, align 8
  store i1 true, ptr %resource.live.14, align 1
  %v.35 = getelementptr i8, ptr %local.14, i64 0
  %v.36 = load i32, ptr %local.4, align 4
  %v.37 = sext i32 %v.36 to i64
  %v.38 = call i1 @zil.store.3.reserve(ptr %v.35, i64 %v.37)
  %v.39 = xor i1 %v.38, true
  br i1 %v.39, label %bb.7, label %bb.8
bb.7:
  %tmp.32 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.41 = insertvalue { i1, {}, i32 } %tmp.32, {} zeroinitializer, 1
  %tmp.33 = load i1, ptr %resource.live.14, align 1
  br i1 %tmp.33, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.14, align 1
  call void @zil.store.3.drop(ptr %local.14)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.13, align 1
  %tmp.34 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.34, label %resource.drop.10, label %resource.done.10
resource.drop.10:
  store i1 false, ptr %resource.live.12, align 1
  call void @zil.store.3.drop(ptr %local.12)
  br label %resource.done.10
resource.done.10:
  store i1 false, ptr %resource.live.12, align 1
  %tmp.35 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.35, label %package.drop.11, label %package.done.11
package.drop.11:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.36 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.36)
  %tmp.37 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.37)
  %tmp.38 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.38)
  %tmp.39 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.39)
  %tmp.40 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.40)
  br label %package.done.11
package.done.11:
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %array.live.9.0, align 1
  store i1 false, ptr %array.live.9.1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.41 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.41, label %package.drop.12, label %package.done.12
package.drop.12:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.42 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.42)
  %tmp.43 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.43)
  br label %package.done.12
package.done.12:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.44 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.44, label %array.drop.13, label %array.done.13
array.drop.13:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.45 = load i1, ptr %array.live.6.0, align 1
  br i1 %tmp.45, label %array.element.drop.14, label %array.element.done.14
array.element.drop.14:
  store i1 false, ptr %array.live.6.0, align 1
  %tmp.46 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.46)
  %tmp.47 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.47)
  br label %array.element.done.14
array.element.done.14:
  %tmp.48 = load i1, ptr %array.live.6.1, align 1
  br i1 %tmp.48, label %array.element.drop.15, label %array.element.done.15
array.element.drop.15:
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.49 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.49)
  %tmp.50 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.50)
  br label %array.element.done.15
array.element.done.15:
  br label %array.done.13
array.done.13:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.51 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.51, label %package.drop.16, label %package.done.16
package.drop.16:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.52 = load i1, ptr %package.live.5.0, align 1
  br i1 %tmp.52, label %package.field.drop.17, label %package.field.done.17
package.field.drop.17:
  store i1 false, ptr %package.live.5.0, align 1
  %tmp.53 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.53)
  br label %package.field.done.17
package.field.done.17:
  %tmp.54 = load i1, ptr %package.live.5.1, align 1
  br i1 %tmp.54, label %package.field.drop.18, label %package.field.done.18
package.field.drop.18:
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.55 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.55)
  %tmp.56 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.56)
  %tmp.57 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.57)
  %tmp.58 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.58)
  br label %package.field.done.18
package.field.done.18:
  br label %package.done.16
package.done.16:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %package.live.5.0, align 1
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.59 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.59, label %resource.drop.19, label %resource.done.19
resource.drop.19:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.19
resource.done.19:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, {}, i32 } %v.41
bb.8:
  br label %bb.9
bb.9:
  %v.42 = add i32 0, 0
  store i32 %v.42, ptr %local.15, align 4
  br label %bb.10
bb.10:
  %v.43 = load i32, ptr %local.15, align 4
  %v.44 = load i32, ptr %local.4, align 4
  %v.45 = icmp slt i32 %v.43, %v.44
  br i1 %v.45, label %bb.11, label %bb.12
bb.11:
  store i1 false, ptr %resource.live.17, align 1
  store i1 false, ptr %resource.live.16, align 1
  %v.46 = getelementptr i8, ptr %local.14, i64 0
  %v.47 = add i32 0, 68
  %v.48 = load i32, ptr %local.15, align 4
  %tmp.60 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.47, i32 %v.48)
  %v.49 = extractvalue { i32, i1 } %tmp.60, 0
  %tmp.61 = extractvalue { i32, i1 } %tmp.60, 1
  br i1 %tmp.61, label %zil.trap, label %checked.20
checked.20:
  %v.50 = call { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.5(i32 %v.49)
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.50, ptr %local.16, align 8
  store i1 true, ptr %resource.live.16, align 1
  %v.51 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.16, align 8
  store i1 false, ptr %resource.live.16, align 1
  %v.52 = call { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.fn.12(ptr %v.46, { { i32 }, { ptr, i64, i64 }, i32 } %v.51)
  store { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } %v.52, ptr %local.17, align 8
  store i1 true, ptr %resource.live.17, align 1
  %tmp.62 = load i1, ptr %resource.live.17, align 1
  br i1 %tmp.62, label %error.drop.21, label %error.done.21
error.drop.21:
  store i1 false, ptr %resource.live.17, align 1
  %tmp.63 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} }, ptr %local.17, i32 0, i32 0
  %tmp.64 = load i1, ptr %tmp.63, align 1
  br i1 %tmp.64, label %error.payload.21, label %success.payload.21
error.payload.21:
  %tmp.65 = getelementptr { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} }, ptr %local.17, i32 0, i32 1
  %tmp.66 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.65, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.66)
  %tmp.67 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.65, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.67)
  br label %error.done.21
success.payload.21:
  br label %error.done.21
error.done.21:
  %tmp.68 = load i1, ptr %resource.live.16, align 1
  br i1 %tmp.68, label %package.drop.22, label %package.done.22
package.drop.22:
  store i1 false, ptr %resource.live.16, align 1
  %tmp.69 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.16, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.69)
  %tmp.70 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.16, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.70)
  br label %package.done.22
package.done.22:
  %v.53 = load i32, ptr %local.15, align 4
  %v.54 = add i32 0, 1
  %tmp.71 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.53, i32 %v.54)
  %v.55 = extractvalue { i32, i1 } %tmp.71, 0
  %tmp.72 = extractvalue { i32, i1 } %tmp.71, 1
  br i1 %tmp.72, label %zil.trap, label %checked.23
checked.23:
  store i32 %v.55, ptr %local.15, align 4
  store i1 false, ptr %resource.live.17, align 1
  store i1 false, ptr %resource.live.16, align 1
  br label %bb.10
bb.12:
  %v.56 = getelementptr i8, ptr %local.14, i64 0
  %tmp.73 = getelementptr { ptr, i64, i64 }, ptr %v.56, i32 0, i32 1
  %v.57 = load i64, ptr %tmp.73, align 8
  %v.58 = load i32, ptr %local.4, align 4
  %v.59 = sext i32 %v.58 to i64
  %v.60 = icmp ne i64 %v.57, %v.59
  br i1 %v.60, label %bb.13, label %bb.14
bb.13:
  %v.61 = add i32 0, 2
  %tmp.74 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.62 = insertvalue { i1, {}, i32 } %tmp.74, i32 %v.61, 2
  %tmp.75 = load i1, ptr %resource.live.14, align 1
  br i1 %tmp.75, label %resource.drop.24, label %resource.done.24
resource.drop.24:
  store i1 false, ptr %resource.live.14, align 1
  call void @zil.store.3.drop(ptr %local.14)
  br label %resource.done.24
resource.done.24:
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.13, align 1
  %tmp.76 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.76, label %resource.drop.25, label %resource.done.25
resource.drop.25:
  store i1 false, ptr %resource.live.12, align 1
  call void @zil.store.3.drop(ptr %local.12)
  br label %resource.done.25
resource.done.25:
  store i1 false, ptr %resource.live.12, align 1
  %tmp.77 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.77, label %package.drop.26, label %package.done.26
package.drop.26:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.78 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.78)
  %tmp.79 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.79)
  %tmp.80 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.80)
  %tmp.81 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.81)
  %tmp.82 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.82)
  br label %package.done.26
package.done.26:
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %array.live.9.0, align 1
  store i1 false, ptr %array.live.9.1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.83 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.83, label %package.drop.27, label %package.done.27
package.drop.27:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.84 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.84)
  %tmp.85 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.85)
  br label %package.done.27
package.done.27:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.86 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.86, label %array.drop.28, label %array.done.28
array.drop.28:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.87 = load i1, ptr %array.live.6.0, align 1
  br i1 %tmp.87, label %array.element.drop.29, label %array.element.done.29
array.element.drop.29:
  store i1 false, ptr %array.live.6.0, align 1
  %tmp.88 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.88)
  %tmp.89 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.89)
  br label %array.element.done.29
array.element.done.29:
  %tmp.90 = load i1, ptr %array.live.6.1, align 1
  br i1 %tmp.90, label %array.element.drop.30, label %array.element.done.30
array.element.drop.30:
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.91 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.91)
  %tmp.92 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.92)
  br label %array.element.done.30
array.element.done.30:
  br label %array.done.28
array.done.28:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.93 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.93, label %package.drop.31, label %package.done.31
package.drop.31:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.94 = load i1, ptr %package.live.5.0, align 1
  br i1 %tmp.94, label %package.field.drop.32, label %package.field.done.32
package.field.drop.32:
  store i1 false, ptr %package.live.5.0, align 1
  %tmp.95 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.95)
  br label %package.field.done.32
package.field.done.32:
  %tmp.96 = load i1, ptr %package.live.5.1, align 1
  br i1 %tmp.96, label %package.field.drop.33, label %package.field.done.33
package.field.drop.33:
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.97 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.97)
  %tmp.98 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.98)
  %tmp.99 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.99)
  %tmp.100 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.100)
  br label %package.field.done.33
package.field.done.33:
  br label %package.done.31
package.done.31:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %package.live.5.0, align 1
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.101 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.101, label %resource.drop.34, label %resource.done.34
resource.drop.34:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.34
resource.done.34:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, {}, i32 } %v.62
bb.14:
  br label %bb.15
bb.15:
  %v.63 = add i32 0, 0
  store i32 %v.63, ptr %local.18, align 4
  br label %bb.16
bb.16:
  %v.64 = getelementptr i8, ptr %local.14, i64 0
  %tmp.102 = getelementptr { ptr, i64, i64 }, ptr %v.64, i32 0, i32 1
  %v.65 = load i64, ptr %tmp.102, align 8
  %v.66 = add i64 0, 0
  %v.67 = icmp sgt i64 %v.65, %v.66
  br i1 %v.67, label %bb.17, label %bb.18
bb.17:
  %v.68 = getelementptr i8, ptr %local.14, i64 0
  %v.69 = call { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.13(ptr %v.68)
  %v.70 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.69, 0
  br i1 %v.70, label %bb.19, label %bb.20
bb.18:
  %v.77 = load i32, ptr %local.18, align 4
  %v.78 = load i32, ptr %local.4, align 4
  %v.79 = add i32 0, 68
  %tmp.103 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.78, i32 %v.79)
  %v.80 = extractvalue { i32, i1 } %tmp.103, 0
  %tmp.104 = extractvalue { i32, i1 } %tmp.103, 1
  br i1 %tmp.104, label %zil.trap, label %checked.35
checked.35:
  %v.81 = load i32, ptr %local.4, align 4
  %v.82 = load i32, ptr %local.4, align 4
  %v.83 = add i32 0, 1
  %tmp.105 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.82, i32 %v.83)
  %v.84 = extractvalue { i32, i1 } %tmp.105, 0
  %tmp.106 = extractvalue { i32, i1 } %tmp.105, 1
  br i1 %tmp.106, label %zil.trap, label %checked.36
checked.36:
  %tmp.107 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.81, i32 %v.84)
  %v.85 = extractvalue { i32, i1 } %tmp.107, 0
  %tmp.108 = extractvalue { i32, i1 } %tmp.107, 1
  br i1 %tmp.108, label %zil.trap, label %checked.37
checked.37:
  %v.86 = add i32 0, 2
  %tmp.109 = icmp eq i32 %v.86, 0
  %tmp.110 = icmp eq i32 %v.85, -2147483648
  %tmp.111 = icmp eq i32 %v.86, -1
  %tmp.112 = and i1 %tmp.110, %tmp.111
  %tmp.113 = or i1 %tmp.109, %tmp.112
  br i1 %tmp.113, label %zil.trap, label %checked.38
checked.38:
  %v.87 = sdiv i32 %v.85, %v.86
  %tmp.114 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.80, i32 %v.87)
  %v.88 = extractvalue { i32, i1 } %tmp.114, 0
  %tmp.115 = extractvalue { i32, i1 } %tmp.114, 1
  br i1 %tmp.115, label %zil.trap, label %checked.39
checked.39:
  %v.89 = icmp ne i32 %v.77, %v.88
  br i1 %v.89, label %bb.21, label %bb.22
bb.19:
  %tmp.116 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.69, 0
  %tmp.117 = icmp ne i1 %tmp.116, true
  br i1 %tmp.117, label %zil.trap, label %checked.40
checked.40:
  %tmp.118 = insertvalue { i1, {}, i32 } zeroinitializer, i1 true, 0
  %v.72 = insertvalue { i1, {}, i32 } %tmp.118, {} zeroinitializer, 1
  %tmp.119 = load i1, ptr %resource.live.14, align 1
  br i1 %tmp.119, label %resource.drop.41, label %resource.done.41
resource.drop.41:
  store i1 false, ptr %resource.live.14, align 1
  call void @zil.store.3.drop(ptr %local.14)
  br label %resource.done.41
resource.done.41:
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.13, align 1
  %tmp.120 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.120, label %resource.drop.42, label %resource.done.42
resource.drop.42:
  store i1 false, ptr %resource.live.12, align 1
  call void @zil.store.3.drop(ptr %local.12)
  br label %resource.done.42
resource.done.42:
  store i1 false, ptr %resource.live.12, align 1
  %tmp.121 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.121, label %package.drop.43, label %package.done.43
package.drop.43:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.122 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.122)
  %tmp.123 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.123)
  %tmp.124 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.124)
  %tmp.125 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.125)
  %tmp.126 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.126)
  br label %package.done.43
package.done.43:
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %array.live.9.0, align 1
  store i1 false, ptr %array.live.9.1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.127 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.127, label %package.drop.44, label %package.done.44
package.drop.44:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.128 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.128)
  %tmp.129 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.129)
  br label %package.done.44
package.done.44:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.130 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.130, label %array.drop.45, label %array.done.45
array.drop.45:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.131 = load i1, ptr %array.live.6.0, align 1
  br i1 %tmp.131, label %array.element.drop.46, label %array.element.done.46
array.element.drop.46:
  store i1 false, ptr %array.live.6.0, align 1
  %tmp.132 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.132)
  %tmp.133 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.133)
  br label %array.element.done.46
array.element.done.46:
  %tmp.134 = load i1, ptr %array.live.6.1, align 1
  br i1 %tmp.134, label %array.element.drop.47, label %array.element.done.47
array.element.drop.47:
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.135 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.135)
  %tmp.136 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.136)
  br label %array.element.done.47
array.element.done.47:
  br label %array.done.45
array.done.45:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.137 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.137, label %package.drop.48, label %package.done.48
package.drop.48:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.138 = load i1, ptr %package.live.5.0, align 1
  br i1 %tmp.138, label %package.field.drop.49, label %package.field.done.49
package.field.drop.49:
  store i1 false, ptr %package.live.5.0, align 1
  %tmp.139 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.139)
  br label %package.field.done.49
package.field.done.49:
  %tmp.140 = load i1, ptr %package.live.5.1, align 1
  br i1 %tmp.140, label %package.field.drop.50, label %package.field.done.50
package.field.drop.50:
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.141 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.141)
  %tmp.142 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.142)
  %tmp.143 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.143)
  %tmp.144 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.144)
  br label %package.field.done.50
package.field.done.50:
  br label %package.done.48
package.done.48:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %package.live.5.0, align 1
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.145 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.145, label %resource.drop.51, label %resource.done.51
resource.drop.51:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.51
resource.done.51:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, {}, i32 } %v.72
bb.20:
  %tmp.146 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.69, 0
  %tmp.147 = icmp ne i1 %tmp.146, false
  br i1 %tmp.147, label %zil.trap, label %checked.52
checked.52:
  %v.73 = extractvalue { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.69, 2
  store i1 false, ptr %resource.live.19, align 1
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.73, ptr %local.19, align 8
  store i1 true, ptr %resource.live.19, align 1
  %v.74 = load i32, ptr %local.18, align 4
  %tmp.148 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.19, i32 0, i32 2
  %v.75 = load i32, ptr %tmp.148, align 4
  %tmp.149 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.74, i32 %v.75)
  %v.76 = extractvalue { i32, i1 } %tmp.149, 0
  %tmp.150 = extractvalue { i32, i1 } %tmp.149, 1
  br i1 %tmp.150, label %zil.trap, label %checked.53
checked.53:
  store i32 %v.76, ptr %local.18, align 4
  %tmp.151 = load i1, ptr %resource.live.19, align 1
  br i1 %tmp.151, label %package.drop.54, label %package.done.54
package.drop.54:
  store i1 false, ptr %resource.live.19, align 1
  %tmp.152 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.19, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.152)
  %tmp.153 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.19, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.153)
  br label %package.done.54
package.done.54:
  store i1 false, ptr %resource.live.19, align 1
  br label %bb.16
bb.21:
  %v.90 = add i32 0, 3
  %tmp.154 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.91 = insertvalue { i1, {}, i32 } %tmp.154, i32 %v.90, 2
  %tmp.155 = load i1, ptr %resource.live.14, align 1
  br i1 %tmp.155, label %resource.drop.55, label %resource.done.55
resource.drop.55:
  store i1 false, ptr %resource.live.14, align 1
  call void @zil.store.3.drop(ptr %local.14)
  br label %resource.done.55
resource.done.55:
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.13, align 1
  %tmp.156 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.156, label %resource.drop.56, label %resource.done.56
resource.drop.56:
  store i1 false, ptr %resource.live.12, align 1
  call void @zil.store.3.drop(ptr %local.12)
  br label %resource.done.56
resource.done.56:
  store i1 false, ptr %resource.live.12, align 1
  %tmp.157 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.157, label %package.drop.57, label %package.done.57
package.drop.57:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.158 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.158)
  %tmp.159 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.159)
  %tmp.160 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.160)
  %tmp.161 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.161)
  %tmp.162 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.162)
  br label %package.done.57
package.done.57:
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %array.live.9.0, align 1
  store i1 false, ptr %array.live.9.1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.163 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.163, label %package.drop.58, label %package.done.58
package.drop.58:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.164 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.164)
  %tmp.165 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.165)
  br label %package.done.58
package.done.58:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.166 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.166, label %array.drop.59, label %array.done.59
array.drop.59:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.167 = load i1, ptr %array.live.6.0, align 1
  br i1 %tmp.167, label %array.element.drop.60, label %array.element.done.60
array.element.drop.60:
  store i1 false, ptr %array.live.6.0, align 1
  %tmp.168 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.168)
  %tmp.169 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.169)
  br label %array.element.done.60
array.element.done.60:
  %tmp.170 = load i1, ptr %array.live.6.1, align 1
  br i1 %tmp.170, label %array.element.drop.61, label %array.element.done.61
array.element.drop.61:
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.171 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.171)
  %tmp.172 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.172)
  br label %array.element.done.61
array.element.done.61:
  br label %array.done.59
array.done.59:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.173 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.173, label %package.drop.62, label %package.done.62
package.drop.62:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.174 = load i1, ptr %package.live.5.0, align 1
  br i1 %tmp.174, label %package.field.drop.63, label %package.field.done.63
package.field.drop.63:
  store i1 false, ptr %package.live.5.0, align 1
  %tmp.175 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.175)
  br label %package.field.done.63
package.field.done.63:
  %tmp.176 = load i1, ptr %package.live.5.1, align 1
  br i1 %tmp.176, label %package.field.drop.64, label %package.field.done.64
package.field.drop.64:
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.177 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.177)
  %tmp.178 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.178)
  %tmp.179 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.179)
  %tmp.180 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.180)
  br label %package.field.done.64
package.field.done.64:
  br label %package.done.62
package.done.62:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %package.live.5.0, align 1
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.181 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.181, label %resource.drop.65, label %resource.done.65
resource.drop.65:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.65
resource.done.65:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, {}, i32 } %v.91
bb.22:
  br label %bb.23
bb.23:
  %tmp.182 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 2
  %v.92 = load i32, ptr %tmp.182, align 4
  %v.93 = add i32 0, 1
  %v.94 = icmp ne i32 %v.92, %v.93
  br i1 %v.94, label %bb.24, label %bb.25
bb.24:
  %v.95 = or i1 false, true
  store i1 %v.95, ptr %local.20, align 1
  br label %bb.26
bb.25:
  %tmp.183 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 2
  %v.96 = load i32, ptr %tmp.183, align 4
  %v.97 = add i32 0, 65
  %v.98 = icmp ne i32 %v.96, %v.97
  store i1 %v.98, ptr %local.20, align 1
  br label %bb.26
bb.26:
  %v.99 = load i1, ptr %local.20, align 1
  br i1 %v.99, label %bb.27, label %bb.28
bb.27:
  %v.100 = add i32 0, 4
  %tmp.184 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.101 = insertvalue { i1, {}, i32 } %tmp.184, i32 %v.100, 2
  %tmp.185 = load i1, ptr %resource.live.14, align 1
  br i1 %tmp.185, label %resource.drop.66, label %resource.done.66
resource.drop.66:
  store i1 false, ptr %resource.live.14, align 1
  call void @zil.store.3.drop(ptr %local.14)
  br label %resource.done.66
resource.done.66:
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.13, align 1
  %tmp.186 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.186, label %resource.drop.67, label %resource.done.67
resource.drop.67:
  store i1 false, ptr %resource.live.12, align 1
  call void @zil.store.3.drop(ptr %local.12)
  br label %resource.done.67
resource.done.67:
  store i1 false, ptr %resource.live.12, align 1
  %tmp.187 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.187, label %package.drop.68, label %package.done.68
package.drop.68:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.188 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.188)
  %tmp.189 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.189)
  %tmp.190 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.190)
  %tmp.191 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.191)
  %tmp.192 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.192)
  br label %package.done.68
package.done.68:
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %array.live.9.0, align 1
  store i1 false, ptr %array.live.9.1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.193 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.193, label %package.drop.69, label %package.done.69
package.drop.69:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.194 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.194)
  %tmp.195 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.195)
  br label %package.done.69
package.done.69:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.196 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.196, label %array.drop.70, label %array.done.70
array.drop.70:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.197 = load i1, ptr %array.live.6.0, align 1
  br i1 %tmp.197, label %array.element.drop.71, label %array.element.done.71
array.element.drop.71:
  store i1 false, ptr %array.live.6.0, align 1
  %tmp.198 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.198)
  %tmp.199 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.199)
  br label %array.element.done.71
array.element.done.71:
  %tmp.200 = load i1, ptr %array.live.6.1, align 1
  br i1 %tmp.200, label %array.element.drop.72, label %array.element.done.72
array.element.drop.72:
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.201 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.201)
  %tmp.202 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.202)
  br label %array.element.done.72
array.element.done.72:
  br label %array.done.70
array.done.70:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.203 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.203, label %package.drop.73, label %package.done.73
package.drop.73:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.204 = load i1, ptr %package.live.5.0, align 1
  br i1 %tmp.204, label %package.field.drop.74, label %package.field.done.74
package.field.drop.74:
  store i1 false, ptr %package.live.5.0, align 1
  %tmp.205 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.205)
  br label %package.field.done.74
package.field.done.74:
  %tmp.206 = load i1, ptr %package.live.5.1, align 1
  br i1 %tmp.206, label %package.field.drop.75, label %package.field.done.75
package.field.drop.75:
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.207 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.207)
  %tmp.208 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.208)
  %tmp.209 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.209)
  %tmp.210 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.210)
  br label %package.field.done.75
package.field.done.75:
  br label %package.done.73
package.done.73:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %package.live.5.0, align 1
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.211 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.211, label %resource.drop.76, label %resource.done.76
resource.drop.76:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.76
resource.done.76:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, {}, i32 } %v.101
bb.28:
  br label %bb.29
bb.29:
  %v.102 = add i32 0, 72
  %v.103 = call %zil_env_1 @zil.fn.15(i32 %v.102)
  store i1 false, ptr %resource.live.21, align 1
  store %zil_env_1 %v.103, ptr %local.21, align 4
  store i1 true, ptr %resource.live.21, align 1
  %v.104 = call i32 @zil.fn.17(ptr %local.21)
  %v.105 = add i32 0, 73
  %v.106 = icmp ne i32 %v.104, %v.105
  br i1 %v.106, label %bb.30, label %bb.31
bb.30:
  %v.107 = or i1 false, true
  store i1 %v.107, ptr %local.22, align 1
  br label %bb.32
bb.31:
  %v.108 = call i32 @zil.fn.17(ptr %local.21)
  %v.109 = add i32 0, 74
  %v.110 = icmp ne i32 %v.108, %v.109
  store i1 %v.110, ptr %local.22, align 1
  br label %bb.32
bb.32:
  %v.111 = load i1, ptr %local.22, align 1
  br i1 %v.111, label %bb.33, label %bb.34
bb.33:
  %v.112 = add i32 0, 5
  %tmp.212 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.113 = insertvalue { i1, {}, i32 } %tmp.212, i32 %v.112, 2
  %tmp.213 = load i1, ptr %resource.live.21, align 1
  br i1 %tmp.213, label %closure.drop.77, label %closure.done.77
closure.drop.77:
  store i1 false, ptr %resource.live.21, align 1
  call void @zil.closure.drop.1(ptr %local.21)
  br label %closure.done.77
closure.done.77:
  store i1 false, ptr %resource.live.21, align 1
  %tmp.214 = load i1, ptr %resource.live.14, align 1
  br i1 %tmp.214, label %resource.drop.78, label %resource.done.78
resource.drop.78:
  store i1 false, ptr %resource.live.14, align 1
  call void @zil.store.3.drop(ptr %local.14)
  br label %resource.done.78
resource.done.78:
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.13, align 1
  %tmp.215 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.215, label %resource.drop.79, label %resource.done.79
resource.drop.79:
  store i1 false, ptr %resource.live.12, align 1
  call void @zil.store.3.drop(ptr %local.12)
  br label %resource.done.79
resource.done.79:
  store i1 false, ptr %resource.live.12, align 1
  %tmp.216 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.216, label %package.drop.80, label %package.done.80
package.drop.80:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.217 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.217)
  %tmp.218 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.218)
  %tmp.219 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.219)
  %tmp.220 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.220)
  %tmp.221 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.221)
  br label %package.done.80
package.done.80:
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %array.live.9.0, align 1
  store i1 false, ptr %array.live.9.1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.222 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.222, label %package.drop.81, label %package.done.81
package.drop.81:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.223 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.223)
  %tmp.224 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.224)
  br label %package.done.81
package.done.81:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.225 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.225, label %array.drop.82, label %array.done.82
array.drop.82:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.226 = load i1, ptr %array.live.6.0, align 1
  br i1 %tmp.226, label %array.element.drop.83, label %array.element.done.83
array.element.drop.83:
  store i1 false, ptr %array.live.6.0, align 1
  %tmp.227 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.227)
  %tmp.228 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.228)
  br label %array.element.done.83
array.element.done.83:
  %tmp.229 = load i1, ptr %array.live.6.1, align 1
  br i1 %tmp.229, label %array.element.drop.84, label %array.element.done.84
array.element.drop.84:
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.230 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.230)
  %tmp.231 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.231)
  br label %array.element.done.84
array.element.done.84:
  br label %array.done.82
array.done.82:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.232 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.232, label %package.drop.85, label %package.done.85
package.drop.85:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.233 = load i1, ptr %package.live.5.0, align 1
  br i1 %tmp.233, label %package.field.drop.86, label %package.field.done.86
package.field.drop.86:
  store i1 false, ptr %package.live.5.0, align 1
  %tmp.234 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.234)
  br label %package.field.done.86
package.field.done.86:
  %tmp.235 = load i1, ptr %package.live.5.1, align 1
  br i1 %tmp.235, label %package.field.drop.87, label %package.field.done.87
package.field.drop.87:
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.236 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.236)
  %tmp.237 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.237)
  %tmp.238 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.238)
  %tmp.239 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.239)
  br label %package.field.done.87
package.field.done.87:
  br label %package.done.85
package.done.85:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %package.live.5.0, align 1
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.240 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.240, label %resource.drop.88, label %resource.done.88
resource.drop.88:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.88
resource.done.88:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, {}, i32 } %v.113
bb.34:
  br label %bb.35
bb.35:
  %v.114 = add i32 0, 0
  %tmp.241 = insertvalue { i1, {}, i32 } zeroinitializer, i1 false, 0
  %v.115 = insertvalue { i1, {}, i32 } %tmp.241, i32 %v.114, 2
  %tmp.242 = load i1, ptr %resource.live.21, align 1
  br i1 %tmp.242, label %closure.drop.89, label %closure.done.89
closure.drop.89:
  store i1 false, ptr %resource.live.21, align 1
  call void @zil.closure.drop.1(ptr %local.21)
  br label %closure.done.89
closure.done.89:
  store i1 false, ptr %resource.live.21, align 1
  %tmp.243 = load i1, ptr %resource.live.14, align 1
  br i1 %tmp.243, label %resource.drop.90, label %resource.done.90
resource.drop.90:
  store i1 false, ptr %resource.live.14, align 1
  call void @zil.store.3.drop(ptr %local.14)
  br label %resource.done.90
resource.done.90:
  store i1 false, ptr %resource.live.14, align 1
  store i1 false, ptr %resource.live.13, align 1
  %tmp.244 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.244, label %resource.drop.91, label %resource.done.91
resource.drop.91:
  store i1 false, ptr %resource.live.12, align 1
  call void @zil.store.3.drop(ptr %local.12)
  br label %resource.done.91
resource.done.91:
  store i1 false, ptr %resource.live.12, align 1
  %tmp.245 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.245, label %package.drop.92, label %package.done.92
package.drop.92:
  store i1 false, ptr %resource.live.11, align 1
  %tmp.246 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.246)
  %tmp.247 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.247)
  %tmp.248 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.248)
  %tmp.249 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.249)
  %tmp.250 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.11, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.250)
  br label %package.done.92
package.done.92:
  store i1 false, ptr %resource.live.11, align 1
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %array.live.9.0, align 1
  store i1 false, ptr %array.live.9.1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.251 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.251, label %package.drop.93, label %package.done.93
package.drop.93:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.252 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.252)
  %tmp.253 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.253)
  br label %package.done.93
package.done.93:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.254 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.254, label %array.drop.94, label %array.done.94
array.drop.94:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.255 = load i1, ptr %array.live.6.0, align 1
  br i1 %tmp.255, label %array.element.drop.95, label %array.element.done.95
array.element.drop.95:
  store i1 false, ptr %array.live.6.0, align 1
  %tmp.256 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.256)
  %tmp.257 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.257)
  br label %array.element.done.95
array.element.done.95:
  %tmp.258 = load i1, ptr %array.live.6.1, align 1
  br i1 %tmp.258, label %array.element.drop.96, label %array.element.done.96
array.element.drop.96:
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.259 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.259)
  %tmp.260 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.260)
  br label %array.element.done.96
array.element.done.96:
  br label %array.done.94
array.done.94:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.261 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.261, label %package.drop.97, label %package.done.97
package.drop.97:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.262 = load i1, ptr %package.live.5.0, align 1
  br i1 %tmp.262, label %package.field.drop.98, label %package.field.done.98
package.field.drop.98:
  store i1 false, ptr %package.live.5.0, align 1
  %tmp.263 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.263)
  br label %package.field.done.98
package.field.done.98:
  %tmp.264 = load i1, ptr %package.live.5.1, align 1
  br i1 %tmp.264, label %package.field.drop.99, label %package.field.done.99
package.field.drop.99:
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.265 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.265)
  %tmp.266 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.266)
  %tmp.267 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.267)
  %tmp.268 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.5, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.268)
  br label %package.field.done.99
package.field.done.99:
  br label %package.done.97
package.done.97:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %package.live.5.0, align 1
  store i1 false, ptr %package.live.5.1, align 1
  %tmp.269 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.269, label %resource.drop.100, label %resource.done.100
resource.drop.100:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.100
resource.done.100:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, {}, i32 } %v.115
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

define i32 @zil.fn.4(ptr %arg.0) {
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
  call void @zil.fn.3(ptr %local.5)
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
  call void @zil.fn.3(ptr %local.2)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, { ptr, i64, i64 }, i32 } %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.6(i32 %arg.0, i1 %arg.1) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %local.5 = alloca i1, align 1
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { ptr, i64, i64 }, align 8
  %local.8 = alloca i32, align 4
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { i32 }, align 4
  %resource.live.10 = alloca i1, align 1
  store i1 false, ptr %resource.live.10, align 1
  %local.10 = alloca { ptr, i64, i64 }, align 8
  %enum.slot.15 = alloca { i32, [0 x i8] }, align 4
  %enum.slot.18 = alloca { i32, [0 x i8] }, align 4
  store i32 %arg.0, ptr %local.0, align 4
  store i1 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = load i32, ptr %local.2, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.2, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.3, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.4 = getelementptr i8, ptr %local.4, i64 0
  %v.5 = add i64 0, 1
  %v.6 = call i1 @zil.bytes.reserve(ptr %v.4, i64 %v.5)
  %v.7 = xor i1 %v.6, true
  br i1 %v.7, label %bb.1, label %bb.2
bb.1:
  %v.8 = or i1 false, true
  store i1 %v.8, ptr %local.5, align 1
  br label %bb.3
bb.2:
  %v.9 = getelementptr i8, ptr %local.4, i64 0
  %v.10 = load i32, ptr %local.0, align 4
  %tmp.0 = icmp ugt i32 %v.10, 255
  br i1 %tmp.0, label %zil.trap, label %checked.0
checked.0:
  %v.11 = trunc i32 %v.10 to i8
  %v.12 = call i1 @zil.bytes.push(ptr %v.9, i8 %v.11)
  %v.13 = xor i1 %v.12, true
  store i1 %v.13, ptr %local.5, align 1
  br label %bb.3
bb.3:
  %v.14 = load i1, ptr %local.5, align 1
  br i1 %v.14, label %bb.4, label %bb.5
bb.4:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.15, align 4
  %tmp.1 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.15, i32 0, i32 0
  store i32 0, ptr %tmp.1, align 4
  %tmp.2 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.15, i32 0, i32 1
  %v.15 = load { i32, [0 x i8] }, ptr %enum.slot.15, align 4
  %tmp.3 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } zeroinitializer, i1 true, 0
  %v.16 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %tmp.3, { i32, [0 x i8] } %v.15, 1
  %tmp.4 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.4, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.5 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.5, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.3(ptr %local.3)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.16
bb.5:
  br label %bb.6
bb.6:
  %v.17 = load i1, ptr %local.1, align 1
  br i1 %v.17, label %bb.7, label %bb.8
bb.7:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.18, align 4
  %tmp.6 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.18, i32 0, i32 0
  store i32 1, ptr %tmp.6, align 4
  %tmp.7 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.18, i32 0, i32 1
  %v.18 = load { i32, [0 x i8] }, ptr %enum.slot.18, align 4
  %tmp.8 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } zeroinitializer, i1 true, 0
  %v.19 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %tmp.8, { i32, [0 x i8] } %v.18, 1
  %tmp.9 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.9, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.10 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.10, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.3(ptr %local.3)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.19
bb.8:
  br label %bb.9
bb.9:
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  %v.20 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.6, align 1
  store { i32 } %v.20, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.21 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.7, align 1
  store { ptr, i64, i64 } %v.21, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.22 = load i32, ptr %local.0, align 4
  store i32 %v.22, ptr %local.8, align 4
  %v.23 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  store { i32 } %v.23, ptr %local.9, align 4
  store i1 true, ptr %resource.live.9, align 1
  %v.24 = load { ptr, i64, i64 }, ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
  store { ptr, i64, i64 } %v.24, ptr %local.10, align 8
  store i1 true, ptr %resource.live.10, align 1
  %v.25 = load i32, ptr %local.8, align 4
  %v.26 = load { i32 }, ptr %local.9, align 4
  store i1 false, ptr %resource.live.9, align 1
  %v.27 = load { ptr, i64, i64 }, ptr %local.10, align 8
  store i1 false, ptr %resource.live.10, align 1
%tmp.11 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } zeroinitializer, { i32 } %v.26, 0
%tmp.12 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.11, { ptr, i64, i64 } %v.27, 1
%v.28 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.12, i32 %v.25, 2
  %tmp.13 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } zeroinitializer, i1 false, 0
  %v.29 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %tmp.13, { { i32 }, { ptr, i64, i64 }, i32 } %v.28, 2
  %tmp.14 = load i1, ptr %resource.live.10, align 1
  br i1 %tmp.14, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.10, align 1
  call void @zil.bytes.drop(ptr %local.10)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.10, align 1
  %tmp.15 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.15, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.fn.3(ptr %local.9)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.16 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.16, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.bytes.drop(ptr %local.7)
  br label %resource.done.7
resource.done.7:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.17 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.17, label %resource.drop.8, label %resource.done.8
resource.drop.8:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.3(ptr %local.6)
  br label %resource.done.8
resource.done.8:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.18 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.18, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.19 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.19, label %resource.drop.10, label %resource.done.10
resource.drop.10:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.3(ptr %local.3)
  br label %resource.done.10
resource.done.10:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.29
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } @zil.fn.7({ { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, align 8
  store { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.1 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.1)
  %tmp.2 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.2)
  %tmp.3 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.3)
  %tmp.4 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.4)
  %tmp.5 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.5)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } @zil.fn.8() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca [2 x { { i32 }, { ptr, i64, i64 }, i32 }], align 8
  %array.live.4.0 = alloca i1, align 1
  store i1 false, ptr %array.live.4.0, align 1
  %array.live.4.1 = alloca i1, align 1
  store i1 false, ptr %array.live.4.1, align 1
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca [2 x { { i32 }, { ptr, i64, i64 }, i32 }], align 8
  %array.live.7.0 = alloca i1, align 1
  store i1 false, ptr %array.live.7.0, align 1
  %array.live.7.1 = alloca i1, align 1
  store i1 false, ptr %array.live.7.1, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = add i32 0, 90
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i32 0, 65
  %v.4 = call { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.5(i32 %v.3)
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.4, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.5 = add i32 0, 66
  %v.6 = call { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.5(i32 %v.5)
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.6, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.7 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.8 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
%tmp.0 = insertvalue [2 x { { i32 }, { ptr, i64, i64 }, i32 }] zeroinitializer, { { i32 }, { ptr, i64, i64 }, i32 } %v.7, 0
%v.9 = insertvalue [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %tmp.0, { { i32 }, { ptr, i64, i64 }, i32 } %v.8, 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %array.live.4.0, align 1
  store i1 false, ptr %array.live.4.1, align 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.9, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  store i1 true, ptr %array.live.4.0, align 1
  store i1 true, ptr %array.live.4.1, align 1
  %v.10 = add i32 0, 1
  store i32 %v.10, ptr %local.5, align 4
  %v.11 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.11, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.12 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %array.live.4.0, align 1
  store i1 false, ptr %array.live.4.1, align 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.12, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  store i1 true, ptr %array.live.7.0, align 1
  store i1 true, ptr %array.live.7.1, align 1
  %v.13 = load i32, ptr %local.5, align 4
  %v.14 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %v.15 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
%tmp.1 = insertvalue { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } zeroinitializer, { i32 } %v.14, 0
%tmp.2 = insertvalue { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %tmp.1, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.15, 1
%v.16 = insertvalue { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %tmp.2, i32 %v.13, 2
  %tmp.3 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.3, label %array.drop.0, label %array.done.0
array.drop.0:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.4 = load i1, ptr %array.live.7.0, align 1
  br i1 %tmp.4, label %array.element.drop.1, label %array.element.done.1
array.element.drop.1:
  store i1 false, ptr %array.live.7.0, align 1
  %tmp.5 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.5)
  %tmp.6 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.6)
  br label %array.element.done.1
array.element.done.1:
  %tmp.7 = load i1, ptr %array.live.7.1, align 1
  br i1 %tmp.7, label %array.element.drop.2, label %array.element.done.2
array.element.drop.2:
  store i1 false, ptr %array.live.7.1, align 1
  %tmp.8 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.8)
  %tmp.9 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.9)
  br label %array.element.done.2
array.element.done.2:
  br label %array.done.0
array.done.0:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
  %tmp.10 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.10, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.3(ptr %local.6)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.11 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.11, label %array.drop.4, label %array.done.4
array.drop.4:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.12 = load i1, ptr %array.live.4.0, align 1
  br i1 %tmp.12, label %array.element.drop.5, label %array.element.done.5
array.element.drop.5:
  store i1 false, ptr %array.live.4.0, align 1
  %tmp.13 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.13)
  %tmp.14 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.14)
  br label %array.element.done.5
array.element.done.5:
  %tmp.15 = load i1, ptr %array.live.4.1, align 1
  br i1 %tmp.15, label %array.element.drop.6, label %array.element.done.6
array.element.drop.6:
  store i1 false, ptr %array.live.4.1, align 1
  %tmp.16 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.16)
  %tmp.17 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.17)
  br label %array.element.done.6
array.element.done.6:
  br label %array.done.4
array.done.4:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %array.live.4.0, align 1
  store i1 false, ptr %array.live.4.1, align 1
  %tmp.18 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.18, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.19 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.19)
  %tmp.20 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.20)
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.21 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.21, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.22 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.22)
  %tmp.23 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.23)
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.24 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.24, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.3(ptr %local.1)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.1, align 1
  ret { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %v.16
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } @zil.fn.9() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca [2 x { { i32 }, { ptr, i64, i64 }, i32 }], align 8
  %array.live.4.0 = alloca i1, align 1
  store i1 false, ptr %array.live.4.0, align 1
  %array.live.4.1 = alloca i1, align 1
  store i1 false, ptr %array.live.4.1, align 1
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca [2 x { { i32 }, { ptr, i64, i64 }, i32 }], align 8
  %array.live.7.0 = alloca i1, align 1
  store i1 false, ptr %array.live.7.0, align 1
  %array.live.7.1 = alloca i1, align 1
  store i1 false, ptr %array.live.7.1, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  %v.0 = add i32 0, 90
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = add i32 0, 65
  %v.4 = or i1 false, false
  %v.5 = call { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.6(i32 %v.3, i1 %v.4)
  %v.6 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.5, 0
  br i1 %v.6, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.5, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.7 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.5, 1
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } zeroinitializer, i1 true, 0
  %v.8 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } %tmp.2, { i32, [0 x i8] } %v.7, 1
  %tmp.3 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.3(ptr %local.1)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } %v.8
bb.2:
  %tmp.4 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.5, 0
  %tmp.5 = icmp ne i1 %tmp.4, false
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.9 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.5, 2
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.9, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.10 = add i32 0, 66
  %v.11 = or i1 false, false
  %v.12 = call { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.6(i32 %v.10, i1 %v.11)
  %v.13 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 0
  br i1 %v.13, label %bb.3, label %bb.4
bb.3:
  %tmp.6 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 0
  %tmp.7 = icmp ne i1 %tmp.6, true
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  %v.14 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 1
  %tmp.8 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } zeroinitializer, i1 true, 0
  %v.15 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } %tmp.8, { i32, [0 x i8] } %v.14, 1
  %tmp.9 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.9, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.10 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.10)
  %tmp.11 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.11)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.12 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.12, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.3(ptr %local.1)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } %v.15
bb.4:
  %tmp.13 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 0
  %tmp.14 = icmp ne i1 %tmp.13, false
  br i1 %tmp.14, label %zil.trap, label %checked.6
checked.6:
  %v.16 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 2
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.16, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.17 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  %v.18 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
%tmp.15 = insertvalue [2 x { { i32 }, { ptr, i64, i64 }, i32 }] zeroinitializer, { { i32 }, { ptr, i64, i64 }, i32 } %v.17, 0
%v.19 = insertvalue [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %tmp.15, { { i32 }, { ptr, i64, i64 }, i32 } %v.18, 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %array.live.4.0, align 1
  store i1 false, ptr %array.live.4.1, align 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.19, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  store i1 true, ptr %array.live.4.0, align 1
  store i1 true, ptr %array.live.4.1, align 1
  %v.20 = add i32 0, 1
  store i32 %v.20, ptr %local.5, align 4
  %v.21 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.21, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.22 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %array.live.4.0, align 1
  store i1 false, ptr %array.live.4.1, align 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.22, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  store i1 true, ptr %array.live.7.0, align 1
  store i1 true, ptr %array.live.7.1, align 1
  %v.23 = load i32, ptr %local.5, align 4
  %v.24 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %v.25 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
%tmp.16 = insertvalue { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } zeroinitializer, { i32 } %v.24, 0
%tmp.17 = insertvalue { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %tmp.16, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.25, 1
%v.26 = insertvalue { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %tmp.17, i32 %v.23, 2
  %tmp.18 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } zeroinitializer, i1 false, 0
  %v.27 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } %tmp.18, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %v.26, 2
  %tmp.19 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.19, label %array.drop.7, label %array.done.7
array.drop.7:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.20 = load i1, ptr %array.live.7.0, align 1
  br i1 %tmp.20, label %array.element.drop.8, label %array.element.done.8
array.element.drop.8:
  store i1 false, ptr %array.live.7.0, align 1
  %tmp.21 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.21)
  %tmp.22 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.22)
  br label %array.element.done.8
array.element.done.8:
  %tmp.23 = load i1, ptr %array.live.7.1, align 1
  br i1 %tmp.23, label %array.element.drop.9, label %array.element.done.9
array.element.drop.9:
  store i1 false, ptr %array.live.7.1, align 1
  %tmp.24 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.24)
  %tmp.25 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.25)
  br label %array.element.done.9
array.element.done.9:
  br label %array.done.7
array.done.7:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
  %tmp.26 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.26, label %resource.drop.10, label %resource.done.10
resource.drop.10:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.3(ptr %local.6)
  br label %resource.done.10
resource.done.10:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.27 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.27, label %array.drop.11, label %array.done.11
array.drop.11:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.28 = load i1, ptr %array.live.4.0, align 1
  br i1 %tmp.28, label %array.element.drop.12, label %array.element.done.12
array.element.drop.12:
  store i1 false, ptr %array.live.4.0, align 1
  %tmp.29 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.29)
  %tmp.30 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.30)
  br label %array.element.done.12
array.element.done.12:
  %tmp.31 = load i1, ptr %array.live.4.1, align 1
  br i1 %tmp.31, label %array.element.drop.13, label %array.element.done.13
array.element.drop.13:
  store i1 false, ptr %array.live.4.1, align 1
  %tmp.32 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 1, i32 0
  call void @zil.fn.3(ptr %tmp.32)
  %tmp.33 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.33)
  br label %array.element.done.13
array.element.done.13:
  br label %array.done.11
array.done.11:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %array.live.4.0, align 1
  store i1 false, ptr %array.live.4.1, align 1
  %tmp.34 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.34, label %package.drop.14, label %package.done.14
package.drop.14:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.35 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.35)
  %tmp.36 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.36)
  br label %package.done.14
package.done.14:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.37 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.37, label %package.drop.15, label %package.done.15
package.drop.15:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.38 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 0
  call void @zil.fn.3(ptr %tmp.38)
  %tmp.39 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.39)
  br label %package.done.15
package.done.15:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.40 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.40, label %resource.drop.16, label %resource.done.16
resource.drop.16:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.3(ptr %local.1)
  br label %resource.done.16
resource.done.16:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } %v.27
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.10() {
entry:
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { ptr, i64, i64 } @zil.fn.11({ ptr, i64, i64 } %arg.0) {
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
  call void @zil.store.3.drop(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret { ptr, i64, i64 } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.fn.12(ptr %arg.0, { { i32 }, { ptr, i64, i64 }, i32 } %arg.1) {
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
  %v.2 = call { i1, { { i32 }, { ptr, i64, i64 }, i32 }, {} } @zil.store.3.push(ptr %v.0, { { i32 }, { ptr, i64, i64 }, i32 } %v.1)
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

define { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.13(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.store.3.pop(ptr %v.0)
  ret { i1, {}, { { i32 }, { ptr, i64, i64 }, i32 } } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.14(i32 %arg.0) {
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
  %v.3 = load i32, ptr %local.0, align 4
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
  call void @zil.fn.3(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  ret %zil_env_0 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_1 @zil.fn.15(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca %zil_env_0, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call %zil_env_0 @zil.fn.14(i32 %v.0)
  store i1 false, ptr %resource.live.1, align 1
  store %zil_env_0 %v.1, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.2 = load %zil_env_0, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %tmp.0 = insertvalue %zil_env_1 zeroinitializer, %zil_env_0 %v.2, 0
  %v.3 = insertvalue %zil_env_1 %tmp.0, i1 true, 1
  %tmp.1 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.1, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.closure.drop.0(ptr %local.1)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_1 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.16(ptr %env) {
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

define i32 @zil.fn.17(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %v.2 = call i32 @zil.fn.16(ptr %v.1)
  ret i32 %v.2
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

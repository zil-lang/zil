; ZIL verified LLVM module
; verified-fingerprint: 2ab87d953050b9c7c0a7d709b3635f2fcf0fab3b22ae448943f92f1d46ba8dcf
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
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.0() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, align 8
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, align 8
  %package.live.1.0 = alloca i1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca [2 x { { i32 }, { ptr, i64, i64 }, i32 }], align 8
  %array.live.2.0 = alloca i1, align 1
  store i1 false, ptr %array.live.2.0, align 1
  %array.live.2.1 = alloca i1, align 1
  store i1 false, ptr %array.live.2.1, align 1
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca [2 x { { i32 }, { ptr, i64, i64 }, i32 }], align 8
  %array.live.5.0 = alloca i1, align 1
  store i1 false, ptr %array.live.5.0, align 1
  %array.live.5.1 = alloca i1, align 1
  store i1 false, ptr %array.live.5.1, align 1
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca [2 x { { i32 }, { ptr, i64, i64 }, i32 }], align 8
  %array.live.8.0 = alloca i1, align 1
  store i1 false, ptr %array.live.8.0, align 1
  %array.live.8.1 = alloca i1, align 1
  store i1 false, ptr %array.live.8.1, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %array.live.8.0, align 1
  store i1 false, ptr %array.live.8.1, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %array.live.5.0, align 1
  store i1 false, ptr %array.live.5.1, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.0, align 1
  %v.0 = or i1 false, false
  %v.1 = call { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } @zil.fn.5(i1 %v.0)
  %v.2 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } %v.1, 0
  br i1 %v.2, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } %v.1, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.3 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } %v.1, 1
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.4 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.2, { i32, [0 x i8] } %v.3, 1
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %array.live.5.0, align 1
  store i1 false, ptr %array.live.5.1, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %array.live.8.0, align 1
  store i1 false, ptr %array.live.8.1, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.4
bb.2:
  %tmp.3 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } %v.1, 0
  %tmp.4 = icmp ne i1 %tmp.3, false
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.5 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } %v.1, 2
  store { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } %v.5, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  %v.6 = load { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  %v.7 = call { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } @zil.fn.6({ { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } %v.6)
  %tmp.5 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.5, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.6 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.6)
  %tmp.7 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.0, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.7)
  %tmp.8 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.0, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.8)
  %tmp.9 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.0, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.9)
  %tmp.10 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.0, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.10)
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } %v.7, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.0, align 1
  store i1 true, ptr %package.live.1.1, align 1
  %tmp.11 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1
  %v.8 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %tmp.11, align 8
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %array.live.2.0, align 1
  store i1 false, ptr %array.live.2.1, align 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.8, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  store i1 true, ptr %array.live.2.0, align 1
  store i1 true, ptr %array.live.2.1, align 1
  %tmp.12 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 0
  %v.9 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %tmp.12, align 8
  store i1 false, ptr %array.live.2.0, align 1
  store i1 false, ptr %resource.live.3, align 1
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.9, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.10 = add i32 0, 67
  %v.11 = or i1 false, false
  %v.12 = call { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.3(i32 %v.10, i1 %v.11)
  %v.13 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 0
  br i1 %v.13, label %bb.3, label %bb.4
bb.3:
  %tmp.13 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 0
  %tmp.14 = icmp ne i1 %tmp.13, true
  br i1 %tmp.14, label %zil.trap, label %checked.3
checked.3:
  %v.14 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 1
  %tmp.15 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.15 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.15, { i32, [0 x i8] } %v.14, 1
  %tmp.16 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.16, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.17 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.17)
  %tmp.18 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.18)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.19 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.19, label %array.drop.5, label %array.done.5
array.drop.5:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.20 = load i1, ptr %array.live.2.0, align 1
  br i1 %tmp.20, label %array.element.drop.6, label %array.element.done.6
array.element.drop.6:
  store i1 false, ptr %array.live.2.0, align 1
  %tmp.21 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.21)
  %tmp.22 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.22)
  br label %array.element.done.6
array.element.done.6:
  %tmp.23 = load i1, ptr %array.live.2.1, align 1
  br i1 %tmp.23, label %array.element.drop.7, label %array.element.done.7
array.element.drop.7:
  store i1 false, ptr %array.live.2.1, align 1
  %tmp.24 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.24)
  %tmp.25 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.25)
  br label %array.element.done.7
array.element.done.7:
  br label %array.done.5
array.done.5:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %array.live.2.0, align 1
  store i1 false, ptr %array.live.2.1, align 1
  %tmp.26 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.26, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.27 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.27, label %package.field.drop.9, label %package.field.done.9
package.field.drop.9:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.28 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.28)
  br label %package.field.done.9
package.field.done.9:
  %tmp.29 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.29, label %package.field.drop.10, label %package.field.done.10
package.field.drop.10:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.30 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.30)
  %tmp.31 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.31)
  %tmp.32 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.32)
  %tmp.33 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.33)
  br label %package.field.done.10
package.field.done.10:
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %array.live.5.0, align 1
  store i1 false, ptr %array.live.5.1, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %array.live.8.0, align 1
  store i1 false, ptr %array.live.8.1, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.15
bb.4:
  %tmp.34 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 0
  %tmp.35 = icmp ne i1 %tmp.34, false
  br i1 %tmp.35, label %zil.trap, label %checked.11
checked.11:
  %v.16 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 2
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.16, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %tmp.36 = load i1, ptr %array.live.2.0, align 1
  br i1 %tmp.36, label %array.element.drop.12, label %array.element.done.12
array.element.drop.12:
  store i1 false, ptr %array.live.2.0, align 1
  %tmp.37 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.37)
  %tmp.38 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.38)
  br label %array.element.done.12
array.element.done.12:
  %v.17 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  %tmp.39 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 0
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.17, ptr %tmp.39, align 8
  store i1 true, ptr %array.live.2.0, align 1
  %tmp.40 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.40, label %package.drop.13, label %package.done.13
package.drop.13:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.41 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.4, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.41)
  %tmp.42 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.4, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.42)
  br label %package.done.13
package.done.13:
  %v.18 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, align 8
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %array.live.2.0, align 1
  store i1 false, ptr %array.live.2.1, align 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.18, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  store i1 true, ptr %array.live.5.0, align 1
  store i1 true, ptr %array.live.5.1, align 1
  %tmp.43 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.43, label %package.field.drop.14, label %package.field.done.14
package.field.drop.14:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.44 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.44)
  %tmp.45 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.45)
  %tmp.46 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.46)
  %tmp.47 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.47)
  br label %package.field.done.14
package.field.done.14:
  %v.19 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %array.live.5.0, align 1
  store i1 false, ptr %array.live.5.1, align 1
  %tmp.48 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.19, ptr %tmp.48, align 8
  store i1 true, ptr %package.live.1.1, align 1
  %tmp.49 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.49, label %array.drop.15, label %array.done.15
array.drop.15:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.50 = load i1, ptr %array.live.5.0, align 1
  br i1 %tmp.50, label %array.element.drop.16, label %array.element.done.16
array.element.drop.16:
  store i1 false, ptr %array.live.5.0, align 1
  %tmp.51 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.5, i32 0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.51)
  %tmp.52 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.5, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.52)
  br label %array.element.done.16
array.element.done.16:
  %tmp.53 = load i1, ptr %array.live.5.1, align 1
  br i1 %tmp.53, label %array.element.drop.17, label %array.element.done.17
array.element.drop.17:
  store i1 false, ptr %array.live.5.1, align 1
  %tmp.54 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.5, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.54)
  %tmp.55 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.5, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.55)
  br label %array.element.done.17
array.element.done.17:
  br label %array.done.15
array.done.15:
  %v.20 = add i32 0, 68
  %v.21 = or i1 false, false
  %v.22 = call { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.3(i32 %v.20, i1 %v.21)
  %v.23 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.22, 0
  br i1 %v.23, label %bb.5, label %bb.6
bb.5:
  %tmp.56 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.22, 0
  %tmp.57 = icmp ne i1 %tmp.56, true
  br i1 %tmp.57, label %zil.trap, label %checked.18
checked.18:
  %v.24 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.22, 1
  %tmp.58 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.25 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.58, { i32, [0 x i8] } %v.24, 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %array.live.5.0, align 1
  store i1 false, ptr %array.live.5.1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %tmp.59 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.59, label %package.drop.19, label %package.done.19
package.drop.19:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.60 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.60)
  %tmp.61 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.61)
  br label %package.done.19
package.done.19:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.62 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.62, label %array.drop.20, label %array.done.20
array.drop.20:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.63 = load i1, ptr %array.live.2.0, align 1
  br i1 %tmp.63, label %array.element.drop.21, label %array.element.done.21
array.element.drop.21:
  store i1 false, ptr %array.live.2.0, align 1
  %tmp.64 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.64)
  %tmp.65 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.65)
  br label %array.element.done.21
array.element.done.21:
  %tmp.66 = load i1, ptr %array.live.2.1, align 1
  br i1 %tmp.66, label %array.element.drop.22, label %array.element.done.22
array.element.drop.22:
  store i1 false, ptr %array.live.2.1, align 1
  %tmp.67 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.67)
  %tmp.68 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.68)
  br label %array.element.done.22
array.element.done.22:
  br label %array.done.20
array.done.20:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %array.live.2.0, align 1
  store i1 false, ptr %array.live.2.1, align 1
  %tmp.69 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.69, label %package.drop.23, label %package.done.23
package.drop.23:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.70 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.70, label %package.field.drop.24, label %package.field.done.24
package.field.drop.24:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.71 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.71)
  br label %package.field.done.24
package.field.done.24:
  %tmp.72 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.72, label %package.field.drop.25, label %package.field.done.25
package.field.drop.25:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.73 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.73)
  %tmp.74 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.74)
  %tmp.75 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.75)
  %tmp.76 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.76)
  br label %package.field.done.25
package.field.done.25:
  br label %package.done.23
package.done.23:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %array.live.8.0, align 1
  store i1 false, ptr %array.live.8.1, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.25
bb.6:
  %tmp.77 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.22, 0
  %tmp.78 = icmp ne i1 %tmp.77, false
  br i1 %tmp.78, label %zil.trap, label %checked.26
checked.26:
  %v.26 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.22, 2
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.26, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  %v.27 = add i32 0, 69
  %v.28 = or i1 false, false
  %v.29 = call { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.3(i32 %v.27, i1 %v.28)
  %v.30 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.29, 0
  br i1 %v.30, label %bb.7, label %bb.8
bb.7:
  %tmp.79 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.29, 0
  %tmp.80 = icmp ne i1 %tmp.79, true
  br i1 %tmp.80, label %zil.trap, label %checked.27
checked.27:
  %v.31 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.29, 1
  %tmp.81 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.32 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.81, { i32, [0 x i8] } %v.31, 1
  %tmp.82 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.82, label %package.drop.28, label %package.done.28
package.drop.28:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.83 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.83)
  %tmp.84 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.6, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.84)
  br label %package.done.28
package.done.28:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %array.live.5.0, align 1
  store i1 false, ptr %array.live.5.1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %tmp.85 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.85, label %package.drop.29, label %package.done.29
package.drop.29:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.86 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.86)
  %tmp.87 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.87)
  br label %package.done.29
package.done.29:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.88 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.88, label %array.drop.30, label %array.done.30
array.drop.30:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.89 = load i1, ptr %array.live.2.0, align 1
  br i1 %tmp.89, label %array.element.drop.31, label %array.element.done.31
array.element.drop.31:
  store i1 false, ptr %array.live.2.0, align 1
  %tmp.90 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.90)
  %tmp.91 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.91)
  br label %array.element.done.31
array.element.done.31:
  %tmp.92 = load i1, ptr %array.live.2.1, align 1
  br i1 %tmp.92, label %array.element.drop.32, label %array.element.done.32
array.element.drop.32:
  store i1 false, ptr %array.live.2.1, align 1
  %tmp.93 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.93)
  %tmp.94 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.94)
  br label %array.element.done.32
array.element.done.32:
  br label %array.done.30
array.done.30:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %array.live.2.0, align 1
  store i1 false, ptr %array.live.2.1, align 1
  %tmp.95 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.95, label %package.drop.33, label %package.done.33
package.drop.33:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.96 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.96, label %package.field.drop.34, label %package.field.done.34
package.field.drop.34:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.97 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.97)
  br label %package.field.done.34
package.field.done.34:
  %tmp.98 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.98, label %package.field.drop.35, label %package.field.done.35
package.field.drop.35:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.99 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.99)
  %tmp.100 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.100)
  %tmp.101 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.101)
  %tmp.102 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.102)
  br label %package.field.done.35
package.field.done.35:
  br label %package.done.33
package.done.33:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %array.live.8.0, align 1
  store i1 false, ptr %array.live.8.1, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.32
bb.8:
  %tmp.103 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.29, 0
  %tmp.104 = icmp ne i1 %tmp.103, false
  br i1 %tmp.104, label %zil.trap, label %checked.36
checked.36:
  %v.33 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.29, 2
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.33, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.34 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  %v.35 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
%tmp.105 = insertvalue [2 x { { i32 }, { ptr, i64, i64 }, i32 }] zeroinitializer, { { i32 }, { ptr, i64, i64 }, i32 } %v.34, 0
%v.36 = insertvalue [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %tmp.105, { { i32 }, { ptr, i64, i64 }, i32 } %v.35, 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.36, ptr %local.8, align 8
  store i1 true, ptr %resource.live.8, align 1
  store i1 true, ptr %array.live.8.0, align 1
  store i1 true, ptr %array.live.8.1, align 1
  %tmp.106 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.106, label %package.field.drop.37, label %package.field.done.37
package.field.drop.37:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.107 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.107)
  %tmp.108 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.108)
  %tmp.109 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.109)
  %tmp.110 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.110)
  br label %package.field.done.37
package.field.done.37:
  %v.37 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.8, align 8
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %array.live.8.0, align 1
  store i1 false, ptr %array.live.8.1, align 1
  %tmp.111 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.37, ptr %tmp.111, align 8
  store i1 true, ptr %package.live.1.1, align 1
  %tmp.112 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.112, label %array.drop.38, label %array.done.38
array.drop.38:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.113 = load i1, ptr %array.live.8.0, align 1
  br i1 %tmp.113, label %array.element.drop.39, label %array.element.done.39
array.element.drop.39:
  store i1 false, ptr %array.live.8.0, align 1
  %tmp.114 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.8, i32 0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.114)
  %tmp.115 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.8, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.115)
  br label %array.element.done.39
array.element.done.39:
  %tmp.116 = load i1, ptr %array.live.8.1, align 1
  br i1 %tmp.116, label %array.element.drop.40, label %array.element.done.40
array.element.drop.40:
  store i1 false, ptr %array.live.8.1, align 1
  %tmp.117 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.8, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.117)
  %tmp.118 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.8, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.118)
  br label %array.element.done.40
array.element.done.40:
  br label %array.done.38
array.done.38:
  %tmp.119 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.119, label %package.drop.41, label %package.done.41
package.drop.41:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.120 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.120)
  %tmp.121 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.7, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.121)
  br label %package.done.41
package.done.41:
  %tmp.122 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.122, label %package.drop.42, label %package.done.42
package.drop.42:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.123 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.6, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.123)
  %tmp.124 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.6, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.124)
  br label %package.done.42
package.done.42:
  %tmp.125 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 2
  %v.38 = load i32, ptr %tmp.125, align 4
  %v.39 = add i32 0, 65
  %tmp.126 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.38, i32 %v.39)
  %v.40 = extractvalue { i32, i1 } %tmp.126, 0
  %tmp.127 = extractvalue { i32, i1 } %tmp.126, 1
  br i1 %tmp.127, label %zil.trap, label %checked.43
checked.43:
  %tmp.128 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.41 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.128, i32 %v.40, 2
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %array.live.8.0, align 1
  store i1 false, ptr %array.live.8.1, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %array.live.5.0, align 1
  store i1 false, ptr %array.live.5.1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %tmp.129 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.129, label %package.drop.44, label %package.done.44
package.drop.44:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.130 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.130)
  %tmp.131 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.131)
  br label %package.done.44
package.done.44:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.132 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.132, label %array.drop.45, label %array.done.45
array.drop.45:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.133 = load i1, ptr %array.live.2.0, align 1
  br i1 %tmp.133, label %array.element.drop.46, label %array.element.done.46
array.element.drop.46:
  store i1 false, ptr %array.live.2.0, align 1
  %tmp.134 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.134)
  %tmp.135 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.135)
  br label %array.element.done.46
array.element.done.46:
  %tmp.136 = load i1, ptr %array.live.2.1, align 1
  br i1 %tmp.136, label %array.element.drop.47, label %array.element.done.47
array.element.drop.47:
  store i1 false, ptr %array.live.2.1, align 1
  %tmp.137 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.137)
  %tmp.138 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.2, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.138)
  br label %array.element.done.47
array.element.done.47:
  br label %array.done.45
array.done.45:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %array.live.2.0, align 1
  store i1 false, ptr %array.live.2.1, align 1
  %tmp.139 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.139, label %package.drop.48, label %package.done.48
package.drop.48:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.140 = load i1, ptr %package.live.1.0, align 1
  br i1 %tmp.140, label %package.field.drop.49, label %package.field.done.49
package.field.drop.49:
  store i1 false, ptr %package.live.1.0, align 1
  %tmp.141 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.141)
  br label %package.field.done.49
package.field.done.49:
  %tmp.142 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.142, label %package.field.drop.50, label %package.field.done.50
package.field.drop.50:
  store i1 false, ptr %package.live.1.1, align 1
  %tmp.143 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.143)
  %tmp.144 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.144)
  %tmp.145 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.145)
  %tmp.146 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.1, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.146)
  br label %package.field.done.50
package.field.done.50:
  br label %package.done.48
package.done.48:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.0, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.41
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

define { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.3(i32 %arg.0, i1 %arg.1) {
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
  call void @zil.fn.2(ptr %local.3)
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
  call void @zil.fn.2(ptr %local.3)
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
  call void @zil.fn.2(ptr %local.9)
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
  call void @zil.fn.2(ptr %local.6)
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
  call void @zil.fn.2(ptr %local.3)
  br label %resource.done.10
resource.done.10:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.29
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.4(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { ptr, i64, i64 }, align 8
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { ptr, i64, i64 }, align 8
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = freeze { ptr, i64, i64 } zeroinitializer
  store i1 false, ptr %resource.live.1, align 1
  store { ptr, i64, i64 } %v.0, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  %v.1 = getelementptr i8, ptr %local.1, i64 0
  %v.2 = add i64 0, 1
  %v.3 = call i1 @zil.bytes.reserve(ptr %v.1, i64 %v.2)
  %v.4 = getelementptr i8, ptr %local.1, i64 0
  %v.5 = load i32, ptr %local.0, align 4
  %tmp.0 = icmp ugt i32 %v.5, 255
  br i1 %tmp.0, label %zil.trap, label %checked.0
checked.0:
  %v.6 = trunc i32 %v.5 to i8
  %v.7 = call i1 @zil.bytes.push(ptr %v.4, i8 %v.6)
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  %v.8 = load i32, ptr %local.0, align 4
  store i32 %v.8, ptr %local.2, align 4
  %v.9 = load i32, ptr %local.2, align 4
%v.10 = insertvalue { i32 } zeroinitializer, i32 %v.9, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.10, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.11 = load { ptr, i64, i64 }, ptr %local.1, align 8
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.11, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.12 = load i32, ptr %local.0, align 4
  store i32 %v.12, ptr %local.5, align 4
  %v.13 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.13, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.14 = load { ptr, i64, i64 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.14, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  %v.15 = load i32, ptr %local.5, align 4
  %v.16 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %v.17 = load { ptr, i64, i64 }, ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
%tmp.1 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } zeroinitializer, { i32 } %v.16, 0
%tmp.2 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.1, { ptr, i64, i64 } %v.17, 1
%v.18 = insertvalue { { i32 }, { ptr, i64, i64 }, i32 } %tmp.2, i32 %v.15, 2
  %tmp.3 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.bytes.drop(ptr %local.7)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.4 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.2(ptr %local.6)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.5 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.6 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.6, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.2(ptr %local.3)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.7 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.7, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.bytes.drop(ptr %local.1)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.1, align 1
  ret { { i32 }, { ptr, i64, i64 }, i32 } %v.18
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } @zil.fn.5(i1 %arg.0) {
entry:
  %local.0 = alloca i1, align 1
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { { i32 }, { ptr, i64, i64 }, i32 }, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca [2 x { { i32 }, { ptr, i64, i64 }, i32 }], align 8
  %array.live.5.0 = alloca i1, align 1
  store i1 false, ptr %array.live.5.0, align 1
  %array.live.5.1 = alloca i1, align 1
  store i1 false, ptr %array.live.5.1, align 1
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
  store i1 %arg.0, ptr %local.0, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = add i32 0, 90
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = add i32 0, 65
  %v.4 = or i1 false, false
  %v.5 = call { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.3(i32 %v.3, i1 %v.4)
  %v.6 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.5, 0
  br i1 %v.6, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.5, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.7 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.5, 1
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } zeroinitializer, i1 true, 0
  %v.8 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } %tmp.2, { i32, [0 x i8] } %v.7, 1
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.2(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } %v.8
bb.2:
  %tmp.4 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.5, 0
  %tmp.5 = icmp ne i1 %tmp.4, false
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.9 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.5, 2
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.9, ptr %local.3, align 8
  store i1 true, ptr %resource.live.3, align 1
  %v.10 = add i32 0, 66
  %v.11 = load i1, ptr %local.0, align 1
  %v.12 = call { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.3(i32 %v.10, i1 %v.11)
  %v.13 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 0
  br i1 %v.13, label %bb.3, label %bb.4
bb.3:
  %tmp.6 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 0
  %tmp.7 = icmp ne i1 %tmp.6, true
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  %v.14 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 1
  %tmp.8 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } zeroinitializer, i1 true, 0
  %v.15 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } %tmp.8, { i32, [0 x i8] } %v.14, 1
  %tmp.9 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.9, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.10 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.10)
  %tmp.11 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.11)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.12 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.12, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.2(ptr %local.2)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } %v.15
bb.4:
  %tmp.13 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 0
  %tmp.14 = icmp ne i1 %tmp.13, false
  br i1 %tmp.14, label %zil.trap, label %checked.6
checked.6:
  %v.16 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } %v.12, 2
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.16, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.17 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, align 8
  store i1 false, ptr %resource.live.3, align 1
  %v.18 = load { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
%tmp.15 = insertvalue [2 x { { i32 }, { ptr, i64, i64 }, i32 }] zeroinitializer, { { i32 }, { ptr, i64, i64 }, i32 } %v.17, 0
%v.19 = insertvalue [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %tmp.15, { { i32 }, { ptr, i64, i64 }, i32 } %v.18, 1
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %array.live.5.0, align 1
  store i1 false, ptr %array.live.5.1, align 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.19, ptr %local.5, align 8
  store i1 true, ptr %resource.live.5, align 1
  store i1 true, ptr %array.live.5.0, align 1
  store i1 true, ptr %array.live.5.1, align 1
  %v.20 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.20, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.21 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.5, align 8
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %array.live.5.0, align 1
  store i1 false, ptr %array.live.5.1, align 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.21, ptr %local.7, align 8
  store i1 true, ptr %resource.live.7, align 1
  store i1 true, ptr %array.live.7.0, align 1
  store i1 true, ptr %array.live.7.1, align 1
  %v.22 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %v.23 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, align 8
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
%tmp.16 = insertvalue { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } zeroinitializer, { i32 } %v.22, 0
%v.24 = insertvalue { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } %tmp.16, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.23, 1
  %tmp.17 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } zeroinitializer, i1 false, 0
  %v.25 = insertvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } %tmp.17, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } %v.24, 2
  %tmp.18 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.18, label %array.drop.7, label %array.done.7
array.drop.7:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.19 = load i1, ptr %array.live.7.0, align 1
  br i1 %tmp.19, label %array.element.drop.8, label %array.element.done.8
array.element.drop.8:
  store i1 false, ptr %array.live.7.0, align 1
  %tmp.20 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.20)
  %tmp.21 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.21)
  br label %array.element.done.8
array.element.done.8:
  %tmp.22 = load i1, ptr %array.live.7.1, align 1
  br i1 %tmp.22, label %array.element.drop.9, label %array.element.done.9
array.element.drop.9:
  store i1 false, ptr %array.live.7.1, align 1
  %tmp.23 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.23)
  %tmp.24 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.24)
  br label %array.element.done.9
array.element.done.9:
  br label %array.done.7
array.done.7:
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %array.live.7.0, align 1
  store i1 false, ptr %array.live.7.1, align 1
  %tmp.25 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.25, label %resource.drop.10, label %resource.done.10
resource.drop.10:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.2(ptr %local.6)
  br label %resource.done.10
resource.done.10:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.26 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.26, label %array.drop.11, label %array.done.11
array.drop.11:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.27 = load i1, ptr %array.live.5.0, align 1
  br i1 %tmp.27, label %array.element.drop.12, label %array.element.done.12
array.element.drop.12:
  store i1 false, ptr %array.live.5.0, align 1
  %tmp.28 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.5, i32 0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.28)
  %tmp.29 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.5, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.29)
  br label %array.element.done.12
array.element.done.12:
  %tmp.30 = load i1, ptr %array.live.5.1, align 1
  br i1 %tmp.30, label %array.element.drop.13, label %array.element.done.13
array.element.drop.13:
  store i1 false, ptr %array.live.5.1, align 1
  %tmp.31 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.5, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.31)
  %tmp.32 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.5, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.32)
  br label %array.element.done.13
array.element.done.13:
  br label %array.done.11
array.done.11:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %array.live.5.0, align 1
  store i1 false, ptr %array.live.5.1, align 1
  %tmp.33 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.33, label %package.drop.14, label %package.done.14
package.drop.14:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.34 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.4, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.34)
  %tmp.35 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.4, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.35)
  br label %package.done.14
package.done.14:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.36 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.36, label %package.drop.15, label %package.done.15
package.drop.15:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.37 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.37)
  %tmp.38 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.3, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.38)
  br label %package.done.15
package.done.15:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.39 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.39, label %resource.drop.16, label %resource.done.16
resource.drop.16:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.2(ptr %local.2)
  br label %resource.done.16
resource.done.16:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } } %v.25
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } @zil.fn.6({ { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, align 8
  store { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.0, align 8
  store i1 false, ptr %resource.live.0, align 1
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %package.drop.0, label %package.done.0
package.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.1 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.1)
  %tmp.2 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.0, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.2)
  %tmp.3 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.0, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.3)
  %tmp.4 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.0, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.4)
  %tmp.5 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, ptr %local.0, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.5)
  br label %package.done.0
package.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, i32 } @zil.fn.7() {
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
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32 }, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca [2 x { { i32 }, { ptr, i64, i64 }, i32 }], align 8
  %array.live.6.0 = alloca i1, align 1
  store i1 false, ptr %array.live.6.0, align 1
  %array.live.6.1 = alloca i1, align 1
  store i1 false, ptr %array.live.6.1, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %array.live.6.1, align 1
  store i1 false, ptr %resource.live.5, align 1
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
  %v.4 = call { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.4(i32 %v.3)
  store { { i32 }, { ptr, i64, i64 }, i32 } %v.4, ptr %local.2, align 8
  store i1 true, ptr %resource.live.2, align 1
  %v.5 = add i32 0, 66
  %v.6 = call { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.4(i32 %v.5)
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
  %v.10 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.10, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.11 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, align 8
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %array.live.4.0, align 1
  store i1 false, ptr %array.live.4.1, align 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.11, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  store i1 true, ptr %array.live.6.0, align 1
  store i1 true, ptr %array.live.6.1, align 1
  %v.12 = load { i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  %v.13 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %array.live.6.1, align 1
%tmp.1 = insertvalue { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } zeroinitializer, { i32 } %v.12, 0
%v.14 = insertvalue { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } %tmp.1, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.13, 1
  %tmp.2 = insertvalue { i1, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, i32 } zeroinitializer, i1 true, 0
  %v.15 = insertvalue { i1, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, i32 } %tmp.2, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] } %v.14, 1
  %tmp.3 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.3, label %array.drop.0, label %array.done.0
array.drop.0:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.4 = load i1, ptr %array.live.6.0, align 1
  br i1 %tmp.4, label %array.element.drop.1, label %array.element.done.1
array.element.drop.1:
  store i1 false, ptr %array.live.6.0, align 1
  %tmp.5 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.5)
  %tmp.6 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.6)
  br label %array.element.done.1
array.element.done.1:
  %tmp.7 = load i1, ptr %array.live.6.1, align 1
  br i1 %tmp.7, label %array.element.drop.2, label %array.element.done.2
array.element.drop.2:
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.8 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.8)
  %tmp.9 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.6, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.9)
  br label %array.element.done.2
array.element.done.2:
  br label %array.done.0
array.done.0:
  store i1 false, ptr %resource.live.6, align 1
  store i1 false, ptr %array.live.6.0, align 1
  store i1 false, ptr %array.live.6.1, align 1
  %tmp.10 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.10, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.2(ptr %local.5)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.11 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.11, label %array.drop.4, label %array.done.4
array.drop.4:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.12 = load i1, ptr %array.live.4.0, align 1
  br i1 %tmp.12, label %array.element.drop.5, label %array.element.done.5
array.element.drop.5:
  store i1 false, ptr %array.live.4.0, align 1
  %tmp.13 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.13)
  %tmp.14 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.14)
  br label %array.element.done.5
array.element.done.5:
  %tmp.15 = load i1, ptr %array.live.4.1, align 1
  br i1 %tmp.15, label %array.element.drop.6, label %array.element.done.6
array.element.drop.6:
  store i1 false, ptr %array.live.4.1, align 1
  %tmp.16 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.16)
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
  call void @zil.fn.2(ptr %tmp.19)
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
  call void @zil.fn.2(ptr %tmp.22)
  %tmp.23 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.23)
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.24 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.24, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.2(ptr %local.1)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }] }, i32 } %v.15
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.0()
  %entry.error = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 2
  ret i32 %entry.success
}

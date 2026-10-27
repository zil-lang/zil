; ZIL verified LLVM module
; verified-fingerprint: a234bd529679ffe5138b6a748fdf8d09f10d68de4a5ce034cdbdc3b05c02e539
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

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.0() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, align 8
  %package.live.0.0 = alloca i1, align 1
  store i1 false, ptr %package.live.0.0, align 1
  %package.live.0.1 = alloca i1, align 1
  store i1 false, ptr %package.live.0.1, align 1
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca [2 x { { i32 }, { ptr, i64, i64 }, i32 }], align 8
  %array.live.1.0 = alloca i1, align 1
  store i1 false, ptr %array.live.1.0, align 1
  %array.live.1.1 = alloca i1, align 1
  store i1 false, ptr %array.live.1.1, align 1
  %enum.slot.6 = alloca { i32, [0 x i8] }, align 4
  br label %bb.0
bb.0:
  %v.0 = call { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } @zil.fn.8()
  %v.1 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } %v.0, 0
  br i1 %v.1, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } %v.0, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.2 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } %v.0, 1
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.3 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.2, { i32, [0 x i8] } %v.2, 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.3
bb.2:
  %tmp.3 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } %v.0, 0
  %tmp.4 = icmp ne i1 %tmp.3, false
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.4 = extractvalue { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } %v.0, 2
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %package.live.0.0, align 1
  store i1 false, ptr %package.live.0.1, align 1
  store { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %v.4, ptr %local.0, align 8
  store i1 true, ptr %resource.live.0, align 1
  store i1 true, ptr %package.live.0.0, align 1
  store i1 true, ptr %package.live.0.1, align 1
  %tmp.5 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 1
  %v.5 = load [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %tmp.5, align 8
  store i1 false, ptr %package.live.0.1, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %array.live.1.0, align 1
  store i1 false, ptr %array.live.1.1, align 1
  store [2 x { { i32 }, { ptr, i64, i64 }, i32 }] %v.5, ptr %local.1, align 8
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %array.live.1.0, align 1
  store i1 true, ptr %array.live.1.1, align 1
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.6, align 4
  %tmp.6 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.6, i32 0, i32 0
  store i32 1, ptr %tmp.6, align 4
  %tmp.7 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.6, i32 0, i32 1
  %v.6 = load { i32, [0 x i8] }, ptr %enum.slot.6, align 4
  %tmp.8 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.7 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.8, { i32, [0 x i8] } %v.6, 1
  %tmp.9 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.9, label %array.drop.2, label %array.done.2
array.drop.2:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.10 = load i1, ptr %array.live.1.0, align 1
  br i1 %tmp.10, label %array.element.drop.3, label %array.element.done.3
array.element.drop.3:
  store i1 false, ptr %array.live.1.0, align 1
  %tmp.11 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.1, i32 0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.11)
  %tmp.12 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.1, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.12)
  br label %array.element.done.3
array.element.done.3:
  %tmp.13 = load i1, ptr %array.live.1.1, align 1
  br i1 %tmp.13, label %array.element.drop.4, label %array.element.done.4
array.element.drop.4:
  store i1 false, ptr %array.live.1.1, align 1
  %tmp.14 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.1, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.14)
  %tmp.15 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.1, i32 0, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.15)
  br label %array.element.done.4
array.element.done.4:
  br label %array.done.2
array.done.2:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %array.live.1.0, align 1
  store i1 false, ptr %array.live.1.1, align 1
  %tmp.16 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.16, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.17 = load i1, ptr %package.live.0.0, align 1
  br i1 %tmp.17, label %package.field.drop.6, label %package.field.done.6
package.field.drop.6:
  store i1 false, ptr %package.live.0.0, align 1
  %tmp.18 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.18)
  br label %package.field.done.6
package.field.done.6:
  %tmp.19 = load i1, ptr %package.live.0.1, align 1
  br i1 %tmp.19, label %package.field.drop.7, label %package.field.done.7
package.field.drop.7:
  store i1 false, ptr %package.live.0.1, align 1
  %tmp.20 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.20)
  %tmp.21 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.21)
  %tmp.22 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.22)
  %tmp.23 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 1, i32 1, i32 1
  call void @zil.bytes.drop(ptr %tmp.23)
  br label %package.field.done.7
package.field.done.7:
  br label %package.done.5
package.done.5:
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %package.live.0.0, align 1
  store i1 false, ptr %package.live.0.1, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.7
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

define i32 @zil.fn.3(ptr %arg.0) {
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

define { { i32 }, { ptr, i64, i64 }, i32 } @zil.fn.4(i32 %arg.0) {
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
  call void @zil.fn.2(ptr %local.5)
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
  call void @zil.fn.2(ptr %local.2)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.2, align 1
  ret { { i32 }, { ptr, i64, i64 }, i32 } %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.5(i32 %arg.0, i1 %arg.1) {
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

define { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } @zil.fn.6({ { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %arg.0) {
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
  call void @zil.fn.2(ptr %tmp.1)
  %tmp.2 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 1, i32 0, i32 0
  call void @zil.fn.2(ptr %tmp.2)
  %tmp.3 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 1, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.3)
  %tmp.4 = getelementptr { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 }, ptr %local.0, i32 0, i32 1, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.4)
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

define { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } @zil.fn.7() {
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
  call void @zil.fn.2(ptr %tmp.5)
  %tmp.6 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.6)
  br label %array.element.done.1
array.element.done.1:
  %tmp.7 = load i1, ptr %array.live.7.1, align 1
  br i1 %tmp.7, label %array.element.drop.2, label %array.element.done.2
array.element.drop.2:
  store i1 false, ptr %array.live.7.1, align 1
  %tmp.8 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.8)
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
  call void @zil.fn.2(ptr %local.6)
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
  ret { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } %v.16
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } @zil.fn.8() {
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
  %v.5 = call { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.5(i32 %v.3, i1 %v.4)
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
  call void @zil.fn.2(ptr %local.1)
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
  %v.12 = call { i1, { i32, [0 x i8] }, { { i32 }, { ptr, i64, i64 }, i32 } } @zil.fn.5(i32 %v.10, i1 %v.11)
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
  call void @zil.fn.2(ptr %tmp.10)
  %tmp.11 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.11)
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.12 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.12, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.2(ptr %local.1)
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
  call void @zil.fn.2(ptr %tmp.21)
  %tmp.22 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.22)
  br label %array.element.done.8
array.element.done.8:
  %tmp.23 = load i1, ptr %array.live.7.1, align 1
  br i1 %tmp.23, label %array.element.drop.9, label %array.element.done.9
array.element.drop.9:
  store i1 false, ptr %array.live.7.1, align 1
  %tmp.24 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.7, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.24)
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
  call void @zil.fn.2(ptr %local.6)
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
  call void @zil.fn.2(ptr %tmp.29)
  %tmp.30 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.30)
  br label %array.element.done.12
array.element.done.12:
  %tmp.31 = load i1, ptr %array.live.4.1, align 1
  br i1 %tmp.31, label %array.element.drop.13, label %array.element.done.13
array.element.drop.13:
  store i1 false, ptr %array.live.4.1, align 1
  %tmp.32 = getelementptr [2 x { { i32 }, { ptr, i64, i64 }, i32 }], ptr %local.4, i32 0, i32 1, i32 0
  call void @zil.fn.2(ptr %tmp.32)
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
  call void @zil.fn.2(ptr %tmp.35)
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
  call void @zil.fn.2(ptr %tmp.38)
  %tmp.39 = getelementptr { { i32 }, { ptr, i64, i64 }, i32 }, ptr %local.2, i32 0, i32 1
  call void @zil.bytes.drop(ptr %tmp.39)
  br label %package.done.15
package.done.15:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.40 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.40, label %resource.drop.16, label %resource.done.16
resource.drop.16:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.2(ptr %local.1)
  br label %resource.done.16
resource.done.16:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { i32, [0 x i8] }, { { i32 }, [2 x { { i32 }, { ptr, i64, i64 }, i32 }], i32 } } %v.27
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

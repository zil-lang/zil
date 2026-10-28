; ZIL verified LLVM module
; verified-fingerprint: e7667595c5a433c388317ba1387ec5c2edb5065ea710052903a824d72421c65b
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { ptr, ptr }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
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
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.sadd.with.overflow.i64(i64, i64)
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)
declare { i64, i1 } @llvm.ssub.with.overflow.i64(i64, i64)

declare i32 @getchar() nonlazybind
define { i1, i1, i32 } @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i1, align 1
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { ptr, i64, i64 }, align 8
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32 }, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { ptr, i64, i64 }, align 8
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { i32 }, align 4
  %local.8 = alloca ptr, align 8
  %local.9 = alloca ptr, align 8
  %local.10 = alloca { ptr, i64 }, align 8
  %local.11 = alloca %zil_env_0, align 8
  %local.12 = alloca i32, align 4
  %local.13 = alloca i32, align 4
  %local.14 = alloca i32, align 4
  %local.15 = alloca i32, align 4
  %local.16 = alloca i1, align 1
  %resource.live.17 = alloca i1, align 1
  store i1 false, ptr %resource.live.17, align 1
  %local.17 = alloca { ptr, i64, i64 }, align 8
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.17, align 1
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = call i32 @getchar()
  store i32 %v.1, ptr %local.0, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = add i32 0, 48
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.2, i32 %v.3)
  %v.4 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.4, ptr %local.1, align 4
  %v.5 = load i32, ptr %local.1, align 4
  %v.6 = add i32 0, 0
  %v.7 = icmp slt i32 %v.5, %v.6
  br i1 %v.7, label %bb.1, label %bb.2
bb.1:
  %v.8 = or i1 false, true
  store i1 %v.8, ptr %local.2, align 1
  br label %bb.3
bb.2:
  %v.9 = load i32, ptr %local.1, align 4
  %v.10 = add i32 0, 3
  %v.11 = icmp sgt i32 %v.9, %v.10
  store i1 %v.11, ptr %local.2, align 1
  br label %bb.3
bb.3:
  %v.12 = load i1, ptr %local.2, align 1
  br i1 %v.12, label %bb.4, label %bb.5
bb.4:
  %v.13 = add i32 0, 9
  %tmp.2 = insertvalue { i1, i1, i32 } zeroinitializer, i1 false, 0
  %v.14 = insertvalue { i1, i1, i32 } %tmp.2, i32 %v.13, 2
  store i1 false, ptr %resource.live.17, align 1
  ret { i1, i1, i32 } %v.14
bb.5:
  br label %bb.6
bb.6:
  %v.15 = add i32 0, 90
  %v.16 = call { i32 } @zil.fn.4(i32 %v.15)
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.16, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.17 = call { ptr, i64, i64 } @zil.fn.6()
  store i1 false, ptr %resource.live.4, align 1
  store { ptr, i64, i64 } %v.17, ptr %local.4, align 8
  store i1 true, ptr %resource.live.4, align 1
  %v.18 = getelementptr i8, ptr %local.4, i64 0
  %v.19 = add i64 0, 1
  %v.20 = call i1 @zil.bytes.reserve(ptr %v.18, i64 %v.19)
  %v.21 = xor i1 %v.20, true
  br i1 %v.21, label %bb.7, label %bb.8
bb.7:
  %v.22 = or i1 false, true
  %tmp.3 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.23 = insertvalue { i1, i1, i32 } %tmp.3, i1 %v.22, 1
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
  store i1 false, ptr %resource.live.17, align 1
  ret { i1, i1, i32 } %v.23
bb.8:
  br label %bb.9
bb.9:
  %v.24 = getelementptr i8, ptr %local.4, i64 0
  %v.25 = add i32 0, 65
  %tmp.6 = icmp ugt i32 %v.25, 255
  br i1 %tmp.6, label %zil.trap, label %checked.3
checked.3:
  %v.26 = trunc i32 %v.25 to i8
  %v.27 = call i1 @zil.bytes.push(ptr %v.24, i8 %v.26)
  %v.28 = xor i1 %v.27, true
  br i1 %v.28, label %bb.10, label %bb.11
bb.10:
  %v.29 = or i1 false, true
  %tmp.7 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.30 = insertvalue { i1, i1, i32 } %tmp.7, i1 %v.29, 1
  %tmp.8 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.8, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.9 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.9, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.3(ptr %local.3)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.17, align 1
  ret { i1, i1, i32 } %v.30
bb.11:
  br label %bb.12
bb.12:
  %v.31 = add i32 0, 83
  %v.32 = call { i32 } @zil.fn.4(i32 %v.31)
  store i1 false, ptr %resource.live.5, align 1
  store { i32 } %v.32, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.33 = call { ptr, i64, i64 } @zil.fn.6()
  store i1 false, ptr %resource.live.6, align 1
  store { ptr, i64, i64 } %v.33, ptr %local.6, align 8
  store i1 true, ptr %resource.live.6, align 1
  %v.34 = add i32 0, 84
  %v.35 = call { i32 } @zil.fn.4(i32 %v.34)
  store i1 false, ptr %resource.live.7, align 1
  store { i32 } %v.35, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.36 = getelementptr i8, ptr %local.4, i64 0
  store ptr %v.36, ptr %local.8, align 8
  %v.37 = getelementptr i8, ptr %local.6, i64 0
  store ptr %v.37, ptr %local.9, align 8
  %v.38 = getelementptr i8, ptr %local.4, i64 0
  %tmp.10 = load ptr, ptr %v.38, align 8
  %tmp.11 = getelementptr { ptr, i64, i64 }, ptr %v.38, i32 0, i32 1
  %tmp.12 = load i64, ptr %tmp.11, align 8
  %tmp.13 = insertvalue { ptr, i64 } poison, ptr %tmp.10, 0
  %v.39 = insertvalue { ptr, i64 } %tmp.13, i64 %tmp.12, 1
  store { ptr, i64 } %v.39, ptr %local.10, align 8
  %v.40 = load ptr, ptr %local.8, align 8
  %v.41 = load ptr, ptr %local.9, align 8
  %tmp.14 = insertvalue %zil_env_0 zeroinitializer, ptr %v.40, 0
  %v.42 = insertvalue %zil_env_0 %tmp.14, ptr %v.41, 1
  store %zil_env_0 %v.42, ptr %local.11, align 8
  %v.43 = add i32 0, 0
  store i32 %v.43, ptr %local.12, align 4
  %v.44 = add i32 0, 0
  %v.45 = load i32, ptr %local.1, align 4
  store i32 %v.44, ptr %local.13, align 4
  br label %bb.13
bb.13:
  %v.46 = load i32, ptr %local.13, align 4
  %v.47 = icmp slt i32 %v.46, %v.45
  br i1 %v.47, label %bb.14, label %bb.16
bb.14:
  store i32 %v.46, ptr %local.14, align 4
  %v.48 = load i32, ptr %local.12, align 4
  %v.49 = getelementptr i8, ptr %local.11, i64 0
  %v.50 = getelementptr i8, ptr %v.49, i64 0
  %v.51 = getelementptr i8, ptr %v.50, i64 0
  %v.52 = call { i1, i1, i32 } @zil.fn.14(ptr %v.51)
  %v.53 = extractvalue { i1, i1, i32 } %v.52, 0
  br i1 %v.53, label %bb.17, label %bb.18
bb.15:
  %v.58 = add i32 0, 1
  %tmp.15 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.46, i32 %v.58)
  %v.59 = extractvalue { i32, i1 } %tmp.15, 0
  %tmp.16 = extractvalue { i32, i1 } %tmp.15, 1
  br i1 %tmp.16, label %zil.trap, label %checked.6
checked.6:
  store i32 %v.59, ptr %local.13, align 4
  br label %bb.13
bb.16:
  %v.60 = load { ptr, i64 }, ptr %local.10, align 8
  %v.61 = add i32 0, 0
  %tmp.17 = extractvalue { ptr, i64 } %v.60, 0
  %tmp.18 = extractvalue { ptr, i64 } %v.60, 1
  %tmp.19 = sext i32 %v.61 to i64
  %tmp.20 = icmp uge i64 %tmp.19, %tmp.18
  br i1 %tmp.20, label %zil.trap, label %checked.7
checked.7:
  %v.62 = getelementptr i8, ptr %tmp.17, i64 %tmp.19
  %v.63 = getelementptr i8, ptr %v.62, i64 0
  %v.64 = load i8, ptr %v.63, align 1
  %v.65 = zext i8 %v.64 to i32
  store i32 %v.65, ptr %local.15, align 4
  %v.66 = getelementptr i8, ptr %local.4, i64 0
  %v.67 = call i1 @zil.fn.10(ptr %v.66)
  %v.68 = xor i1 %v.67, true
  br i1 %v.68, label %bb.19, label %bb.20
bb.17:
  %tmp.21 = extractvalue { i1, i1, i32 } %v.52, 0
  %tmp.22 = icmp ne i1 %tmp.21, true
  br i1 %tmp.22, label %zil.trap, label %checked.8
checked.8:
  %v.54 = extractvalue { i1, i1, i32 } %v.52, 1
  %tmp.23 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.55 = insertvalue { i1, i1, i32 } %tmp.23, i1 %v.54, 1
  %tmp.24 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.24, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.fn.3(ptr %local.7)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.25 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.25, label %resource.drop.10, label %resource.done.10
resource.drop.10:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.bytes.drop(ptr %local.6)
  br label %resource.done.10
resource.done.10:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.26 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.26, label %resource.drop.11, label %resource.done.11
resource.drop.11:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.3(ptr %local.5)
  br label %resource.done.11
resource.done.11:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.27 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.27, label %resource.drop.12, label %resource.done.12
resource.drop.12:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.12
resource.done.12:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.28 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.28, label %resource.drop.13, label %resource.done.13
resource.drop.13:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.3(ptr %local.3)
  br label %resource.done.13
resource.done.13:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.17, align 1
  ret { i1, i1, i32 } %v.55
bb.18:
  %tmp.29 = extractvalue { i1, i1, i32 } %v.52, 0
  %tmp.30 = icmp ne i1 %tmp.29, false
  br i1 %tmp.30, label %zil.trap, label %checked.14
checked.14:
  %v.56 = extractvalue { i1, i1, i32 } %v.52, 2
  %tmp.31 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.48, i32 %v.56)
  %v.57 = extractvalue { i32, i1 } %tmp.31, 0
  %tmp.32 = extractvalue { i32, i1 } %tmp.31, 1
  br i1 %tmp.32, label %zil.trap, label %checked.15
checked.15:
  store i32 %v.57, ptr %local.12, align 4
  br label %bb.15
bb.19:
  %v.69 = or i1 false, true
  store i1 %v.69, ptr %local.16, align 1
  br label %bb.21
bb.20:
  %v.70 = getelementptr i8, ptr %local.6, i64 0
  %v.71 = call i1 @zil.fn.10(ptr %v.70)
  %v.72 = xor i1 %v.71, true
  store i1 %v.72, ptr %local.16, align 1
  br label %bb.21
bb.21:
  %v.73 = load i1, ptr %local.16, align 1
  br i1 %v.73, label %bb.22, label %bb.23
bb.22:
  %v.74 = or i1 false, true
  %tmp.33 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.75 = insertvalue { i1, i1, i32 } %tmp.33, i1 %v.74, 1
  %tmp.34 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.34, label %resource.drop.16, label %resource.done.16
resource.drop.16:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.fn.3(ptr %local.7)
  br label %resource.done.16
resource.done.16:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.35 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.35, label %resource.drop.17, label %resource.done.17
resource.drop.17:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.bytes.drop(ptr %local.6)
  br label %resource.done.17
resource.done.17:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.36 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.36, label %resource.drop.18, label %resource.done.18
resource.drop.18:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.3(ptr %local.5)
  br label %resource.done.18
resource.done.18:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.37 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.37, label %resource.drop.19, label %resource.done.19
resource.drop.19:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.19
resource.done.19:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.38 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.38, label %resource.drop.20, label %resource.done.20
resource.drop.20:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.3(ptr %local.3)
  br label %resource.done.20
resource.done.20:
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.17, align 1
  ret { i1, i1, i32 } %v.75
bb.23:
  br label %bb.24
bb.24:
  %v.76 = load { ptr, i64, i64 }, ptr %local.6, align 8
  store i1 false, ptr %resource.live.6, align 1
  store { ptr, i64, i64 } %v.76, ptr %local.17, align 8
  store i1 true, ptr %resource.live.17, align 1
  %v.77 = load { ptr, i64, i64 }, ptr %local.17, align 8
  store i1 false, ptr %resource.live.17, align 1
  call void @zil.fn.11({ ptr, i64, i64 } %v.77)
  %tmp.39 = load i1, ptr %resource.live.17, align 1
  br i1 %tmp.39, label %resource.drop.21, label %resource.done.21
resource.drop.21:
  store i1 false, ptr %resource.live.17, align 1
  call void @zil.bytes.drop(ptr %local.17)
  br label %resource.done.21
resource.done.21:
  %v.79 = load i32, ptr %local.12, align 4
  %v.80 = load i32, ptr %local.1, align 4
  %v.81 = load i32, ptr %local.1, align 4
  %v.82 = load i32, ptr %local.1, align 4
  %v.83 = add i32 0, 1
  %tmp.40 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.82, i32 %v.83)
  %v.84 = extractvalue { i32, i1 } %tmp.40, 0
  %tmp.41 = extractvalue { i32, i1 } %tmp.40, 1
  br i1 %tmp.41, label %zil.trap, label %checked.22
checked.22:
  %tmp.42 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.81, i32 %v.84)
  %v.85 = extractvalue { i32, i1 } %tmp.42, 0
  %tmp.43 = extractvalue { i32, i1 } %tmp.42, 1
  br i1 %tmp.43, label %zil.trap, label %checked.23
checked.23:
  %v.86 = add i32 0, 2
  %tmp.44 = icmp eq i32 %v.86, 0
  %tmp.45 = icmp eq i32 %v.85, -2147483648
  %tmp.46 = icmp eq i32 %v.86, -1
  %tmp.47 = and i1 %tmp.45, %tmp.46
  %tmp.48 = or i1 %tmp.44, %tmp.47
  br i1 %tmp.48, label %zil.trap, label %checked.24
checked.24:
  %v.87 = sdiv i32 %v.85, %v.86
  %tmp.49 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.80, i32 %v.87)
  %v.88 = extractvalue { i32, i1 } %tmp.49, 0
  %tmp.50 = extractvalue { i32, i1 } %tmp.49, 1
  br i1 %tmp.50, label %zil.trap, label %checked.25
checked.25:
  %tmp.51 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.79, i32 %v.88)
  %v.89 = extractvalue { i32, i1 } %tmp.51, 0
  %tmp.52 = extractvalue { i32, i1 } %tmp.51, 1
  br i1 %tmp.52, label %zil.trap, label %checked.26
checked.26:
  %v.90 = load i32, ptr %local.15, align 4
  %tmp.53 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.89, i32 %v.90)
  %v.91 = extractvalue { i32, i1 } %tmp.53, 0
  %tmp.54 = extractvalue { i32, i1 } %tmp.53, 1
  br i1 %tmp.54, label %zil.trap, label %checked.27
checked.27:
  %v.92 = add i32 0, 65
  %tmp.55 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.91, i32 %v.92)
  %v.93 = extractvalue { i32, i1 } %tmp.55, 0
  %tmp.56 = extractvalue { i32, i1 } %tmp.55, 1
  br i1 %tmp.56, label %zil.trap, label %checked.28
checked.28:
  %tmp.57 = insertvalue { i1, i1, i32 } zeroinitializer, i1 false, 0
  %v.94 = insertvalue { i1, i1, i32 } %tmp.57, i32 %v.93, 2
  store i1 false, ptr %resource.live.17, align 1
  %tmp.58 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.58, label %resource.drop.29, label %resource.done.29
resource.drop.29:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.fn.3(ptr %local.7)
  br label %resource.done.29
resource.done.29:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.59 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.59, label %resource.drop.30, label %resource.done.30
resource.drop.30:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.bytes.drop(ptr %local.6)
  br label %resource.done.30
resource.done.30:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.60 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.60, label %resource.drop.31, label %resource.done.31
resource.drop.31:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.3(ptr %local.5)
  br label %resource.done.31
resource.done.31:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.61 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.61, label %resource.drop.32, label %resource.done.32
resource.drop.32:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.bytes.drop(ptr %local.4)
  br label %resource.done.32
resource.done.32:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.62 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.62, label %resource.drop.33, label %resource.done.33
resource.drop.33:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.3(ptr %local.3)
  br label %resource.done.33
resource.done.33:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, i1, i32 } %v.94
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

define { i32 } @zil.fn.4(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  ret { i32 } %v.2
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

define i32 @zil.fn.7(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.0, i32 0, i32 1
  %v.1 = load i64, ptr %tmp.0, align 8
  %tmp.1 = icmp slt i64 %v.1, -2147483648
  %tmp.2 = icmp sgt i64 %v.1, 2147483647
  %tmp.3 = or i1 %tmp.1, %tmp.2
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.2 = trunc i64 %v.1 to i32
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.8(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call i32 @zil.fn.7(ptr %v.0)
  ret i32 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.9(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = add i64 0, 0
  %v.2 = call i1 @zil.bytes.truncate(ptr %v.0, i64 %v.1)
  ret i1 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i1 @zil.fn.10(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call i1 @zil.fn.9(ptr %v.0)
  ret i1 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.11({ ptr, i64, i64 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { ptr, i64, i64 }, align 8
  store { ptr, i64, i64 } %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.bytes.drop(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, i1, i32 } @zil.fn.12(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i64, align 8
  %local.2 = alloca i64, align 8
  %local.3 = alloca i32, align 4
  %local.4 = alloca i1, align 1
  %local.5 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { ptr, i64, i64 }, ptr %v.0, i32 0, i32 1
  %v.1 = load i64, ptr %tmp.0, align 8
  store i64 %v.1, ptr %local.1, align 8
  %v.2 = load ptr, ptr %local.0, align 8
  %tmp.1 = getelementptr { ptr, i64, i64 }, ptr %v.2, i32 0, i32 2
  %v.3 = load i64, ptr %tmp.1, align 8
  store i64 %v.3, ptr %local.2, align 8
  %v.4 = add i32 0, 0
  store i32 %v.4, ptr %local.3, align 4
  %v.5 = load i64, ptr %local.1, align 8
  %v.6 = add i64 0, 0
  %v.7 = icmp sgt i64 %v.5, %v.6
  br i1 %v.7, label %bb.1, label %bb.2
bb.1:
  %v.8 = load ptr, ptr %local.0, align 8
  %tmp.2 = load ptr, ptr %v.8, align 8
  %tmp.3 = getelementptr { ptr, i64, i64 }, ptr %v.8, i32 0, i32 1
  %tmp.4 = load i64, ptr %tmp.3, align 8
  %tmp.5 = insertvalue { ptr, i64 } poison, ptr %tmp.2, 0
  %v.9 = insertvalue { ptr, i64 } %tmp.5, i64 %tmp.4, 1
  %v.10 = load i64, ptr %local.1, align 8
  %v.11 = add i64 0, 1
  %tmp.6 = call { i64, i1 } @llvm.ssub.with.overflow.i64(i64 %v.10, i64 %v.11)
  %v.12 = extractvalue { i64, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i64, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.0
checked.0:
  %tmp.8 = extractvalue { ptr, i64 } %v.9, 0
  %tmp.9 = extractvalue { ptr, i64 } %v.9, 1
  %tmp.10 = icmp uge i64 %v.12, %tmp.9
  br i1 %tmp.10, label %zil.trap, label %checked.1
checked.1:
  %v.13 = getelementptr i8, ptr %tmp.8, i64 %v.12
  %v.14 = getelementptr i8, ptr %v.13, i64 0
  %v.15 = load i8, ptr %v.14, align 1
  %v.16 = zext i8 %v.15 to i32
  store i32 %v.16, ptr %local.3, align 4
  br label %bb.3
bb.2:
  br label %bb.3
bb.3:
  %v.17 = load ptr, ptr %local.0, align 8
  %v.18 = load i64, ptr %local.1, align 8
  %v.19 = add i64 0, 1
  %tmp.11 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.18, i64 %v.19)
  %v.20 = extractvalue { i64, i1 } %tmp.11, 0
  %tmp.12 = extractvalue { i64, i1 } %tmp.11, 1
  br i1 %tmp.12, label %zil.trap, label %checked.2
checked.2:
  %v.21 = call i1 @zil.bytes.reserve(ptr %v.17, i64 %v.20)
  %v.22 = xor i1 %v.21, true
  br i1 %v.22, label %bb.4, label %bb.5
bb.4:
  %v.23 = load ptr, ptr %local.0, align 8
  %tmp.13 = getelementptr { ptr, i64, i64 }, ptr %v.23, i32 0, i32 1
  %v.24 = load i64, ptr %tmp.13, align 8
  %v.25 = load i64, ptr %local.1, align 8
  %v.26 = icmp ne i64 %v.24, %v.25
  br i1 %v.26, label %bb.6, label %bb.7
bb.5:
  br label %bb.18
bb.6:
  %v.27 = or i1 false, true
  store i1 %v.27, ptr %local.4, align 1
  br label %bb.8
bb.7:
  %v.28 = load ptr, ptr %local.0, align 8
  %tmp.14 = getelementptr { ptr, i64, i64 }, ptr %v.28, i32 0, i32 2
  %v.29 = load i64, ptr %tmp.14, align 8
  %v.30 = load i64, ptr %local.2, align 8
  %v.31 = icmp ne i64 %v.29, %v.30
  store i1 %v.31, ptr %local.4, align 1
  br label %bb.8
bb.8:
  %v.32 = load i1, ptr %local.4, align 1
  br i1 %v.32, label %bb.9, label %bb.10
bb.9:
  %v.33 = add i32 0, 88
  call void @zil.fn.5(i32 %v.33)
  br label %bb.11
bb.10:
  br label %bb.11
bb.11:
  %v.35 = load i64, ptr %local.1, align 8
  %v.36 = add i64 0, 0
  %v.37 = icmp sgt i64 %v.35, %v.36
  br i1 %v.37, label %bb.12, label %bb.13
bb.12:
  %v.38 = load ptr, ptr %local.0, align 8
  %tmp.15 = load ptr, ptr %v.38, align 8
  %tmp.16 = getelementptr { ptr, i64, i64 }, ptr %v.38, i32 0, i32 1
  %tmp.17 = load i64, ptr %tmp.16, align 8
  %tmp.18 = insertvalue { ptr, i64 } poison, ptr %tmp.15, 0
  %v.39 = insertvalue { ptr, i64 } %tmp.18, i64 %tmp.17, 1
  %v.40 = load i64, ptr %local.1, align 8
  %v.41 = add i64 0, 1
  %tmp.19 = call { i64, i1 } @llvm.ssub.with.overflow.i64(i64 %v.40, i64 %v.41)
  %v.42 = extractvalue { i64, i1 } %tmp.19, 0
  %tmp.20 = extractvalue { i64, i1 } %tmp.19, 1
  br i1 %tmp.20, label %zil.trap, label %checked.3
checked.3:
  %tmp.21 = extractvalue { ptr, i64 } %v.39, 0
  %tmp.22 = extractvalue { ptr, i64 } %v.39, 1
  %tmp.23 = icmp uge i64 %v.42, %tmp.22
  br i1 %tmp.23, label %zil.trap, label %checked.4
checked.4:
  %v.43 = getelementptr i8, ptr %tmp.21, i64 %v.42
  %v.44 = getelementptr i8, ptr %v.43, i64 0
  %v.45 = load i8, ptr %v.44, align 1
  %v.46 = zext i8 %v.45 to i32
  %v.47 = load i32, ptr %local.3, align 4
  %v.48 = icmp ne i32 %v.46, %v.47
  br i1 %v.48, label %bb.14, label %bb.15
bb.13:
  br label %bb.17
bb.14:
  %v.49 = add i32 0, 89
  call void @zil.fn.5(i32 %v.49)
  br label %bb.16
bb.15:
  br label %bb.16
bb.16:
  br label %bb.17
bb.17:
  %v.51 = or i1 false, true
  %tmp.24 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.52 = insertvalue { i1, i1, i32 } %tmp.24, i1 %v.51, 1
  ret { i1, i1, i32 } %v.52
bb.18:
  %v.53 = load ptr, ptr %local.0, align 8
  %v.54 = add i64 0, 66
  %v.55 = load i64, ptr %local.1, align 8
  %tmp.25 = call { i64, i1 } @llvm.sadd.with.overflow.i64(i64 %v.54, i64 %v.55)
  %v.56 = extractvalue { i64, i1 } %tmp.25, 0
  %tmp.26 = extractvalue { i64, i1 } %tmp.25, 1
  br i1 %tmp.26, label %zil.trap, label %checked.5
checked.5:
  %tmp.27 = icmp ugt i64 %v.56, 255
  br i1 %tmp.27, label %zil.trap, label %checked.6
checked.6:
  %v.57 = trunc i64 %v.56 to i8
  %v.58 = call i1 @zil.bytes.push(ptr %v.53, i8 %v.57)
  %v.59 = xor i1 %v.58, true
  br i1 %v.59, label %bb.19, label %bb.20
bb.19:
  %v.60 = or i1 false, true
  %tmp.28 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.61 = insertvalue { i1, i1, i32 } %tmp.28, i1 %v.60, 1
  ret { i1, i1, i32 } %v.61
bb.20:
  br label %bb.21
bb.21:
  %v.62 = load ptr, ptr %local.0, align 8
  %tmp.29 = getelementptr { ptr, i64, i64 }, ptr %v.62, i32 0, i32 1
  %v.63 = load i64, ptr %tmp.29, align 8
  %tmp.30 = icmp slt i64 %v.63, -2147483648
  %tmp.31 = icmp sgt i64 %v.63, 2147483647
  %tmp.32 = or i1 %tmp.30, %tmp.31
  br i1 %tmp.32, label %zil.trap, label %checked.7
checked.7:
  %v.64 = trunc i64 %v.63 to i32
  store i32 %v.64, ptr %local.5, align 4
  %v.65 = add i32 0, 48
  %v.66 = load i32, ptr %local.5, align 4
  %tmp.33 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.65, i32 %v.66)
  %v.67 = extractvalue { i32, i1 } %tmp.33, 0
  %tmp.34 = extractvalue { i32, i1 } %tmp.33, 1
  br i1 %tmp.34, label %zil.trap, label %checked.8
checked.8:
  call void @zil.fn.5(i32 %v.67)
  %v.69 = load i32, ptr %local.5, align 4
  %tmp.35 = insertvalue { i1, i1, i32 } zeroinitializer, i1 false, 0
  %v.70 = insertvalue { i1, i1, i32 } %tmp.35, i32 %v.69, 2
  ret { i1, i1, i32 } %v.70
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, i1, i32 } @zil.fn.13(%zil_env_0 %env) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  %v.0 = extractvalue %zil_env_0 %env, 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = extractvalue %zil_env_0 %env, 1
  store ptr %v.1, ptr %local.1, align 8
  %v.2 = add i32 0, 70
  %v.3 = call { i32 } @zil.fn.4(i32 %v.2)
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.3, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.4 = load ptr, ptr %local.0, align 8
  %v.5 = call i32 @zil.fn.8(ptr %v.4)
  %v.6 = load ptr, ptr %local.1, align 8
  %v.7 = call { i1, i1, i32 } @zil.fn.12(ptr %v.6)
  %v.8 = extractvalue { i1, i1, i32 } %v.7, 0
  br i1 %v.8, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, i1, i32 } %v.7, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.9 = extractvalue { i1, i1, i32 } %v.7, 1
  %tmp.2 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.10 = insertvalue { i1, i1, i32 } %tmp.2, i1 %v.9, 1
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.3(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, i1, i32 } %v.10
bb.2:
  %tmp.4 = extractvalue { i1, i1, i32 } %v.7, 0
  %tmp.5 = icmp ne i1 %tmp.4, false
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.11 = extractvalue { i1, i1, i32 } %v.7, 2
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  %tmp.8 = insertvalue { i1, i1, i32 } zeroinitializer, i1 false, 0
  %v.13 = insertvalue { i1, i1, i32 } %tmp.8, i32 %v.12, 2
  %tmp.9 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.9, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.3(ptr %local.2)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, i1, i32 } %v.13
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, i1, i32 } @zil.fn.14(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = load %zil_env_0, ptr %v.0, align 8
  %v.1 = call { i1, i1, i32 } @zil.fn.13(%zil_env_0 %tmp.0)
  ret { i1, i1, i32 } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, i1, i32 } @zil.fn.1()
  %entry.error = extractvalue { i1, i1, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, i1, i32 } %entry.result, 2
  ret i32 %entry.success
}

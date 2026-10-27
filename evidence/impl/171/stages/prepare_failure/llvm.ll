; ZIL verified LLVM module
; verified-fingerprint: 7bfa74d73ffa5b876fc761af8b2bf35be17311ff3b0b10bf3999b3fdd4aa266a
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.1(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32, i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  %v.2 = call i32 @putchar(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.2(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32, i32 }, ptr %v.0, i32 0, i32 1
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3({ i32, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i32, i32 }, align 4
  store { i32, i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = getelementptr i8, ptr %local.0, i64 0
  %tmp.0 = getelementptr { i32, i32 }, ptr %v.0, i32 0, i32 1
  %v.1 = load i32, ptr %tmp.0, align 4
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.fn.1(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.1
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
  %v.1 = getelementptr { i32, i32 }, ptr %v.0, i32 0, i32 1
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.5(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load i32, ptr %local.1, align 4
  %tmp.0 = getelementptr { i32, i32 }, ptr %v.0, i32 0, i32 1
  store i32 %v.1, ptr %tmp.0, align 4
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32, i32 }, ptr %v.0, i32 0, i32 1
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = load i32, ptr %v.2, align 4
  %v.4 = load i32, ptr %local.1, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.4)
  %v.5 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.7() {
entry:
  %enum.slot.0 = alloca { i32, [0 x i8] }, align 4
  br label %bb.0
bb.0:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.0, align 4
  %tmp.0 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.0, i32 0, i32 0
  store i32 0, ptr %tmp.0, align 4
  %tmp.1 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.0, i32 0, i32 1
  %v.0 = load { i32, [0 x i8] }, ptr %enum.slot.0, align 4
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.1 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.2, { i32, [0 x i8] } %v.0, 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.8() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32, i32 }, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 65
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 42
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = load i32, ptr %local.1, align 4
%tmp.0 = insertvalue { i32, i32 } zeroinitializer, i32 %v.2, 0
%v.4 = insertvalue { i32, i32 } %tmp.0, i32 %v.3, 1
  store i1 false, ptr %resource.live.2, align 1
  store { i32, i32 } %v.4, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.5 = getelementptr i8, ptr %local.2, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.7()
  %v.8 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.7, 0
  br i1 %v.8, label %bb.1, label %bb.2
bb.1:
  %tmp.1 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.7, 0
  %tmp.2 = icmp ne i1 %tmp.1, true
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.9 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.7, 1
  %tmp.3 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.10 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.3, { i32, [0 x i8] } %v.9, 1
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.10
bb.2:
  %tmp.5 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.7, 0
  %tmp.6 = icmp ne i1 %tmp.5, false
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  %v.11 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.7, 2
  %v.12 = call i32 @zil.fn.6(ptr %v.6, i32 %v.11)
  %tmp.7 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.13 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.7, i32 %v.12, 2
  %tmp.8 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.8, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.13
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.8()
  %entry.error = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 2
  ret i32 %entry.success
}

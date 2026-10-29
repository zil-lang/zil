; ZIL verified LLVM module
; verified-fingerprint: e0c4beb9a4a3247c638410b1fd10ac9b90deab1c31965c791148a35702f5f133
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.1(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32, { i8, i32, [2 x i32] } }, ptr %v.0, i32 0, i32 0
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
  %local.1 = alloca ptr, align 8
  %index.slot.12 = alloca [2 x i32], align 4
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = getelementptr { i32, { i8, i32, [2 x i32] } }, ptr %v.0, i32 0, i32 1
  %v.2 = getelementptr { i8, i32, [2 x i32] }, ptr %v.1, i32 0, i32 1
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  store ptr %v.3, ptr %local.1, align 8
  %v.4 = load ptr, ptr %local.1, align 8
  %v.5 = load i32, ptr %v.4, align 4
  %v.6 = load ptr, ptr %local.0, align 8
  %v.7 = getelementptr { i32, { i8, i32, [2 x i32] } }, ptr %v.6, i32 0, i32 1
  %v.8 = getelementptr i8, ptr %v.7, i64 0
  %v.9 = load { i8, i32, [2 x i32] }, ptr %v.8, align 4
%v.10 = extractvalue { i8, i32, [2 x i32] } %v.9, 2
  %v.11 = add i32 0, 1
  %tmp.0 = sext i32 %v.11 to i64
  %tmp.1 = icmp ult i64 %tmp.0, 2
  br i1 %tmp.1, label %index.ok.0, label %zil.trap
index.ok.0:
  store [2 x i32] %v.10, ptr %index.slot.12, align 4
  %tmp.2 = getelementptr [2 x i32], ptr %index.slot.12, i64 0, i64 %tmp.0
  %v.12 = load i32, ptr %tmp.2, align 4
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.13
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i8, align 1
  %local.2 = alloca i32, align 4
  %local.3 = alloca [2 x i32], align 4
  %local.4 = alloca { i8, i32, [2 x i32] }, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32, { i8, i32, [2 x i32] } }, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 65
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 1
  %tmp.0 = icmp ugt i32 %v.1, 255
  br i1 %tmp.0, label %zil.trap, label %checked.0
checked.0:
  %v.2 = trunc i32 %v.1 to i8
  store i8 %v.2, ptr %local.1, align 1
  %v.3 = add i32 0, 20
  store i32 %v.3, ptr %local.2, align 4
  %v.4 = add i32 0, 2
  %v.5 = add i32 0, 22
%tmp.1 = insertvalue [2 x i32] zeroinitializer, i32 %v.4, 0
%v.6 = insertvalue [2 x i32] %tmp.1, i32 %v.5, 1
  store [2 x i32] %v.6, ptr %local.3, align 4
  %v.7 = load i8, ptr %local.1, align 1
  %v.8 = load i32, ptr %local.2, align 4
  %v.9 = load [2 x i32], ptr %local.3, align 4
%tmp.2 = insertvalue { i8, i32, [2 x i32] } zeroinitializer, i8 %v.7, 0
%tmp.3 = insertvalue { i8, i32, [2 x i32] } %tmp.2, i32 %v.8, 1
%v.10 = insertvalue { i8, i32, [2 x i32] } %tmp.3, [2 x i32] %v.9, 2
  store { i8, i32, [2 x i32] } %v.10, ptr %local.4, align 4
  %v.11 = load i32, ptr %local.0, align 4
  %v.12 = load { i8, i32, [2 x i32] }, ptr %local.4, align 4
%tmp.4 = insertvalue { i32, { i8, i32, [2 x i32] } } zeroinitializer, i32 %v.11, 0
%v.13 = insertvalue { i32, { i8, i32, [2 x i32] } } %tmp.4, { i8, i32, [2 x i32] } %v.12, 1
  store i1 false, ptr %resource.live.5, align 1
  store { i32, { i8, i32, [2 x i32] } } %v.13, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.14 = getelementptr i8, ptr %local.5, i64 0
  %v.15 = getelementptr i8, ptr %v.14, i64 0
  %v.16 = getelementptr i8, ptr %v.15, i64 0
  %v.17 = call i32 @zil.fn.2(ptr %v.16)
  %v.18 = add i32 0, 42
  %tmp.5 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.17, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.1
checked.1:
  %tmp.7 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.7, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.1(ptr %local.5)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.5, align 1
  ret i32 %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.3()
  ret i32 %entry.result
}

; ZIL verified LLVM module
; verified-fingerprint: e39d68e831fc34f91f6edc11177cf3550ea56b7e16bdafd0041c9ddfaf3dd57b
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare i32 @getchar() nonlazybind
declare i32 @putchar(i32) nonlazybind
define i32 @zil.fn.2() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = call i32 @getchar()
  store i32 %v.1, ptr %local.0, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = add i32 0, 48
  %v.4 = icmp eq i32 %v.2, %v.3
  br i1 %v.4, label %bb.1, label %bb.2
bb.1:
  %v.5 = call i32 @zil.fn.5()
  store i32 %v.5, ptr %local.1, align 4
  %v.6 = load i32, ptr %local.1, align 4
  %v.7 = call i32 @putchar(i32 %v.6)
  %v.8 = add i32 0, 0
  ret i32 %v.8
bb.2:
  br label %bb.3
bb.3:
  %v.9 = load i32, ptr %local.0, align 4
  %v.10 = add i32 0, 49
  %v.11 = icmp eq i32 %v.9, %v.10
  br i1 %v.11, label %bb.4, label %bb.5
bb.4:
  %v.12 = call i32 @zil.fn.6()
  store i32 %v.12, ptr %local.2, align 4
  %v.13 = load i32, ptr %local.2, align 4
  %v.14 = call i32 @putchar(i32 %v.13)
  %v.15 = add i32 0, 0
  ret i32 %v.15
bb.5:
  br label %bb.6
bb.6:
  %v.16 = add i32 0, 7
  ret i32 %v.16
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.3({ i32, [2 x i32] } %arg.0) {
entry:
  %local.0 = alloca { i32, [2 x i32] }, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %index.slot.10 = alloca [2 x i32], align 4
  store { i32, [2 x i32] } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i32, [2 x i32] }, ptr %local.0, align 4
  %v.1 = extractvalue { i32, [2 x i32] } %v.0, 0
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = add i32 0, 0
  %v.3 = add i32 0, 2
  store i32 %v.2, ptr %local.2, align 4
  br label %bb.1
bb.1:
  %v.4 = load i32, ptr %local.2, align 4
  %v.5 = icmp slt i32 %v.4, %v.3
  br i1 %v.5, label %bb.2, label %bb.4
bb.2:
  store i32 %v.4, ptr %local.3, align 4
  %v.6 = load i32, ptr %local.1, align 4
  %v.7 = load { i32, [2 x i32] }, ptr %local.0, align 4
  %v.8 = extractvalue { i32, [2 x i32] } %v.7, 1
  %v.9 = load i32, ptr %local.3, align 4
  %tmp.0 = sext i32 %v.9 to i64
  %tmp.1 = icmp ult i64 %tmp.0, 2
  br i1 %tmp.1, label %index.ok.0, label %zil.trap
index.ok.0:
  store [2 x i32] %v.8, ptr %index.slot.10, align 4
  %tmp.2 = getelementptr [2 x i32], ptr %index.slot.10, i64 0, i64 %tmp.0
  %v.10 = load i32, ptr %tmp.2, align 4
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.11, ptr %local.1, align 4
  br label %bb.3
bb.3:
  %v.12 = add i32 0, 1
  %tmp.5 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.13, ptr %local.2, align 4
  br label %bb.1
bb.4:
  %v.14 = load i32, ptr %local.1, align 4
  ret i32 %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.4({ i32, [3 x i32] } %arg.0) {
entry:
  %local.0 = alloca { i32, [3 x i32] }, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %index.slot.10 = alloca [3 x i32], align 4
  store { i32, [3 x i32] } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i32, [3 x i32] }, ptr %local.0, align 4
  %v.1 = extractvalue { i32, [3 x i32] } %v.0, 0
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = add i32 0, 0
  %v.3 = add i32 0, 3
  store i32 %v.2, ptr %local.2, align 4
  br label %bb.1
bb.1:
  %v.4 = load i32, ptr %local.2, align 4
  %v.5 = icmp slt i32 %v.4, %v.3
  br i1 %v.5, label %bb.2, label %bb.4
bb.2:
  store i32 %v.4, ptr %local.3, align 4
  %v.6 = load i32, ptr %local.1, align 4
  %v.7 = load { i32, [3 x i32] }, ptr %local.0, align 4
  %v.8 = extractvalue { i32, [3 x i32] } %v.7, 1
  %v.9 = load i32, ptr %local.3, align 4
  %tmp.0 = sext i32 %v.9 to i64
  %tmp.1 = icmp ult i64 %tmp.0, 3
  br i1 %tmp.1, label %index.ok.0, label %zil.trap
index.ok.0:
  store [3 x i32] %v.8, ptr %index.slot.10, align 4
  %tmp.2 = getelementptr [3 x i32], ptr %index.slot.10, i64 0, i64 %tmp.0
  %v.10 = load i32, ptr %tmp.2, align 4
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.11, ptr %local.1, align 4
  br label %bb.3
bb.3:
  %v.12 = add i32 0, 1
  %tmp.5 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.13, ptr %local.2, align 4
  br label %bb.1
bb.4:
  %v.14 = load i32, ptr %local.1, align 4
  ret i32 %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.5() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca [2 x i32], align 4
  %local.2 = alloca { { i32, [2 x i32] }, { i32, [2 x i32] } }, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 65
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 4
  %v.2 = add i32 0, 5
  %tmp.0 = insertvalue [2 x i32] zeroinitializer, i32 %v.1, 0
  %v.3 = insertvalue [2 x i32] %tmp.0, i32 %v.2, 1
  store [2 x i32] %v.3, ptr %local.1, align 4
  %v.4 = load i32, ptr %local.0, align 4
  %v.5 = load [2 x i32], ptr %local.1, align 4
  %tmp.1 = insertvalue { i32, [2 x i32] } zeroinitializer, i32 %v.4, 0
  %v.6 = insertvalue { i32, [2 x i32] } %tmp.1, [2 x i32] %v.5, 1
  %v.7 = call { { i32, [2 x i32] }, { i32, [2 x i32] } } @zil.fn.7({ i32, [2 x i32] } %v.6)
  store { { i32, [2 x i32] }, { i32, [2 x i32] } } %v.7, ptr %local.2, align 4
  %v.8 = load { { i32, [2 x i32] }, { i32, [2 x i32] } }, ptr %local.2, align 4
  %v.9 = extractvalue { { i32, [2 x i32] }, { i32, [2 x i32] } } %v.8, 0
  %v.10 = call i32 @zil.fn.3({ i32, [2 x i32] } %v.9)
  %v.11 = load { { i32, [2 x i32] }, { i32, [2 x i32] } }, ptr %local.2, align 4
  %v.12 = extractvalue { { i32, [2 x i32] }, { i32, [2 x i32] } } %v.11, 1
  %v.13 = call i32 @zil.fn.3({ i32, [2 x i32] } %v.12)
  %v.14 = icmp ne i32 %v.10, %v.13
  br i1 %v.14, label %bb.1, label %bb.2
bb.1:
  %v.15 = add i32 0, 0
  ret i32 %v.15
bb.2:
  br label %bb.3
bb.3:
  %v.16 = load { { i32, [2 x i32] }, { i32, [2 x i32] } }, ptr %local.2, align 4
  %v.17 = extractvalue { { i32, [2 x i32] }, { i32, [2 x i32] } } %v.16, 1
  %v.18 = call i32 @zil.fn.3({ i32, [2 x i32] } %v.17)
  ret i32 %v.18
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.6() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca [3 x i32], align 4
  %local.2 = alloca { { i32, [3 x i32] }, { i32, [3 x i32] } }, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 66
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 6
  %v.2 = add i32 0, 7
  %v.3 = add i32 0, 8
  %tmp.0 = insertvalue [3 x i32] zeroinitializer, i32 %v.1, 0
  %tmp.1 = insertvalue [3 x i32] %tmp.0, i32 %v.2, 1
  %v.4 = insertvalue [3 x i32] %tmp.1, i32 %v.3, 2
  store [3 x i32] %v.4, ptr %local.1, align 4
  %v.5 = load i32, ptr %local.0, align 4
  %v.6 = load [3 x i32], ptr %local.1, align 4
  %tmp.2 = insertvalue { i32, [3 x i32] } zeroinitializer, i32 %v.5, 0
  %v.7 = insertvalue { i32, [3 x i32] } %tmp.2, [3 x i32] %v.6, 1
  %v.8 = call { { i32, [3 x i32] }, { i32, [3 x i32] } } @zil.fn.8({ i32, [3 x i32] } %v.7)
  store { { i32, [3 x i32] }, { i32, [3 x i32] } } %v.8, ptr %local.2, align 4
  %v.9 = load { { i32, [3 x i32] }, { i32, [3 x i32] } }, ptr %local.2, align 4
  %v.10 = extractvalue { { i32, [3 x i32] }, { i32, [3 x i32] } } %v.9, 0
  %v.11 = call i32 @zil.fn.4({ i32, [3 x i32] } %v.10)
  %v.12 = load { { i32, [3 x i32] }, { i32, [3 x i32] } }, ptr %local.2, align 4
  %v.13 = extractvalue { { i32, [3 x i32] }, { i32, [3 x i32] } } %v.12, 1
  %v.14 = call i32 @zil.fn.4({ i32, [3 x i32] } %v.13)
  %v.15 = icmp ne i32 %v.11, %v.14
  br i1 %v.15, label %bb.1, label %bb.2
bb.1:
  %v.16 = add i32 0, 0
  ret i32 %v.16
bb.2:
  br label %bb.3
bb.3:
  %v.17 = load { { i32, [3 x i32] }, { i32, [3 x i32] } }, ptr %local.2, align 4
  %v.18 = extractvalue { { i32, [3 x i32] }, { i32, [3 x i32] } } %v.17, 1
  %v.19 = call i32 @zil.fn.4({ i32, [3 x i32] } %v.18)
  ret i32 %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}
define { { i32, [2 x i32] }, { i32, [2 x i32] } } @zil.fn.7({ i32, [2 x i32] } %arg.0) {
entry:
  %local.0 = alloca { i32, [2 x i32] }, align 4
  store { i32, [2 x i32] } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i32, [2 x i32] }, ptr %local.0, align 4
  %v.1 = load { i32, [2 x i32] }, ptr %local.0, align 4
  %tmp.0 = insertvalue { { i32, [2 x i32] }, { i32, [2 x i32] } } zeroinitializer, { i32, [2 x i32] } %v.0, 0
  %v.2 = insertvalue { { i32, [2 x i32] }, { i32, [2 x i32] } } %tmp.0, { i32, [2 x i32] } %v.1, 1
  ret { { i32, [2 x i32] }, { i32, [2 x i32] } } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}
define { { i32, [3 x i32] }, { i32, [3 x i32] } } @zil.fn.8({ i32, [3 x i32] } %arg.0) {
entry:
  %local.0 = alloca { i32, [3 x i32] }, align 4
  store { i32, [3 x i32] } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i32, [3 x i32] }, ptr %local.0, align 4
  %v.1 = load { i32, [3 x i32] }, ptr %local.0, align 4
  %tmp.0 = insertvalue { { i32, [3 x i32] }, { i32, [3 x i32] } } zeroinitializer, { i32, [3 x i32] } %v.0, 0
  %v.2 = insertvalue { { i32, [3 x i32] }, { i32, [3 x i32] } } %tmp.0, { i32, [3 x i32] } %v.1, 1
  ret { { i32, [3 x i32] }, { i32, [3 x i32] } } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.2()
  ret i32 %entry.result
}

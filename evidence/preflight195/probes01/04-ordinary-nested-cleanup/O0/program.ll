; ZIL verified LLVM module
; verified-fingerprint: 22e83f94228f007856efe313bdb53d8f858603a011c1d5e1429e12d92995024b
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.1(i32 %arg.0) {
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

define void @zil.fn.2(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.1(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %local.4 = alloca i32, align 4
  %local.5 = alloca i32, align 4
  %local.6 = alloca i32, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  %v.1 = add i32 0, 2
  store i32 %v.0, ptr %local.0, align 4
  br label %bb.1
bb.1:
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = icmp slt i32 %v.2, %v.1
  br i1 %v.3, label %bb.2, label %bb.4
bb.2:
  store i32 %v.2, ptr %local.1, align 4
  %v.4 = add i32 0, 65
  %v.5 = load i32, ptr %local.1, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.6, ptr %local.2, align 4
  %v.7 = load i32, ptr %local.2, align 4
%v.8 = insertvalue { i32 } zeroinitializer, i32 %v.7, 0
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.8, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.9 = add i32 0, 0
  %v.10 = add i32 0, 3
  store i32 %v.9, ptr %local.4, align 4
  br label %bb.5
bb.3:
  %v.23 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.2, i32 %v.23)
  %v.24 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.24, ptr %local.0, align 4
  br label %bb.1
bb.4:
  %v.25 = add i32 0, 0
  ret i32 %v.25
bb.5:
  %v.11 = load i32, ptr %local.4, align 4
  %v.12 = icmp slt i32 %v.11, %v.10
  br i1 %v.12, label %bb.6, label %bb.8
bb.6:
  store i32 %v.11, ptr %local.5, align 4
  %v.13 = add i32 0, 48
  %v.14 = load i32, ptr %local.5, align 4
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.15, ptr %local.6, align 4
  %v.16 = load i32, ptr %local.6, align 4
%v.17 = insertvalue { i32 } zeroinitializer, i32 %v.16, 0
  store i1 false, ptr %resource.live.7, align 1
  store { i32 } %v.17, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.18 = load i32, ptr %local.5, align 4
  %v.19 = add i32 0, 0
  %v.20 = icmp eq i32 %v.18, %v.19
  br i1 %v.20, label %bb.9, label %bb.10
bb.7:
  %v.21 = add i32 0, 1
  %tmp.6 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.11, i32 %v.21)
  %v.22 = extractvalue { i32, i1 } %tmp.6, 0
  %tmp.7 = extractvalue { i32, i1 } %tmp.6, 1
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  store i32 %v.22, ptr %local.4, align 4
  br label %bb.5
bb.8:
  %tmp.8 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.8, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.2(ptr %local.3)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.3, align 1
  br label %bb.3
bb.9:
  %tmp.9 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.9, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.fn.2(ptr %local.7)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.7, align 1
  br label %bb.7
bb.10:
  br label %bb.11
bb.11:
  %tmp.10 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.10, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.fn.2(ptr %local.7)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.7, align 1
  br label %bb.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.3()
  ret i32 %entry.result
}

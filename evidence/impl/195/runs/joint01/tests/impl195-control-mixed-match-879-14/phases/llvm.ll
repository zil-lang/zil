; ZIL verified LLVM module
; verified-fingerprint: 00853461030dc07c25ff6dd6817a36ee39d38557a1d30a2e24364b6793dd1031
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @getchar() nonlazybind
define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
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
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.3(i32 %arg.0) {
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

define void @zil.fn.4(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.3(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca %zil_env_0, align 4
  %local.3 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = call i32 @zil.fn.1()
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.1, align 4
  %v.3 = insertvalue %zil_env_0 zeroinitializer, i32 %v.2, 0
  store %zil_env_0 %v.3, ptr %local.2, align 4
  %v.4 = getelementptr i8, ptr %local.2, i64 0
  %v.5 = getelementptr i8, ptr %v.4, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = load i32, ptr %local.0, align 4
  %v.8 = call i32 @zil.fn.7(ptr %v.6, i32 %v.7)
  store i32 %v.8, ptr %local.3, align 4
  %v.9 = add i32 0, 65
  %v.10 = load i32, ptr %local.3, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  call void @zil.fn.3(i32 %v.11)
  %v.13 = add i32 0, 0
  ret i32 %v.13
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6(ptr %env) {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %tmp.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  %v.0 = load i32, ptr %tmp.0, align 4
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = add i32 0, 1
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.1, i32 %v.2)
  %v.3 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.3, ptr %local.0, align 4
  %tmp.3 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store i32 %v.3, ptr %tmp.3, align 4
  %v.4 = add i32 0, 64
  %v.5 = load i32, ptr %local.0, align 4
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  call void @zil.fn.3(i32 %v.6)
  %v.8 = load i32, ptr %local.0, align 4
  ret i32 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.7(ptr %arg.0, i32 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.2, align 4
  %v.1 = add i32 0, 0
  %v.2 = load i32, ptr %local.1, align 4
  store i32 %v.1, ptr %local.3, align 4
  br label %bb.1
bb.1:
  %v.3 = load i32, ptr %local.3, align 4
  %v.4 = icmp slt i32 %v.3, %v.2
  br i1 %v.4, label %bb.2, label %bb.4
bb.2:
  store i32 %v.3, ptr %local.4, align 4
  %v.5 = add i32 0, 48
  %v.6 = load i32, ptr %local.4, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.7, ptr %local.5, align 4
  %v.8 = load i32, ptr %local.5, align 4
%v.9 = insertvalue { i32 } zeroinitializer, i32 %v.8, 0
  store i1 false, ptr %resource.live.6, align 1
  store { i32 } %v.9, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.10 = load i32, ptr %local.4, align 4
  %v.11 = add i32 0, 0
  %v.12 = icmp eq i32 %v.10, %v.11
  %v.13 = or i1 false, true
  %v.14 = icmp eq i1 %v.12, %v.13
  br i1 %v.14, label %bb.6, label %bb.5
bb.3:
  %v.29 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.29)
  %v.30 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.30, ptr %local.3, align 4
  br label %bb.1
bb.4:
  %v.31 = load i32, ptr %local.2, align 4
  ret i32 %v.31
bb.5:
  %v.20 = or i1 false, false
  %v.21 = icmp eq i1 %v.12, %v.20
  br i1 %v.21, label %bb.10, label %bb.9
bb.6:
  %v.15 = load i32, ptr %local.1, align 4
  %v.16 = add i32 0, 1
  %v.17 = icmp eq i32 %v.15, %v.16
  br i1 %v.17, label %bb.7, label %bb.8
bb.7:
  %v.18 = load ptr, ptr %local.0, align 8
  %v.19 = call i32 @zil.fn.6(ptr %v.18)
  %tmp.4 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.4(ptr %local.6)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.6, align 1
  ret i32 %v.19
bb.8:
  %tmp.5 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.4(ptr %local.6)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.6, align 1
  br label %bb.3
bb.9:
  br label %zil.trap
bb.10:
  %v.22 = load i32, ptr %local.4, align 4
  %v.23 = add i32 0, 3
  %v.24 = icmp eq i32 %v.22, %v.23
  br i1 %v.24, label %bb.11, label %bb.12
bb.11:
  %tmp.6 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.6, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.4(ptr %local.6)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.6, align 1
  br label %bb.4
bb.12:
  br label %bb.13
bb.13:
  %v.25 = load i32, ptr %local.2, align 4
  %v.26 = load ptr, ptr %local.0, align 8
  %v.27 = call i32 @zil.fn.6(ptr %v.26)
  %tmp.7 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.25, i32 %v.27)
  %v.28 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.5
checked.5:
  store i32 %v.28, ptr %local.2, align 4
  br label %bb.14
bb.14:
  %tmp.9 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.9, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.4(ptr %local.6)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.6, align 1
  br label %bb.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.5()
  ret i32 %entry.result
}

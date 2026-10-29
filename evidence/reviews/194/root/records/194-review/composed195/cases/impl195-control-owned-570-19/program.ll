; ZIL verified LLVM module
; verified-fingerprint: 1393527d45cc0700f584606e8b3048130c8d006e48bf9df01e99aca1028ab4ea
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32 }, i1 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  call void @zil.fn.4(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)
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
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca %zil_env_0, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_0, align 4
  %local.5 = alloca i32, align 4
  %local.6 = alloca i32, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  %v.0 = call i32 @zil.fn.1()
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 72
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.1, align 4
%v.3 = insertvalue { i32 } zeroinitializer, i32 %v.2, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.3, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.4 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { i32 } %v.4, 0
  %v.5 = insertvalue %zil_env_0 %tmp.0, i1 true, 1
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_0 %v.5, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.6 = load %zil_env_0, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_0 %v.6, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.7 = load i32, ptr %local.0, align 4
  %v.8 = load %zil_env_0, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  %v.9 = call i32 @zil.fn.7(%zil_env_0 %v.8, i32 %v.7)
  store i32 %v.9, ptr %local.5, align 4
  %tmp.1 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.1, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.0(ptr %local.4)
  br label %closure.done.0
closure.done.0:
  %v.10 = add i32 0, 90
  call void @zil.fn.3(i32 %v.10)
  %v.12 = add i32 0, 0
  store i32 %v.12, ptr %local.6, align 4
  %v.13 = load i32, ptr %local.0, align 4
  %v.14 = add i32 0, 1
  %v.15 = icmp sgt i32 %v.13, %v.14
  br i1 %v.15, label %bb.1, label %bb.2
bb.1:
  %v.16 = add i32 0, 1
  store i32 %v.16, ptr %local.6, align 4
  br label %bb.3
bb.2:
  br label %bb.3
bb.3:
  %v.17 = load i32, ptr %local.0, align 4
  %v.18 = add i32 0, 2
  %v.19 = icmp sgt i32 %v.17, %v.18
  br i1 %v.19, label %bb.4, label %bb.5
bb.4:
  %v.20 = add i32 0, 2
  store i32 %v.20, ptr %local.6, align 4
  br label %bb.6
bb.5:
  br label %bb.6
bb.6:
  %v.21 = load i32, ptr %local.5, align 4
  %v.22 = add i32 0, 72
  %v.23 = load i32, ptr %local.6, align 4
  %tmp.2 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.22, i32 %v.23)
  %v.24 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %tmp.4 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.21, i32 %v.24)
  %v.25 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.6 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.6, label %closure.drop.3, label %closure.done.3
closure.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.0(ptr %local.3)
  br label %closure.done.3
closure.done.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.7 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.7, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.4(ptr %local.2)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.25
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.6(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = add i32 0, 67
  call void @zil.fn.3(i32 %v.1)
  %v.3 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.3, i32 0, i32 0
  %v.4 = load i32, ptr %tmp.0, align 4
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.7(%zil_env_0 %arg.0, i32 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca i32, align 4
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  store %zil_env_0 %arg.0, ptr %local.0, align 4
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
  br i1 %v.12, label %bb.5, label %bb.6
bb.3:
  %v.19 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.19)
  %v.20 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  store i32 %v.20, ptr %local.3, align 4
  br label %bb.1
bb.4:
  %v.21 = load i32, ptr %local.2, align 4
  %tmp.4 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.4, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.21
bb.5:
  %tmp.5 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.4(ptr %local.6)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.6, align 1
  br label %bb.3
bb.6:
  br label %bb.7
bb.7:
  %v.13 = load i32, ptr %local.4, align 4
  %v.14 = add i32 0, 3
  %v.15 = icmp eq i32 %v.13, %v.14
  br i1 %v.15, label %bb.8, label %bb.9
bb.8:
  %tmp.6 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.6, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.4(ptr %local.6)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.6, align 1
  br label %bb.4
bb.9:
  br label %bb.10
bb.10:
  %v.16 = load i32, ptr %local.2, align 4
  %v.17 = call i32 @zil.fn.6(ptr %local.0)
  %tmp.7 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.16, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.5
checked.5:
  store i32 %v.18, ptr %local.2, align 4
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

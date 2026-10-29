; ZIL verified LLVM module
; verified-fingerprint: e37e1f7909de90a1a87fcafc67a7fd49ed28efaa727f73e12849b9a8e7e9133d
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
  call void @zil.fn.1(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

declare i32 @putchar(i32) nonlazybind
define void @zil.fn.1(ptr %arg.0) {
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

define { i1, { { i32 } }, { { i32 } } } @zil.fn.2(i1 %arg.0) {
entry:
  %local.0 = alloca i1, align 1
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32 }, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  store i1 %arg.0, ptr %local.0, align 1
  br label %bb.0
bb.0:
  %v.0 = load i1, ptr %local.0, align 1
  br i1 %v.0, label %bb.1, label %bb.2
bb.1:
  store i1 false, ptr %resource.live.3, align 1
  %v.1 = add i32 0, 68
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.1, align 4
%v.3 = insertvalue { i32 } zeroinitializer, i32 %v.2, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.3, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.4 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.4, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.5 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
%v.6 = insertvalue { { i32 } } zeroinitializer, { i32 } %v.5, 0
  %tmp.0 = insertvalue { i1, { { i32 } }, { { i32 } } } zeroinitializer, i1 true, 0
  %v.7 = insertvalue { i1, { { i32 } }, { { i32 } } } %tmp.0, { { i32 } } %v.6, 1
  %tmp.1 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.1(ptr %local.3)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { { i32 } }, { { i32 } } } %v.7
bb.2:
  br label %bb.3
bb.3:
  store i1 false, ptr %resource.live.6, align 1
  %v.8 = add i32 0, 69
  store i32 %v.8, ptr %local.4, align 4
  %v.9 = load i32, ptr %local.4, align 4
%v.10 = insertvalue { i32 } zeroinitializer, i32 %v.9, 0
  store i1 false, ptr %resource.live.5, align 1
  store { i32 } %v.10, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.11 = load { i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  store { i32 } %v.11, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.12 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
%v.13 = insertvalue { { i32 } } zeroinitializer, { i32 } %v.12, 0
  %tmp.3 = insertvalue { i1, { { i32 } }, { { i32 } } } zeroinitializer, i1 false, 0
  %v.14 = insertvalue { i1, { { i32 } }, { { i32 } } } %tmp.3, { { i32 } } %v.13, 2
  %tmp.4 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.1(ptr %local.6)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.5 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.5, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.1(ptr %local.5)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.5, align 1
  ret { i1, { { i32 } }, { { i32 } } } %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { { i32 } }, { { i32 } } } @zil.fn.3({ i1, { { i32 } }, { { i32 } } } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i1, { { i32 } }, { { i32 } } }, align 4
  store { i1, { { i32 } }, { { i32 } } } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i1, { { i32 } }, { { i32 } } }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  %tmp.0 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.0, label %error.drop.0, label %error.done.0
error.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  %tmp.1 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.0, i32 0, i32 0
  %tmp.2 = load i1, ptr %tmp.1, align 1
  br i1 %tmp.2, label %error.payload.0, label %success.payload.0
error.payload.0:
  %tmp.3 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.0, i32 0, i32 1
  %tmp.4 = getelementptr { { i32 } }, ptr %tmp.3, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.4)
  br label %error.done.0
success.payload.0:
  %tmp.5 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.0, i32 0, i32 2
  %tmp.6 = getelementptr { { i32 } }, ptr %tmp.5, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.6)
  br label %error.done.0
error.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, { { i32 } }, { { i32 } } } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.4() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 67
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.1, align 1
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { i32 } %v.3, 0
  %v.4 = insertvalue %zil_env_0 %tmp.0, i1 true, 1
  %tmp.1 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.1, align 1
  ret %zil_env_0 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.5() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_0, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i1, { { i32 } }, { { i32 } } }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i1, { { i32 } }, { { i32 } } }, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i1, { { i32 } }, { { i32 } } }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i1, { { i32 } }, { { i32 } } }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %v.0 = call %zil_env_0 @zil.fn.4()
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_0 %v.0, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.1 = or i1 false, true
  %v.2 = call { i1, { { i32 } }, { { i32 } } } @zil.fn.2(i1 %v.1)
  store { i1, { { i32 } }, { { i32 } } } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = load { i1, { { i32 } }, { { i32 } } }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.4 = call { i1, { { i32 } }, { { i32 } } } @zil.fn.3({ i1, { { i32 } }, { { i32 } } } %v.3)
  store { i1, { { i32 } }, { { i32 } } } %v.4, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %error.drop.0, label %error.done.0
error.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.1 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.2, i32 0, i32 0
  %tmp.2 = load i1, ptr %tmp.1, align 1
  br i1 %tmp.2, label %error.payload.0, label %success.payload.0
error.payload.0:
  %tmp.3 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.2, i32 0, i32 1
  %tmp.4 = getelementptr { { i32 } }, ptr %tmp.3, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.4)
  br label %error.done.0
success.payload.0:
  %tmp.5 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.2, i32 0, i32 2
  %tmp.6 = getelementptr { { i32 } }, ptr %tmp.5, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.6)
  br label %error.done.0
error.done.0:
  %tmp.7 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.7, label %error.drop.1, label %error.done.1
error.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.8 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.1, i32 0, i32 0
  %tmp.9 = load i1, ptr %tmp.8, align 1
  br i1 %tmp.9, label %error.payload.1, label %success.payload.1
error.payload.1:
  %tmp.10 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.1, i32 0, i32 1
  %tmp.11 = getelementptr { { i32 } }, ptr %tmp.10, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.11)
  br label %error.done.1
success.payload.1:
  %tmp.12 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.1, i32 0, i32 2
  %tmp.13 = getelementptr { { i32 } }, ptr %tmp.12, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.13)
  br label %error.done.1
error.done.1:
  %v.5 = or i1 false, false
  %v.6 = call { i1, { { i32 } }, { { i32 } } } @zil.fn.2(i1 %v.5)
  store { i1, { { i32 } }, { { i32 } } } %v.6, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.7 = load { i1, { { i32 } }, { { i32 } } }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  %v.8 = call { i1, { { i32 } }, { { i32 } } } @zil.fn.3({ i1, { { i32 } }, { { i32 } } } %v.7)
  store { i1, { { i32 } }, { { i32 } } } %v.8, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %tmp.14 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.14, label %error.drop.2, label %error.done.2
error.drop.2:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.15 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.4, i32 0, i32 0
  %tmp.16 = load i1, ptr %tmp.15, align 1
  br i1 %tmp.16, label %error.payload.2, label %success.payload.2
error.payload.2:
  %tmp.17 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.4, i32 0, i32 1
  %tmp.18 = getelementptr { { i32 } }, ptr %tmp.17, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.18)
  br label %error.done.2
success.payload.2:
  %tmp.19 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.4, i32 0, i32 2
  %tmp.20 = getelementptr { { i32 } }, ptr %tmp.19, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.20)
  br label %error.done.2
error.done.2:
  %tmp.21 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.21, label %error.drop.3, label %error.done.3
error.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.22 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.3, i32 0, i32 0
  %tmp.23 = load i1, ptr %tmp.22, align 1
  br i1 %tmp.23, label %error.payload.3, label %success.payload.3
error.payload.3:
  %tmp.24 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.3, i32 0, i32 1
  %tmp.25 = getelementptr { { i32 } }, ptr %tmp.24, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.25)
  br label %error.done.3
success.payload.3:
  %tmp.26 = getelementptr { i1, { { i32 } }, { { i32 } } }, ptr %local.3, i32 0, i32 2
  %tmp.27 = getelementptr { { i32 } }, ptr %tmp.26, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.27)
  br label %error.done.3
error.done.3:
  %v.9 = call i32 @zil.fn.6(ptr %local.0)
  %v.10 = add i32 0, 67
  %tmp.28 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.28, 0
  %tmp.29 = extractvalue { i32, i1 } %tmp.28, 1
  br i1 %tmp.29, label %zil.trap, label %checked.4
checked.4:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %resource.live.3, align 1
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  %tmp.30 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.30, label %closure.drop.5, label %closure.done.5
closure.drop.5:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.0(ptr %local.0)
  br label %closure.done.5
closure.done.5:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.11
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
  %v.1 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.1, i32 0, i32 0
  %v.2 = load i32, ptr %tmp.0, align 4
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.5()
  ret i32 %entry.result
}

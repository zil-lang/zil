; ZIL verified LLVM module
; verified-fingerprint: f131c7779bfd9402e9e5c6b5be26614a9fbc7ceedfab66c7ec62220dfb7ade85
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

define { i1, i1, i32 } @zil.fn.2() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 65
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
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_0 %v.4, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.5 = getelementptr i8, ptr %local.2, i64 0
  %v.6 = getelementptr i8, ptr %v.5, i64 0
  %v.7 = getelementptr i8, ptr %v.6, i64 0
  %v.8 = add i32 0, 7
  %v.9 = or i1 false, true
  %v.10 = call { i1, i1, i32 } @zil.fn.4(ptr %v.7, i32 %v.8, i1 %v.9)
  %v.11 = extractvalue { i1, i1, i32 } %v.10, 0
  br i1 %v.11, label %bb.1, label %bb.2
bb.1:
  %tmp.1 = extractvalue { i1, i1, i32 } %v.10, 0
  %tmp.2 = icmp ne i1 %tmp.1, true
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.12 = extractvalue { i1, i1, i32 } %v.10, 1
  %tmp.3 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.13 = insertvalue { i1, i1, i32 } %tmp.3, i1 %v.12, 1
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.5 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.5, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, i1, i32 } %v.13
bb.2:
  %tmp.6 = extractvalue { i1, i1, i32 } %v.10, 0
  %tmp.7 = icmp ne i1 %tmp.6, false
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  %v.14 = extractvalue { i1, i1, i32 } %v.10, 2
  %v.15 = add i32 0, 7
  %tmp.8 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.14, i32 %v.15)
  %v.16 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.4
checked.4:
  %tmp.10 = insertvalue { i1, i1, i32 } zeroinitializer, i1 false, 0
  %v.17 = insertvalue { i1, i1, i32 } %tmp.10, i32 %v.16, 2
  %tmp.11 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.11, label %closure.drop.5, label %closure.done.5
closure.drop.5:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.0(ptr %local.2)
  br label %closure.done.5
closure.done.5:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.12 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.12, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.1(ptr %local.1)
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, i1, i32 } %v.17
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.3(ptr %env, i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.1, align 8
  %v.1 = load ptr, ptr %local.1, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.1, i32 0, i32 0
  %v.2 = load i32, ptr %tmp.0, align 4
  %v.3 = load i32, ptr %local.0, align 4
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, i1, i32 } @zil.fn.4(ptr %arg.0, i32 %arg.1, i1 %arg.2) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %local.2 = alloca i1, align 1
  %local.3 = alloca i32, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i32 %arg.1, ptr %local.1, align 4
  store i1 %arg.2, ptr %local.2, align 1
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 66
  store i32 %v.0, ptr %local.3, align 4
  %v.1 = load i32, ptr %local.3, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.4, align 1
  store { i32 } %v.2, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.3 = load i1, ptr %local.2, align 1
  br i1 %v.3, label %bb.1, label %bb.2
bb.1:
  %v.4 = or i1 false, false
  %tmp.0 = insertvalue { i1, i1, i32 } zeroinitializer, i1 true, 0
  %v.5 = insertvalue { i1, i1, i32 } %tmp.0, i1 %v.4, 1
  %tmp.1 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.1(ptr %local.4)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.4, align 1
  ret { i1, i1, i32 } %v.5
bb.2:
  br label %bb.3
bb.3:
  %v.6 = load ptr, ptr %local.0, align 8
  %v.7 = load i32, ptr %local.1, align 4
  %v.8 = call i32 @zil.fn.3(ptr %v.6, i32 %v.7)
  %tmp.2 = insertvalue { i1, i1, i32 } zeroinitializer, i1 false, 0
  %v.9 = insertvalue { i1, i1, i32 } %tmp.2, i32 %v.8, 2
  %tmp.3 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.fn.1(ptr %local.4)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.4, align 1
  ret { i1, i1, i32 } %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, i1, i32 } @zil.fn.2()
  %entry.error = extractvalue { i1, i1, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, i1, i32 } %entry.result, 2
  ret i32 %entry.success
}

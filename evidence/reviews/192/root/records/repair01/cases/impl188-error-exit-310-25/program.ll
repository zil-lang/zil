; ZIL verified LLVM module
; verified-fingerprint: 8c0a097f542b26c672418bad89849f10c37b5caf188e22a89517ad71aa8c70b0
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
  %tmp.0 = getelementptr { i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  %v.2 = call i32 @putchar(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.2() {
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
  %v.8 = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.6(ptr %v.7)
  %v.9 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.8, 0
  br i1 %v.9, label %bb.1, label %bb.2
bb.1:
  %tmp.1 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.8, 0
  %tmp.2 = icmp ne i1 %tmp.1, true
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.10 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.8, 1
  %tmp.3 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.11 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.3, { i32, [0 x i8] } %v.10, 1
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
  ret { i1, { i32, [0 x i8] }, i32 } %v.11
bb.2:
  %tmp.6 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.8, 0
  %tmp.7 = icmp ne i1 %tmp.6, false
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  %v.12 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.8, 2
  %v.13 = add i32 0, 14
  %tmp.8 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.12, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.8, 0
  %tmp.9 = extractvalue { i32, i1 } %tmp.8, 1
  br i1 %tmp.9, label %zil.trap, label %checked.4
checked.4:
  %tmp.10 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.15 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.10, i32 %v.14, 2
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
  ret { i1, { i32, [0 x i8] }, i32 } %v.15
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.3(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %enum.slot.4 = alloca { i32, [0 x i8] }, align 4
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32 }, ptr %v.1, i32 0, i32 0
  %v.2 = load i32, ptr %tmp.0, align 4
  store i32 %v.2, ptr %local.1, align 4
  %v.3 = or i1 false, true
  br i1 %v.3, label %bb.1, label %bb.2
bb.1:
  store { i32, [0 x i8] } zeroinitializer, ptr %enum.slot.4, align 4
  %tmp.1 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.4, i32 0, i32 0
  store i32 0, ptr %tmp.1, align 4
  %tmp.2 = getelementptr { i32, [0 x i8] }, ptr %enum.slot.4, i32 0, i32 1
  %v.4 = load { i32, [0 x i8] }, ptr %enum.slot.4, align 4
  %tmp.3 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.5 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.3, { i32, [0 x i8] } %v.4, 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.5
bb.2:
  br label %bb.3
bb.3:
  %v.6 = add i32 0, 7
  %tmp.4 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.7 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.4, i32 %v.6, 2
  ret { i1, { i32, [0 x i8] }, i32 } %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.4(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.3(ptr %v.0)
  ret { i1, { i32, [0 x i8] }, i32 } %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.5(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 66
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load ptr, ptr %local.0, align 8
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.4(ptr %v.4)
  %v.6 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.5, 0
  br i1 %v.6, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.5, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.7 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.5, 1
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.8 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.2, { i32, [0 x i8] } %v.7, 1
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.8
bb.2:
  %tmp.4 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.5, 0
  %tmp.5 = icmp ne i1 %tmp.4, false
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.9 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.5, 2
  store i32 %v.9, ptr %local.3, align 4
  %v.10 = load ptr, ptr %local.0, align 8
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.4(ptr %v.11)
  %v.13 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 0
  br i1 %v.13, label %bb.3, label %bb.4
bb.3:
  %tmp.6 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 0
  %tmp.7 = icmp ne i1 %tmp.6, true
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  %v.14 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 1
  %tmp.8 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.15 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.8, { i32, [0 x i8] } %v.14, 1
  %tmp.9 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.9, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.15
bb.4:
  %tmp.10 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 0
  %tmp.11 = icmp ne i1 %tmp.10, false
  br i1 %tmp.11, label %zil.trap, label %checked.5
checked.5:
  %v.16 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.12, 2
  %v.17 = load i32, ptr %local.3, align 4
  %tmp.12 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.16, i32 %v.17)
  %v.18 = extractvalue { i32, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i32, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.6
checked.6:
  %tmp.14 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.19 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.14, i32 %v.18, 2
  %tmp.15 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.15, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.7
resource.done.7:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [0 x i8] }, i32 } @zil.fn.6(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 67
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load ptr, ptr %local.0, align 8
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.5(ptr %v.4)
  %v.6 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.5, 0
  br i1 %v.6, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.5, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.7 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.5, 1
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.8 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.2, { i32, [0 x i8] } %v.7, 1
  %tmp.3 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.8
bb.2:
  %tmp.4 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.5, 0
  %tmp.5 = icmp ne i1 %tmp.4, false
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.9 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.5, 2
  %tmp.6 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.10 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.6, i32 %v.9, 2
  %tmp.7 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.7, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.1(ptr %local.2)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.2, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.10
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.2()
  %entry.error = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, { i32, [0 x i8] }, i32 } %entry.result, 2
  ret i32 %entry.success
}

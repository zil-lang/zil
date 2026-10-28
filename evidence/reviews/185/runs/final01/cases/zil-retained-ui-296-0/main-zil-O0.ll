; ZIL verified LLVM module
; verified-fingerprint: 2306a89f00f4e19dc037c0aa8d01a723545520cb23a492fe5a66a7faf4011736
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type { { i32, i32 }, i1 }
%zil_env_1 = type { { i32, i32 }, i1 }
%zil_env_2 = type { { i32, i32 }, i1 }
%zil_env_3 = type { { i32, i32 }, { i32, i32 }, i1, i1 }
%zil_env_4 = type { { i32, i32 }, i32, i1 }
%zil_env_5 = type { i32 }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  call void @zil.fn.0(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
define internal void @zil.closure.drop.1(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  call void @zil.fn.14(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
define internal void @zil.closure.drop.2(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_2, ptr %env, i32 0, i32 1
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_2, ptr %env, i32 0, i32 0
  call void @zil.fn.14(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
define internal void @zil.closure.drop.3(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_3, ptr %env, i32 0, i32 2
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_3, ptr %env, i32 0, i32 0
  call void @zil.fn.14(ptr %payload.0)
  br label %done.0
done.0:
  %flag.ptr.1 = getelementptr %zil_env_3, ptr %env, i32 0, i32 3
  %live.1 = load i1, ptr %flag.ptr.1, align 1
  br i1 %live.1, label %drop.1, label %done.1
drop.1:
  store i1 false, ptr %flag.ptr.1, align 1
  %payload.1 = getelementptr %zil_env_3, ptr %env, i32 0, i32 1
  call void @zil.fn.14(ptr %payload.1)
  br label %done.1
done.1:
  ret void
}
define internal void @zil.closure.drop.4(ptr %env) {
entry:
  %flag.ptr.0 = getelementptr %zil_env_4, ptr %env, i32 0, i32 2
  %live.0 = load i1, ptr %flag.ptr.0, align 1
  br i1 %live.0, label %drop.0, label %done.0
drop.0:
  store i1 false, ptr %flag.ptr.0, align 1
  %payload.0 = getelementptr %zil_env_4, ptr %env, i32 0, i32 0
  call void @zil.fn.14(ptr %payload.0)
  br label %done.0
done.0:
  ret void
}
define internal void @zil.closure.drop.5(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define void @zil.fn.0(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32, i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.9(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_0 @zil.fn.1({ i32, i32 } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i32, i32 }, align 4
  store { i32, i32 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i32, i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  %tmp.0 = insertvalue %zil_env_0 zeroinitializer, { i32, i32 } %v.0, 0
  %v.1 = insertvalue %zil_env_0 %tmp.0, i1 true, 1
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.fn.0(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret %zil_env_0 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [1 x i32] }, i32 } @zil.fn.2(i1 %arg.0) {
entry:
  %local.0 = alloca i1, align 1
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32, i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_3, align 4
  %local.5 = alloca i32, align 4
  %local.6 = alloca i32, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { i32, i32 }, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca %zil_env_3, align 4
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { i32, i32 }, align 4
  %local.10 = alloca i32, align 4
  store i1 %arg.0, ptr %local.0, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  %v.0 = add i32 0, 90
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = load i32, ptr %local.1, align 4
  %v.3 = load i32, ptr %local.2, align 4
%tmp.0 = insertvalue { i32, i32 } zeroinitializer, i32 %v.2, 0
%v.4 = insertvalue { i32, i32 } %tmp.0, i32 %v.3, 1
  store i1 false, ptr %resource.live.3, align 1
  store { i32, i32 } %v.4, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.5 = add i32 0, 1
  %v.6 = call %zil_env_3 @zil.fn.18(i32 %v.5)
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_3 %v.6, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.7 = add i32 0, 66
  store i32 %v.7, ptr %local.5, align 4
  %v.8 = add i32 0, 2
  store i32 %v.8, ptr %local.6, align 4
  %v.9 = load i32, ptr %local.5, align 4
  %v.10 = load i32, ptr %local.6, align 4
%tmp.1 = insertvalue { i32, i32 } zeroinitializer, i32 %v.9, 0
%v.11 = insertvalue { i32, i32 } %tmp.1, i32 %v.10, 1
  store i1 false, ptr %resource.live.7, align 1
  store { i32, i32 } %v.11, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.12 = load %zil_env_3, ptr %local.4, align 4
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_3 %v.12, ptr %local.8, align 4
  store i1 true, ptr %resource.live.8, align 1
  %v.13 = load { i32, i32 }, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
  store { i32, i32 } %v.13, ptr %local.9, align 4
  store i1 true, ptr %resource.live.9, align 1
  %v.14 = load i1, ptr %local.0, align 1
  %v.15 = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.12(i1 %v.14)
  %v.16 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.15, 0
  br i1 %v.16, label %bb.1, label %bb.2
bb.1:
  %tmp.2 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.15, 0
  %tmp.3 = icmp ne i1 %tmp.2, true
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %v.17 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.15, 1
  %tmp.4 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 true, 0
  %v.18 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.4, { i32, [1 x i32] } %v.17, 1
  %tmp.5 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.5, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.fn.0(ptr %local.9)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.9, align 1
  %tmp.6 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.6, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.closure.drop.3(ptr %local.8)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.7 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.7, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.fn.0(ptr %local.7)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.8 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.8, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.3(ptr %local.4)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.9 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.9, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.0(ptr %local.3)
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.18
bb.2:
  %tmp.10 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.15, 0
  %tmp.11 = icmp ne i1 %tmp.10, false
  br i1 %tmp.11, label %zil.trap, label %checked.6
checked.6:
  %v.19 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.15, 2
  %v.20 = load %zil_env_3, ptr %local.8, align 4
  store i1 false, ptr %resource.live.8, align 1
  %v.21 = load { i32, i32 }, ptr %local.9, align 4
  store i1 false, ptr %resource.live.9, align 1
  %v.22 = call i32 @zil.fn.27(%zil_env_3 %v.20, { i32, i32 } %v.21, i32 %v.19)
  store i32 %v.22, ptr %local.10, align 4
  %tmp.12 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.12, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.9, align 1
  call void @zil.fn.0(ptr %local.9)
  br label %resource.done.7
resource.done.7:
  %tmp.13 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.13, label %closure.drop.8, label %closure.done.8
closure.drop.8:
  store i1 false, ptr %resource.live.8, align 1
  call void @zil.closure.drop.3(ptr %local.8)
  br label %closure.done.8
closure.done.8:
  %v.23 = load i32, ptr %local.10, align 4
  call void @zil.fn.11(i32 %v.23)
  %v.25 = add i32 0, 0
  %tmp.14 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.26 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.14, i32 %v.25, 2
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.8, align 1
  %tmp.15 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.15, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.fn.0(ptr %local.7)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.16 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.16, label %closure.drop.10, label %closure.done.10
closure.drop.10:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.3(ptr %local.4)
  br label %closure.done.10
closure.done.10:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.17 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.17, label %resource.drop.11, label %resource.done.11
resource.drop.11:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.0(ptr %local.3)
  br label %resource.done.11
resource.done.11:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.26
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [1 x i32] }, i32 } @zil.fn.3(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca %zil_env_4, align 4
  %local.2 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = call %zil_env_4 @zil.fn.19(i32 %v.0)
  store i1 false, ptr %resource.live.1, align 1
  store %zil_env_4 %v.1, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.2 = getelementptr i8, ptr %local.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = getelementptr i8, ptr %v.3, i64 0
  %v.5 = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.28(ptr %v.4)
  %v.6 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.5, 0
  br i1 %v.6, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.5, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.7 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.5, 1
  %tmp.2 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 true, 0
  %v.8 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.2, { i32, [1 x i32] } %v.7, 1
  %tmp.3 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.3, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.closure.drop.4(ptr %local.1)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.8
bb.2:
  %tmp.4 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.5, 0
  %tmp.5 = icmp ne i1 %tmp.4, false
  br i1 %tmp.5, label %zil.trap, label %checked.2
checked.2:
  %v.9 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.5, 2
  store i32 %v.9, ptr %local.2, align 4
  %v.10 = add i32 0, 58
  call void @zil.fn.9(i32 %v.10)
  %v.12 = load i32, ptr %local.2, align 4
  call void @zil.fn.11(i32 %v.12)
  %v.14 = add i32 0, 58
  call void @zil.fn.9(i32 %v.14)
  %v.16 = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.25(ptr %local.1)
  %v.17 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.16, 0
  br i1 %v.17, label %bb.3, label %bb.4
bb.3:
  %tmp.6 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.16, 0
  %tmp.7 = icmp ne i1 %tmp.6, true
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  %v.18 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.16, 1
  %tmp.8 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 true, 0
  %v.19 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.8, { i32, [1 x i32] } %v.18, 1
  %tmp.9 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.9, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.closure.drop.4(ptr %local.1)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.19
bb.4:
  %tmp.10 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.16, 0
  %tmp.11 = icmp ne i1 %tmp.10, false
  br i1 %tmp.11, label %zil.trap, label %checked.5
checked.5:
  %v.20 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.16, 2
  call void @zil.fn.11(i32 %v.20)
  %v.22 = add i32 0, 0
  %tmp.12 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.23 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.12, i32 %v.22, 2
  %tmp.13 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.13, label %closure.drop.6, label %closure.done.6
closure.drop.6:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.closure.drop.4(ptr %local.1)
  br label %closure.done.6
closure.done.6:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.23
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [1 x i32] }, i32 } @zil.fn.4() {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 false, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_2, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %local.3 = alloca i32, align 4
  %local.4 = alloca i1, align 1
  %local.5 = alloca i1, align 1
  %enum.slot.42 = alloca { i32, [1 x i32] }, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  %v.1 = call %zil_env_2 @zil.fn.17(i32 %v.0)
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_2 %v.1, ptr %local.0, align 4
  store i1 true, ptr %resource.live.0, align 1
  %v.2 = add i32 0, 0
  %v.3 = call i32 @zil.fn.23(ptr %local.0, i32 %v.2)
  call void @zil.fn.11(i32 %v.3)
  %v.5 = add i32 0, 10
  call void @zil.fn.9(i32 %v.5)
  %v.7 = add i32 0, 0
  %v.8 = add i32 0, 32
  store i32 %v.7, ptr %local.1, align 4
  br label %bb.1
bb.1:
  %v.9 = load i32, ptr %local.1, align 4
  %v.10 = icmp slt i32 %v.9, %v.8
  br i1 %v.10, label %bb.2, label %bb.4
bb.2:
  store i32 %v.9, ptr %local.2, align 4
  %v.11 = call i32 @zil.fn.8()
  store i32 %v.11, ptr %local.3, align 4
  %v.12 = load i32, ptr %local.3, align 4
  %v.13 = add i32 0, 0
  %v.14 = icmp slt i32 %v.12, %v.13
  br i1 %v.14, label %bb.5, label %bb.6
bb.3:
  %v.40 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.9, i32 %v.40)
  %v.41 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.41, ptr %local.1, align 4
  br label %bb.1
bb.4:
  store { i32, [1 x i32] } zeroinitializer, ptr %enum.slot.42, align 4
  %tmp.2 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.42, i32 0, i32 0
  store i32 1, ptr %tmp.2, align 4
  %tmp.3 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.42, i32 0, i32 1
  %v.42 = load { i32, [1 x i32] }, ptr %enum.slot.42, align 4
  %tmp.4 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 true, 0
  %v.43 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.4, { i32, [1 x i32] } %v.42, 1
  %tmp.5 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.5, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.2(ptr %local.0)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.43
bb.5:
  %v.15 = or i1 false, true
  store i1 %v.15, ptr %local.4, align 1
  br label %bb.7
bb.6:
  %v.16 = load i32, ptr %local.3, align 4
  %v.17 = add i32 0, 113
  %v.18 = icmp eq i32 %v.16, %v.17
  store i1 %v.18, ptr %local.4, align 1
  br label %bb.7
bb.7:
  %v.19 = load i1, ptr %local.4, align 1
  br i1 %v.19, label %bb.8, label %bb.9
bb.8:
  %v.20 = add i32 0, 0
  %tmp.6 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.21 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.6, i32 %v.20, 2
  %tmp.7 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.7, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.2(ptr %local.0)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.21
bb.9:
  br label %bb.10
bb.10:
  %v.22 = load i32, ptr %local.3, align 4
  %v.23 = add i32 0, 10
  %v.24 = icmp eq i32 %v.22, %v.23
  br i1 %v.24, label %bb.11, label %bb.12
bb.11:
  %v.25 = or i1 false, true
  store i1 %v.25, ptr %local.5, align 1
  br label %bb.13
bb.12:
  %v.26 = load i32, ptr %local.3, align 4
  %v.27 = add i32 0, 32
  %v.28 = icmp eq i32 %v.26, %v.27
  store i1 %v.28, ptr %local.5, align 1
  br label %bb.13
bb.13:
  %v.29 = load i1, ptr %local.5, align 1
  br i1 %v.29, label %bb.14, label %bb.15
bb.14:
  br label %bb.3
bb.15:
  br label %bb.16
bb.16:
  %v.30 = load i32, ptr %local.3, align 4
  %v.31 = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.13(i32 %v.30)
  %v.32 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.31, 0
  br i1 %v.32, label %bb.17, label %bb.18
bb.17:
  %tmp.8 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.31, 0
  %tmp.9 = icmp ne i1 %tmp.8, true
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  %v.33 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.31, 1
  %tmp.10 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 true, 0
  %v.34 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.10, { i32, [1 x i32] } %v.33, 1
  %tmp.11 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.11, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.2(ptr %local.0)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.34
bb.18:
  %tmp.12 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.31, 0
  %tmp.13 = icmp ne i1 %tmp.12, false
  br i1 %tmp.13, label %zil.trap, label %checked.5
checked.5:
  %v.35 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.31, 2
  %v.36 = call i32 @zil.fn.23(ptr %local.0, i32 %v.35)
  call void @zil.fn.11(i32 %v.36)
  %v.38 = add i32 0, 10
  call void @zil.fn.9(i32 %v.38)
  br label %bb.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [1 x i32] }, i32 } @zil.fn.5() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca %zil_env_1, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_1, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca %zil_env_1, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca %zil_env_2, align 4
  %local.5 = alloca i1, align 1
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca %zil_env_3, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca %zil_env_3, align 4
  %local.8 = alloca %zil_env_5, align 4
  %local.9 = alloca i32, align 4
  %local.10 = alloca i32, align 4
  %resource.live.11 = alloca i1, align 1
  store i1 false, ptr %resource.live.11, align 1
  %local.11 = alloca { i32, i32 }, align 4
  %resource.live.12 = alloca i1, align 1
  store i1 false, ptr %resource.live.12, align 1
  %local.12 = alloca { i32, i32 }, align 4
  %resource.live.13 = alloca i1, align 1
  store i1 false, ptr %resource.live.13, align 1
  %local.13 = alloca %zil_env_0, align 4
  %enum.slot.121 = alloca { i32, [1 x i32] }, align 4
  br label %bb.0
bb.0:
  %v.0 = call i32 @zil.fn.8()
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
  %v.2 = add i32 0, 108
  %v.3 = icmp eq i32 %v.1, %v.2
  br i1 %v.3, label %bb.1, label %bb.2
bb.1:
  %v.4 = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.4()
  ret { i1, { i32, [1 x i32] }, i32 } %v.4
bb.2:
  br label %bb.3
bb.3:
  %v.5 = load i32, ptr %local.0, align 4
  %v.6 = add i32 0, 115
  %v.7 = icmp eq i32 %v.5, %v.6
  br i1 %v.7, label %bb.4, label %bb.5
bb.4:
  %v.8 = add i32 0, 21
  %v.9 = call %zil_env_1 @zil.fn.16(i32 %v.8)
  store i1 false, ptr %resource.live.1, align 1
  store %zil_env_1 %v.9, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.10 = getelementptr i8, ptr %local.1, i64 0
  %v.11 = getelementptr i8, ptr %v.10, i64 0
  %v.12 = getelementptr i8, ptr %v.11, i64 0
  %v.13 = call i32 @zil.fn.29(ptr %v.12)
  call void @zil.fn.11(i32 %v.13)
  %v.15 = load %zil_env_1, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_1 %v.15, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.16 = call i32 @zil.fn.22(ptr %local.2)
  call void @zil.fn.11(i32 %v.16)
  %v.18 = add i32 0, 0
  %tmp.0 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.19 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.0, i32 %v.18, 2
  %tmp.1 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.1, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.1(ptr %local.2)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.2 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.2, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.closure.drop.1(ptr %local.1)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.1, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.19
bb.5:
  br label %bb.6
bb.6:
  %v.20 = load i32, ptr %local.0, align 4
  %v.21 = add i32 0, 122
  %v.22 = icmp eq i32 %v.20, %v.21
  br i1 %v.22, label %bb.7, label %bb.8
bb.7:
  %v.23 = add i32 0, 1
  %v.24 = call %zil_env_1 @zil.fn.16(i32 %v.23)
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_1 %v.24, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.25 = add i32 0, 90
  call void @zil.fn.9(i32 %v.25)
  %v.27 = add i32 0, 0
  %tmp.3 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.28 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.3, i32 %v.27, 2
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.1(ptr %local.3)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.28
bb.8:
  br label %bb.9
bb.9:
  %v.29 = load i32, ptr %local.0, align 4
  %v.30 = add i32 0, 109
  %v.31 = icmp eq i32 %v.29, %v.30
  br i1 %v.31, label %bb.10, label %bb.11
bb.10:
  %v.32 = add i32 0, 0
  %v.33 = call %zil_env_2 @zil.fn.17(i32 %v.32)
  store i1 false, ptr %resource.live.4, align 1
  store %zil_env_2 %v.33, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  %v.34 = getelementptr i8, ptr %local.4, i64 0
  %v.35 = getelementptr i8, ptr %v.34, i64 0
  %v.36 = getelementptr i8, ptr %v.35, i64 0
  %v.37 = call i32 @zil.fn.30(ptr %v.36)
  call void @zil.fn.11(i32 %v.37)
  %v.39 = add i32 0, 1
  %v.40 = call i32 @zil.fn.23(ptr %local.4, i32 %v.39)
  call void @zil.fn.11(i32 %v.40)
  %v.42 = add i32 0, 0
  %tmp.5 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.43 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.5, i32 %v.42, 2
  %tmp.6 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.6, label %closure.drop.3, label %closure.done.3
closure.drop.3:
  store i1 false, ptr %resource.live.4, align 1
  call void @zil.closure.drop.2(ptr %local.4)
  br label %closure.done.3
closure.done.3:
  store i1 false, ptr %resource.live.4, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.43
bb.11:
  br label %bb.12
bb.12:
  %v.44 = load i32, ptr %local.0, align 4
  %v.45 = add i32 0, 48
  %v.46 = icmp eq i32 %v.44, %v.45
  br i1 %v.46, label %bb.13, label %bb.14
bb.13:
  %v.47 = or i1 false, true
  store i1 %v.47, ptr %local.5, align 1
  br label %bb.15
bb.14:
  %v.48 = load i32, ptr %local.0, align 4
  %v.49 = add i32 0, 49
  %v.50 = icmp eq i32 %v.48, %v.49
  store i1 %v.50, ptr %local.5, align 1
  br label %bb.15
bb.15:
  %v.51 = load i1, ptr %local.5, align 1
  br i1 %v.51, label %bb.16, label %bb.17
bb.16:
  store i1 false, ptr %resource.live.7, align 1
  %v.52 = add i32 0, 1
  %v.53 = call %zil_env_3 @zil.fn.18(i32 %v.52)
  store i1 false, ptr %resource.live.6, align 1
  store %zil_env_3 %v.53, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.54 = load %zil_env_3, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  store %zil_env_3 %v.54, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.55 = load i32, ptr %local.0, align 4
  %v.56 = add i32 0, 49
  %v.57 = icmp eq i32 %v.55, %v.56
  %v.58 = load %zil_env_3, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
  %v.59 = call i32 @zil.fn.31(%zil_env_3 %v.58, i1 %v.57)
  call void @zil.fn.11(i32 %v.59)
  %tmp.7 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.7, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.7, align 1
  call void @zil.closure.drop.3(ptr %local.7)
  br label %closure.done.4
closure.done.4:
  %v.61 = add i32 0, 0
  %tmp.8 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.62 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.8, i32 %v.61, 2
  store i1 false, ptr %resource.live.7, align 1
  %tmp.9 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.9, label %closure.drop.5, label %closure.done.5
closure.drop.5:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.closure.drop.3(ptr %local.6)
  br label %closure.done.5
closure.done.5:
  store i1 false, ptr %resource.live.6, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.62
bb.17:
  br label %bb.18
bb.18:
  %v.63 = load i32, ptr %local.0, align 4
  %v.64 = add i32 0, 112
  %v.65 = icmp eq i32 %v.63, %v.64
  br i1 %v.65, label %bb.19, label %bb.20
bb.19:
  %v.66 = or i1 false, true
  %v.67 = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.2(i1 %v.66)
  ret { i1, { i32, [1 x i32] }, i32 } %v.67
bb.20:
  br label %bb.21
bb.21:
  %v.68 = load i32, ptr %local.0, align 4
  %v.69 = add i32 0, 80
  %v.70 = icmp eq i32 %v.68, %v.69
  br i1 %v.70, label %bb.22, label %bb.23
bb.22:
  %v.71 = or i1 false, false
  %v.72 = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.2(i1 %v.71)
  ret { i1, { i32, [1 x i32] }, i32 } %v.72
bb.23:
  br label %bb.24
bb.24:
  %v.73 = load i32, ptr %local.0, align 4
  %v.74 = add i32 0, 101
  %v.75 = icmp eq i32 %v.73, %v.74
  br i1 %v.75, label %bb.25, label %bb.26
bb.25:
  %v.76 = add i32 0, 1
  %v.77 = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.3(i32 %v.76)
  ret { i1, { i32, [1 x i32] }, i32 } %v.77
bb.26:
  br label %bb.27
bb.27:
  %v.78 = load i32, ptr %local.0, align 4
  %v.79 = add i32 0, 102
  %v.80 = icmp eq i32 %v.78, %v.79
  br i1 %v.80, label %bb.28, label %bb.29
bb.28:
  %v.81 = add i32 0, 2
  %v.82 = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.3(i32 %v.81)
  ret { i1, { i32, [1 x i32] }, i32 } %v.82
bb.29:
  br label %bb.30
bb.30:
  %v.83 = load i32, ptr %local.0, align 4
  %v.84 = add i32 0, 69
  %v.85 = icmp eq i32 %v.83, %v.84
  br i1 %v.85, label %bb.31, label %bb.32
bb.31:
  %v.86 = add i32 0, 0
  %v.87 = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.3(i32 %v.86)
  ret { i1, { i32, [1 x i32] }, i32 } %v.87
bb.32:
  br label %bb.33
bb.33:
  %v.88 = load i32, ptr %local.0, align 4
  %v.89 = add i32 0, 99
  %v.90 = icmp eq i32 %v.88, %v.89
  br i1 %v.90, label %bb.34, label %bb.35
bb.34:
  %v.91 = add i32 0, 20
  %v.92 = call %zil_env_5 @zil.fn.20(i32 %v.91)
  store %zil_env_5 %v.92, ptr %local.8, align 4
  %v.93 = load %zil_env_5, ptr %local.8, align 4
  %v.94 = add i32 0, 1
  %v.95 = call i32 @zil.fn.26(%zil_env_5 %v.93, i32 %v.94)
  %v.96 = load %zil_env_5, ptr %local.8, align 4
  %v.97 = add i32 0, 1
  %v.98 = call i32 @zil.fn.26(%zil_env_5 %v.96, i32 %v.97)
  %tmp.10 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.95, i32 %v.98)
  %v.99 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.6
checked.6:
  call void @zil.fn.11(i32 %v.99)
  %v.101 = add i32 0, 0
  %tmp.12 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.102 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.12, i32 %v.101, 2
  ret { i1, { i32, [1 x i32] }, i32 } %v.102
bb.35:
  br label %bb.36
bb.36:
  %v.103 = load i32, ptr %local.0, align 4
  %v.104 = add i32 0, 103
  %v.105 = icmp eq i32 %v.103, %v.104
  br i1 %v.105, label %bb.37, label %bb.38
bb.37:
  store i1 false, ptr %resource.live.12, align 1
  %v.106 = add i32 0, 71
  store i32 %v.106, ptr %local.9, align 4
  %v.107 = add i32 0, 7
  store i32 %v.107, ptr %local.10, align 4
  %v.108 = load i32, ptr %local.9, align 4
  %v.109 = load i32, ptr %local.10, align 4
%tmp.13 = insertvalue { i32, i32 } zeroinitializer, i32 %v.108, 0
%v.110 = insertvalue { i32, i32 } %tmp.13, i32 %v.109, 1
  store i1 false, ptr %resource.live.11, align 1
  store { i32, i32 } %v.110, ptr %local.11, align 4
  store i1 true, ptr %resource.live.11, align 1
  %v.111 = load { i32, i32 }, ptr %local.11, align 4
  store i1 false, ptr %resource.live.11, align 1
  store { i32, i32 } %v.111, ptr %local.12, align 4
  store i1 true, ptr %resource.live.12, align 1
  %v.112 = load { i32, i32 }, ptr %local.12, align 4
  store i1 false, ptr %resource.live.12, align 1
  %v.113 = call %zil_env_0 @zil.fn.1({ i32, i32 } %v.112)
  %tmp.14 = load i1, ptr %resource.live.12, align 1
  br i1 %tmp.14, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.12, align 1
  call void @zil.fn.0(ptr %local.12)
  br label %resource.done.7
resource.done.7:
  store i1 false, ptr %resource.live.13, align 1
  store %zil_env_0 %v.113, ptr %local.13, align 4
  store i1 true, ptr %resource.live.13, align 1
  %v.114 = call i32 @zil.fn.21(ptr %local.13)
  call void @zil.fn.11(i32 %v.114)
  %v.116 = call i32 @zil.fn.21(ptr %local.13)
  call void @zil.fn.11(i32 %v.116)
  %v.118 = add i32 0, 0
  %tmp.15 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.119 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.15, i32 %v.118, 2
  %tmp.16 = load i1, ptr %resource.live.13, align 1
  br i1 %tmp.16, label %closure.drop.8, label %closure.done.8
closure.drop.8:
  store i1 false, ptr %resource.live.13, align 1
  call void @zil.closure.drop.0(ptr %local.13)
  br label %closure.done.8
closure.done.8:
  store i1 false, ptr %resource.live.13, align 1
  store i1 false, ptr %resource.live.12, align 1
  %tmp.17 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.17, label %resource.drop.9, label %resource.done.9
resource.drop.9:
  store i1 false, ptr %resource.live.11, align 1
  call void @zil.fn.0(ptr %local.11)
  br label %resource.done.9
resource.done.9:
  store i1 false, ptr %resource.live.11, align 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.119
bb.38:
  br label %bb.39
bb.39:
  %v.120 = load i32, ptr %local.0, align 4
  store { i32, [1 x i32] } zeroinitializer, ptr %enum.slot.121, align 4
  %tmp.18 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.121, i32 0, i32 0
  store i32 0, ptr %tmp.18, align 4
  %tmp.19 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.121, i32 0, i32 1
  %tmp.20 = getelementptr { i32 }, ptr %tmp.19, i32 0, i32 0
  store i32 %v.120, ptr %tmp.20, align 4
  %v.121 = load { i32, [1 x i32] }, ptr %enum.slot.121, align 4
  %tmp.21 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 true, 0
  %v.122 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.21, { i32, [1 x i32] } %v.121, 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.122
zil.trap:
  call void @llvm.trap()
  unreachable
}

declare i32 @getchar() nonlazybind
declare i32 @putchar(i32) nonlazybind
define i32 @zil.fn.8() {
entry:
  br label %bb.0
bb.0:
  %v.0 = call i32 @getchar()
  ret i32 %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.9(i32 %arg.0) {
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

define void @zil.fn.10(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i1, align 1
  %local.2 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = add i32 0, -10
  %v.2 = icmp sle i32 %v.0, %v.1
  br i1 %v.2, label %bb.1, label %bb.2
bb.1:
  %v.3 = or i1 false, true
  store i1 %v.3, ptr %local.1, align 1
  br label %bb.3
bb.2:
  %v.4 = load i32, ptr %local.0, align 4
  %v.5 = add i32 0, 10
  %v.6 = icmp sge i32 %v.4, %v.5
  store i1 %v.6, ptr %local.1, align 1
  br label %bb.3
bb.3:
  %v.7 = load i1, ptr %local.1, align 1
  br i1 %v.7, label %bb.4, label %bb.5
bb.4:
  %v.8 = load i32, ptr %local.0, align 4
  %v.9 = add i32 0, 10
  %tmp.0 = icmp eq i32 %v.9, 0
  %tmp.1 = icmp eq i32 %v.8, -2147483648
  %tmp.2 = icmp eq i32 %v.9, -1
  %tmp.3 = and i1 %tmp.1, %tmp.2
  %tmp.4 = or i1 %tmp.0, %tmp.3
  br i1 %tmp.4, label %zil.trap, label %checked.0
checked.0:
  %v.10 = sdiv i32 %v.8, %v.9
  call void @zil.fn.10(i32 %v.10)
  br label %bb.6
bb.5:
  br label %bb.6
bb.6:
  %v.12 = load i32, ptr %local.0, align 4
  %v.13 = add i32 0, 10
  %tmp.5 = icmp eq i32 %v.13, 0
  %tmp.6 = icmp eq i32 %v.12, -2147483648
  %tmp.7 = icmp eq i32 %v.13, -1
  %tmp.8 = and i1 %tmp.6, %tmp.7
  %tmp.9 = or i1 %tmp.5, %tmp.8
  br i1 %tmp.9, label %zil.trap, label %checked.1
checked.1:
  %v.14 = srem i32 %v.12, %v.13
  store i32 %v.14, ptr %local.2, align 4
  %v.15 = load i32, ptr %local.2, align 4
  %v.16 = add i32 0, 0
  %v.17 = icmp slt i32 %v.15, %v.16
  br i1 %v.17, label %bb.7, label %bb.8
bb.7:
  %v.18 = load i32, ptr %local.2, align 4
  %tmp.10 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 0, i32 %v.18)
  %v.19 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.2
checked.2:
  store i32 %v.19, ptr %local.2, align 4
  br label %bb.9
bb.8:
  br label %bb.9
bb.9:
  %v.20 = add i32 0, 48
  %v.21 = load i32, ptr %local.2, align 4
  %tmp.12 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.20, i32 %v.21)
  %v.22 = extractvalue { i32, i1 } %tmp.12, 0
  %tmp.13 = extractvalue { i32, i1 } %tmp.12, 1
  br i1 %tmp.13, label %zil.trap, label %checked.3
checked.3:
  call void @zil.fn.9(i32 %v.22)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.11(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = add i32 0, 0
  %v.2 = icmp slt i32 %v.0, %v.1
  br i1 %v.2, label %bb.1, label %bb.2
bb.1:
  %v.3 = add i32 0, 45
  call void @zil.fn.9(i32 %v.3)
  br label %bb.3
bb.2:
  br label %bb.3
bb.3:
  %v.5 = load i32, ptr %local.0, align 4
  call void @zil.fn.10(i32 %v.5)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [1 x i32] }, i32 } @zil.fn.12(i1 %arg.0) {
entry:
  %local.0 = alloca i1, align 1
  %enum.slot.2 = alloca { i32, [1 x i32] }, align 4
  store i1 %arg.0, ptr %local.0, align 1
  br label %bb.0
bb.0:
  %v.0 = load i1, ptr %local.0, align 1
  br i1 %v.0, label %bb.1, label %bb.2
bb.1:
  %v.1 = add i32 0, 0
  store { i32, [1 x i32] } zeroinitializer, ptr %enum.slot.2, align 4
  %tmp.0 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.2, i32 0, i32 0
  store i32 2, ptr %tmp.0, align 4
  %tmp.1 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.2, i32 0, i32 1
  %tmp.2 = getelementptr { i32 }, ptr %tmp.1, i32 0, i32 0
  store i32 %v.1, ptr %tmp.2, align 4
  %v.2 = load { i32, [1 x i32] }, ptr %enum.slot.2, align 4
  %tmp.3 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 true, 0
  %v.3 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.3, { i32, [1 x i32] } %v.2, 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.3
bb.2:
  br label %bb.3
bb.3:
  %v.4 = add i32 0, 0
  %tmp.4 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.5 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.4, i32 %v.4, 2
  ret { i1, { i32, [1 x i32] }, i32 } %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [1 x i32] }, i32 } @zil.fn.13(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %enum.slot.16 = alloca { i32, [1 x i32] }, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = add i32 0, 43
  %v.2 = icmp eq i32 %v.0, %v.1
  br i1 %v.2, label %bb.1, label %bb.2
bb.1:
  %v.3 = add i32 0, 1
  %tmp.0 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.4 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.0, i32 %v.3, 2
  ret { i1, { i32, [1 x i32] }, i32 } %v.4
bb.2:
  br label %bb.3
bb.3:
  %v.5 = load i32, ptr %local.0, align 4
  %v.6 = add i32 0, 45
  %v.7 = icmp eq i32 %v.5, %v.6
  br i1 %v.7, label %bb.4, label %bb.5
bb.4:
  %v.8 = add i32 0, -1
  %tmp.1 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.9 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.1, i32 %v.8, 2
  ret { i1, { i32, [1 x i32] }, i32 } %v.9
bb.5:
  br label %bb.6
bb.6:
  %v.10 = load i32, ptr %local.0, align 4
  %v.11 = add i32 0, 61
  %v.12 = icmp eq i32 %v.10, %v.11
  br i1 %v.12, label %bb.7, label %bb.8
bb.7:
  %v.13 = add i32 0, 0
  %tmp.2 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.14 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.2, i32 %v.13, 2
  ret { i1, { i32, [1 x i32] }, i32 } %v.14
bb.8:
  br label %bb.9
bb.9:
  %v.15 = load i32, ptr %local.0, align 4
  store { i32, [1 x i32] } zeroinitializer, ptr %enum.slot.16, align 4
  %tmp.3 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.16, i32 0, i32 0
  store i32 0, ptr %tmp.3, align 4
  %tmp.4 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.16, i32 0, i32 1
  %tmp.5 = getelementptr { i32 }, ptr %tmp.4, i32 0, i32 0
  store i32 %v.15, ptr %tmp.5, align 4
  %v.16 = load { i32, [1 x i32] }, ptr %enum.slot.16, align 4
  %tmp.6 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 true, 0
  %v.17 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.6, { i32, [1 x i32] } %v.16, 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.17
zil.trap:
  call void @llvm.trap()
  unreachable
}

define void @zil.fn.14(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32, i32 }, ptr %v.0, i32 0, i32 0
  %v.1 = load i32, ptr %tmp.0, align 4
  call void @zil.fn.9(i32 %v.1)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.15({ i32, i32 } %arg.0) {
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
  call void @zil.fn.14(ptr %local.0)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_1 @zil.fn.16(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32, i32 }, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 66
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.0, align 4
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = load i32, ptr %local.1, align 4
  %v.3 = load i32, ptr %local.2, align 4
%tmp.0 = insertvalue { i32, i32 } zeroinitializer, i32 %v.2, 0
%v.4 = insertvalue { i32, i32 } %tmp.0, i32 %v.3, 1
  store i1 false, ptr %resource.live.3, align 1
  store { i32, i32 } %v.4, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.5 = load { i32, i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  %tmp.1 = insertvalue %zil_env_1 zeroinitializer, { i32, i32 } %v.5, 0
  %v.6 = insertvalue %zil_env_1 %tmp.1, i1 true, 1
  %tmp.2 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.14(ptr %local.3)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  ret %zil_env_1 %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_2 @zil.fn.17(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32, i32 }, align 4
  %local.4 = alloca i32, align 4
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32, i32 }, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 70
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = load i32, ptr %local.1, align 4
  %v.3 = load i32, ptr %local.2, align 4
%tmp.0 = insertvalue { i32, i32 } zeroinitializer, i32 %v.2, 0
%v.4 = insertvalue { i32, i32 } %tmp.0, i32 %v.3, 1
  store i1 false, ptr %resource.live.3, align 1
  store { i32, i32 } %v.4, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.5 = add i32 0, 65
  store i32 %v.5, ptr %local.4, align 4
  %v.6 = load i32, ptr %local.0, align 4
  store i32 %v.6, ptr %local.5, align 4
  %v.7 = load i32, ptr %local.4, align 4
  %v.8 = load i32, ptr %local.5, align 4
%tmp.1 = insertvalue { i32, i32 } zeroinitializer, i32 %v.7, 0
%v.9 = insertvalue { i32, i32 } %tmp.1, i32 %v.8, 1
  store i1 false, ptr %resource.live.6, align 1
  store { i32, i32 } %v.9, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.10 = load { i32, i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %tmp.2 = insertvalue %zil_env_2 zeroinitializer, { i32, i32 } %v.10, 0
  %v.11 = insertvalue %zil_env_2 %tmp.2, i1 true, 1
  %tmp.3 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.14(ptr %local.6)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.14(ptr %local.3)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.3, align 1
  ret %zil_env_2 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_3 @zil.fn.18(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32, i32 }, align 4
  %local.4 = alloca i32, align 4
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32, i32 }, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 67
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.0, align 4
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = load i32, ptr %local.1, align 4
  %v.3 = load i32, ptr %local.2, align 4
%tmp.0 = insertvalue { i32, i32 } zeroinitializer, i32 %v.2, 0
%v.4 = insertvalue { i32, i32 } %tmp.0, i32 %v.3, 1
  store i1 false, ptr %resource.live.3, align 1
  store { i32, i32 } %v.4, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.5 = add i32 0, 68
  store i32 %v.5, ptr %local.4, align 4
  %v.6 = add i32 0, 0
  store i32 %v.6, ptr %local.5, align 4
  %v.7 = load i32, ptr %local.4, align 4
  %v.8 = load i32, ptr %local.5, align 4
%tmp.1 = insertvalue { i32, i32 } zeroinitializer, i32 %v.7, 0
%v.9 = insertvalue { i32, i32 } %tmp.1, i32 %v.8, 1
  store i1 false, ptr %resource.live.6, align 1
  store { i32, i32 } %v.9, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.10 = load { i32, i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  %v.11 = load { i32, i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %tmp.2 = insertvalue %zil_env_3 zeroinitializer, { i32, i32 } %v.10, 0
  %tmp.3 = insertvalue %zil_env_3 %tmp.2, { i32, i32 } %v.11, 1
  %tmp.4 = insertvalue %zil_env_3 %tmp.3, i1 true, 2
  %v.12 = insertvalue %zil_env_3 %tmp.4, i1 true, 3
  %tmp.5 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.5, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.6, align 1
  call void @zil.fn.14(ptr %local.6)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.6 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.6, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.14(ptr %local.3)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.3, align 1
  ret %zil_env_3 %v.12
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_4 @zil.fn.19(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32, i32 }, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 69
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = add i32 0, 0
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = load i32, ptr %local.1, align 4
  %v.3 = load i32, ptr %local.2, align 4
%tmp.0 = insertvalue { i32, i32 } zeroinitializer, i32 %v.2, 0
%v.4 = insertvalue { i32, i32 } %tmp.0, i32 %v.3, 1
  store i1 false, ptr %resource.live.3, align 1
  store { i32, i32 } %v.4, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.5 = load { i32, i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  %v.6 = load i32, ptr %local.0, align 4
  %tmp.1 = insertvalue %zil_env_4 zeroinitializer, { i32, i32 } %v.5, 0
  %tmp.2 = insertvalue %zil_env_4 %tmp.1, i32 %v.6, 1
  %v.7 = insertvalue %zil_env_4 %tmp.2, i1 true, 2
  %tmp.3 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.3, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.14(ptr %local.3)
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.3, align 1
  ret %zil_env_4 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define %zil_env_5 @zil.fn.20(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = insertvalue %zil_env_5 zeroinitializer, i32 %v.0, 0
  ret %zil_env_5 %v.1
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.21(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_0, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32, i32 }, ptr %v.1, i32 0, i32 1
  %v.2 = load i32, ptr %tmp.0, align 4
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.22(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_1, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = load ptr, ptr %local.0, align 8
  %tmp.0 = getelementptr { i32, i32 }, ptr %v.1, i32 0, i32 1
  %v.2 = load i32, ptr %tmp.0, align 4
  ret i32 %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.23(ptr %env, i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca ptr, align 8
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_2, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.1, align 8
  %v.1 = load ptr, ptr %local.1, align 8
  %v.2 = load ptr, ptr %local.1, align 8
  %tmp.0 = getelementptr { i32, i32 }, ptr %v.2, i32 0, i32 1
  %v.3 = load i32, ptr %tmp.0, align 4
  %v.4 = load i32, ptr %local.0, align 4
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.3, i32 %v.4)
  %v.5 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %tmp.3 = getelementptr { i32, i32 }, ptr %v.1, i32 0, i32 1
  store i32 %v.5, ptr %tmp.3, align 4
  %v.6 = load ptr, ptr %local.1, align 8
  %tmp.4 = getelementptr { i32, i32 }, ptr %v.6, i32 0, i32 1
  %v.7 = load i32, ptr %tmp.4, align 4
  ret i32 %v.7
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.24(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca ptr, align 8
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32, i32 }, align 4
  %local.3 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_3, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %v.1 = getelementptr %zil_env_3, ptr %env, i32 0, i32 1
  store ptr %v.1, ptr %local.1, align 8
  store i1 false, ptr %resource.live.2, align 1
  %tmp.0 = getelementptr %zil_env_3, ptr %env, i32 0, i32 2
  store i1 false, ptr %tmp.0, align 1
  %tmp.1 = getelementptr %zil_env_3, ptr %env, i32 0, i32 0
  %v.2 = load { i32, i32 }, ptr %tmp.1, align 4
  store { i32, i32 } %v.2, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.3 = load { i32, i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  %v.4 = call i32 @zil.fn.15({ i32, i32 } %v.3)
  store i32 %v.4, ptr %local.3, align 4
  %tmp.2 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.2, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.fn.14(ptr %local.2)
  br label %resource.done.0
resource.done.0:
  %v.5 = load i32, ptr %local.3, align 4
  %v.6 = load ptr, ptr %local.1, align 8
  %tmp.3 = getelementptr { i32, i32 }, ptr %v.6, i32 0, i32 1
  %v.7 = load i32, ptr %tmp.3, align 4
  %tmp.4 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.5, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.4, 0
  %tmp.5 = extractvalue { i32, i1 } %tmp.4, 1
  br i1 %tmp.5, label %zil.trap, label %checked.1
checked.1:
  store i1 false, ptr %resource.live.2, align 1
  ret i32 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [1 x i32] }, i32 } @zil.fn.25(ptr %env) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  %enum.slot.15 = alloca { i32, [1 x i32] }, align 4
  br label %bb.0
bb.0:
  %v.0 = getelementptr %zil_env_4, ptr %env, i32 0, i32 0
  store ptr %v.0, ptr %local.0, align 8
  %tmp.0 = getelementptr %zil_env_4, ptr %env, i32 0, i32 1
  %v.1 = load i32, ptr %tmp.0, align 4
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load ptr, ptr %local.0, align 8
  %v.3 = load ptr, ptr %local.0, align 8
  %tmp.1 = getelementptr { i32, i32 }, ptr %v.3, i32 0, i32 1
  %v.4 = load i32, ptr %tmp.1, align 4
  %v.5 = add i32 0, 1
  %tmp.2 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.5)
  %v.6 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.0
checked.0:
  %tmp.4 = getelementptr { i32, i32 }, ptr %v.2, i32 0, i32 1
  store i32 %v.6, ptr %tmp.4, align 4
  %v.7 = load ptr, ptr %local.0, align 8
  %tmp.5 = getelementptr { i32, i32 }, ptr %v.7, i32 0, i32 1
  %v.8 = load i32, ptr %tmp.5, align 4
  call void @zil.fn.11(i32 %v.8)
  %v.10 = load ptr, ptr %local.0, align 8
  %tmp.6 = getelementptr { i32, i32 }, ptr %v.10, i32 0, i32 1
  %v.11 = load i32, ptr %tmp.6, align 4
  %v.12 = load i32, ptr %local.1, align 4
  %v.13 = icmp eq i32 %v.11, %v.12
  br i1 %v.13, label %bb.1, label %bb.2
bb.1:
  %v.14 = load i32, ptr %local.1, align 4
  store { i32, [1 x i32] } zeroinitializer, ptr %enum.slot.15, align 4
  %tmp.7 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.15, i32 0, i32 0
  store i32 2, ptr %tmp.7, align 4
  %tmp.8 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.15, i32 0, i32 1
  %tmp.9 = getelementptr { i32 }, ptr %tmp.8, i32 0, i32 0
  store i32 %v.14, ptr %tmp.9, align 4
  %v.15 = load { i32, [1 x i32] }, ptr %enum.slot.15, align 4
  %tmp.10 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 true, 0
  %v.16 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.10, { i32, [1 x i32] } %v.15, 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.16
bb.2:
  br label %bb.3
bb.3:
  %v.17 = load ptr, ptr %local.0, align 8
  %tmp.11 = getelementptr { i32, i32 }, ptr %v.17, i32 0, i32 1
  %v.18 = load i32, ptr %tmp.11, align 4
  %tmp.12 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.19 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.12, i32 %v.18, 2
  ret { i1, { i32, [1 x i32] }, i32 } %v.19
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.26(%zil_env_5 %env, i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = extractvalue %zil_env_5 %env, 0
  store i32 %v.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %tmp.0 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.1, i32 %v.2)
  %v.3 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.27(%zil_env_3 %arg.0, { i32, i32 } %arg.1, i32 %arg.2) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_3, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 true, ptr %resource.live.1, align 1
  %local.1 = alloca { i32, i32 }, align 4
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca %zil_env_3, align 4
  %closure.once.3 = alloca %zil_env_3, align 4
  store %zil_env_3 %arg.0, ptr %local.0, align 4
  store { i32, i32 } %arg.1, ptr %local.1, align 4
  store i32 %arg.2, ptr %local.2, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = add i32 0, 72
  call void @zil.fn.9(i32 %v.0)
  %v.2 = load %zil_env_3, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_3 %v.2, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = load %zil_env_3, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store %zil_env_3 %v.3, ptr %closure.once.3, align 4
  %v.4 = call i32 @zil.fn.24(ptr %closure.once.3)
  call void @zil.closure.drop.3(ptr %closure.once.3)
  %v.5 = getelementptr i8, ptr %local.1, i64 0
  %tmp.0 = getelementptr { i32, i32 }, ptr %v.5, i32 0, i32 1
  %v.6 = load i32, ptr %tmp.0, align 4
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.4, i32 %v.6)
  %v.7 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.8 = load i32, ptr %local.2, align 4
  %tmp.3 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.7, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %tmp.5 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.5, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.closure.drop.3(ptr %local.3)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.3, align 1
  %tmp.6 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.6, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.1, align 1
  call void @zil.fn.0(ptr %local.1)
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.7 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.7, label %closure.drop.4, label %closure.done.4
closure.drop.4:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.3(ptr %local.0)
  br label %closure.done.4
closure.done.4:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i1, { i32, [1 x i32] }, i32 } @zil.fn.28(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i32, align 4
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.25(ptr %v.0)
  %v.2 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.1, 0
  br i1 %v.2, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.1, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.3 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.1, 1
  %tmp.2 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 true, 0
  %v.4 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.2, { i32, [1 x i32] } %v.3, 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.4
bb.2:
  %tmp.3 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.1, 0
  %tmp.4 = icmp ne i1 %tmp.3, false
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  %v.5 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.1, 2
  store i32 %v.5, ptr %local.1, align 4
  %v.6 = load i32, ptr %local.1, align 4
  %v.7 = load ptr, ptr %local.0, align 8
  %v.8 = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.25(ptr %v.7)
  %v.9 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.8, 0
  br i1 %v.9, label %bb.3, label %bb.4
bb.3:
  %tmp.5 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.8, 0
  %tmp.6 = icmp ne i1 %tmp.5, true
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  %v.10 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.8, 1
  %tmp.7 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 true, 0
  %v.11 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.7, { i32, [1 x i32] } %v.10, 1
  ret { i1, { i32, [1 x i32] }, i32 } %v.11
bb.4:
  %tmp.8 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.8, 0
  %tmp.9 = icmp ne i1 %tmp.8, false
  br i1 %tmp.9, label %zil.trap, label %checked.3
checked.3:
  %v.12 = extractvalue { i1, { i32, [1 x i32] }, i32 } %v.8, 2
  %tmp.10 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.12)
  %v.13 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.4
checked.4:
  %tmp.12 = insertvalue { i1, { i32, [1 x i32] }, i32 } zeroinitializer, i1 false, 0
  %v.14 = insertvalue { i1, { i32, [1 x i32] }, i32 } %tmp.12, i32 %v.13, 2
  ret { i1, { i32, [1 x i32] }, i32 } %v.14
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.29(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = call i32 @zil.fn.22(ptr %v.0)
  %v.2 = load ptr, ptr %local.0, align 8
  %v.3 = call i32 @zil.fn.22(ptr %v.2)
  ret i32 %v.3
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.30(ptr %arg.0) {
entry:
  %local.0 = alloca ptr, align 8
  store ptr %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = add i32 0, 1
  %v.2 = call i32 @zil.fn.23(ptr %v.0, i32 %v.1)
  %v.3 = load ptr, ptr %local.0, align 8
  %v.4 = add i32 0, 1
  %v.5 = call i32 @zil.fn.23(ptr %v.3, i32 %v.4)
  ret i32 %v.5
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.31(%zil_env_3 %arg.0, i1 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca %zil_env_3, align 4
  %local.1 = alloca i1, align 1
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca %zil_env_3, align 4
  %closure.once.2 = alloca %zil_env_3, align 4
  store %zil_env_3 %arg.0, ptr %local.0, align 4
  store i1 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  %v.0 = load i1, ptr %local.1, align 1
  br i1 %v.0, label %bb.1, label %bb.2
bb.1:
  store i1 false, ptr %resource.live.2, align 1
  %v.1 = load %zil_env_3, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store %zil_env_3 %v.1, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.2 = load %zil_env_3, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store %zil_env_3 %v.2, ptr %closure.once.2, align 4
  %v.3 = call i32 @zil.fn.24(ptr %closure.once.2)
  call void @zil.closure.drop.3(ptr %closure.once.2)
  %tmp.0 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.0, label %closure.drop.0, label %closure.done.0
closure.drop.0:
  store i1 false, ptr %resource.live.2, align 1
  call void @zil.closure.drop.3(ptr %local.2)
  br label %closure.done.0
closure.done.0:
  store i1 false, ptr %resource.live.2, align 1
  %tmp.1 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.1, label %closure.drop.1, label %closure.done.1
closure.drop.1:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.3(ptr %local.0)
  br label %closure.done.1
closure.done.1:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.3
bb.2:
  br label %bb.3
bb.3:
  %v.4 = add i32 0, 0
  %tmp.2 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.2, label %closure.drop.2, label %closure.done.2
closure.drop.2:
  store i1 false, ptr %resource.live.0, align 1
  call void @zil.closure.drop.3(ptr %local.0)
  br label %closure.done.2
closure.done.2:
  store i1 false, ptr %resource.live.0, align 1
  ret i32 %v.4
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call { i1, { i32, [1 x i32] }, i32 } @zil.fn.5()
  %entry.error = extractvalue { i1, { i32, [1 x i32] }, i32 } %entry.result, 0
  br i1 %entry.error, label %error, label %success
error:
  ret i32 1
success:
  %entry.success = extractvalue { i1, { i32, [1 x i32] }, i32 } %entry.result, 2
  ret i32 %entry.success
}

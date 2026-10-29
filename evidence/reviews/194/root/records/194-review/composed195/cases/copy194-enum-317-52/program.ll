; ZIL verified LLVM module
; verified-fingerprint: 5148f0f2d0f4244e048d4015f1566d2546fd78fa30200229504046a07c2d4d6b
target triple = "x86_64-unknown-linux-gnu"

%zil_env_0 = type {  }
define internal void @zil.closure.drop.0(ptr %env) {
entry:
  ret void
}
declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca %zil_env_0, align 1
  %local.1 = alloca { i32, [1 x i32] }, align 4
  %local.2 = alloca i32, align 4
  %enum.slot.5 = alloca { i32, [1 x i32] }, align 4
  %enum.slot.11 = alloca { i32, [1 x i32] }, align 4
  br label %bb.0
bb.0:
  %v.0 = freeze %zil_env_0 zeroinitializer
  store %zil_env_0 %v.0, ptr %local.0, align 1
  %v.1 = getelementptr i8, ptr %local.0, i64 0
  %v.2 = getelementptr i8, ptr %v.1, i64 0
  %v.3 = getelementptr i8, ptr %v.2, i64 0
  %v.4 = add i32 0, 7
  store { i32, [1 x i32] } zeroinitializer, ptr %enum.slot.5, align 4
  %tmp.0 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.5, i32 0, i32 0
  store i32 0, ptr %tmp.0, align 4
  %tmp.1 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.5, i32 0, i32 1
  %tmp.2 = getelementptr { i32 }, ptr %tmp.1, i32 0, i32 0
  store i32 %v.4, ptr %tmp.2, align 4
  %v.5 = load { i32, [1 x i32] }, ptr %enum.slot.5, align 4
  %v.6 = call { i32, [1 x i32] } @zil.fn.2(ptr %v.3, { i32, [1 x i32] } %v.5)
  store { i32, [1 x i32] } %v.6, ptr %local.1, align 4
  %v.7 = load { i32, [1 x i32] }, ptr %local.1, align 4
  %v.8 = extractvalue { i32, [1 x i32] } %v.7, 0
  %v.9 = add i32 0, 0
  %v.10 = icmp eq i32 %v.8, %v.9
  br i1 %v.10, label %bb.2, label %bb.1
bb.1:
  %v.15 = extractvalue { i32, [1 x i32] } %v.7, 0
  %v.16 = add i32 0, 1
  %v.17 = icmp eq i32 %v.15, %v.16
  br i1 %v.17, label %bb.4, label %bb.3
bb.2:
  %tmp.3 = extractvalue { i32, [1 x i32] } %v.7, 0
  %tmp.4 = icmp ne i32 %tmp.3, 0
  br i1 %tmp.4, label %zil.trap, label %checked.0
checked.0:
  store { i32, [1 x i32] } %v.7, ptr %enum.slot.11, align 4
  %tmp.5 = getelementptr { i32, [1 x i32] }, ptr %enum.slot.11, i32 0, i32 1
  %tmp.6 = getelementptr { i32 }, ptr %tmp.5, i32 0, i32 0
  %v.11 = load i32, ptr %tmp.6, align 4
  store i32 %v.11, ptr %local.2, align 4
  %v.12 = load i32, ptr %local.2, align 4
  %v.13 = add i32 0, 7
  %tmp.7 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.12, i32 %v.13)
  %v.14 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.14
bb.3:
  br label %zil.trap
bb.4:
  %v.18 = add i32 0, 1
  ret i32 %v.18
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i32, [1 x i32] } @zil.fn.1(%zil_env_0 %env, { i32, [1 x i32] } %arg.0) {
entry:
  %local.0 = alloca { i32, [1 x i32] }, align 4
  store { i32, [1 x i32] } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i32, [1 x i32] }, ptr %local.0, align 4
  ret { i32, [1 x i32] } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { i32, [1 x i32] } @zil.fn.2(ptr %arg.0, { i32, [1 x i32] } %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca { i32, [1 x i32] }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store { i32, [1 x i32] } %arg.1, ptr %local.1, align 4
  br label %bb.0
bb.0:
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = load { i32, [1 x i32] }, ptr %local.1, align 4
  %tmp.0 = load %zil_env_0, ptr %v.0, align 1
  %v.2 = call { i32, [1 x i32] } @zil.fn.1(%zil_env_0 %tmp.0, { i32, [1 x i32] } %v.1)
  ret { i32, [1 x i32] } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

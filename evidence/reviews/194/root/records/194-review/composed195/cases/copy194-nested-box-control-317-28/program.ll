; ZIL verified LLVM module
; verified-fingerprint: 44ba2974042eaf1a0765a824399427a049efea40ec404133236d26f0afe7ad9d
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca { i32 }, align 4
  %local.2 = alloca { { i32 } }, align 4
  %local.3 = alloca { { i32 } }, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 7
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store { i32 } %v.2, ptr %local.1, align 4
  %v.3 = load { i32 }, ptr %local.1, align 4
%v.4 = insertvalue { { i32 } } zeroinitializer, { i32 } %v.3, 0
  store { { i32 } } %v.4, ptr %local.2, align 4
  %v.5 = load { { i32 } }, ptr %local.2, align 4
  %v.6 = call { { i32 } } @zil.fn.1({ { i32 } } %v.5)
  store { { i32 } } %v.6, ptr %local.3, align 4
  %v.7 = load { { i32 } }, ptr %local.3, align 4
%v.8 = extractvalue { { i32 } } %v.7, 0
%v.9 = extractvalue { i32 } %v.8, 0
  %v.10 = add i32 0, 7
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.9, i32 %v.10)
  %v.11 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  ret i32 %v.11
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32 } } @zil.fn.1({ { i32 } } %arg.0) {
entry:
  %local.0 = alloca { { i32 } }, align 4
  store { { i32 } } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { { i32 } }, ptr %local.0, align 4
  ret { { i32 } } %v.0
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

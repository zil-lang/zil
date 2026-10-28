; ZIL verified LLVM module
; verified-fingerprint: 6a6328e85e35baa16848b4e64979d5dba6e47fcdc8b749a1c32fb7acd6cc3d17
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define i32 @zil.fn.0() {
entry:
  %local.0 = alloca { { i32, i1 }, { i32, i1 } }, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 7
  %v.1 = or i1 false, true
%tmp.0 = insertvalue { i32, i1 } zeroinitializer, i32 %v.0, 0
%v.2 = insertvalue { i32, i1 } %tmp.0, i1 %v.1, 1
  %v.3 = call { { i32, i1 }, { i32, i1 } } @zil.fn.1({ i32, i1 } %v.2)
  store { { i32, i1 }, { i32, i1 } } %v.3, ptr %local.0, align 4
  %v.4 = load { { i32, i1 }, { i32, i1 } }, ptr %local.0, align 4
%v.5 = extractvalue { { i32, i1 }, { i32, i1 } } %v.4, 0
%v.6 = extractvalue { i32, i1 } %v.5, 0
  %v.7 = load { { i32, i1 }, { i32, i1 } }, ptr %local.0, align 4
%v.8 = extractvalue { { i32, i1 }, { i32, i1 } } %v.7, 1
%v.9 = extractvalue { i32, i1 } %v.8, 0
  %tmp.1 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.6, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.1, 0
  %tmp.2 = extractvalue { i32, i1 } %tmp.1, 1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.11 = add i32 0, 14
  %tmp.3 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.10, i32 %v.11)
  %v.12 = extractvalue { i32, i1 } %tmp.3, 0
  %tmp.4 = extractvalue { i32, i1 } %tmp.3, 1
  br i1 %tmp.4, label %zil.trap, label %checked.1
checked.1:
  ret i32 %v.12
zil.trap:
  call void @llvm.trap()
  unreachable
}

define { { i32, i1 }, { i32, i1 } } @zil.fn.1({ i32, i1 } %arg.0) {
entry:
  %local.0 = alloca { i32, i1 }, align 4
  store { i32, i1 } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load { i32, i1 }, ptr %local.0, align 4
  %v.1 = load { i32, i1 }, ptr %local.0, align 4
%tmp.0 = insertvalue { { i32, i1 }, { i32, i1 } } zeroinitializer, { i32, i1 } %v.0, 0
%v.2 = insertvalue { { i32, i1 }, { i32, i1 } } %tmp.0, { i32, i1 } %v.1, 1
  ret { { i32, i1 }, { i32, i1 } } %v.2
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.0()
  ret i32 %entry.result
}

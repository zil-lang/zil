; ZIL verified LLVM module
; verified-fingerprint: d6f2d298f4b9afd3c91ad9cdd22ddceef3fe8932d958b060cd0a3d8a15b66d8d
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define { i32 } @zil.fn.0({ i32 } %arg.0, i1 %arg.1) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i32 }, align 4
  %local.1 = alloca i1, align 1
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %local.4 = alloca [2 x i32], align 4
  %local.5 = alloca i32, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { i32 }, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { i32 }, align 4
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { i32, { i32 }, [2 x i32], { i32 } }, align 4
  %local.10 = alloca i32, align 4
  %resource.live.11 = alloca i1, align 1
  store i1 false, ptr %resource.live.11, align 1
  %local.11 = alloca { i32 }, align 4
  %local.12 = alloca [2 x i32], align 4
  %local.13 = alloca i32, align 4
  %resource.live.14 = alloca i1, align 1
  store i1 false, ptr %resource.live.14, align 1
  %local.14 = alloca { i32 }, align 4
  %resource.live.15 = alloca i1, align 1
  store i1 false, ptr %resource.live.15, align 1
  %local.15 = alloca { i32 }, align 4
  %resource.live.16 = alloca i1, align 1
  store i1 false, ptr %resource.live.16, align 1
  %local.16 = alloca { i32 }, align 4
  %resource.live.17 = alloca i1, align 1
  store i1 false, ptr %resource.live.17, align 1
  %local.17 = alloca { i32, { i32 }, [2 x i32], { i32 } }, align 4
  %resource.live.18 = alloca i1, align 1
  store i1 false, ptr %resource.live.18, align 1
  %local.18 = alloca { i32, { i32 }, [2 x i32], { i32 } }, align 4
  %package.live.18.1 = alloca i1, align 1
  store i1 false, ptr %package.live.18.1, align 1
  %package.live.18.3 = alloca i1, align 1
  store i1 false, ptr %package.live.18.3, align 1
  store { i32 } %arg.0, ptr %local.0, align 4
  store i1 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.9, align 1
  %v.0 = load i1, ptr %local.1, align 1
  br i1 %v.0, label %bb.1, label %bb.2
bb.1:
  store i1 false, ptr %resource.live.8, align 1
  store i1 false, ptr %resource.live.7, align 1
  %v.1 = add i32 0, 7
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = load { i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.2, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.3 = add i32 0, 11
  %v.4 = add i32 0, 13
%tmp.0 = insertvalue [2 x i32] zeroinitializer, i32 %v.3, 0
%v.5 = insertvalue [2 x i32] %tmp.0, i32 %v.4, 1
  store [2 x i32] %v.5, ptr %local.4, align 4
  %v.6 = add i32 0, 29
  store i32 %v.6, ptr %local.5, align 4
  %v.7 = load i32, ptr %local.5, align 4
%v.8 = insertvalue { i32 } zeroinitializer, i32 %v.7, 0
  store i1 false, ptr %resource.live.6, align 1
  store { i32 } %v.8, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.9 = load i32, ptr %local.2, align 4
  %v.10 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  store { i32 } %v.10, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.11 = load [2 x i32], ptr %local.4, align 4
  %v.12 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  store { i32 } %v.12, ptr %local.8, align 4
  store i1 true, ptr %resource.live.8, align 1
  %v.13 = load { i32 }, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
  %v.14 = load { i32 }, ptr %local.8, align 4
  store i1 false, ptr %resource.live.8, align 1
%tmp.1 = insertvalue { i32, { i32 }, [2 x i32], { i32 } } zeroinitializer, i32 %v.9, 0
%tmp.2 = insertvalue { i32, { i32 }, [2 x i32], { i32 } } %tmp.1, { i32 } %v.13, 1
%tmp.3 = insertvalue { i32, { i32 }, [2 x i32], { i32 } } %tmp.2, [2 x i32] %v.11, 2
%v.15 = insertvalue { i32, { i32 }, [2 x i32], { i32 } } %tmp.3, { i32 } %v.14, 3
  %tmp.4 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.4, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.8, align 1
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.8, align 1
  %tmp.5 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.5, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.7, align 1
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.6 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.6, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.6, align 1
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.7 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.7, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.3, align 1
  store { i32, { i32 }, [2 x i32], { i32 } } %v.15, ptr %local.9, align 4
  store i1 true, ptr %resource.live.9, align 1
  br label %bb.3
bb.2:
  store i1 false, ptr %resource.live.16, align 1
  store i1 false, ptr %resource.live.15, align 1
  %v.16 = add i32 0, 9
  store i32 %v.16, ptr %local.10, align 4
  %v.17 = load { i32 }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.11, align 1
  store { i32 } %v.17, ptr %local.11, align 4
  store i1 true, ptr %resource.live.11, align 1
  %v.18 = add i32 0, 17
  %v.19 = add i32 0, 19
%tmp.8 = insertvalue [2 x i32] zeroinitializer, i32 %v.18, 0
%v.20 = insertvalue [2 x i32] %tmp.8, i32 %v.19, 1
  store [2 x i32] %v.20, ptr %local.12, align 4
  %v.21 = add i32 0, 29
  store i32 %v.21, ptr %local.13, align 4
  %v.22 = load i32, ptr %local.13, align 4
%v.23 = insertvalue { i32 } zeroinitializer, i32 %v.22, 0
  store i1 false, ptr %resource.live.14, align 1
  store { i32 } %v.23, ptr %local.14, align 4
  store i1 true, ptr %resource.live.14, align 1
  %v.24 = load i32, ptr %local.10, align 4
  %v.25 = load { i32 }, ptr %local.11, align 4
  store i1 false, ptr %resource.live.11, align 1
  store { i32 } %v.25, ptr %local.15, align 4
  store i1 true, ptr %resource.live.15, align 1
  %v.26 = load [2 x i32], ptr %local.12, align 4
  %v.27 = load { i32 }, ptr %local.14, align 4
  store i1 false, ptr %resource.live.14, align 1
  store { i32 } %v.27, ptr %local.16, align 4
  store i1 true, ptr %resource.live.16, align 1
  %v.28 = load { i32 }, ptr %local.15, align 4
  store i1 false, ptr %resource.live.15, align 1
  %v.29 = load { i32 }, ptr %local.16, align 4
  store i1 false, ptr %resource.live.16, align 1
%tmp.9 = insertvalue { i32, { i32 }, [2 x i32], { i32 } } zeroinitializer, i32 %v.24, 0
%tmp.10 = insertvalue { i32, { i32 }, [2 x i32], { i32 } } %tmp.9, { i32 } %v.28, 1
%tmp.11 = insertvalue { i32, { i32 }, [2 x i32], { i32 } } %tmp.10, [2 x i32] %v.26, 2
%v.30 = insertvalue { i32, { i32 }, [2 x i32], { i32 } } %tmp.11, { i32 } %v.29, 3
  %tmp.12 = load i1, ptr %resource.live.16, align 1
  br i1 %tmp.12, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.16, align 1
  br label %resource.done.4
resource.done.4:
  store i1 false, ptr %resource.live.16, align 1
  %tmp.13 = load i1, ptr %resource.live.15, align 1
  br i1 %tmp.13, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.15, align 1
  br label %resource.done.5
resource.done.5:
  store i1 false, ptr %resource.live.15, align 1
  %tmp.14 = load i1, ptr %resource.live.14, align 1
  br i1 %tmp.14, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.14, align 1
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.14, align 1
  %tmp.15 = load i1, ptr %resource.live.11, align 1
  br i1 %tmp.15, label %resource.drop.7, label %resource.done.7
resource.drop.7:
  store i1 false, ptr %resource.live.11, align 1
  br label %resource.done.7
resource.done.7:
  store i1 false, ptr %resource.live.11, align 1
  store { i32, { i32 }, [2 x i32], { i32 } } %v.30, ptr %local.9, align 4
  store i1 true, ptr %resource.live.9, align 1
  br label %bb.3
bb.3:
  %v.31 = load { i32, { i32 }, [2 x i32], { i32 } }, ptr %local.9, align 4
  store i1 false, ptr %resource.live.9, align 1
  %tmp.16 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.16, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.9, align 1
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.17, align 1
  store { i32, { i32 }, [2 x i32], { i32 } } %v.31, ptr %local.17, align 4
  store i1 true, ptr %resource.live.17, align 1
  %v.32 = load { i32, { i32 }, [2 x i32], { i32 } }, ptr %local.17, align 4
  store i1 false, ptr %resource.live.17, align 1
  store i1 false, ptr %resource.live.18, align 1
  store i1 false, ptr %package.live.18.1, align 1
  store i1 false, ptr %package.live.18.3, align 1
  store { i32, { i32 }, [2 x i32], { i32 } } %v.32, ptr %local.18, align 4
  store i1 true, ptr %resource.live.18, align 1
  store i1 true, ptr %package.live.18.1, align 1
  store i1 true, ptr %package.live.18.3, align 1
  %tmp.17 = getelementptr { i32, { i32 }, [2 x i32], { i32 } }, ptr %local.18, i32 0, i32 3
  %v.33 = load { i32 }, ptr %tmp.17, align 4
  store i1 false, ptr %package.live.18.3, align 1
  %tmp.18 = load i1, ptr %resource.live.18, align 1
  br i1 %tmp.18, label %package.drop.9, label %package.done.9
package.drop.9:
  store i1 false, ptr %resource.live.18, align 1
  %tmp.19 = load i1, ptr %package.live.18.1, align 1
  br i1 %tmp.19, label %package.field.drop.10, label %package.field.done.10
package.field.drop.10:
  store i1 false, ptr %package.live.18.1, align 1
  br label %package.field.done.10
package.field.done.10:
  %tmp.20 = load i1, ptr %package.live.18.3, align 1
  br i1 %tmp.20, label %package.field.drop.11, label %package.field.done.11
package.field.drop.11:
  store i1 false, ptr %package.live.18.3, align 1
  br label %package.field.done.11
package.field.done.11:
  br label %package.done.9
package.done.9:
  store i1 false, ptr %resource.live.18, align 1
  store i1 false, ptr %package.live.18.1, align 1
  store i1 false, ptr %package.live.18.3, align 1
  %tmp.21 = load i1, ptr %resource.live.17, align 1
  br i1 %tmp.21, label %package.drop.12, label %package.done.12
package.drop.12:
  store i1 false, ptr %resource.live.17, align 1
  br label %package.done.12
package.done.12:
  store i1 false, ptr %resource.live.17, align 1
  store i1 false, ptr %resource.live.9, align 1
  %tmp.22 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.22, label %resource.drop.13, label %resource.done.13
resource.drop.13:
  store i1 false, ptr %resource.live.0, align 1
  br label %resource.done.13
resource.done.13:
  store i1 false, ptr %resource.live.0, align 1
  ret { i32 } %v.33
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32 }, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.1, align 1
  %v.0 = add i32 0, 17
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = load i32, ptr %local.0, align 4
%v.2 = insertvalue { i32 } zeroinitializer, i32 %v.1, 0
  store { i32 } %v.2, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  %v.3 = or i1 false, false
  %v.4 = load { i32 }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  %v.5 = call { i32 } @zil.fn.0({ i32 } %v.4, i1 %v.3)
  %tmp.0 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.0, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.1, align 1
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.5, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.6 = getelementptr i8, ptr %local.2, i64 0
  %tmp.1 = getelementptr { i32 }, ptr %v.6, i32 0, i32 0
  %v.7 = load i32, ptr %tmp.1, align 4
  %v.8 = add i32 0, 29
  %tmp.2 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.7, i32 %v.8)
  %v.9 = extractvalue { i32, i1 } %tmp.2, 0
  %tmp.3 = extractvalue { i32, i1 } %tmp.2, 1
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %tmp.4 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.2, align 1
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.1, align 1
  ret i32 %v.9
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

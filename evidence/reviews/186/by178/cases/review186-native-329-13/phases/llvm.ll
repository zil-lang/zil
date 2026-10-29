; ZIL verified LLVM module
; verified-fingerprint: 82615dc5013bec136e4dfd38a4775d111905badf6ddc3d5d025ce9d1a3b2b228
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)

define { i32 } @zil.fn.0({ i32, { i32 }, [2 x i32], { i32 } } %arg.0) {
entry:
  %resource.live.0 = alloca i1, align 1
  store i1 true, ptr %resource.live.0, align 1
  %local.0 = alloca { i32, { i32 }, [2 x i32], { i32 } }, align 4
  %resource.live.1 = alloca i1, align 1
  store i1 false, ptr %resource.live.1, align 1
  %local.1 = alloca { i32, { i32 }, [2 x i32], { i32 } }, align 4
  %package.live.1.1 = alloca i1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  %package.live.1.3 = alloca i1, align 1
  store i1 false, ptr %package.live.1.3, align 1
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %resource.live.4 = alloca i1, align 1
  store i1 false, ptr %resource.live.4, align 1
  %local.4 = alloca { i32, { i32 }, [2 x i32], { i32 } }, align 4
  %package.live.4.1 = alloca i1, align 1
  store i1 false, ptr %package.live.4.1, align 1
  %package.live.4.3 = alloca i1, align 1
  store i1 false, ptr %package.live.4.3, align 1
  store { i32, { i32 }, [2 x i32], { i32 } } %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = load { i32, { i32 }, [2 x i32], { i32 } }, ptr %local.0, align 4
  store i1 false, ptr %resource.live.0, align 1
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.3, align 1
  store { i32, { i32 }, [2 x i32], { i32 } } %v.0, ptr %local.1, align 4
  store i1 true, ptr %resource.live.1, align 1
  store i1 true, ptr %package.live.1.1, align 1
  store i1 true, ptr %package.live.1.3, align 1
  %v.1 = add i32 0, 31
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = load i32, ptr %local.2, align 4
%v.3 = insertvalue { i32 } zeroinitializer, i32 %v.2, 0
  store { i32 } %v.3, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %tmp.0 = load i1, ptr %package.live.1.3, align 1
  br i1 %tmp.0, label %package.field.drop.0, label %package.field.done.0
package.field.drop.0:
  store i1 false, ptr %package.live.1.3, align 1
  br label %package.field.done.0
package.field.done.0:
  %v.4 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  %tmp.1 = getelementptr { i32, { i32 }, [2 x i32], { i32 } }, ptr %local.1, i32 0, i32 3
  store { i32 } %v.4, ptr %tmp.1, align 4
  store i1 true, ptr %package.live.1.3, align 1
  %tmp.2 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.2, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.3, align 1
  br label %resource.done.1
resource.done.1:
  %v.5 = load { i32, { i32 }, [2 x i32], { i32 } }, ptr %local.1, align 4
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.3, align 1
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %package.live.4.1, align 1
  store i1 false, ptr %package.live.4.3, align 1
  store { i32, { i32 }, [2 x i32], { i32 } } %v.5, ptr %local.4, align 4
  store i1 true, ptr %resource.live.4, align 1
  store i1 true, ptr %package.live.4.1, align 1
  store i1 true, ptr %package.live.4.3, align 1
  %tmp.3 = getelementptr { i32, { i32 }, [2 x i32], { i32 } }, ptr %local.4, i32 0, i32 3
  %v.6 = load { i32 }, ptr %tmp.3, align 4
  store i1 false, ptr %package.live.4.3, align 1
  %tmp.4 = load i1, ptr %resource.live.4, align 1
  br i1 %tmp.4, label %package.drop.2, label %package.done.2
package.drop.2:
  store i1 false, ptr %resource.live.4, align 1
  %tmp.5 = load i1, ptr %package.live.4.1, align 1
  br i1 %tmp.5, label %package.field.drop.3, label %package.field.done.3
package.field.drop.3:
  store i1 false, ptr %package.live.4.1, align 1
  br label %package.field.done.3
package.field.done.3:
  %tmp.6 = load i1, ptr %package.live.4.3, align 1
  br i1 %tmp.6, label %package.field.drop.4, label %package.field.done.4
package.field.drop.4:
  store i1 false, ptr %package.live.4.3, align 1
  br label %package.field.done.4
package.field.done.4:
  br label %package.done.2
package.done.2:
  store i1 false, ptr %resource.live.4, align 1
  store i1 false, ptr %package.live.4.1, align 1
  store i1 false, ptr %package.live.4.3, align 1
  store i1 false, ptr %resource.live.3, align 1
  %tmp.7 = load i1, ptr %resource.live.1, align 1
  br i1 %tmp.7, label %package.drop.5, label %package.done.5
package.drop.5:
  store i1 false, ptr %resource.live.1, align 1
  %tmp.8 = load i1, ptr %package.live.1.1, align 1
  br i1 %tmp.8, label %package.field.drop.6, label %package.field.done.6
package.field.drop.6:
  store i1 false, ptr %package.live.1.1, align 1
  br label %package.field.done.6
package.field.done.6:
  %tmp.9 = load i1, ptr %package.live.1.3, align 1
  br i1 %tmp.9, label %package.field.drop.7, label %package.field.done.7
package.field.drop.7:
  store i1 false, ptr %package.live.1.3, align 1
  br label %package.field.done.7
package.field.done.7:
  br label %package.done.5
package.done.5:
  store i1 false, ptr %resource.live.1, align 1
  store i1 false, ptr %package.live.1.1, align 1
  store i1 false, ptr %package.live.1.3, align 1
  %tmp.10 = load i1, ptr %resource.live.0, align 1
  br i1 %tmp.10, label %package.drop.8, label %package.done.8
package.drop.8:
  store i1 false, ptr %resource.live.0, align 1
  br label %package.done.8
package.done.8:
  store i1 false, ptr %resource.live.0, align 1
  ret { i32 } %v.6
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.1() {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  %resource.live.2 = alloca i1, align 1
  store i1 false, ptr %resource.live.2, align 1
  %local.2 = alloca { i32 }, align 4
  %local.3 = alloca [2 x i32], align 4
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32 }, align 4
  %resource.live.6 = alloca i1, align 1
  store i1 false, ptr %resource.live.6, align 1
  %local.6 = alloca { i32 }, align 4
  %resource.live.7 = alloca i1, align 1
  store i1 false, ptr %resource.live.7, align 1
  %local.7 = alloca { i32 }, align 4
  %resource.live.8 = alloca i1, align 1
  store i1 false, ptr %resource.live.8, align 1
  %local.8 = alloca { i32, { i32 }, [2 x i32], { i32 } }, align 4
  %resource.live.9 = alloca i1, align 1
  store i1 false, ptr %resource.live.9, align 1
  %local.9 = alloca { i32, { i32 }, [2 x i32], { i32 } }, align 4
  %resource.live.10 = alloca i1, align 1
  store i1 false, ptr %resource.live.10, align 1
  %local.10 = alloca { i32 }, align 4
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.9, align 1
  store i1 false, ptr %resource.live.7, align 1
  store i1 false, ptr %resource.live.6, align 1
  %v.0 = add i32 0, 7
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = add i32 0, 17
  store i32 %v.1, ptr %local.1, align 4
  %v.2 = load i32, ptr %local.1, align 4
%v.3 = insertvalue { i32 } zeroinitializer, i32 %v.2, 0
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.3, ptr %local.2, align 4
  store i1 true, ptr %resource.live.2, align 1
  %v.4 = add i32 0, 11
  %v.5 = add i32 0, 13
%tmp.0 = insertvalue [2 x i32] zeroinitializer, i32 %v.4, 0
%v.6 = insertvalue [2 x i32] %tmp.0, i32 %v.5, 1
  store [2 x i32] %v.6, ptr %local.3, align 4
  %v.7 = add i32 0, 19
  store i32 %v.7, ptr %local.4, align 4
  %v.8 = load i32, ptr %local.4, align 4
%v.9 = insertvalue { i32 } zeroinitializer, i32 %v.8, 0
  store i1 false, ptr %resource.live.5, align 1
  store { i32 } %v.9, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.10 = load i32, ptr %local.0, align 4
  %v.11 = load { i32 }, ptr %local.2, align 4
  store i1 false, ptr %resource.live.2, align 1
  store { i32 } %v.11, ptr %local.6, align 4
  store i1 true, ptr %resource.live.6, align 1
  %v.12 = load [2 x i32], ptr %local.3, align 4
  %v.13 = load { i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  store { i32 } %v.13, ptr %local.7, align 4
  store i1 true, ptr %resource.live.7, align 1
  %v.14 = load { i32 }, ptr %local.6, align 4
  store i1 false, ptr %resource.live.6, align 1
  %v.15 = load { i32 }, ptr %local.7, align 4
  store i1 false, ptr %resource.live.7, align 1
%tmp.1 = insertvalue { i32, { i32 }, [2 x i32], { i32 } } zeroinitializer, i32 %v.10, 0
%tmp.2 = insertvalue { i32, { i32 }, [2 x i32], { i32 } } %tmp.1, { i32 } %v.14, 1
%tmp.3 = insertvalue { i32, { i32 }, [2 x i32], { i32 } } %tmp.2, [2 x i32] %v.12, 2
%v.16 = insertvalue { i32, { i32 }, [2 x i32], { i32 } } %tmp.3, { i32 } %v.15, 3
  %tmp.4 = load i1, ptr %resource.live.7, align 1
  br i1 %tmp.4, label %resource.drop.0, label %resource.done.0
resource.drop.0:
  store i1 false, ptr %resource.live.7, align 1
  br label %resource.done.0
resource.done.0:
  store i1 false, ptr %resource.live.7, align 1
  %tmp.5 = load i1, ptr %resource.live.6, align 1
  br i1 %tmp.5, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.6, align 1
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.6, align 1
  %tmp.6 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.6, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.5, align 1
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.7 = load i1, ptr %resource.live.2, align 1
  br i1 %tmp.7, label %resource.drop.3, label %resource.done.3
resource.drop.3:
  store i1 false, ptr %resource.live.2, align 1
  br label %resource.done.3
resource.done.3:
  store i1 false, ptr %resource.live.2, align 1
  store i1 false, ptr %resource.live.8, align 1
  store { i32, { i32 }, [2 x i32], { i32 } } %v.16, ptr %local.8, align 4
  store i1 true, ptr %resource.live.8, align 1
  %v.17 = load { i32, { i32 }, [2 x i32], { i32 } }, ptr %local.8, align 4
  store i1 false, ptr %resource.live.8, align 1
  store { i32, { i32 }, [2 x i32], { i32 } } %v.17, ptr %local.9, align 4
  store i1 true, ptr %resource.live.9, align 1
  %v.18 = load { i32, { i32 }, [2 x i32], { i32 } }, ptr %local.9, align 4
  store i1 false, ptr %resource.live.9, align 1
  %v.19 = call { i32 } @zil.fn.0({ i32, { i32 }, [2 x i32], { i32 } } %v.18)
  %tmp.8 = load i1, ptr %resource.live.9, align 1
  br i1 %tmp.8, label %package.drop.4, label %package.done.4
package.drop.4:
  store i1 false, ptr %resource.live.9, align 1
  br label %package.done.4
package.done.4:
  store i1 false, ptr %resource.live.10, align 1
  store { i32 } %v.19, ptr %local.10, align 4
  store i1 true, ptr %resource.live.10, align 1
  %v.20 = getelementptr i8, ptr %local.10, i64 0
  %tmp.9 = getelementptr { i32 }, ptr %v.20, i32 0, i32 0
  %v.21 = load i32, ptr %tmp.9, align 4
  %v.22 = add i32 0, 31
  %tmp.10 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.21, i32 %v.22)
  %v.23 = extractvalue { i32, i1 } %tmp.10, 0
  %tmp.11 = extractvalue { i32, i1 } %tmp.10, 1
  br i1 %tmp.11, label %zil.trap, label %checked.5
checked.5:
  %tmp.12 = load i1, ptr %resource.live.10, align 1
  br i1 %tmp.12, label %resource.drop.6, label %resource.done.6
resource.drop.6:
  store i1 false, ptr %resource.live.10, align 1
  br label %resource.done.6
resource.done.6:
  store i1 false, ptr %resource.live.10, align 1
  store i1 false, ptr %resource.live.9, align 1
  %tmp.13 = load i1, ptr %resource.live.8, align 1
  br i1 %tmp.13, label %package.drop.7, label %package.done.7
package.drop.7:
  store i1 false, ptr %resource.live.8, align 1
  br label %package.done.7
package.done.7:
  store i1 false, ptr %resource.live.8, align 1
  ret i32 %v.23
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.1()
  ret i32 %entry.result
}

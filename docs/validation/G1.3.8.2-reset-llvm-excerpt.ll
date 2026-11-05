; Generated excerpt only, not a standalone input. Full original LLVM SHA256: 6404d5e70ae938e077387eaade37cdb8cd9e0ba6845358e6a8129785fe426d6a
; Actual CLI source 5a7163c644a88cdcfd0e7faafad7e0cca9850568, branch_failure / Clang O0.
define { i1, { i32, [0 x i8] }, i32 } @zil.fn.4(ptr %arg.0, i1 %arg.1) {
entry:
  %local.0 = alloca ptr, align 8
  %local.1 = alloca i1, align 1
  %local.2 = alloca i32, align 4
  %resource.live.3 = alloca i1, align 1
  store i1 false, ptr %resource.live.3, align 1
  %local.3 = alloca { i32 }, align 4
  %local.4 = alloca i32, align 4
  %resource.live.5 = alloca i1, align 1
  store i1 false, ptr %resource.live.5, align 1
  %local.5 = alloca { i32 }, align 4
  store ptr %arg.0, ptr %local.0, align 8
  store i1 %arg.1, ptr %local.1, align 1
  br label %bb.0
bb.0:
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.3, align 1
  %v.0 = load ptr, ptr %local.0, align 8
  %v.1 = add i32 0, 66
  store i32 %v.1, ptr %local.2, align 4
  %v.2 = load i32, ptr %local.2, align 4
  %v.3 = insertvalue { i32 } zeroinitializer, i32 %v.2, 0
  store { i32 } %v.3, ptr %local.3, align 4
  store i1 true, ptr %resource.live.3, align 1
  %v.4 = add i32 0, 67
  store i32 %v.4, ptr %local.4, align 4
  %v.5 = load i32, ptr %local.4, align 4
  %v.6 = insertvalue { i32 } zeroinitializer, i32 %v.5, 0
  store { i32 } %v.6, ptr %local.5, align 4
  store i1 true, ptr %resource.live.5, align 1
  %v.7 = load i1, ptr %local.1, align 1
  %v.8 = call { i1, { i32, [0 x i8] }, i32 } @zil.fn.2(i1 %v.7)
  %v.9 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.8, 0
  br i1 %v.9, label %bb.1, label %bb.2
bb.1:
  %tmp.0 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.8, 0
  %tmp.1 = icmp ne i1 %tmp.0, true
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  %v.10 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.8, 1
  %tmp.2 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 true, 0
  %v.11 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.2, { i32, [0 x i8] } %v.10, 1
  %tmp.3 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.3, label %resource.drop.1, label %resource.done.1
resource.drop.1:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.1(ptr %local.5)
  br label %resource.done.1
resource.done.1:
  store i1 false, ptr %resource.live.5, align 1
  %tmp.4 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.4, label %resource.drop.2, label %resource.done.2
resource.drop.2:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.1(ptr %local.3)
  br label %resource.done.2
resource.done.2:
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.11
bb.2:
  %tmp.5 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.8, 0
  %tmp.6 = icmp ne i1 %tmp.5, false
  br i1 %tmp.6, label %zil.trap, label %checked.3
checked.3:
  %v.12 = extractvalue { i1, { i32, [0 x i8] }, i32 } %v.8, 2
  %v.13 = load { i32 }, ptr %local.3, align 4
  store i1 false, ptr %resource.live.3, align 1
  %v.14 = load { i32 }, ptr %local.5, align 4
  store i1 false, ptr %resource.live.5, align 1
  %v.15 = call { { i32 }, i32 } @zil.fn.3({ i32 } %v.13, { i32 } %v.14, i32 %v.12)
  %tmp.7 = getelementptr { { i32 }, i32 }, ptr %v.0, i32 0, i32 0
  call void @zil.fn.1(ptr %tmp.7)
  store { { i32 }, i32 } %v.15, ptr %v.0, align 4
  %tmp.8 = load i1, ptr %resource.live.5, align 1
  br i1 %tmp.8, label %resource.drop.4, label %resource.done.4
resource.drop.4:
  store i1 false, ptr %resource.live.5, align 1
  call void @zil.fn.1(ptr %local.5)
  br label %resource.done.4
resource.done.4:
  %tmp.9 = load i1, ptr %resource.live.3, align 1
  br i1 %tmp.9, label %resource.drop.5, label %resource.done.5
resource.drop.5:
  store i1 false, ptr %resource.live.3, align 1
  call void @zil.fn.1(ptr %local.3)
  br label %resource.done.5
resource.done.5:
  %v.16 = add i32 0, 7
  %tmp.10 = insertvalue { i1, { i32, [0 x i8] }, i32 } zeroinitializer, i1 false, 0
  %v.17 = insertvalue { i1, { i32, [0 x i8] }, i32 } %tmp.10, i32 %v.16, 2
  store i1 false, ptr %resource.live.5, align 1
  store i1 false, ptr %resource.live.3, align 1
  ret { i1, { i32, [0 x i8] }, i32 } %v.17
zil.trap:
  call void @llvm.trap()
  unreachable
}

; ZIL verified LLVM module
; verified-fingerprint: 0e8df47f2f3f57e72a1c0f22f0b73c7631703811fef5e7f5a58bf566c7a8e0d8
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.ssub.with.overflow.i32(i32, i32)
declare i32 @getchar() nonlazybind
declare i32 @putchar(i32) nonlazybind
define i32 @zil.fn.2(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  %local.1 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = add i32 0, 0
  %v.2 = icmp slt i32 %v.0, %v.1
  br i1 %v.2, label %bb.1, label %bb.2
bb.1:
  %v.3 = add i32 0, -2147483648
  ret i32 %v.3
bb.2:
  br label %bb.3
bb.3:
  %v.4 = load i32, ptr %local.0, align 4
  %v.5 = add i32 0, 0
  %v.6 = icmp eq i32 %v.4, %v.5
  br i1 %v.6, label %bb.4, label %bb.5
bb.4:
  %v.7 = add i32 0, -257
  ret i32 %v.7
bb.5:
  br label %bb.6
bb.6:
  %v.8 = load i32, ptr %local.0, align 4
  %v.9 = add i32 0, 1
  %v.10 = icmp eq i32 %v.8, %v.9
  br i1 %v.10, label %bb.7, label %bb.8
bb.7:
  %v.11 = add i32 0, -256
  ret i32 %v.11
bb.8:
  br label %bb.9
bb.9:
  %v.12 = load i32, ptr %local.0, align 4
  %v.13 = add i32 0, 2
  %v.14 = icmp eq i32 %v.12, %v.13
  br i1 %v.14, label %bb.10, label %bb.11
bb.10:
  %v.15 = add i32 0, -1
  ret i32 %v.15
bb.11:
  br label %bb.12
bb.12:
  %v.16 = load i32, ptr %local.0, align 4
  %v.17 = add i32 0, 3
  %v.18 = icmp eq i32 %v.16, %v.17
  br i1 %v.18, label %bb.13, label %bb.14
bb.13:
  %v.19 = add i32 0, 2147483647
  ret i32 %v.19
bb.14:
  br label %bb.15
bb.15:
  %v.20 = load i32, ptr %local.0, align 4
  %v.21 = add i32 0, 4
  %v.22 = icmp eq i32 %v.20, %v.21
  br i1 %v.22, label %bb.16, label %bb.17
bb.16:
  %v.23 = add i32 0, -2147483392
  ret i32 %v.23
bb.17:
  br label %bb.18
bb.18:
  %v.24 = load i32, ptr %local.0, align 4
  store i32 %v.24, ptr %local.1, align 4
  br label %bb.19
bb.19:
  %v.25 = load i32, ptr %local.1, align 4
  %v.26 = add i32 0, 4
  %v.27 = icmp sgt i32 %v.25, %v.26
  br i1 %v.27, label %bb.20, label %bb.21
bb.20:
  %v.28 = load i32, ptr %local.1, align 4
  %v.29 = add i32 0, 1
  %tmp.0 = call { i32, i1 } @llvm.ssub.with.overflow.i32(i32 %v.28, i32 %v.29)
  %v.30 = extractvalue { i32, i1 } %tmp.0, 0
  %tmp.1 = extractvalue { i32, i1 } %tmp.0, 1
  br i1 %tmp.1, label %zil.trap, label %checked.0
checked.0:
  store i32 %v.30, ptr %local.1, align 4
  br label %bb.19
bb.21:
  %v.31 = add i32 0, -2
  ret i32 %v.31
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i64 @zil.fn.3(i32 %arg.0) {
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
  %v.3 = add i64 0, -9223372036854775808
  ret i64 %v.3
bb.2:
  br label %bb.3
bb.3:
  %v.4 = load i32, ptr %local.0, align 4
  %v.5 = add i32 0, 0
  %v.6 = icmp eq i32 %v.4, %v.5
  br i1 %v.6, label %bb.4, label %bb.5
bb.4:
  %v.7 = add i64 0, -4294967297
  ret i64 %v.7
bb.5:
  br label %bb.6
bb.6:
  %v.8 = load i32, ptr %local.0, align 4
  %v.9 = add i32 0, 1
  %v.10 = icmp eq i32 %v.8, %v.9
  br i1 %v.10, label %bb.7, label %bb.8
bb.7:
  %v.11 = add i64 0, -4294967296
  ret i64 %v.11
bb.8:
  br label %bb.9
bb.9:
  %v.12 = load i32, ptr %local.0, align 4
  %v.13 = add i32 0, 2
  %v.14 = icmp eq i32 %v.12, %v.13
  br i1 %v.14, label %bb.10, label %bb.11
bb.10:
  %v.15 = add i64 0, -1
  ret i64 %v.15
bb.11:
  br label %bb.12
bb.12:
  %v.16 = load i32, ptr %local.0, align 4
  %v.17 = add i32 0, 3
  %v.18 = icmp eq i32 %v.16, %v.17
  br i1 %v.18, label %bb.13, label %bb.14
bb.13:
  %v.19 = add i64 0, 9223372036854775807
  ret i64 %v.19
bb.14:
  br label %bb.15
bb.15:
  %v.20 = load i32, ptr %local.0, align 4
  %v.21 = add i32 0, 4
  %v.22 = icmp eq i32 %v.20, %v.21
  br i1 %v.22, label %bb.16, label %bb.17
bb.16:
  %v.23 = add i64 0, -9223372036854775552
  ret i64 %v.23
bb.17:
  br label %bb.18
bb.18:
  %v.24 = add i64 0, -2
  ret i64 %v.24
zil.trap:
  call void @llvm.trap()
  unreachable
}
define void @zil.fn.4(i32 %arg.0) {
entry:
  %local.0 = alloca i32, align 4
  store i32 %arg.0, ptr %local.0, align 4
  br label %bb.0
bb.0:
  %v.0 = load i32, ptr %local.0, align 4
  %v.1 = add i32 0, 255
  %v.2 = and i32 %v.0, %v.1
  %v.3 = call i32 @putchar(i32 %v.2)
  %v.4 = load i32, ptr %local.0, align 4
  %v.5 = add i32 0, 8
  %tmp.0 = icmp uge i32 %v.5, 32
  br i1 %tmp.0, label %zil.trap, label %checked.0
checked.0:
  %v.6 = ashr i32 %v.4, %v.5
  %v.7 = add i32 0, 255
  %v.8 = and i32 %v.6, %v.7
  %v.9 = call i32 @putchar(i32 %v.8)
  %v.10 = load i32, ptr %local.0, align 4
  %v.11 = add i32 0, 16
  %tmp.1 = icmp uge i32 %v.11, 32
  br i1 %tmp.1, label %zil.trap, label %checked.1
checked.1:
  %v.12 = ashr i32 %v.10, %v.11
  %v.13 = add i32 0, 255
  %v.14 = and i32 %v.12, %v.13
  %v.15 = call i32 @putchar(i32 %v.14)
  %v.16 = load i32, ptr %local.0, align 4
  %v.17 = add i32 0, 24
  %tmp.2 = icmp uge i32 %v.17, 32
  br i1 %tmp.2, label %zil.trap, label %checked.2
checked.2:
  %v.18 = ashr i32 %v.16, %v.17
  %v.19 = add i32 0, 255
  %v.20 = and i32 %v.18, %v.19
  %v.21 = call i32 @putchar(i32 %v.20)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}
define void @zil.fn.5(i64 %arg.0) {
entry:
  %local.0 = alloca i64, align 8
  store i64 %arg.0, ptr %local.0, align 8
  br label %bb.0
bb.0:
  %v.0 = load i64, ptr %local.0, align 8
  %v.1 = add i64 0, 255
  %v.2 = and i64 %v.0, %v.1
  %tmp.0 = icmp slt i64 %v.2, -2147483648
  %tmp.1 = icmp sgt i64 %v.2, 2147483647
  %tmp.2 = or i1 %tmp.0, %tmp.1
  br i1 %tmp.2, label %zil.trap, label %checked.0
checked.0:
  %v.3 = trunc i64 %v.2 to i32
  %v.4 = call i32 @putchar(i32 %v.3)
  %v.5 = load i64, ptr %local.0, align 8
  %v.6 = add i64 0, 8
  %tmp.3 = icmp uge i64 %v.6, 64
  br i1 %tmp.3, label %zil.trap, label %checked.1
checked.1:
  %v.7 = ashr i64 %v.5, %v.6
  %v.8 = add i64 0, 255
  %v.9 = and i64 %v.7, %v.8
  %tmp.4 = icmp slt i64 %v.9, -2147483648
  %tmp.5 = icmp sgt i64 %v.9, 2147483647
  %tmp.6 = or i1 %tmp.4, %tmp.5
  br i1 %tmp.6, label %zil.trap, label %checked.2
checked.2:
  %v.10 = trunc i64 %v.9 to i32
  %v.11 = call i32 @putchar(i32 %v.10)
  %v.12 = load i64, ptr %local.0, align 8
  %v.13 = add i64 0, 16
  %tmp.7 = icmp uge i64 %v.13, 64
  br i1 %tmp.7, label %zil.trap, label %checked.3
checked.3:
  %v.14 = ashr i64 %v.12, %v.13
  %v.15 = add i64 0, 255
  %v.16 = and i64 %v.14, %v.15
  %tmp.8 = icmp slt i64 %v.16, -2147483648
  %tmp.9 = icmp sgt i64 %v.16, 2147483647
  %tmp.10 = or i1 %tmp.8, %tmp.9
  br i1 %tmp.10, label %zil.trap, label %checked.4
checked.4:
  %v.17 = trunc i64 %v.16 to i32
  %v.18 = call i32 @putchar(i32 %v.17)
  %v.19 = load i64, ptr %local.0, align 8
  %v.20 = add i64 0, 24
  %tmp.11 = icmp uge i64 %v.20, 64
  br i1 %tmp.11, label %zil.trap, label %checked.5
checked.5:
  %v.21 = ashr i64 %v.19, %v.20
  %v.22 = add i64 0, 255
  %v.23 = and i64 %v.21, %v.22
  %tmp.12 = icmp slt i64 %v.23, -2147483648
  %tmp.13 = icmp sgt i64 %v.23, 2147483647
  %tmp.14 = or i1 %tmp.12, %tmp.13
  br i1 %tmp.14, label %zil.trap, label %checked.6
checked.6:
  %v.24 = trunc i64 %v.23 to i32
  %v.25 = call i32 @putchar(i32 %v.24)
  %v.26 = load i64, ptr %local.0, align 8
  %v.27 = add i64 0, 32
  %tmp.15 = icmp uge i64 %v.27, 64
  br i1 %tmp.15, label %zil.trap, label %checked.7
checked.7:
  %v.28 = ashr i64 %v.26, %v.27
  %v.29 = add i64 0, 255
  %v.30 = and i64 %v.28, %v.29
  %tmp.16 = icmp slt i64 %v.30, -2147483648
  %tmp.17 = icmp sgt i64 %v.30, 2147483647
  %tmp.18 = or i1 %tmp.16, %tmp.17
  br i1 %tmp.18, label %zil.trap, label %checked.8
checked.8:
  %v.31 = trunc i64 %v.30 to i32
  %v.32 = call i32 @putchar(i32 %v.31)
  %v.33 = load i64, ptr %local.0, align 8
  %v.34 = add i64 0, 40
  %tmp.19 = icmp uge i64 %v.34, 64
  br i1 %tmp.19, label %zil.trap, label %checked.9
checked.9:
  %v.35 = ashr i64 %v.33, %v.34
  %v.36 = add i64 0, 255
  %v.37 = and i64 %v.35, %v.36
  %tmp.20 = icmp slt i64 %v.37, -2147483648
  %tmp.21 = icmp sgt i64 %v.37, 2147483647
  %tmp.22 = or i1 %tmp.20, %tmp.21
  br i1 %tmp.22, label %zil.trap, label %checked.10
checked.10:
  %v.38 = trunc i64 %v.37 to i32
  %v.39 = call i32 @putchar(i32 %v.38)
  %v.40 = load i64, ptr %local.0, align 8
  %v.41 = add i64 0, 48
  %tmp.23 = icmp uge i64 %v.41, 64
  br i1 %tmp.23, label %zil.trap, label %checked.11
checked.11:
  %v.42 = ashr i64 %v.40, %v.41
  %v.43 = add i64 0, 255
  %v.44 = and i64 %v.42, %v.43
  %tmp.24 = icmp slt i64 %v.44, -2147483648
  %tmp.25 = icmp sgt i64 %v.44, 2147483647
  %tmp.26 = or i1 %tmp.24, %tmp.25
  br i1 %tmp.26, label %zil.trap, label %checked.12
checked.12:
  %v.45 = trunc i64 %v.44 to i32
  %v.46 = call i32 @putchar(i32 %v.45)
  %v.47 = load i64, ptr %local.0, align 8
  %v.48 = add i64 0, 56
  %tmp.27 = icmp uge i64 %v.48, 64
  br i1 %tmp.27, label %zil.trap, label %checked.13
checked.13:
  %v.49 = ashr i64 %v.47, %v.48
  %v.50 = add i64 0, 255
  %v.51 = and i64 %v.49, %v.50
  %tmp.28 = icmp slt i64 %v.51, -2147483648
  %tmp.29 = icmp sgt i64 %v.51, 2147483647
  %tmp.30 = or i1 %tmp.28, %tmp.29
  br i1 %tmp.30, label %zil.trap, label %checked.14
checked.14:
  %v.52 = trunc i64 %v.51 to i32
  %v.53 = call i32 @putchar(i32 %v.52)
  ret void
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @zil.fn.6() {
entry:
  %local.0 = alloca i32, align 4
  br label %bb.0
bb.0:
  %v.0 = add i32 0, 0
  store i32 %v.0, ptr %local.0, align 4
  %v.1 = call i32 @getchar()
  store i32 %v.1, ptr %local.0, align 4
  %v.2 = load i32, ptr %local.0, align 4
  %v.3 = call i32 @zil.fn.2(i32 %v.2)
  call void @zil.fn.4(i32 %v.3)
  %v.5 = load i32, ptr %local.0, align 4
  %v.6 = call i64 @zil.fn.3(i32 %v.5)
  call void @zil.fn.5(i64 %v.6)
  %v.8 = add i32 0, 0
  ret i32 %v.8
zil.trap:
  call void @llvm.trap()
  unreachable
}
define i32 @main() {
entry:
  %entry.result = call i32 @zil.fn.6()
  ret i32 %entry.result
}

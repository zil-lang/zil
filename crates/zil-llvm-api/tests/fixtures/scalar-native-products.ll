; Exact generated scalar bodies from the fixed 425-function workload.
; Diagnostic reproduction only; workload performance is measured on all 425 functions.
; Generator: tools/research/reactive-query-graph/native_workloads.py at 2f9ada619
; Source: restored cold, modules=20/functions=20/steps=16; bodies zil.fn.6 and zil.fn.7.
target triple = "x86_64-unknown-linux-gnu"

declare void @llvm.trap()
declare { i32, i1 } @llvm.sadd.with.overflow.i32(i32, i32)
declare { i32, i1 } @llvm.smul.with.overflow.i32(i32, i32)

define i32 @zil.fn.6(i32 %arg.0) {
entry:
  %local.1 = alloca i32, align 4
  br label %bb.0
bb.0:
  store i32 %arg.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = add i32 0, 3
  %tmp.0 = icmp eq i32 %v.2, 0
  %tmp.1 = icmp eq i32 %v.1, -2147483648
  %tmp.2 = icmp eq i32 %v.2, -1
  %tmp.3 = and i1 %tmp.1, %tmp.2
  %tmp.4 = or i1 %tmp.0, %tmp.3
  br i1 %tmp.4, label %zil.trap, label %checked.0
checked.0:
  %v.3 = srem i32 %v.1, %v.2
  %v.4 = add i32 0, 0
  %v.5 = icmp eq i32 %v.3, %v.4
  br i1 %v.5, label %bb.1, label %bb.2
bb.1:
  %v.6 = load i32, ptr %local.1, align 4
  %v.7 = add i32 0, 2
  %tmp.5 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.6, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.1
checked.1:
  %v.9 = add i32 0, 2
  %tmp.7 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.8, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  %v.11 = add i32 0, 251
  %tmp.9 = icmp eq i32 %v.11, 0
  %tmp.10 = icmp eq i32 %v.10, -2147483648
  %tmp.11 = icmp eq i32 %v.11, -1
  %tmp.12 = and i1 %tmp.10, %tmp.11
  %tmp.13 = or i1 %tmp.9, %tmp.12
  br i1 %tmp.13, label %zil.trap, label %checked.3
checked.3:
  %v.12 = srem i32 %v.10, %v.11
  store i32 %v.12, ptr %local.1, align 4
  br label %bb.3
bb.2:
  %v.13 = load i32, ptr %local.1, align 4
  %v.14 = add i32 0, 5
  %tmp.14 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i32, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.4
checked.4:
  %v.16 = add i32 0, 251
  %tmp.16 = icmp eq i32 %v.16, 0
  %tmp.17 = icmp eq i32 %v.15, -2147483648
  %tmp.18 = icmp eq i32 %v.16, -1
  %tmp.19 = and i1 %tmp.17, %tmp.18
  %tmp.20 = or i1 %tmp.16, %tmp.19
  br i1 %tmp.20, label %zil.trap, label %checked.5
checked.5:
  %v.17 = srem i32 %v.15, %v.16
  store i32 %v.17, ptr %local.1, align 4
  br label %bb.3
bb.3:
  %v.18 = load i32, ptr %local.1, align 4
  %v.19 = add i32 0, 3
  %tmp.21 = icmp eq i32 %v.19, 0
  %tmp.22 = icmp eq i32 %v.18, -2147483648
  %tmp.23 = icmp eq i32 %v.19, -1
  %tmp.24 = and i1 %tmp.22, %tmp.23
  %tmp.25 = or i1 %tmp.21, %tmp.24
  br i1 %tmp.25, label %zil.trap, label %checked.6
checked.6:
  %v.20 = srem i32 %v.18, %v.19
  %v.21 = add i32 0, 0
  %v.22 = icmp eq i32 %v.20, %v.21
  br i1 %v.22, label %bb.4, label %bb.5
bb.4:
  %v.23 = load i32, ptr %local.1, align 4
  %v.24 = add i32 0, 2
  %tmp.26 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.23, i32 %v.24)
  %v.25 = extractvalue { i32, i1 } %tmp.26, 0
  %tmp.27 = extractvalue { i32, i1 } %tmp.26, 1
  br i1 %tmp.27, label %zil.trap, label %checked.7
checked.7:
  %v.26 = add i32 0, 3
  %tmp.28 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.25, i32 %v.26)
  %v.27 = extractvalue { i32, i1 } %tmp.28, 0
  %tmp.29 = extractvalue { i32, i1 } %tmp.28, 1
  br i1 %tmp.29, label %zil.trap, label %checked.8
checked.8:
  %v.28 = add i32 0, 251
  %tmp.30 = icmp eq i32 %v.28, 0
  %tmp.31 = icmp eq i32 %v.27, -2147483648
  %tmp.32 = icmp eq i32 %v.28, -1
  %tmp.33 = and i1 %tmp.31, %tmp.32
  %tmp.34 = or i1 %tmp.30, %tmp.33
  br i1 %tmp.34, label %zil.trap, label %checked.9
checked.9:
  %v.29 = srem i32 %v.27, %v.28
  store i32 %v.29, ptr %local.1, align 4
  br label %bb.6
bb.5:
  %v.30 = load i32, ptr %local.1, align 4
  %v.31 = add i32 0, 6
  %tmp.35 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.30, i32 %v.31)
  %v.32 = extractvalue { i32, i1 } %tmp.35, 0
  %tmp.36 = extractvalue { i32, i1 } %tmp.35, 1
  br i1 %tmp.36, label %zil.trap, label %checked.10
checked.10:
  %v.33 = add i32 0, 251
  %tmp.37 = icmp eq i32 %v.33, 0
  %tmp.38 = icmp eq i32 %v.32, -2147483648
  %tmp.39 = icmp eq i32 %v.33, -1
  %tmp.40 = and i1 %tmp.38, %tmp.39
  %tmp.41 = or i1 %tmp.37, %tmp.40
  br i1 %tmp.41, label %zil.trap, label %checked.11
checked.11:
  %v.34 = srem i32 %v.32, %v.33
  store i32 %v.34, ptr %local.1, align 4
  br label %bb.6
bb.6:
  %v.35 = load i32, ptr %local.1, align 4
  %v.36 = add i32 0, 3
  %tmp.42 = icmp eq i32 %v.36, 0
  %tmp.43 = icmp eq i32 %v.35, -2147483648
  %tmp.44 = icmp eq i32 %v.36, -1
  %tmp.45 = and i1 %tmp.43, %tmp.44
  %tmp.46 = or i1 %tmp.42, %tmp.45
  br i1 %tmp.46, label %zil.trap, label %checked.12
checked.12:
  %v.37 = srem i32 %v.35, %v.36
  %v.38 = add i32 0, 0
  %v.39 = icmp eq i32 %v.37, %v.38
  br i1 %v.39, label %bb.7, label %bb.8
bb.7:
  %v.40 = load i32, ptr %local.1, align 4
  %v.41 = add i32 0, 2
  %tmp.47 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.40, i32 %v.41)
  %v.42 = extractvalue { i32, i1 } %tmp.47, 0
  %tmp.48 = extractvalue { i32, i1 } %tmp.47, 1
  br i1 %tmp.48, label %zil.trap, label %checked.13
checked.13:
  %v.43 = add i32 0, 4
  %tmp.49 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.42, i32 %v.43)
  %v.44 = extractvalue { i32, i1 } %tmp.49, 0
  %tmp.50 = extractvalue { i32, i1 } %tmp.49, 1
  br i1 %tmp.50, label %zil.trap, label %checked.14
checked.14:
  %v.45 = add i32 0, 251
  %tmp.51 = icmp eq i32 %v.45, 0
  %tmp.52 = icmp eq i32 %v.44, -2147483648
  %tmp.53 = icmp eq i32 %v.45, -1
  %tmp.54 = and i1 %tmp.52, %tmp.53
  %tmp.55 = or i1 %tmp.51, %tmp.54
  br i1 %tmp.55, label %zil.trap, label %checked.15
checked.15:
  %v.46 = srem i32 %v.44, %v.45
  store i32 %v.46, ptr %local.1, align 4
  br label %bb.9
bb.8:
  %v.47 = load i32, ptr %local.1, align 4
  %v.48 = add i32 0, 7
  %tmp.56 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.47, i32 %v.48)
  %v.49 = extractvalue { i32, i1 } %tmp.56, 0
  %tmp.57 = extractvalue { i32, i1 } %tmp.56, 1
  br i1 %tmp.57, label %zil.trap, label %checked.16
checked.16:
  %v.50 = add i32 0, 251
  %tmp.58 = icmp eq i32 %v.50, 0
  %tmp.59 = icmp eq i32 %v.49, -2147483648
  %tmp.60 = icmp eq i32 %v.50, -1
  %tmp.61 = and i1 %tmp.59, %tmp.60
  %tmp.62 = or i1 %tmp.58, %tmp.61
  br i1 %tmp.62, label %zil.trap, label %checked.17
checked.17:
  %v.51 = srem i32 %v.49, %v.50
  store i32 %v.51, ptr %local.1, align 4
  br label %bb.9
bb.9:
  %v.52 = load i32, ptr %local.1, align 4
  %v.53 = add i32 0, 3
  %tmp.63 = icmp eq i32 %v.53, 0
  %tmp.64 = icmp eq i32 %v.52, -2147483648
  %tmp.65 = icmp eq i32 %v.53, -1
  %tmp.66 = and i1 %tmp.64, %tmp.65
  %tmp.67 = or i1 %tmp.63, %tmp.66
  br i1 %tmp.67, label %zil.trap, label %checked.18
checked.18:
  %v.54 = srem i32 %v.52, %v.53
  %v.55 = add i32 0, 0
  %v.56 = icmp eq i32 %v.54, %v.55
  br i1 %v.56, label %bb.10, label %bb.11
bb.10:
  %v.57 = load i32, ptr %local.1, align 4
  %v.58 = add i32 0, 2
  %tmp.68 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.57, i32 %v.58)
  %v.59 = extractvalue { i32, i1 } %tmp.68, 0
  %tmp.69 = extractvalue { i32, i1 } %tmp.68, 1
  br i1 %tmp.69, label %zil.trap, label %checked.19
checked.19:
  %v.60 = add i32 0, 5
  %tmp.70 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.59, i32 %v.60)
  %v.61 = extractvalue { i32, i1 } %tmp.70, 0
  %tmp.71 = extractvalue { i32, i1 } %tmp.70, 1
  br i1 %tmp.71, label %zil.trap, label %checked.20
checked.20:
  %v.62 = add i32 0, 251
  %tmp.72 = icmp eq i32 %v.62, 0
  %tmp.73 = icmp eq i32 %v.61, -2147483648
  %tmp.74 = icmp eq i32 %v.62, -1
  %tmp.75 = and i1 %tmp.73, %tmp.74
  %tmp.76 = or i1 %tmp.72, %tmp.75
  br i1 %tmp.76, label %zil.trap, label %checked.21
checked.21:
  %v.63 = srem i32 %v.61, %v.62
  store i32 %v.63, ptr %local.1, align 4
  br label %bb.12
bb.11:
  %v.64 = load i32, ptr %local.1, align 4
  %v.65 = add i32 0, 8
  %tmp.77 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.64, i32 %v.65)
  %v.66 = extractvalue { i32, i1 } %tmp.77, 0
  %tmp.78 = extractvalue { i32, i1 } %tmp.77, 1
  br i1 %tmp.78, label %zil.trap, label %checked.22
checked.22:
  %v.67 = add i32 0, 251
  %tmp.79 = icmp eq i32 %v.67, 0
  %tmp.80 = icmp eq i32 %v.66, -2147483648
  %tmp.81 = icmp eq i32 %v.67, -1
  %tmp.82 = and i1 %tmp.80, %tmp.81
  %tmp.83 = or i1 %tmp.79, %tmp.82
  br i1 %tmp.83, label %zil.trap, label %checked.23
checked.23:
  %v.68 = srem i32 %v.66, %v.67
  store i32 %v.68, ptr %local.1, align 4
  br label %bb.12
bb.12:
  %v.69 = load i32, ptr %local.1, align 4
  %v.70 = add i32 0, 3
  %tmp.84 = icmp eq i32 %v.70, 0
  %tmp.85 = icmp eq i32 %v.69, -2147483648
  %tmp.86 = icmp eq i32 %v.70, -1
  %tmp.87 = and i1 %tmp.85, %tmp.86
  %tmp.88 = or i1 %tmp.84, %tmp.87
  br i1 %tmp.88, label %zil.trap, label %checked.24
checked.24:
  %v.71 = srem i32 %v.69, %v.70
  %v.72 = add i32 0, 0
  %v.73 = icmp eq i32 %v.71, %v.72
  br i1 %v.73, label %bb.13, label %bb.14
bb.13:
  %v.74 = load i32, ptr %local.1, align 4
  %v.75 = add i32 0, 2
  %tmp.89 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.74, i32 %v.75)
  %v.76 = extractvalue { i32, i1 } %tmp.89, 0
  %tmp.90 = extractvalue { i32, i1 } %tmp.89, 1
  br i1 %tmp.90, label %zil.trap, label %checked.25
checked.25:
  %v.77 = add i32 0, 6
  %tmp.91 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.76, i32 %v.77)
  %v.78 = extractvalue { i32, i1 } %tmp.91, 0
  %tmp.92 = extractvalue { i32, i1 } %tmp.91, 1
  br i1 %tmp.92, label %zil.trap, label %checked.26
checked.26:
  %v.79 = add i32 0, 251
  %tmp.93 = icmp eq i32 %v.79, 0
  %tmp.94 = icmp eq i32 %v.78, -2147483648
  %tmp.95 = icmp eq i32 %v.79, -1
  %tmp.96 = and i1 %tmp.94, %tmp.95
  %tmp.97 = or i1 %tmp.93, %tmp.96
  br i1 %tmp.97, label %zil.trap, label %checked.27
checked.27:
  %v.80 = srem i32 %v.78, %v.79
  store i32 %v.80, ptr %local.1, align 4
  br label %bb.15
bb.14:
  %v.81 = load i32, ptr %local.1, align 4
  %v.82 = add i32 0, 9
  %tmp.98 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.81, i32 %v.82)
  %v.83 = extractvalue { i32, i1 } %tmp.98, 0
  %tmp.99 = extractvalue { i32, i1 } %tmp.98, 1
  br i1 %tmp.99, label %zil.trap, label %checked.28
checked.28:
  %v.84 = add i32 0, 251
  %tmp.100 = icmp eq i32 %v.84, 0
  %tmp.101 = icmp eq i32 %v.83, -2147483648
  %tmp.102 = icmp eq i32 %v.84, -1
  %tmp.103 = and i1 %tmp.101, %tmp.102
  %tmp.104 = or i1 %tmp.100, %tmp.103
  br i1 %tmp.104, label %zil.trap, label %checked.29
checked.29:
  %v.85 = srem i32 %v.83, %v.84
  store i32 %v.85, ptr %local.1, align 4
  br label %bb.15
bb.15:
  %v.86 = load i32, ptr %local.1, align 4
  %v.87 = add i32 0, 3
  %tmp.105 = icmp eq i32 %v.87, 0
  %tmp.106 = icmp eq i32 %v.86, -2147483648
  %tmp.107 = icmp eq i32 %v.87, -1
  %tmp.108 = and i1 %tmp.106, %tmp.107
  %tmp.109 = or i1 %tmp.105, %tmp.108
  br i1 %tmp.109, label %zil.trap, label %checked.30
checked.30:
  %v.88 = srem i32 %v.86, %v.87
  %v.89 = add i32 0, 0
  %v.90 = icmp eq i32 %v.88, %v.89
  br i1 %v.90, label %bb.16, label %bb.17
bb.16:
  %v.91 = load i32, ptr %local.1, align 4
  %v.92 = add i32 0, 2
  %tmp.110 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.91, i32 %v.92)
  %v.93 = extractvalue { i32, i1 } %tmp.110, 0
  %tmp.111 = extractvalue { i32, i1 } %tmp.110, 1
  br i1 %tmp.111, label %zil.trap, label %checked.31
checked.31:
  %v.94 = add i32 0, 7
  %tmp.112 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.93, i32 %v.94)
  %v.95 = extractvalue { i32, i1 } %tmp.112, 0
  %tmp.113 = extractvalue { i32, i1 } %tmp.112, 1
  br i1 %tmp.113, label %zil.trap, label %checked.32
checked.32:
  %v.96 = add i32 0, 251
  %tmp.114 = icmp eq i32 %v.96, 0
  %tmp.115 = icmp eq i32 %v.95, -2147483648
  %tmp.116 = icmp eq i32 %v.96, -1
  %tmp.117 = and i1 %tmp.115, %tmp.116
  %tmp.118 = or i1 %tmp.114, %tmp.117
  br i1 %tmp.118, label %zil.trap, label %checked.33
checked.33:
  %v.97 = srem i32 %v.95, %v.96
  store i32 %v.97, ptr %local.1, align 4
  br label %bb.18
bb.17:
  %v.98 = load i32, ptr %local.1, align 4
  %v.99 = add i32 0, 10
  %tmp.119 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.98, i32 %v.99)
  %v.100 = extractvalue { i32, i1 } %tmp.119, 0
  %tmp.120 = extractvalue { i32, i1 } %tmp.119, 1
  br i1 %tmp.120, label %zil.trap, label %checked.34
checked.34:
  %v.101 = add i32 0, 251
  %tmp.121 = icmp eq i32 %v.101, 0
  %tmp.122 = icmp eq i32 %v.100, -2147483648
  %tmp.123 = icmp eq i32 %v.101, -1
  %tmp.124 = and i1 %tmp.122, %tmp.123
  %tmp.125 = or i1 %tmp.121, %tmp.124
  br i1 %tmp.125, label %zil.trap, label %checked.35
checked.35:
  %v.102 = srem i32 %v.100, %v.101
  store i32 %v.102, ptr %local.1, align 4
  br label %bb.18
bb.18:
  %v.103 = load i32, ptr %local.1, align 4
  %v.104 = add i32 0, 3
  %tmp.126 = icmp eq i32 %v.104, 0
  %tmp.127 = icmp eq i32 %v.103, -2147483648
  %tmp.128 = icmp eq i32 %v.104, -1
  %tmp.129 = and i1 %tmp.127, %tmp.128
  %tmp.130 = or i1 %tmp.126, %tmp.129
  br i1 %tmp.130, label %zil.trap, label %checked.36
checked.36:
  %v.105 = srem i32 %v.103, %v.104
  %v.106 = add i32 0, 0
  %v.107 = icmp eq i32 %v.105, %v.106
  br i1 %v.107, label %bb.19, label %bb.20
bb.19:
  %v.108 = load i32, ptr %local.1, align 4
  %v.109 = add i32 0, 2
  %tmp.131 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.108, i32 %v.109)
  %v.110 = extractvalue { i32, i1 } %tmp.131, 0
  %tmp.132 = extractvalue { i32, i1 } %tmp.131, 1
  br i1 %tmp.132, label %zil.trap, label %checked.37
checked.37:
  %v.111 = add i32 0, 8
  %tmp.133 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.110, i32 %v.111)
  %v.112 = extractvalue { i32, i1 } %tmp.133, 0
  %tmp.134 = extractvalue { i32, i1 } %tmp.133, 1
  br i1 %tmp.134, label %zil.trap, label %checked.38
checked.38:
  %v.113 = add i32 0, 251
  %tmp.135 = icmp eq i32 %v.113, 0
  %tmp.136 = icmp eq i32 %v.112, -2147483648
  %tmp.137 = icmp eq i32 %v.113, -1
  %tmp.138 = and i1 %tmp.136, %tmp.137
  %tmp.139 = or i1 %tmp.135, %tmp.138
  br i1 %tmp.139, label %zil.trap, label %checked.39
checked.39:
  %v.114 = srem i32 %v.112, %v.113
  store i32 %v.114, ptr %local.1, align 4
  br label %bb.21
bb.20:
  %v.115 = load i32, ptr %local.1, align 4
  %v.116 = add i32 0, 11
  %tmp.140 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.115, i32 %v.116)
  %v.117 = extractvalue { i32, i1 } %tmp.140, 0
  %tmp.141 = extractvalue { i32, i1 } %tmp.140, 1
  br i1 %tmp.141, label %zil.trap, label %checked.40
checked.40:
  %v.118 = add i32 0, 251
  %tmp.142 = icmp eq i32 %v.118, 0
  %tmp.143 = icmp eq i32 %v.117, -2147483648
  %tmp.144 = icmp eq i32 %v.118, -1
  %tmp.145 = and i1 %tmp.143, %tmp.144
  %tmp.146 = or i1 %tmp.142, %tmp.145
  br i1 %tmp.146, label %zil.trap, label %checked.41
checked.41:
  %v.119 = srem i32 %v.117, %v.118
  store i32 %v.119, ptr %local.1, align 4
  br label %bb.21
bb.21:
  %v.120 = load i32, ptr %local.1, align 4
  %v.121 = add i32 0, 3
  %tmp.147 = icmp eq i32 %v.121, 0
  %tmp.148 = icmp eq i32 %v.120, -2147483648
  %tmp.149 = icmp eq i32 %v.121, -1
  %tmp.150 = and i1 %tmp.148, %tmp.149
  %tmp.151 = or i1 %tmp.147, %tmp.150
  br i1 %tmp.151, label %zil.trap, label %checked.42
checked.42:
  %v.122 = srem i32 %v.120, %v.121
  %v.123 = add i32 0, 0
  %v.124 = icmp eq i32 %v.122, %v.123
  br i1 %v.124, label %bb.22, label %bb.23
bb.22:
  %v.125 = load i32, ptr %local.1, align 4
  %v.126 = add i32 0, 2
  %tmp.152 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.125, i32 %v.126)
  %v.127 = extractvalue { i32, i1 } %tmp.152, 0
  %tmp.153 = extractvalue { i32, i1 } %tmp.152, 1
  br i1 %tmp.153, label %zil.trap, label %checked.43
checked.43:
  %v.128 = add i32 0, 9
  %tmp.154 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.127, i32 %v.128)
  %v.129 = extractvalue { i32, i1 } %tmp.154, 0
  %tmp.155 = extractvalue { i32, i1 } %tmp.154, 1
  br i1 %tmp.155, label %zil.trap, label %checked.44
checked.44:
  %v.130 = add i32 0, 251
  %tmp.156 = icmp eq i32 %v.130, 0
  %tmp.157 = icmp eq i32 %v.129, -2147483648
  %tmp.158 = icmp eq i32 %v.130, -1
  %tmp.159 = and i1 %tmp.157, %tmp.158
  %tmp.160 = or i1 %tmp.156, %tmp.159
  br i1 %tmp.160, label %zil.trap, label %checked.45
checked.45:
  %v.131 = srem i32 %v.129, %v.130
  store i32 %v.131, ptr %local.1, align 4
  br label %bb.24
bb.23:
  %v.132 = load i32, ptr %local.1, align 4
  %v.133 = add i32 0, 12
  %tmp.161 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.132, i32 %v.133)
  %v.134 = extractvalue { i32, i1 } %tmp.161, 0
  %tmp.162 = extractvalue { i32, i1 } %tmp.161, 1
  br i1 %tmp.162, label %zil.trap, label %checked.46
checked.46:
  %v.135 = add i32 0, 251
  %tmp.163 = icmp eq i32 %v.135, 0
  %tmp.164 = icmp eq i32 %v.134, -2147483648
  %tmp.165 = icmp eq i32 %v.135, -1
  %tmp.166 = and i1 %tmp.164, %tmp.165
  %tmp.167 = or i1 %tmp.163, %tmp.166
  br i1 %tmp.167, label %zil.trap, label %checked.47
checked.47:
  %v.136 = srem i32 %v.134, %v.135
  store i32 %v.136, ptr %local.1, align 4
  br label %bb.24
bb.24:
  %v.137 = load i32, ptr %local.1, align 4
  %v.138 = add i32 0, 3
  %tmp.168 = icmp eq i32 %v.138, 0
  %tmp.169 = icmp eq i32 %v.137, -2147483648
  %tmp.170 = icmp eq i32 %v.138, -1
  %tmp.171 = and i1 %tmp.169, %tmp.170
  %tmp.172 = or i1 %tmp.168, %tmp.171
  br i1 %tmp.172, label %zil.trap, label %checked.48
checked.48:
  %v.139 = srem i32 %v.137, %v.138
  %v.140 = add i32 0, 0
  %v.141 = icmp eq i32 %v.139, %v.140
  br i1 %v.141, label %bb.25, label %bb.26
bb.25:
  %v.142 = load i32, ptr %local.1, align 4
  %v.143 = add i32 0, 2
  %tmp.173 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.142, i32 %v.143)
  %v.144 = extractvalue { i32, i1 } %tmp.173, 0
  %tmp.174 = extractvalue { i32, i1 } %tmp.173, 1
  br i1 %tmp.174, label %zil.trap, label %checked.49
checked.49:
  %v.145 = add i32 0, 10
  %tmp.175 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.144, i32 %v.145)
  %v.146 = extractvalue { i32, i1 } %tmp.175, 0
  %tmp.176 = extractvalue { i32, i1 } %tmp.175, 1
  br i1 %tmp.176, label %zil.trap, label %checked.50
checked.50:
  %v.147 = add i32 0, 251
  %tmp.177 = icmp eq i32 %v.147, 0
  %tmp.178 = icmp eq i32 %v.146, -2147483648
  %tmp.179 = icmp eq i32 %v.147, -1
  %tmp.180 = and i1 %tmp.178, %tmp.179
  %tmp.181 = or i1 %tmp.177, %tmp.180
  br i1 %tmp.181, label %zil.trap, label %checked.51
checked.51:
  %v.148 = srem i32 %v.146, %v.147
  store i32 %v.148, ptr %local.1, align 4
  br label %bb.27
bb.26:
  %v.149 = load i32, ptr %local.1, align 4
  %v.150 = add i32 0, 2
  %tmp.182 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.149, i32 %v.150)
  %v.151 = extractvalue { i32, i1 } %tmp.182, 0
  %tmp.183 = extractvalue { i32, i1 } %tmp.182, 1
  br i1 %tmp.183, label %zil.trap, label %checked.52
checked.52:
  %v.152 = add i32 0, 251
  %tmp.184 = icmp eq i32 %v.152, 0
  %tmp.185 = icmp eq i32 %v.151, -2147483648
  %tmp.186 = icmp eq i32 %v.152, -1
  %tmp.187 = and i1 %tmp.185, %tmp.186
  %tmp.188 = or i1 %tmp.184, %tmp.187
  br i1 %tmp.188, label %zil.trap, label %checked.53
checked.53:
  %v.153 = srem i32 %v.151, %v.152
  store i32 %v.153, ptr %local.1, align 4
  br label %bb.27
bb.27:
  %v.154 = load i32, ptr %local.1, align 4
  %v.155 = add i32 0, 3
  %tmp.189 = icmp eq i32 %v.155, 0
  %tmp.190 = icmp eq i32 %v.154, -2147483648
  %tmp.191 = icmp eq i32 %v.155, -1
  %tmp.192 = and i1 %tmp.190, %tmp.191
  %tmp.193 = or i1 %tmp.189, %tmp.192
  br i1 %tmp.193, label %zil.trap, label %checked.54
checked.54:
  %v.156 = srem i32 %v.154, %v.155
  %v.157 = add i32 0, 0
  %v.158 = icmp eq i32 %v.156, %v.157
  br i1 %v.158, label %bb.28, label %bb.29
bb.28:
  %v.159 = load i32, ptr %local.1, align 4
  %v.160 = add i32 0, 2
  %tmp.194 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.159, i32 %v.160)
  %v.161 = extractvalue { i32, i1 } %tmp.194, 0
  %tmp.195 = extractvalue { i32, i1 } %tmp.194, 1
  br i1 %tmp.195, label %zil.trap, label %checked.55
checked.55:
  %v.162 = add i32 0, 11
  %tmp.196 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.161, i32 %v.162)
  %v.163 = extractvalue { i32, i1 } %tmp.196, 0
  %tmp.197 = extractvalue { i32, i1 } %tmp.196, 1
  br i1 %tmp.197, label %zil.trap, label %checked.56
checked.56:
  %v.164 = add i32 0, 251
  %tmp.198 = icmp eq i32 %v.164, 0
  %tmp.199 = icmp eq i32 %v.163, -2147483648
  %tmp.200 = icmp eq i32 %v.164, -1
  %tmp.201 = and i1 %tmp.199, %tmp.200
  %tmp.202 = or i1 %tmp.198, %tmp.201
  br i1 %tmp.202, label %zil.trap, label %checked.57
checked.57:
  %v.165 = srem i32 %v.163, %v.164
  store i32 %v.165, ptr %local.1, align 4
  br label %bb.30
bb.29:
  %v.166 = load i32, ptr %local.1, align 4
  %v.167 = add i32 0, 3
  %tmp.203 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.166, i32 %v.167)
  %v.168 = extractvalue { i32, i1 } %tmp.203, 0
  %tmp.204 = extractvalue { i32, i1 } %tmp.203, 1
  br i1 %tmp.204, label %zil.trap, label %checked.58
checked.58:
  %v.169 = add i32 0, 251
  %tmp.205 = icmp eq i32 %v.169, 0
  %tmp.206 = icmp eq i32 %v.168, -2147483648
  %tmp.207 = icmp eq i32 %v.169, -1
  %tmp.208 = and i1 %tmp.206, %tmp.207
  %tmp.209 = or i1 %tmp.205, %tmp.208
  br i1 %tmp.209, label %zil.trap, label %checked.59
checked.59:
  %v.170 = srem i32 %v.168, %v.169
  store i32 %v.170, ptr %local.1, align 4
  br label %bb.30
bb.30:
  %v.171 = load i32, ptr %local.1, align 4
  %v.172 = add i32 0, 3
  %tmp.210 = icmp eq i32 %v.172, 0
  %tmp.211 = icmp eq i32 %v.171, -2147483648
  %tmp.212 = icmp eq i32 %v.172, -1
  %tmp.213 = and i1 %tmp.211, %tmp.212
  %tmp.214 = or i1 %tmp.210, %tmp.213
  br i1 %tmp.214, label %zil.trap, label %checked.60
checked.60:
  %v.173 = srem i32 %v.171, %v.172
  %v.174 = add i32 0, 0
  %v.175 = icmp eq i32 %v.173, %v.174
  br i1 %v.175, label %bb.31, label %bb.32
bb.31:
  %v.176 = load i32, ptr %local.1, align 4
  %v.177 = add i32 0, 2
  %tmp.215 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.176, i32 %v.177)
  %v.178 = extractvalue { i32, i1 } %tmp.215, 0
  %tmp.216 = extractvalue { i32, i1 } %tmp.215, 1
  br i1 %tmp.216, label %zil.trap, label %checked.61
checked.61:
  %v.179 = add i32 0, 12
  %tmp.217 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.178, i32 %v.179)
  %v.180 = extractvalue { i32, i1 } %tmp.217, 0
  %tmp.218 = extractvalue { i32, i1 } %tmp.217, 1
  br i1 %tmp.218, label %zil.trap, label %checked.62
checked.62:
  %v.181 = add i32 0, 251
  %tmp.219 = icmp eq i32 %v.181, 0
  %tmp.220 = icmp eq i32 %v.180, -2147483648
  %tmp.221 = icmp eq i32 %v.181, -1
  %tmp.222 = and i1 %tmp.220, %tmp.221
  %tmp.223 = or i1 %tmp.219, %tmp.222
  br i1 %tmp.223, label %zil.trap, label %checked.63
checked.63:
  %v.182 = srem i32 %v.180, %v.181
  store i32 %v.182, ptr %local.1, align 4
  br label %bb.33
bb.32:
  %v.183 = load i32, ptr %local.1, align 4
  %v.184 = add i32 0, 4
  %tmp.224 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.183, i32 %v.184)
  %v.185 = extractvalue { i32, i1 } %tmp.224, 0
  %tmp.225 = extractvalue { i32, i1 } %tmp.224, 1
  br i1 %tmp.225, label %zil.trap, label %checked.64
checked.64:
  %v.186 = add i32 0, 251
  %tmp.226 = icmp eq i32 %v.186, 0
  %tmp.227 = icmp eq i32 %v.185, -2147483648
  %tmp.228 = icmp eq i32 %v.186, -1
  %tmp.229 = and i1 %tmp.227, %tmp.228
  %tmp.230 = or i1 %tmp.226, %tmp.229
  br i1 %tmp.230, label %zil.trap, label %checked.65
checked.65:
  %v.187 = srem i32 %v.185, %v.186
  store i32 %v.187, ptr %local.1, align 4
  br label %bb.33
bb.33:
  %v.188 = load i32, ptr %local.1, align 4
  %v.189 = add i32 0, 3
  %tmp.231 = icmp eq i32 %v.189, 0
  %tmp.232 = icmp eq i32 %v.188, -2147483648
  %tmp.233 = icmp eq i32 %v.189, -1
  %tmp.234 = and i1 %tmp.232, %tmp.233
  %tmp.235 = or i1 %tmp.231, %tmp.234
  br i1 %tmp.235, label %zil.trap, label %checked.66
checked.66:
  %v.190 = srem i32 %v.188, %v.189
  %v.191 = add i32 0, 0
  %v.192 = icmp eq i32 %v.190, %v.191
  br i1 %v.192, label %bb.34, label %bb.35
bb.34:
  %v.193 = load i32, ptr %local.1, align 4
  %v.194 = add i32 0, 2
  %tmp.236 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.193, i32 %v.194)
  %v.195 = extractvalue { i32, i1 } %tmp.236, 0
  %tmp.237 = extractvalue { i32, i1 } %tmp.236, 1
  br i1 %tmp.237, label %zil.trap, label %checked.67
checked.67:
  %v.196 = add i32 0, 13
  %tmp.238 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.195, i32 %v.196)
  %v.197 = extractvalue { i32, i1 } %tmp.238, 0
  %tmp.239 = extractvalue { i32, i1 } %tmp.238, 1
  br i1 %tmp.239, label %zil.trap, label %checked.68
checked.68:
  %v.198 = add i32 0, 251
  %tmp.240 = icmp eq i32 %v.198, 0
  %tmp.241 = icmp eq i32 %v.197, -2147483648
  %tmp.242 = icmp eq i32 %v.198, -1
  %tmp.243 = and i1 %tmp.241, %tmp.242
  %tmp.244 = or i1 %tmp.240, %tmp.243
  br i1 %tmp.244, label %zil.trap, label %checked.69
checked.69:
  %v.199 = srem i32 %v.197, %v.198
  store i32 %v.199, ptr %local.1, align 4
  br label %bb.36
bb.35:
  %v.200 = load i32, ptr %local.1, align 4
  %v.201 = add i32 0, 5
  %tmp.245 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.200, i32 %v.201)
  %v.202 = extractvalue { i32, i1 } %tmp.245, 0
  %tmp.246 = extractvalue { i32, i1 } %tmp.245, 1
  br i1 %tmp.246, label %zil.trap, label %checked.70
checked.70:
  %v.203 = add i32 0, 251
  %tmp.247 = icmp eq i32 %v.203, 0
  %tmp.248 = icmp eq i32 %v.202, -2147483648
  %tmp.249 = icmp eq i32 %v.203, -1
  %tmp.250 = and i1 %tmp.248, %tmp.249
  %tmp.251 = or i1 %tmp.247, %tmp.250
  br i1 %tmp.251, label %zil.trap, label %checked.71
checked.71:
  %v.204 = srem i32 %v.202, %v.203
  store i32 %v.204, ptr %local.1, align 4
  br label %bb.36
bb.36:
  %v.205 = load i32, ptr %local.1, align 4
  %v.206 = add i32 0, 3
  %tmp.252 = icmp eq i32 %v.206, 0
  %tmp.253 = icmp eq i32 %v.205, -2147483648
  %tmp.254 = icmp eq i32 %v.206, -1
  %tmp.255 = and i1 %tmp.253, %tmp.254
  %tmp.256 = or i1 %tmp.252, %tmp.255
  br i1 %tmp.256, label %zil.trap, label %checked.72
checked.72:
  %v.207 = srem i32 %v.205, %v.206
  %v.208 = add i32 0, 0
  %v.209 = icmp eq i32 %v.207, %v.208
  br i1 %v.209, label %bb.37, label %bb.38
bb.37:
  %v.210 = load i32, ptr %local.1, align 4
  %v.211 = add i32 0, 2
  %tmp.257 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.210, i32 %v.211)
  %v.212 = extractvalue { i32, i1 } %tmp.257, 0
  %tmp.258 = extractvalue { i32, i1 } %tmp.257, 1
  br i1 %tmp.258, label %zil.trap, label %checked.73
checked.73:
  %v.213 = add i32 0, 1
  %tmp.259 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.212, i32 %v.213)
  %v.214 = extractvalue { i32, i1 } %tmp.259, 0
  %tmp.260 = extractvalue { i32, i1 } %tmp.259, 1
  br i1 %tmp.260, label %zil.trap, label %checked.74
checked.74:
  %v.215 = add i32 0, 251
  %tmp.261 = icmp eq i32 %v.215, 0
  %tmp.262 = icmp eq i32 %v.214, -2147483648
  %tmp.263 = icmp eq i32 %v.215, -1
  %tmp.264 = and i1 %tmp.262, %tmp.263
  %tmp.265 = or i1 %tmp.261, %tmp.264
  br i1 %tmp.265, label %zil.trap, label %checked.75
checked.75:
  %v.216 = srem i32 %v.214, %v.215
  store i32 %v.216, ptr %local.1, align 4
  br label %bb.39
bb.38:
  %v.217 = load i32, ptr %local.1, align 4
  %v.218 = add i32 0, 6
  %tmp.266 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.217, i32 %v.218)
  %v.219 = extractvalue { i32, i1 } %tmp.266, 0
  %tmp.267 = extractvalue { i32, i1 } %tmp.266, 1
  br i1 %tmp.267, label %zil.trap, label %checked.76
checked.76:
  %v.220 = add i32 0, 251
  %tmp.268 = icmp eq i32 %v.220, 0
  %tmp.269 = icmp eq i32 %v.219, -2147483648
  %tmp.270 = icmp eq i32 %v.220, -1
  %tmp.271 = and i1 %tmp.269, %tmp.270
  %tmp.272 = or i1 %tmp.268, %tmp.271
  br i1 %tmp.272, label %zil.trap, label %checked.77
checked.77:
  %v.221 = srem i32 %v.219, %v.220
  store i32 %v.221, ptr %local.1, align 4
  br label %bb.39
bb.39:
  %v.222 = load i32, ptr %local.1, align 4
  %v.223 = add i32 0, 3
  %tmp.273 = icmp eq i32 %v.223, 0
  %tmp.274 = icmp eq i32 %v.222, -2147483648
  %tmp.275 = icmp eq i32 %v.223, -1
  %tmp.276 = and i1 %tmp.274, %tmp.275
  %tmp.277 = or i1 %tmp.273, %tmp.276
  br i1 %tmp.277, label %zil.trap, label %checked.78
checked.78:
  %v.224 = srem i32 %v.222, %v.223
  %v.225 = add i32 0, 0
  %v.226 = icmp eq i32 %v.224, %v.225
  br i1 %v.226, label %bb.40, label %bb.41
bb.40:
  %v.227 = load i32, ptr %local.1, align 4
  %v.228 = add i32 0, 2
  %tmp.278 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.227, i32 %v.228)
  %v.229 = extractvalue { i32, i1 } %tmp.278, 0
  %tmp.279 = extractvalue { i32, i1 } %tmp.278, 1
  br i1 %tmp.279, label %zil.trap, label %checked.79
checked.79:
  %v.230 = add i32 0, 2
  %tmp.280 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.229, i32 %v.230)
  %v.231 = extractvalue { i32, i1 } %tmp.280, 0
  %tmp.281 = extractvalue { i32, i1 } %tmp.280, 1
  br i1 %tmp.281, label %zil.trap, label %checked.80
checked.80:
  %v.232 = add i32 0, 251
  %tmp.282 = icmp eq i32 %v.232, 0
  %tmp.283 = icmp eq i32 %v.231, -2147483648
  %tmp.284 = icmp eq i32 %v.232, -1
  %tmp.285 = and i1 %tmp.283, %tmp.284
  %tmp.286 = or i1 %tmp.282, %tmp.285
  br i1 %tmp.286, label %zil.trap, label %checked.81
checked.81:
  %v.233 = srem i32 %v.231, %v.232
  store i32 %v.233, ptr %local.1, align 4
  br label %bb.42
bb.41:
  %v.234 = load i32, ptr %local.1, align 4
  %v.235 = add i32 0, 7
  %tmp.287 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.234, i32 %v.235)
  %v.236 = extractvalue { i32, i1 } %tmp.287, 0
  %tmp.288 = extractvalue { i32, i1 } %tmp.287, 1
  br i1 %tmp.288, label %zil.trap, label %checked.82
checked.82:
  %v.237 = add i32 0, 251
  %tmp.289 = icmp eq i32 %v.237, 0
  %tmp.290 = icmp eq i32 %v.236, -2147483648
  %tmp.291 = icmp eq i32 %v.237, -1
  %tmp.292 = and i1 %tmp.290, %tmp.291
  %tmp.293 = or i1 %tmp.289, %tmp.292
  br i1 %tmp.293, label %zil.trap, label %checked.83
checked.83:
  %v.238 = srem i32 %v.236, %v.237
  store i32 %v.238, ptr %local.1, align 4
  br label %bb.42
bb.42:
  %v.239 = load i32, ptr %local.1, align 4
  %v.240 = add i32 0, 3
  %tmp.294 = icmp eq i32 %v.240, 0
  %tmp.295 = icmp eq i32 %v.239, -2147483648
  %tmp.296 = icmp eq i32 %v.240, -1
  %tmp.297 = and i1 %tmp.295, %tmp.296
  %tmp.298 = or i1 %tmp.294, %tmp.297
  br i1 %tmp.298, label %zil.trap, label %checked.84
checked.84:
  %v.241 = srem i32 %v.239, %v.240
  %v.242 = add i32 0, 0
  %v.243 = icmp eq i32 %v.241, %v.242
  br i1 %v.243, label %bb.43, label %bb.44
bb.43:
  %v.244 = load i32, ptr %local.1, align 4
  %v.245 = add i32 0, 2
  %tmp.299 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.244, i32 %v.245)
  %v.246 = extractvalue { i32, i1 } %tmp.299, 0
  %tmp.300 = extractvalue { i32, i1 } %tmp.299, 1
  br i1 %tmp.300, label %zil.trap, label %checked.85
checked.85:
  %v.247 = add i32 0, 3
  %tmp.301 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.246, i32 %v.247)
  %v.248 = extractvalue { i32, i1 } %tmp.301, 0
  %tmp.302 = extractvalue { i32, i1 } %tmp.301, 1
  br i1 %tmp.302, label %zil.trap, label %checked.86
checked.86:
  %v.249 = add i32 0, 251
  %tmp.303 = icmp eq i32 %v.249, 0
  %tmp.304 = icmp eq i32 %v.248, -2147483648
  %tmp.305 = icmp eq i32 %v.249, -1
  %tmp.306 = and i1 %tmp.304, %tmp.305
  %tmp.307 = or i1 %tmp.303, %tmp.306
  br i1 %tmp.307, label %zil.trap, label %checked.87
checked.87:
  %v.250 = srem i32 %v.248, %v.249
  store i32 %v.250, ptr %local.1, align 4
  br label %bb.45
bb.44:
  %v.251 = load i32, ptr %local.1, align 4
  %v.252 = add i32 0, 8
  %tmp.308 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.251, i32 %v.252)
  %v.253 = extractvalue { i32, i1 } %tmp.308, 0
  %tmp.309 = extractvalue { i32, i1 } %tmp.308, 1
  br i1 %tmp.309, label %zil.trap, label %checked.88
checked.88:
  %v.254 = add i32 0, 251
  %tmp.310 = icmp eq i32 %v.254, 0
  %tmp.311 = icmp eq i32 %v.253, -2147483648
  %tmp.312 = icmp eq i32 %v.254, -1
  %tmp.313 = and i1 %tmp.311, %tmp.312
  %tmp.314 = or i1 %tmp.310, %tmp.313
  br i1 %tmp.314, label %zil.trap, label %checked.89
checked.89:
  %v.255 = srem i32 %v.253, %v.254
  store i32 %v.255, ptr %local.1, align 4
  br label %bb.45
bb.45:
  %v.256 = load i32, ptr %local.1, align 4
  %v.257 = add i32 0, 3
  %tmp.315 = icmp eq i32 %v.257, 0
  %tmp.316 = icmp eq i32 %v.256, -2147483648
  %tmp.317 = icmp eq i32 %v.257, -1
  %tmp.318 = and i1 %tmp.316, %tmp.317
  %tmp.319 = or i1 %tmp.315, %tmp.318
  br i1 %tmp.319, label %zil.trap, label %checked.90
checked.90:
  %v.258 = srem i32 %v.256, %v.257
  %v.259 = add i32 0, 0
  %v.260 = icmp eq i32 %v.258, %v.259
  br i1 %v.260, label %bb.46, label %bb.47
bb.46:
  %v.261 = load i32, ptr %local.1, align 4
  %v.262 = add i32 0, 2
  %tmp.320 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.261, i32 %v.262)
  %v.263 = extractvalue { i32, i1 } %tmp.320, 0
  %tmp.321 = extractvalue { i32, i1 } %tmp.320, 1
  br i1 %tmp.321, label %zil.trap, label %checked.91
checked.91:
  %v.264 = add i32 0, 4
  %tmp.322 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.263, i32 %v.264)
  %v.265 = extractvalue { i32, i1 } %tmp.322, 0
  %tmp.323 = extractvalue { i32, i1 } %tmp.322, 1
  br i1 %tmp.323, label %zil.trap, label %checked.92
checked.92:
  %v.266 = add i32 0, 251
  %tmp.324 = icmp eq i32 %v.266, 0
  %tmp.325 = icmp eq i32 %v.265, -2147483648
  %tmp.326 = icmp eq i32 %v.266, -1
  %tmp.327 = and i1 %tmp.325, %tmp.326
  %tmp.328 = or i1 %tmp.324, %tmp.327
  br i1 %tmp.328, label %zil.trap, label %checked.93
checked.93:
  %v.267 = srem i32 %v.265, %v.266
  store i32 %v.267, ptr %local.1, align 4
  br label %bb.48
bb.47:
  %v.268 = load i32, ptr %local.1, align 4
  %v.269 = add i32 0, 9
  %tmp.329 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.268, i32 %v.269)
  %v.270 = extractvalue { i32, i1 } %tmp.329, 0
  %tmp.330 = extractvalue { i32, i1 } %tmp.329, 1
  br i1 %tmp.330, label %zil.trap, label %checked.94
checked.94:
  %v.271 = add i32 0, 251
  %tmp.331 = icmp eq i32 %v.271, 0
  %tmp.332 = icmp eq i32 %v.270, -2147483648
  %tmp.333 = icmp eq i32 %v.271, -1
  %tmp.334 = and i1 %tmp.332, %tmp.333
  %tmp.335 = or i1 %tmp.331, %tmp.334
  br i1 %tmp.335, label %zil.trap, label %checked.95
checked.95:
  %v.272 = srem i32 %v.270, %v.271
  store i32 %v.272, ptr %local.1, align 4
  br label %bb.48
bb.48:
  %v.273 = load i32, ptr %local.1, align 4
  ret i32 %v.273
zil.trap:
  call void @llvm.trap()
  unreachable
}

define i32 @zil.fn.7(i32 %arg.0) {
entry:
  %local.1 = alloca i32, align 4
  br label %bb.0
bb.0:
  store i32 %arg.0, ptr %local.1, align 4
  %v.1 = load i32, ptr %local.1, align 4
  %v.2 = add i32 0, 3
  %tmp.0 = icmp eq i32 %v.2, 0
  %tmp.1 = icmp eq i32 %v.1, -2147483648
  %tmp.2 = icmp eq i32 %v.2, -1
  %tmp.3 = and i1 %tmp.1, %tmp.2
  %tmp.4 = or i1 %tmp.0, %tmp.3
  br i1 %tmp.4, label %zil.trap, label %checked.0
checked.0:
  %v.3 = srem i32 %v.1, %v.2
  %v.4 = add i32 0, 0
  %v.5 = icmp eq i32 %v.3, %v.4
  br i1 %v.5, label %bb.1, label %bb.2
bb.1:
  %v.6 = load i32, ptr %local.1, align 4
  %v.7 = add i32 0, 2
  %tmp.5 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.6, i32 %v.7)
  %v.8 = extractvalue { i32, i1 } %tmp.5, 0
  %tmp.6 = extractvalue { i32, i1 } %tmp.5, 1
  br i1 %tmp.6, label %zil.trap, label %checked.1
checked.1:
  %v.9 = add i32 0, 3
  %tmp.7 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.8, i32 %v.9)
  %v.10 = extractvalue { i32, i1 } %tmp.7, 0
  %tmp.8 = extractvalue { i32, i1 } %tmp.7, 1
  br i1 %tmp.8, label %zil.trap, label %checked.2
checked.2:
  %v.11 = add i32 0, 251
  %tmp.9 = icmp eq i32 %v.11, 0
  %tmp.10 = icmp eq i32 %v.10, -2147483648
  %tmp.11 = icmp eq i32 %v.11, -1
  %tmp.12 = and i1 %tmp.10, %tmp.11
  %tmp.13 = or i1 %tmp.9, %tmp.12
  br i1 %tmp.13, label %zil.trap, label %checked.3
checked.3:
  %v.12 = srem i32 %v.10, %v.11
  store i32 %v.12, ptr %local.1, align 4
  br label %bb.3
bb.2:
  %v.13 = load i32, ptr %local.1, align 4
  %v.14 = add i32 0, 8
  %tmp.14 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.13, i32 %v.14)
  %v.15 = extractvalue { i32, i1 } %tmp.14, 0
  %tmp.15 = extractvalue { i32, i1 } %tmp.14, 1
  br i1 %tmp.15, label %zil.trap, label %checked.4
checked.4:
  %v.16 = add i32 0, 251
  %tmp.16 = icmp eq i32 %v.16, 0
  %tmp.17 = icmp eq i32 %v.15, -2147483648
  %tmp.18 = icmp eq i32 %v.16, -1
  %tmp.19 = and i1 %tmp.17, %tmp.18
  %tmp.20 = or i1 %tmp.16, %tmp.19
  br i1 %tmp.20, label %zil.trap, label %checked.5
checked.5:
  %v.17 = srem i32 %v.15, %v.16
  store i32 %v.17, ptr %local.1, align 4
  br label %bb.3
bb.3:
  %v.18 = load i32, ptr %local.1, align 4
  %v.19 = add i32 0, 3
  %tmp.21 = icmp eq i32 %v.19, 0
  %tmp.22 = icmp eq i32 %v.18, -2147483648
  %tmp.23 = icmp eq i32 %v.19, -1
  %tmp.24 = and i1 %tmp.22, %tmp.23
  %tmp.25 = or i1 %tmp.21, %tmp.24
  br i1 %tmp.25, label %zil.trap, label %checked.6
checked.6:
  %v.20 = srem i32 %v.18, %v.19
  %v.21 = add i32 0, 0
  %v.22 = icmp eq i32 %v.20, %v.21
  br i1 %v.22, label %bb.4, label %bb.5
bb.4:
  %v.23 = load i32, ptr %local.1, align 4
  %v.24 = add i32 0, 2
  %tmp.26 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.23, i32 %v.24)
  %v.25 = extractvalue { i32, i1 } %tmp.26, 0
  %tmp.27 = extractvalue { i32, i1 } %tmp.26, 1
  br i1 %tmp.27, label %zil.trap, label %checked.7
checked.7:
  %v.26 = add i32 0, 4
  %tmp.28 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.25, i32 %v.26)
  %v.27 = extractvalue { i32, i1 } %tmp.28, 0
  %tmp.29 = extractvalue { i32, i1 } %tmp.28, 1
  br i1 %tmp.29, label %zil.trap, label %checked.8
checked.8:
  %v.28 = add i32 0, 251
  %tmp.30 = icmp eq i32 %v.28, 0
  %tmp.31 = icmp eq i32 %v.27, -2147483648
  %tmp.32 = icmp eq i32 %v.28, -1
  %tmp.33 = and i1 %tmp.31, %tmp.32
  %tmp.34 = or i1 %tmp.30, %tmp.33
  br i1 %tmp.34, label %zil.trap, label %checked.9
checked.9:
  %v.29 = srem i32 %v.27, %v.28
  store i32 %v.29, ptr %local.1, align 4
  br label %bb.6
bb.5:
  %v.30 = load i32, ptr %local.1, align 4
  %v.31 = add i32 0, 9
  %tmp.35 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.30, i32 %v.31)
  %v.32 = extractvalue { i32, i1 } %tmp.35, 0
  %tmp.36 = extractvalue { i32, i1 } %tmp.35, 1
  br i1 %tmp.36, label %zil.trap, label %checked.10
checked.10:
  %v.33 = add i32 0, 251
  %tmp.37 = icmp eq i32 %v.33, 0
  %tmp.38 = icmp eq i32 %v.32, -2147483648
  %tmp.39 = icmp eq i32 %v.33, -1
  %tmp.40 = and i1 %tmp.38, %tmp.39
  %tmp.41 = or i1 %tmp.37, %tmp.40
  br i1 %tmp.41, label %zil.trap, label %checked.11
checked.11:
  %v.34 = srem i32 %v.32, %v.33
  store i32 %v.34, ptr %local.1, align 4
  br label %bb.6
bb.6:
  %v.35 = load i32, ptr %local.1, align 4
  %v.36 = add i32 0, 3
  %tmp.42 = icmp eq i32 %v.36, 0
  %tmp.43 = icmp eq i32 %v.35, -2147483648
  %tmp.44 = icmp eq i32 %v.36, -1
  %tmp.45 = and i1 %tmp.43, %tmp.44
  %tmp.46 = or i1 %tmp.42, %tmp.45
  br i1 %tmp.46, label %zil.trap, label %checked.12
checked.12:
  %v.37 = srem i32 %v.35, %v.36
  %v.38 = add i32 0, 0
  %v.39 = icmp eq i32 %v.37, %v.38
  br i1 %v.39, label %bb.7, label %bb.8
bb.7:
  %v.40 = load i32, ptr %local.1, align 4
  %v.41 = add i32 0, 2
  %tmp.47 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.40, i32 %v.41)
  %v.42 = extractvalue { i32, i1 } %tmp.47, 0
  %tmp.48 = extractvalue { i32, i1 } %tmp.47, 1
  br i1 %tmp.48, label %zil.trap, label %checked.13
checked.13:
  %v.43 = add i32 0, 5
  %tmp.49 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.42, i32 %v.43)
  %v.44 = extractvalue { i32, i1 } %tmp.49, 0
  %tmp.50 = extractvalue { i32, i1 } %tmp.49, 1
  br i1 %tmp.50, label %zil.trap, label %checked.14
checked.14:
  %v.45 = add i32 0, 251
  %tmp.51 = icmp eq i32 %v.45, 0
  %tmp.52 = icmp eq i32 %v.44, -2147483648
  %tmp.53 = icmp eq i32 %v.45, -1
  %tmp.54 = and i1 %tmp.52, %tmp.53
  %tmp.55 = or i1 %tmp.51, %tmp.54
  br i1 %tmp.55, label %zil.trap, label %checked.15
checked.15:
  %v.46 = srem i32 %v.44, %v.45
  store i32 %v.46, ptr %local.1, align 4
  br label %bb.9
bb.8:
  %v.47 = load i32, ptr %local.1, align 4
  %v.48 = add i32 0, 10
  %tmp.56 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.47, i32 %v.48)
  %v.49 = extractvalue { i32, i1 } %tmp.56, 0
  %tmp.57 = extractvalue { i32, i1 } %tmp.56, 1
  br i1 %tmp.57, label %zil.trap, label %checked.16
checked.16:
  %v.50 = add i32 0, 251
  %tmp.58 = icmp eq i32 %v.50, 0
  %tmp.59 = icmp eq i32 %v.49, -2147483648
  %tmp.60 = icmp eq i32 %v.50, -1
  %tmp.61 = and i1 %tmp.59, %tmp.60
  %tmp.62 = or i1 %tmp.58, %tmp.61
  br i1 %tmp.62, label %zil.trap, label %checked.17
checked.17:
  %v.51 = srem i32 %v.49, %v.50
  store i32 %v.51, ptr %local.1, align 4
  br label %bb.9
bb.9:
  %v.52 = load i32, ptr %local.1, align 4
  %v.53 = add i32 0, 3
  %tmp.63 = icmp eq i32 %v.53, 0
  %tmp.64 = icmp eq i32 %v.52, -2147483648
  %tmp.65 = icmp eq i32 %v.53, -1
  %tmp.66 = and i1 %tmp.64, %tmp.65
  %tmp.67 = or i1 %tmp.63, %tmp.66
  br i1 %tmp.67, label %zil.trap, label %checked.18
checked.18:
  %v.54 = srem i32 %v.52, %v.53
  %v.55 = add i32 0, 0
  %v.56 = icmp eq i32 %v.54, %v.55
  br i1 %v.56, label %bb.10, label %bb.11
bb.10:
  %v.57 = load i32, ptr %local.1, align 4
  %v.58 = add i32 0, 2
  %tmp.68 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.57, i32 %v.58)
  %v.59 = extractvalue { i32, i1 } %tmp.68, 0
  %tmp.69 = extractvalue { i32, i1 } %tmp.68, 1
  br i1 %tmp.69, label %zil.trap, label %checked.19
checked.19:
  %v.60 = add i32 0, 6
  %tmp.70 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.59, i32 %v.60)
  %v.61 = extractvalue { i32, i1 } %tmp.70, 0
  %tmp.71 = extractvalue { i32, i1 } %tmp.70, 1
  br i1 %tmp.71, label %zil.trap, label %checked.20
checked.20:
  %v.62 = add i32 0, 251
  %tmp.72 = icmp eq i32 %v.62, 0
  %tmp.73 = icmp eq i32 %v.61, -2147483648
  %tmp.74 = icmp eq i32 %v.62, -1
  %tmp.75 = and i1 %tmp.73, %tmp.74
  %tmp.76 = or i1 %tmp.72, %tmp.75
  br i1 %tmp.76, label %zil.trap, label %checked.21
checked.21:
  %v.63 = srem i32 %v.61, %v.62
  store i32 %v.63, ptr %local.1, align 4
  br label %bb.12
bb.11:
  %v.64 = load i32, ptr %local.1, align 4
  %v.65 = add i32 0, 11
  %tmp.77 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.64, i32 %v.65)
  %v.66 = extractvalue { i32, i1 } %tmp.77, 0
  %tmp.78 = extractvalue { i32, i1 } %tmp.77, 1
  br i1 %tmp.78, label %zil.trap, label %checked.22
checked.22:
  %v.67 = add i32 0, 251
  %tmp.79 = icmp eq i32 %v.67, 0
  %tmp.80 = icmp eq i32 %v.66, -2147483648
  %tmp.81 = icmp eq i32 %v.67, -1
  %tmp.82 = and i1 %tmp.80, %tmp.81
  %tmp.83 = or i1 %tmp.79, %tmp.82
  br i1 %tmp.83, label %zil.trap, label %checked.23
checked.23:
  %v.68 = srem i32 %v.66, %v.67
  store i32 %v.68, ptr %local.1, align 4
  br label %bb.12
bb.12:
  %v.69 = load i32, ptr %local.1, align 4
  %v.70 = add i32 0, 3
  %tmp.84 = icmp eq i32 %v.70, 0
  %tmp.85 = icmp eq i32 %v.69, -2147483648
  %tmp.86 = icmp eq i32 %v.70, -1
  %tmp.87 = and i1 %tmp.85, %tmp.86
  %tmp.88 = or i1 %tmp.84, %tmp.87
  br i1 %tmp.88, label %zil.trap, label %checked.24
checked.24:
  %v.71 = srem i32 %v.69, %v.70
  %v.72 = add i32 0, 0
  %v.73 = icmp eq i32 %v.71, %v.72
  br i1 %v.73, label %bb.13, label %bb.14
bb.13:
  %v.74 = load i32, ptr %local.1, align 4
  %v.75 = add i32 0, 2
  %tmp.89 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.74, i32 %v.75)
  %v.76 = extractvalue { i32, i1 } %tmp.89, 0
  %tmp.90 = extractvalue { i32, i1 } %tmp.89, 1
  br i1 %tmp.90, label %zil.trap, label %checked.25
checked.25:
  %v.77 = add i32 0, 7
  %tmp.91 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.76, i32 %v.77)
  %v.78 = extractvalue { i32, i1 } %tmp.91, 0
  %tmp.92 = extractvalue { i32, i1 } %tmp.91, 1
  br i1 %tmp.92, label %zil.trap, label %checked.26
checked.26:
  %v.79 = add i32 0, 251
  %tmp.93 = icmp eq i32 %v.79, 0
  %tmp.94 = icmp eq i32 %v.78, -2147483648
  %tmp.95 = icmp eq i32 %v.79, -1
  %tmp.96 = and i1 %tmp.94, %tmp.95
  %tmp.97 = or i1 %tmp.93, %tmp.96
  br i1 %tmp.97, label %zil.trap, label %checked.27
checked.27:
  %v.80 = srem i32 %v.78, %v.79
  store i32 %v.80, ptr %local.1, align 4
  br label %bb.15
bb.14:
  %v.81 = load i32, ptr %local.1, align 4
  %v.82 = add i32 0, 12
  %tmp.98 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.81, i32 %v.82)
  %v.83 = extractvalue { i32, i1 } %tmp.98, 0
  %tmp.99 = extractvalue { i32, i1 } %tmp.98, 1
  br i1 %tmp.99, label %zil.trap, label %checked.28
checked.28:
  %v.84 = add i32 0, 251
  %tmp.100 = icmp eq i32 %v.84, 0
  %tmp.101 = icmp eq i32 %v.83, -2147483648
  %tmp.102 = icmp eq i32 %v.84, -1
  %tmp.103 = and i1 %tmp.101, %tmp.102
  %tmp.104 = or i1 %tmp.100, %tmp.103
  br i1 %tmp.104, label %zil.trap, label %checked.29
checked.29:
  %v.85 = srem i32 %v.83, %v.84
  store i32 %v.85, ptr %local.1, align 4
  br label %bb.15
bb.15:
  %v.86 = load i32, ptr %local.1, align 4
  %v.87 = add i32 0, 3
  %tmp.105 = icmp eq i32 %v.87, 0
  %tmp.106 = icmp eq i32 %v.86, -2147483648
  %tmp.107 = icmp eq i32 %v.87, -1
  %tmp.108 = and i1 %tmp.106, %tmp.107
  %tmp.109 = or i1 %tmp.105, %tmp.108
  br i1 %tmp.109, label %zil.trap, label %checked.30
checked.30:
  %v.88 = srem i32 %v.86, %v.87
  %v.89 = add i32 0, 0
  %v.90 = icmp eq i32 %v.88, %v.89
  br i1 %v.90, label %bb.16, label %bb.17
bb.16:
  %v.91 = load i32, ptr %local.1, align 4
  %v.92 = add i32 0, 2
  %tmp.110 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.91, i32 %v.92)
  %v.93 = extractvalue { i32, i1 } %tmp.110, 0
  %tmp.111 = extractvalue { i32, i1 } %tmp.110, 1
  br i1 %tmp.111, label %zil.trap, label %checked.31
checked.31:
  %v.94 = add i32 0, 8
  %tmp.112 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.93, i32 %v.94)
  %v.95 = extractvalue { i32, i1 } %tmp.112, 0
  %tmp.113 = extractvalue { i32, i1 } %tmp.112, 1
  br i1 %tmp.113, label %zil.trap, label %checked.32
checked.32:
  %v.96 = add i32 0, 251
  %tmp.114 = icmp eq i32 %v.96, 0
  %tmp.115 = icmp eq i32 %v.95, -2147483648
  %tmp.116 = icmp eq i32 %v.96, -1
  %tmp.117 = and i1 %tmp.115, %tmp.116
  %tmp.118 = or i1 %tmp.114, %tmp.117
  br i1 %tmp.118, label %zil.trap, label %checked.33
checked.33:
  %v.97 = srem i32 %v.95, %v.96
  store i32 %v.97, ptr %local.1, align 4
  br label %bb.18
bb.17:
  %v.98 = load i32, ptr %local.1, align 4
  %v.99 = add i32 0, 2
  %tmp.119 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.98, i32 %v.99)
  %v.100 = extractvalue { i32, i1 } %tmp.119, 0
  %tmp.120 = extractvalue { i32, i1 } %tmp.119, 1
  br i1 %tmp.120, label %zil.trap, label %checked.34
checked.34:
  %v.101 = add i32 0, 251
  %tmp.121 = icmp eq i32 %v.101, 0
  %tmp.122 = icmp eq i32 %v.100, -2147483648
  %tmp.123 = icmp eq i32 %v.101, -1
  %tmp.124 = and i1 %tmp.122, %tmp.123
  %tmp.125 = or i1 %tmp.121, %tmp.124
  br i1 %tmp.125, label %zil.trap, label %checked.35
checked.35:
  %v.102 = srem i32 %v.100, %v.101
  store i32 %v.102, ptr %local.1, align 4
  br label %bb.18
bb.18:
  %v.103 = load i32, ptr %local.1, align 4
  %v.104 = add i32 0, 3
  %tmp.126 = icmp eq i32 %v.104, 0
  %tmp.127 = icmp eq i32 %v.103, -2147483648
  %tmp.128 = icmp eq i32 %v.104, -1
  %tmp.129 = and i1 %tmp.127, %tmp.128
  %tmp.130 = or i1 %tmp.126, %tmp.129
  br i1 %tmp.130, label %zil.trap, label %checked.36
checked.36:
  %v.105 = srem i32 %v.103, %v.104
  %v.106 = add i32 0, 0
  %v.107 = icmp eq i32 %v.105, %v.106
  br i1 %v.107, label %bb.19, label %bb.20
bb.19:
  %v.108 = load i32, ptr %local.1, align 4
  %v.109 = add i32 0, 2
  %tmp.131 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.108, i32 %v.109)
  %v.110 = extractvalue { i32, i1 } %tmp.131, 0
  %tmp.132 = extractvalue { i32, i1 } %tmp.131, 1
  br i1 %tmp.132, label %zil.trap, label %checked.37
checked.37:
  %v.111 = add i32 0, 9
  %tmp.133 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.110, i32 %v.111)
  %v.112 = extractvalue { i32, i1 } %tmp.133, 0
  %tmp.134 = extractvalue { i32, i1 } %tmp.133, 1
  br i1 %tmp.134, label %zil.trap, label %checked.38
checked.38:
  %v.113 = add i32 0, 251
  %tmp.135 = icmp eq i32 %v.113, 0
  %tmp.136 = icmp eq i32 %v.112, -2147483648
  %tmp.137 = icmp eq i32 %v.113, -1
  %tmp.138 = and i1 %tmp.136, %tmp.137
  %tmp.139 = or i1 %tmp.135, %tmp.138
  br i1 %tmp.139, label %zil.trap, label %checked.39
checked.39:
  %v.114 = srem i32 %v.112, %v.113
  store i32 %v.114, ptr %local.1, align 4
  br label %bb.21
bb.20:
  %v.115 = load i32, ptr %local.1, align 4
  %v.116 = add i32 0, 3
  %tmp.140 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.115, i32 %v.116)
  %v.117 = extractvalue { i32, i1 } %tmp.140, 0
  %tmp.141 = extractvalue { i32, i1 } %tmp.140, 1
  br i1 %tmp.141, label %zil.trap, label %checked.40
checked.40:
  %v.118 = add i32 0, 251
  %tmp.142 = icmp eq i32 %v.118, 0
  %tmp.143 = icmp eq i32 %v.117, -2147483648
  %tmp.144 = icmp eq i32 %v.118, -1
  %tmp.145 = and i1 %tmp.143, %tmp.144
  %tmp.146 = or i1 %tmp.142, %tmp.145
  br i1 %tmp.146, label %zil.trap, label %checked.41
checked.41:
  %v.119 = srem i32 %v.117, %v.118
  store i32 %v.119, ptr %local.1, align 4
  br label %bb.21
bb.21:
  %v.120 = load i32, ptr %local.1, align 4
  %v.121 = add i32 0, 3
  %tmp.147 = icmp eq i32 %v.121, 0
  %tmp.148 = icmp eq i32 %v.120, -2147483648
  %tmp.149 = icmp eq i32 %v.121, -1
  %tmp.150 = and i1 %tmp.148, %tmp.149
  %tmp.151 = or i1 %tmp.147, %tmp.150
  br i1 %tmp.151, label %zil.trap, label %checked.42
checked.42:
  %v.122 = srem i32 %v.120, %v.121
  %v.123 = add i32 0, 0
  %v.124 = icmp eq i32 %v.122, %v.123
  br i1 %v.124, label %bb.22, label %bb.23
bb.22:
  %v.125 = load i32, ptr %local.1, align 4
  %v.126 = add i32 0, 2
  %tmp.152 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.125, i32 %v.126)
  %v.127 = extractvalue { i32, i1 } %tmp.152, 0
  %tmp.153 = extractvalue { i32, i1 } %tmp.152, 1
  br i1 %tmp.153, label %zil.trap, label %checked.43
checked.43:
  %v.128 = add i32 0, 10
  %tmp.154 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.127, i32 %v.128)
  %v.129 = extractvalue { i32, i1 } %tmp.154, 0
  %tmp.155 = extractvalue { i32, i1 } %tmp.154, 1
  br i1 %tmp.155, label %zil.trap, label %checked.44
checked.44:
  %v.130 = add i32 0, 251
  %tmp.156 = icmp eq i32 %v.130, 0
  %tmp.157 = icmp eq i32 %v.129, -2147483648
  %tmp.158 = icmp eq i32 %v.130, -1
  %tmp.159 = and i1 %tmp.157, %tmp.158
  %tmp.160 = or i1 %tmp.156, %tmp.159
  br i1 %tmp.160, label %zil.trap, label %checked.45
checked.45:
  %v.131 = srem i32 %v.129, %v.130
  store i32 %v.131, ptr %local.1, align 4
  br label %bb.24
bb.23:
  %v.132 = load i32, ptr %local.1, align 4
  %v.133 = add i32 0, 4
  %tmp.161 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.132, i32 %v.133)
  %v.134 = extractvalue { i32, i1 } %tmp.161, 0
  %tmp.162 = extractvalue { i32, i1 } %tmp.161, 1
  br i1 %tmp.162, label %zil.trap, label %checked.46
checked.46:
  %v.135 = add i32 0, 251
  %tmp.163 = icmp eq i32 %v.135, 0
  %tmp.164 = icmp eq i32 %v.134, -2147483648
  %tmp.165 = icmp eq i32 %v.135, -1
  %tmp.166 = and i1 %tmp.164, %tmp.165
  %tmp.167 = or i1 %tmp.163, %tmp.166
  br i1 %tmp.167, label %zil.trap, label %checked.47
checked.47:
  %v.136 = srem i32 %v.134, %v.135
  store i32 %v.136, ptr %local.1, align 4
  br label %bb.24
bb.24:
  %v.137 = load i32, ptr %local.1, align 4
  %v.138 = add i32 0, 3
  %tmp.168 = icmp eq i32 %v.138, 0
  %tmp.169 = icmp eq i32 %v.137, -2147483648
  %tmp.170 = icmp eq i32 %v.138, -1
  %tmp.171 = and i1 %tmp.169, %tmp.170
  %tmp.172 = or i1 %tmp.168, %tmp.171
  br i1 %tmp.172, label %zil.trap, label %checked.48
checked.48:
  %v.139 = srem i32 %v.137, %v.138
  %v.140 = add i32 0, 0
  %v.141 = icmp eq i32 %v.139, %v.140
  br i1 %v.141, label %bb.25, label %bb.26
bb.25:
  %v.142 = load i32, ptr %local.1, align 4
  %v.143 = add i32 0, 2
  %tmp.173 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.142, i32 %v.143)
  %v.144 = extractvalue { i32, i1 } %tmp.173, 0
  %tmp.174 = extractvalue { i32, i1 } %tmp.173, 1
  br i1 %tmp.174, label %zil.trap, label %checked.49
checked.49:
  %v.145 = add i32 0, 11
  %tmp.175 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.144, i32 %v.145)
  %v.146 = extractvalue { i32, i1 } %tmp.175, 0
  %tmp.176 = extractvalue { i32, i1 } %tmp.175, 1
  br i1 %tmp.176, label %zil.trap, label %checked.50
checked.50:
  %v.147 = add i32 0, 251
  %tmp.177 = icmp eq i32 %v.147, 0
  %tmp.178 = icmp eq i32 %v.146, -2147483648
  %tmp.179 = icmp eq i32 %v.147, -1
  %tmp.180 = and i1 %tmp.178, %tmp.179
  %tmp.181 = or i1 %tmp.177, %tmp.180
  br i1 %tmp.181, label %zil.trap, label %checked.51
checked.51:
  %v.148 = srem i32 %v.146, %v.147
  store i32 %v.148, ptr %local.1, align 4
  br label %bb.27
bb.26:
  %v.149 = load i32, ptr %local.1, align 4
  %v.150 = add i32 0, 5
  %tmp.182 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.149, i32 %v.150)
  %v.151 = extractvalue { i32, i1 } %tmp.182, 0
  %tmp.183 = extractvalue { i32, i1 } %tmp.182, 1
  br i1 %tmp.183, label %zil.trap, label %checked.52
checked.52:
  %v.152 = add i32 0, 251
  %tmp.184 = icmp eq i32 %v.152, 0
  %tmp.185 = icmp eq i32 %v.151, -2147483648
  %tmp.186 = icmp eq i32 %v.152, -1
  %tmp.187 = and i1 %tmp.185, %tmp.186
  %tmp.188 = or i1 %tmp.184, %tmp.187
  br i1 %tmp.188, label %zil.trap, label %checked.53
checked.53:
  %v.153 = srem i32 %v.151, %v.152
  store i32 %v.153, ptr %local.1, align 4
  br label %bb.27
bb.27:
  %v.154 = load i32, ptr %local.1, align 4
  %v.155 = add i32 0, 3
  %tmp.189 = icmp eq i32 %v.155, 0
  %tmp.190 = icmp eq i32 %v.154, -2147483648
  %tmp.191 = icmp eq i32 %v.155, -1
  %tmp.192 = and i1 %tmp.190, %tmp.191
  %tmp.193 = or i1 %tmp.189, %tmp.192
  br i1 %tmp.193, label %zil.trap, label %checked.54
checked.54:
  %v.156 = srem i32 %v.154, %v.155
  %v.157 = add i32 0, 0
  %v.158 = icmp eq i32 %v.156, %v.157
  br i1 %v.158, label %bb.28, label %bb.29
bb.28:
  %v.159 = load i32, ptr %local.1, align 4
  %v.160 = add i32 0, 2
  %tmp.194 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.159, i32 %v.160)
  %v.161 = extractvalue { i32, i1 } %tmp.194, 0
  %tmp.195 = extractvalue { i32, i1 } %tmp.194, 1
  br i1 %tmp.195, label %zil.trap, label %checked.55
checked.55:
  %v.162 = add i32 0, 12
  %tmp.196 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.161, i32 %v.162)
  %v.163 = extractvalue { i32, i1 } %tmp.196, 0
  %tmp.197 = extractvalue { i32, i1 } %tmp.196, 1
  br i1 %tmp.197, label %zil.trap, label %checked.56
checked.56:
  %v.164 = add i32 0, 251
  %tmp.198 = icmp eq i32 %v.164, 0
  %tmp.199 = icmp eq i32 %v.163, -2147483648
  %tmp.200 = icmp eq i32 %v.164, -1
  %tmp.201 = and i1 %tmp.199, %tmp.200
  %tmp.202 = or i1 %tmp.198, %tmp.201
  br i1 %tmp.202, label %zil.trap, label %checked.57
checked.57:
  %v.165 = srem i32 %v.163, %v.164
  store i32 %v.165, ptr %local.1, align 4
  br label %bb.30
bb.29:
  %v.166 = load i32, ptr %local.1, align 4
  %v.167 = add i32 0, 6
  %tmp.203 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.166, i32 %v.167)
  %v.168 = extractvalue { i32, i1 } %tmp.203, 0
  %tmp.204 = extractvalue { i32, i1 } %tmp.203, 1
  br i1 %tmp.204, label %zil.trap, label %checked.58
checked.58:
  %v.169 = add i32 0, 251
  %tmp.205 = icmp eq i32 %v.169, 0
  %tmp.206 = icmp eq i32 %v.168, -2147483648
  %tmp.207 = icmp eq i32 %v.169, -1
  %tmp.208 = and i1 %tmp.206, %tmp.207
  %tmp.209 = or i1 %tmp.205, %tmp.208
  br i1 %tmp.209, label %zil.trap, label %checked.59
checked.59:
  %v.170 = srem i32 %v.168, %v.169
  store i32 %v.170, ptr %local.1, align 4
  br label %bb.30
bb.30:
  %v.171 = load i32, ptr %local.1, align 4
  %v.172 = add i32 0, 3
  %tmp.210 = icmp eq i32 %v.172, 0
  %tmp.211 = icmp eq i32 %v.171, -2147483648
  %tmp.212 = icmp eq i32 %v.172, -1
  %tmp.213 = and i1 %tmp.211, %tmp.212
  %tmp.214 = or i1 %tmp.210, %tmp.213
  br i1 %tmp.214, label %zil.trap, label %checked.60
checked.60:
  %v.173 = srem i32 %v.171, %v.172
  %v.174 = add i32 0, 0
  %v.175 = icmp eq i32 %v.173, %v.174
  br i1 %v.175, label %bb.31, label %bb.32
bb.31:
  %v.176 = load i32, ptr %local.1, align 4
  %v.177 = add i32 0, 2
  %tmp.215 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.176, i32 %v.177)
  %v.178 = extractvalue { i32, i1 } %tmp.215, 0
  %tmp.216 = extractvalue { i32, i1 } %tmp.215, 1
  br i1 %tmp.216, label %zil.trap, label %checked.61
checked.61:
  %v.179 = add i32 0, 13
  %tmp.217 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.178, i32 %v.179)
  %v.180 = extractvalue { i32, i1 } %tmp.217, 0
  %tmp.218 = extractvalue { i32, i1 } %tmp.217, 1
  br i1 %tmp.218, label %zil.trap, label %checked.62
checked.62:
  %v.181 = add i32 0, 251
  %tmp.219 = icmp eq i32 %v.181, 0
  %tmp.220 = icmp eq i32 %v.180, -2147483648
  %tmp.221 = icmp eq i32 %v.181, -1
  %tmp.222 = and i1 %tmp.220, %tmp.221
  %tmp.223 = or i1 %tmp.219, %tmp.222
  br i1 %tmp.223, label %zil.trap, label %checked.63
checked.63:
  %v.182 = srem i32 %v.180, %v.181
  store i32 %v.182, ptr %local.1, align 4
  br label %bb.33
bb.32:
  %v.183 = load i32, ptr %local.1, align 4
  %v.184 = add i32 0, 7
  %tmp.224 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.183, i32 %v.184)
  %v.185 = extractvalue { i32, i1 } %tmp.224, 0
  %tmp.225 = extractvalue { i32, i1 } %tmp.224, 1
  br i1 %tmp.225, label %zil.trap, label %checked.64
checked.64:
  %v.186 = add i32 0, 251
  %tmp.226 = icmp eq i32 %v.186, 0
  %tmp.227 = icmp eq i32 %v.185, -2147483648
  %tmp.228 = icmp eq i32 %v.186, -1
  %tmp.229 = and i1 %tmp.227, %tmp.228
  %tmp.230 = or i1 %tmp.226, %tmp.229
  br i1 %tmp.230, label %zil.trap, label %checked.65
checked.65:
  %v.187 = srem i32 %v.185, %v.186
  store i32 %v.187, ptr %local.1, align 4
  br label %bb.33
bb.33:
  %v.188 = load i32, ptr %local.1, align 4
  %v.189 = add i32 0, 3
  %tmp.231 = icmp eq i32 %v.189, 0
  %tmp.232 = icmp eq i32 %v.188, -2147483648
  %tmp.233 = icmp eq i32 %v.189, -1
  %tmp.234 = and i1 %tmp.232, %tmp.233
  %tmp.235 = or i1 %tmp.231, %tmp.234
  br i1 %tmp.235, label %zil.trap, label %checked.66
checked.66:
  %v.190 = srem i32 %v.188, %v.189
  %v.191 = add i32 0, 0
  %v.192 = icmp eq i32 %v.190, %v.191
  br i1 %v.192, label %bb.34, label %bb.35
bb.34:
  %v.193 = load i32, ptr %local.1, align 4
  %v.194 = add i32 0, 2
  %tmp.236 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.193, i32 %v.194)
  %v.195 = extractvalue { i32, i1 } %tmp.236, 0
  %tmp.237 = extractvalue { i32, i1 } %tmp.236, 1
  br i1 %tmp.237, label %zil.trap, label %checked.67
checked.67:
  %v.196 = add i32 0, 1
  %tmp.238 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.195, i32 %v.196)
  %v.197 = extractvalue { i32, i1 } %tmp.238, 0
  %tmp.239 = extractvalue { i32, i1 } %tmp.238, 1
  br i1 %tmp.239, label %zil.trap, label %checked.68
checked.68:
  %v.198 = add i32 0, 251
  %tmp.240 = icmp eq i32 %v.198, 0
  %tmp.241 = icmp eq i32 %v.197, -2147483648
  %tmp.242 = icmp eq i32 %v.198, -1
  %tmp.243 = and i1 %tmp.241, %tmp.242
  %tmp.244 = or i1 %tmp.240, %tmp.243
  br i1 %tmp.244, label %zil.trap, label %checked.69
checked.69:
  %v.199 = srem i32 %v.197, %v.198
  store i32 %v.199, ptr %local.1, align 4
  br label %bb.36
bb.35:
  %v.200 = load i32, ptr %local.1, align 4
  %v.201 = add i32 0, 8
  %tmp.245 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.200, i32 %v.201)
  %v.202 = extractvalue { i32, i1 } %tmp.245, 0
  %tmp.246 = extractvalue { i32, i1 } %tmp.245, 1
  br i1 %tmp.246, label %zil.trap, label %checked.70
checked.70:
  %v.203 = add i32 0, 251
  %tmp.247 = icmp eq i32 %v.203, 0
  %tmp.248 = icmp eq i32 %v.202, -2147483648
  %tmp.249 = icmp eq i32 %v.203, -1
  %tmp.250 = and i1 %tmp.248, %tmp.249
  %tmp.251 = or i1 %tmp.247, %tmp.250
  br i1 %tmp.251, label %zil.trap, label %checked.71
checked.71:
  %v.204 = srem i32 %v.202, %v.203
  store i32 %v.204, ptr %local.1, align 4
  br label %bb.36
bb.36:
  %v.205 = load i32, ptr %local.1, align 4
  %v.206 = add i32 0, 3
  %tmp.252 = icmp eq i32 %v.206, 0
  %tmp.253 = icmp eq i32 %v.205, -2147483648
  %tmp.254 = icmp eq i32 %v.206, -1
  %tmp.255 = and i1 %tmp.253, %tmp.254
  %tmp.256 = or i1 %tmp.252, %tmp.255
  br i1 %tmp.256, label %zil.trap, label %checked.72
checked.72:
  %v.207 = srem i32 %v.205, %v.206
  %v.208 = add i32 0, 0
  %v.209 = icmp eq i32 %v.207, %v.208
  br i1 %v.209, label %bb.37, label %bb.38
bb.37:
  %v.210 = load i32, ptr %local.1, align 4
  %v.211 = add i32 0, 2
  %tmp.257 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.210, i32 %v.211)
  %v.212 = extractvalue { i32, i1 } %tmp.257, 0
  %tmp.258 = extractvalue { i32, i1 } %tmp.257, 1
  br i1 %tmp.258, label %zil.trap, label %checked.73
checked.73:
  %v.213 = add i32 0, 2
  %tmp.259 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.212, i32 %v.213)
  %v.214 = extractvalue { i32, i1 } %tmp.259, 0
  %tmp.260 = extractvalue { i32, i1 } %tmp.259, 1
  br i1 %tmp.260, label %zil.trap, label %checked.74
checked.74:
  %v.215 = add i32 0, 251
  %tmp.261 = icmp eq i32 %v.215, 0
  %tmp.262 = icmp eq i32 %v.214, -2147483648
  %tmp.263 = icmp eq i32 %v.215, -1
  %tmp.264 = and i1 %tmp.262, %tmp.263
  %tmp.265 = or i1 %tmp.261, %tmp.264
  br i1 %tmp.265, label %zil.trap, label %checked.75
checked.75:
  %v.216 = srem i32 %v.214, %v.215
  store i32 %v.216, ptr %local.1, align 4
  br label %bb.39
bb.38:
  %v.217 = load i32, ptr %local.1, align 4
  %v.218 = add i32 0, 9
  %tmp.266 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.217, i32 %v.218)
  %v.219 = extractvalue { i32, i1 } %tmp.266, 0
  %tmp.267 = extractvalue { i32, i1 } %tmp.266, 1
  br i1 %tmp.267, label %zil.trap, label %checked.76
checked.76:
  %v.220 = add i32 0, 251
  %tmp.268 = icmp eq i32 %v.220, 0
  %tmp.269 = icmp eq i32 %v.219, -2147483648
  %tmp.270 = icmp eq i32 %v.220, -1
  %tmp.271 = and i1 %tmp.269, %tmp.270
  %tmp.272 = or i1 %tmp.268, %tmp.271
  br i1 %tmp.272, label %zil.trap, label %checked.77
checked.77:
  %v.221 = srem i32 %v.219, %v.220
  store i32 %v.221, ptr %local.1, align 4
  br label %bb.39
bb.39:
  %v.222 = load i32, ptr %local.1, align 4
  %v.223 = add i32 0, 3
  %tmp.273 = icmp eq i32 %v.223, 0
  %tmp.274 = icmp eq i32 %v.222, -2147483648
  %tmp.275 = icmp eq i32 %v.223, -1
  %tmp.276 = and i1 %tmp.274, %tmp.275
  %tmp.277 = or i1 %tmp.273, %tmp.276
  br i1 %tmp.277, label %zil.trap, label %checked.78
checked.78:
  %v.224 = srem i32 %v.222, %v.223
  %v.225 = add i32 0, 0
  %v.226 = icmp eq i32 %v.224, %v.225
  br i1 %v.226, label %bb.40, label %bb.41
bb.40:
  %v.227 = load i32, ptr %local.1, align 4
  %v.228 = add i32 0, 2
  %tmp.278 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.227, i32 %v.228)
  %v.229 = extractvalue { i32, i1 } %tmp.278, 0
  %tmp.279 = extractvalue { i32, i1 } %tmp.278, 1
  br i1 %tmp.279, label %zil.trap, label %checked.79
checked.79:
  %v.230 = add i32 0, 3
  %tmp.280 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.229, i32 %v.230)
  %v.231 = extractvalue { i32, i1 } %tmp.280, 0
  %tmp.281 = extractvalue { i32, i1 } %tmp.280, 1
  br i1 %tmp.281, label %zil.trap, label %checked.80
checked.80:
  %v.232 = add i32 0, 251
  %tmp.282 = icmp eq i32 %v.232, 0
  %tmp.283 = icmp eq i32 %v.231, -2147483648
  %tmp.284 = icmp eq i32 %v.232, -1
  %tmp.285 = and i1 %tmp.283, %tmp.284
  %tmp.286 = or i1 %tmp.282, %tmp.285
  br i1 %tmp.286, label %zil.trap, label %checked.81
checked.81:
  %v.233 = srem i32 %v.231, %v.232
  store i32 %v.233, ptr %local.1, align 4
  br label %bb.42
bb.41:
  %v.234 = load i32, ptr %local.1, align 4
  %v.235 = add i32 0, 10
  %tmp.287 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.234, i32 %v.235)
  %v.236 = extractvalue { i32, i1 } %tmp.287, 0
  %tmp.288 = extractvalue { i32, i1 } %tmp.287, 1
  br i1 %tmp.288, label %zil.trap, label %checked.82
checked.82:
  %v.237 = add i32 0, 251
  %tmp.289 = icmp eq i32 %v.237, 0
  %tmp.290 = icmp eq i32 %v.236, -2147483648
  %tmp.291 = icmp eq i32 %v.237, -1
  %tmp.292 = and i1 %tmp.290, %tmp.291
  %tmp.293 = or i1 %tmp.289, %tmp.292
  br i1 %tmp.293, label %zil.trap, label %checked.83
checked.83:
  %v.238 = srem i32 %v.236, %v.237
  store i32 %v.238, ptr %local.1, align 4
  br label %bb.42
bb.42:
  %v.239 = load i32, ptr %local.1, align 4
  %v.240 = add i32 0, 3
  %tmp.294 = icmp eq i32 %v.240, 0
  %tmp.295 = icmp eq i32 %v.239, -2147483648
  %tmp.296 = icmp eq i32 %v.240, -1
  %tmp.297 = and i1 %tmp.295, %tmp.296
  %tmp.298 = or i1 %tmp.294, %tmp.297
  br i1 %tmp.298, label %zil.trap, label %checked.84
checked.84:
  %v.241 = srem i32 %v.239, %v.240
  %v.242 = add i32 0, 0
  %v.243 = icmp eq i32 %v.241, %v.242
  br i1 %v.243, label %bb.43, label %bb.44
bb.43:
  %v.244 = load i32, ptr %local.1, align 4
  %v.245 = add i32 0, 2
  %tmp.299 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.244, i32 %v.245)
  %v.246 = extractvalue { i32, i1 } %tmp.299, 0
  %tmp.300 = extractvalue { i32, i1 } %tmp.299, 1
  br i1 %tmp.300, label %zil.trap, label %checked.85
checked.85:
  %v.247 = add i32 0, 4
  %tmp.301 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.246, i32 %v.247)
  %v.248 = extractvalue { i32, i1 } %tmp.301, 0
  %tmp.302 = extractvalue { i32, i1 } %tmp.301, 1
  br i1 %tmp.302, label %zil.trap, label %checked.86
checked.86:
  %v.249 = add i32 0, 251
  %tmp.303 = icmp eq i32 %v.249, 0
  %tmp.304 = icmp eq i32 %v.248, -2147483648
  %tmp.305 = icmp eq i32 %v.249, -1
  %tmp.306 = and i1 %tmp.304, %tmp.305
  %tmp.307 = or i1 %tmp.303, %tmp.306
  br i1 %tmp.307, label %zil.trap, label %checked.87
checked.87:
  %v.250 = srem i32 %v.248, %v.249
  store i32 %v.250, ptr %local.1, align 4
  br label %bb.45
bb.44:
  %v.251 = load i32, ptr %local.1, align 4
  %v.252 = add i32 0, 11
  %tmp.308 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.251, i32 %v.252)
  %v.253 = extractvalue { i32, i1 } %tmp.308, 0
  %tmp.309 = extractvalue { i32, i1 } %tmp.308, 1
  br i1 %tmp.309, label %zil.trap, label %checked.88
checked.88:
  %v.254 = add i32 0, 251
  %tmp.310 = icmp eq i32 %v.254, 0
  %tmp.311 = icmp eq i32 %v.253, -2147483648
  %tmp.312 = icmp eq i32 %v.254, -1
  %tmp.313 = and i1 %tmp.311, %tmp.312
  %tmp.314 = or i1 %tmp.310, %tmp.313
  br i1 %tmp.314, label %zil.trap, label %checked.89
checked.89:
  %v.255 = srem i32 %v.253, %v.254
  store i32 %v.255, ptr %local.1, align 4
  br label %bb.45
bb.45:
  %v.256 = load i32, ptr %local.1, align 4
  %v.257 = add i32 0, 3
  %tmp.315 = icmp eq i32 %v.257, 0
  %tmp.316 = icmp eq i32 %v.256, -2147483648
  %tmp.317 = icmp eq i32 %v.257, -1
  %tmp.318 = and i1 %tmp.316, %tmp.317
  %tmp.319 = or i1 %tmp.315, %tmp.318
  br i1 %tmp.319, label %zil.trap, label %checked.90
checked.90:
  %v.258 = srem i32 %v.256, %v.257
  %v.259 = add i32 0, 0
  %v.260 = icmp eq i32 %v.258, %v.259
  br i1 %v.260, label %bb.46, label %bb.47
bb.46:
  %v.261 = load i32, ptr %local.1, align 4
  %v.262 = add i32 0, 2
  %tmp.320 = call { i32, i1 } @llvm.smul.with.overflow.i32(i32 %v.261, i32 %v.262)
  %v.263 = extractvalue { i32, i1 } %tmp.320, 0
  %tmp.321 = extractvalue { i32, i1 } %tmp.320, 1
  br i1 %tmp.321, label %zil.trap, label %checked.91
checked.91:
  %v.264 = add i32 0, 5
  %tmp.322 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.263, i32 %v.264)
  %v.265 = extractvalue { i32, i1 } %tmp.322, 0
  %tmp.323 = extractvalue { i32, i1 } %tmp.322, 1
  br i1 %tmp.323, label %zil.trap, label %checked.92
checked.92:
  %v.266 = add i32 0, 251
  %tmp.324 = icmp eq i32 %v.266, 0
  %tmp.325 = icmp eq i32 %v.265, -2147483648
  %tmp.326 = icmp eq i32 %v.266, -1
  %tmp.327 = and i1 %tmp.325, %tmp.326
  %tmp.328 = or i1 %tmp.324, %tmp.327
  br i1 %tmp.328, label %zil.trap, label %checked.93
checked.93:
  %v.267 = srem i32 %v.265, %v.266
  store i32 %v.267, ptr %local.1, align 4
  br label %bb.48
bb.47:
  %v.268 = load i32, ptr %local.1, align 4
  %v.269 = add i32 0, 12
  %tmp.329 = call { i32, i1 } @llvm.sadd.with.overflow.i32(i32 %v.268, i32 %v.269)
  %v.270 = extractvalue { i32, i1 } %tmp.329, 0
  %tmp.330 = extractvalue { i32, i1 } %tmp.329, 1
  br i1 %tmp.330, label %zil.trap, label %checked.94
checked.94:
  %v.271 = add i32 0, 251
  %tmp.331 = icmp eq i32 %v.271, 0
  %tmp.332 = icmp eq i32 %v.270, -2147483648
  %tmp.333 = icmp eq i32 %v.271, -1
  %tmp.334 = and i1 %tmp.332, %tmp.333
  %tmp.335 = or i1 %tmp.331, %tmp.334
  br i1 %tmp.335, label %zil.trap, label %checked.95
checked.95:
  %v.272 = srem i32 %v.270, %v.271
  store i32 %v.272, ptr %local.1, align 4
  br label %bb.48
bb.48:
  %v.273 = load i32, ptr %local.1, align 4
  ret i32 %v.273
zil.trap:
  call void @llvm.trap()
  unreachable
}

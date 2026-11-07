# 组合语言输入

`component.zil` 保留没有显式结果类型的函数指针枚举返回值绑定；冻结观察在 Expand 以 `E-GENERIC-ARGUMENT` 拒绝（record enum pattern type cannot be inferred）。它是合法目标的结果推断缺口，不是永久禁止或通过的负例。

`component-explicit.zil` 给该结果绑定添加 `:Message`，组合默认 trait 返回 fnptr、大 Copy 记录式枚举间接调用/返回、mutable life 与子线程共享实际局部地址。main 检查 child=43、parent=42、join 后 value=44，成功退出 0。`replay.py` 以显式 `--entry main` 运行 Clang/LLVM API O0/O2；组合源请求 4 CGU，两个单函数 CT 控制请求 1 CGU。核对实际 backend、builder、pipeline、fallback 和完整返回值。冻结旧观察只能证明其绑定提交，换编译器需重新执行。

# 176 作者限定交付

生产提交：`55a59cdfcfc78dc85d9ac631e51d9105ee750c79`；受测 `crates` tree：`0fe0f25da88ee0a22a3897544f213579899d0cde`。公共 schema/Wire 21，规范输入摘要 `8aecd0f052b3d8eb099d19d26cd0bad56b26d3a7be91c79aca5a962a010711be`。本目录只报告施工方限定验证，独审、主树集成与正式门由集成方记录。

## 已实现的有限合同

普通 `package State<F>` 保存一个具体、自含、内联 Closure 类型。`pack<F: FnMut() -> i32>(f:F, ...) -> State<F>`、`tick<G: FnMut() -> i32>(state:State<G>) -> State<G>` 和嵌套 `twice<H>` 可以跨模块调用；F/G/H 的源变量重命名不改变同一个 State 的名义形状。源码约束另保留工厂 opaque 身份，`forward` 即使返回相同底层 ClosureId，也不能替换另一个工厂产生的字段或整个 State。

`BorrowPackageField(local,index,mutable)` 只给具体 Closure 字段的地址；LLVM 使用父 State 实际字段的 GEP，FnMut 连续调用保留原环境。完整字段 take、call、refill、replace 沿已有 Package 责任旗标执行。FnOnce 先取出后消费。没有资源的 Closure 也唯一发布，不能复制 Load 或重复消费 SSA；这与资源析构义务分别检查。父 State 按字段声明序清理，Closure 的实际环境递归清理；无资源 Closure 的字段清理入口为空函数，父字段标志仍正常撤销。

拥有 State helper 是普通拥有函数边界。Analyze 与 Verify 分别检查真实 Closure 捕获图的自含性；旧的借入/Desc 同步 helper 合同继续生效，没有用空的旧 envelope 取代证明。模板实例化有递归阻断和 `max_call_depth` 约束，源形状替换、字段匹配、Closure 在 State 中的展开按现有预算收费。

## 真实验证

`author-final.log` 为如下实际限定命令的输出；便携入口是 `tools/language/176/check.sh`。

```sh
source /workspace/scratch/39be3faa4442/toolchains/env.sh
CARGO_TARGET_DIR=/workspace/scratch/39be3faa4442/targets/176 \
CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
ZIL_TEST_REPORT_DIR=/workspace/scratch/39be3faa4442/run-logs/176-final-tests \
cargo test -p zil-driver \
  --test impl176_callable_fields \
  --test impl065_helpers --test impl121_callable_nominals \
  --test impl125_factory_helpers --test impl126_helper_sequences \
  --test impl167_owned_arrays --test impl167_array_capture_boundary \
  --test impl168_retained_composition --test impl170_package_fields \
  --offline -j2 -- --nocapture
```

结果为九个测试文件 **127/127**，其中 176 的 **12/12**；18 次 O0/O2 原生执行均由测试核对退出码、stdout 和一次独立 Verify。工具链为 `rustc 1.99.1 (58cf8f9c3 2026-10-13)` 与 Ubuntu clang 17.0.6。未跑整仓或正式性能门。

176 覆盖完整 State 往返、实际原位置连续 FnMut、资源和自含子闭包、ByteBuffer 捕获、泛型函数内部 take/call/refill/return、FnOnce 消费、RHS 错误保护旧字段和已准备参数回滚。失败替换得到 `TA`，成功替换/提取后归还得到 `ATB`，FnOnce 取出后消费得到 `AT`；多模块 `twice` 与连续计数验证没有环境快照。每个真实源、阶段 IR/接口、LLVM、编译报告和 stdout 在 `author-final/`，源与输入摘要见 `SUMMARY.json`。执行文件未收录。

源码负例包括两个不同 opaque 工厂字段混赋和整个 State 混赋、重复字段 take、缺字段 whole move、借用仍使用时替换、无 mutable 权限借用、同一局部环境二次发布、借入环境逃入 State。8 个 raw 篡改同时经对象 API 与 Wire 入口拒绝：越界字段、权限谎报、旧地址跨替换、Copy 读取 Closure 字段、清错字段后覆盖、Load 发布、已消费字段 SSA 复用、无资源 callback 两次发布。均使用实际源码编译产生的候选后作单点改动。

嵌套环境 State 的独立 Verify 预算边界为 raw 与 Wire 均 `15478` 成功、`15477` ResourceLimit；两次重复一致。它是该输入的精确逻辑预算边界，不是内存或运行性能测量。

## 保留的失败与修正

`exploratory/` 保存实际失败日志、失败源/报告及 raw 候选。接线过程中发现的实质问题包括：展开后的 nominal 遮蔽 F；顺序 record 临时绑定 Load 复制 owned Closure；State helper 嵌套返回未替换 G/H；opaque 提示被误当普通 type key；无地址函数的 loan 快路径跳过纯 Closure 唯一发布。最后一项由新增 raw 负例实际检出，修正后重新跑全部上述限定范围。还有早期测试误用保留字 `resource` 和一次局部 rlib 构建项不一致，均保留原日志，未计入通过数。

## 边界与后续

当前是一个 callable 参数、单一 F 的 Package 形状；保留既有至少一个 manages 字段要求。F 出现在一个拥有输入中（直接 F 或完整 State<F>），普通返回 F、&State、dyn、一般借入环境逃逸、捕获整个含 callback 的 State、State 数组及新拥有捕获组合未开放。字段 loan 仍采用保守的父局部来源，不承诺不同 callback 字段可同时独占。Store 与 callback 同一父 State 的并集、真实动态分配失败和组合消费者由 177 在合并 174/175 后验证，本树没有冒称已经运行该组合。

## Rust 四目标

| 目标 | 本片事实 | 结论边界 |
|---|---|---|
| 同类安全 | 真实源、raw/Wire、失败清理、精确预算边界和旧合同回归 | 只支持上述有限协议；独审与完整语言安全不是作者通过数的推论 |
| 运行/内存 | 具体环境内联，字段 GEP 原位置调用，18 次机器行为一致 | 未作 Rust 时间、峰值内存或尾延迟比较；不宣称同级 |
| 表达/维护 | 普通泛型 State API 跨模块、跨 F 重命名、嵌套调用可执行 | 是语言缺口的具体补齐，未测维护成本净收益 |
| 编译成本 | 沿现有有界预算，无正式提速试验 | 编译研究仍独立，未宣称更快 |
| 至少一项突破 | 长期 State 的 callback 字段贯通完整拥有 API 与机器清理 | 工程阶段成果；产品级净突破、B0 和完整语言终态仍由既有门裁决 |

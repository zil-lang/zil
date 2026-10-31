# 228 非作者审查

审查者 `/root/mainline_recovery`，实现作者 `/root/scope_audit`。单片修复后的生产身份为 `c1578e319d606d745835a7c3c609a3bb738b3c15`；测试提交为 `fbb7fbe9b6e45215a1e94d8c764f9aa981c53f0c` 与 `15f36114fcbc7812e2e2d7c53bbb8ed205539966`。共同 228/229 集成候选另附精确绑定，本记录不冒充联合验收。

审查者只读生产并独立写测试；工作树中的生产修复均为作者提交的 cherry-pick，相关生产文件与作者提交逐字比较相同。最后单片执行时允许函数名控制尚未提交，随后原字节进入 15f36114；准确执行身份在 `RESULT-single-slice.json` 中保留，没有把该执行写成干净提交重放。

## 发现及修复

1. c465 的 `for_items` 每轮无条件线性查询模块列表，旧纯函数生成也增加未计费工作。作者 9cb/2122 将类型批次标志并入既有预费遍历，只有实际类型批次查询 imports，并逐模块预费。这是静态预算发现，没有捏造运行失败或性能数字。
2. 2122 的混合生成批次只对非函数声明预检保留名，`fn bytes_len` 经卫生改名后逃过普通规则。独立真实源被 Verify 接受；普通同源在 Expand 拒绝，纯函数生成路径也误接受。作者 c157 在两个既有名称遍历中调用同一普通函数名字规则，原攻击现拒绝；`Copy`、`bool` 作为普通函数名仍可真实执行。

原错误源、报告、完整 IR/接口、源码、机器程序和 stdout/stderr/exit 均保留在归档。attempt1 的 4/1 来自审查夹具把不合法的 bounded module alias 当作语义反例；原输入保留，改成合法的泛型 bounded alias 后才测试 Unsupported。attempt2 的 4/1 是真实保留函数名缺陷；attempt3 的 0/1（5 filtered）是普通/纯函数/混合三路归因。它们没有重标为 Pass。

## 单片验证

修复后独立 6 组通过、无忽略或过滤：7 个单模块正例、2 个多模块正例，17 个普通拒绝、2 个模块名义/字段隐私拒绝及 3 个保留函数名控制。9 组程序各跑 LLVM API O0/O2，共 18 次真实构建和执行，均 exit 0、空 stdout/stderr。源码覆盖嵌套别名向前引用、外层类型引用、类型与值参数/局部遮蔽、模式类型命名空间、函数类型别名、泛型遮蔽、每轮不同数组形状、模块限定路径和不同模块名义身份。

嵌套生成源码分别二分实际工作和节点边界，执行 fresh N、warm N、N−1、warm N。低额度返回 ResourceLimit 且 Verify0，成功时 Verify1，缓存命中仍收费，每次 active query 为 0。阈值及完整报告见收据。选择性 Clippy `-D warnings` 通过。

## 范围

未发现未关闭的本片语义缺陷；一般 Type/Syntax、enum/trait/impl 生成、const 泛型、回流 ConfigReady 和私有名义合并均未开放。没有修改公开 schema、默认额度或安全核。真实源码与机器证据不代表完整 1.4、ZUI、原生 Windows、Rust 时间/RSS/维护/编译成本门完成，B0 Doing、完整语言 NotSatisfied、产品 NotRun 保持。

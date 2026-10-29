# 206：普通完整嵌套 Product

206 已完成有限语言验收。精确受测提交为 `bf44fb1e73889f03e6b48cc87a38475f20b626e6`，非作者审核提交为 `6bc19f7af0efb93d5c51ace7237d6f77de6f6aea`。正式整仓 **2615 通过、0 失败、0 忽略，266 组**；同提交 workspace/all-targets Clippy 通过。后续合入只增加记录、说明与任务状态，编译器、Rust 测试、消费者源和既定预期保持该受测身份。结构化收据见 `evidence/integration/206/final/RESULT.json`。

## 实际能力

普通完整 Package 可以包含完整 child 并继续有限递归组合。父包可以仅含 child；真正 Managed 叶仍由所在子包显式 manages。每个 child 在父内占一个完整责任槽，取到独立局部后才允许部分移动和补入，嵌回或整体转交必须完整。各层保留实际名义身份和声明顺序。

Analyze 与 Verify 独立跟踪实际 SSA/局部的每个 Managed 叶来源。包装、取 child、整体转交不把 Incoming 变为 Created；同类型的两个实际 child 不共享来源位，混合分支保持保守，普通未知调用结果不凭返回类型取得 create 证明。

替换按实际源顺序完整准备 RHS，再清旧 child、发布新 child。失败只清实际已取得的责任，不回滚显式转交。布局保持内联；每层声明序递归清理，局部退出保持逆成功初始化序。没有新增隐式分配、Box、RC、锁或全局表。

这使多层拥有组件能够先作为普通完整值构造、移动、替换及返回。**拥有这些 Product 的持久闭包由207接续**。206没有将 Product 本身开放为 Store/拥有数组元素或错误载荷，也没有开放其内部 Store、具体 Closure、拥有 Sum 或借入组合；已有有限 Copy 伴随类型和旧 profile 保持。

## 验证与归属

| 集合 | 实际结果 | 归属与边界 |
| --- | --- | --- |
| 完整工作区 | 2615/0/0，266组 | root，干净bf44；本次184.6501214504242秒不构成编译速度对比 |
| 全目标 Clippy | Pass，`-D warnings` | 同一bf44源码 |
| 独立 source/raw/Wire | 原45项通过；加入捕获攻击后46项通过，含12次native | 非作者；精确诊断和两旧迁移另作限定复验，重叠集合不相加 |
| 作者最终Linux | 94/0，42次真实分配失败 | O0/O2，20次普通、74次插桩；最终live0 |
| 作者最终Win64/Wine | 94/0，42次真实分配失败 | live0、bad_operations0；不是原生Windows |
| 非作者最终Linux | 94/0，42次真实分配失败 | 逐条核实际输出、退出码、分配统计和原预期 |

三组消费者使用同一CLI04，SHA256 `3bc6fc0a23aee0058ebd25fb941d9110d9432252fcf5ba1d302a8e457990a491`，28,941,608字节，182项编译输入均与bf44一致。每组峰值4个实际分配、80字节请求payload，不代表整个进程RSS或与Rust的成本比较。五模块语义预期保持；原夹具多行if缺分组的Parse Fail保留，修正只增加两个括号。作者最终两次初始运行因临时传输目录在native前未满足干净检出条件而停止，另用相同bf44的干净独立检出通过；旧失败不重标。

## 原失败与修复

首轮完整工作区 `d078f9e2cc5902167e78a5cb815638f9d05a2095` 的2612通过、2失败继续保留为Fail。两个旧非法raw子例只发生精确诊断顺序迁移，输入和其余近邻未变。Analyze原9707字节raw已与205归档逐字比较；旧Verify单元没有序列化raw，因此只报告原测试构造器一致，不伪造旧raw文件。详情见 `changes/206-legacy-diagnostics.md`。

207准备期间发现：局部capture误用新普通Product资格，Types/Analyze拒绝的协调1807字节raw/Wire曾在Verify两个入口被错误接受。非作者复现保留且未native。修复恢复捕获专用旧门；相同原字节现在精确拒绝为 Verify Unsupported / E-UNSUPPORTED / `non-Copy type requires another checker`，旧flat正例保持。207将用新的明确合同开放真正合法捕获，不回写206历史。

Analyze的Address包含Product、未用资源头包含Product两个入口也保留原错误接受及修复。原夹具语法、缺少LLVM动态库路径、Wine长TMPDIR启动问题、测试编译错误和临时传输目录引起的身份失败按实际来源分别留证，不能作为语言通过重标。

## 预算与终态

旧flat186控制保持Types418、Analyze8673、Verify10398。三个新源的Types为753/796/752，Analyze为21120/27114/26951，Verify为27075/33259/31077；各具名输入核对default、精确N、fresh N及N−1 ResourceLimit。新叶表、路径复制和合流在增长前收费；旧无child函数不分配新叶表，默认Limits不提高。固定控制不代表所有旧输入零成本，也不代表编译专项通过。

设计是用户指定的ZIL-LANG-1.4 / Ante-R1，本片保持schema/Wire27。178–180已按该版采用并合入，181仍是施工中的组合总片。完整1.4 **NotSatisfied**，B0 **Doing**，产品及原生Windows **NotRun**。207优先完整Product拥有捕获、默认实际消费、Fn/FnMut/FnOnce返回责任及进入既有State/外层callback Store的布局清理；其余拥有组合、抽象、卫生Syntax、长期外部环境等终态义务保留，编译速度线独立。

## 交付

标准Git bundle收代码、测试、工具、文档、紧凑收据及原历史。source/raw/Wire/IR、命令、输出和原失败另放 `206-all-components-evidence-compact.tar.xz`，执行产物只留摘要，Cargo缓存不入包。包大小和SHA由同名交付JSON提供，避免把容器自身身份写入其包含内容形成循环。线下导入与合并命令见配套 `OFFLINE-SYNC-206.txt`。

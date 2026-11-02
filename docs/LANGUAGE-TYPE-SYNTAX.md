# Type/Syntax 阶段验收

受测源码 `bea1e95b0826dd3e695396cac1665d6a74ad38ec` 已通过本地正式验收、限定独立审查和两套远端 CI，经 [PR20](https://github.com/zil-lang/zil/pull/20) 合入 `fd5fb8a49c8b7d71f9f22536236acc73e9e2db7d`；合入树与候选完全相同。runtime schema/Wire31、CT artifact schema1，Rust1.99.1、LLVM23.1.4。[精简结果与命令](validation/type-syntax.json)。

本批接通普通 CT 函数的 Type/Syntax 值、四类 quote 和 typed holes、锁定 provider、卫生作用域、标准反射与普通代码物化。Member 保留实际名义 owner，生成代码仍经过普通类型、所有权及独立核验。旧标量/聚合 CT、`#if/#for` 与新路径按静态依赖选择；Parse/Core 停点、取消、共享预算及实际查询复用保留。

| 正式检查 | 结果 |
| --- | --- |
| 默认 workspace | 3857 通过，0 失败 |
| reference workspace | 3749 通过，0 失败 |
| text-reference | 64 通过，0 失败 |
| 格式、全仓全部目标 Clippy | 通过 |
| LLVM 迁移检查 | 9 通过 |

Rust 测试均零忽略、零过滤。固定消费者含9个正例、19个拒绝源，生成与手写对照每次执行56个原生用例（36普通、20分配器控制），其中4个真实分配失败，返回后live=0；重放不累加为新覆盖。独审分别检查CT wire与五个golden、实际owner、混合CT路由、provider/卫生及解析缓存边界。

首轮 `b7421b01` 默认3815通过/30失败，修复了旧CT兼容和阶段回归；本次默认全绿后，reference 首跑2853通过/896失败，均受磁盘耗尽影响。清理不用的构建缓存与生成产物后，在同一源码上只补跑未通过的门并全部通过。原失败单列，不改写成成功。

两项诊断迁移已在 changes 登记：Core 的未锁定 `syntax use` 从旧未实现错误改为 `E-SYNTAX-LOCK-MISSING`；CT读取运行期绑定保留拒绝义务，沿既有 `E-CT-RUNTIME` 诊断。未删除旧测试以换取通过。

本批让生成代码与普通语言的类型、资源和失败清理规则贯通，未证明相对Rust的运行、内存或编译速度优势。完整1.4/ZUI、B0、产品门和原生Windows尚未完成；下一主线为结构化长期外部回调的登记、运行、关闭与环境释放。

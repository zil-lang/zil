# 180 独立审核

**限定结论：Pass。** 精确作者源码 `74254d8da12004519ca342b1c0602a5a90831c44`，审核者 `/root/design14_acceptance`，与作者不同。独立测试输入提交 `d732974b3cbe2b83f97ae99fb12b889305e5942e` 相对作者源码仅增加 reviewer 测试，生产字节未变。

## 审核与实跑

阅读 Types、Analyze、Verify、Lower、LLVM 的完整180差异，并追查实际来源/loan/affine转移、投影权限、闭包捕获和返回限制。使用独立 worktree 与空 target 重建，作者及邻近组实际56通过；180自身此次执行36次 native（含6次预期checked trap），10个raw/Wire变异两路拒绝。新增独审6组通过，含7个source边界和7个raw/Wire变异：独占Load复制、重复消费SSA、非法字段、整Package读取/写入、当前调用重复独占实参、把第二再借用重定向到第一个实际地址。

候选攻击允许重算module fingerprint；拒绝来自类型、来源、Hold、贷款或affine权限检查。机器路径从实际len做unsigned比较，覆盖负数/等于len/仅小于capacity，之后才形成元素地址。支持Copy投影及同步helper再借用，借入最后使用后才允许结构变化。Managed字段或整个元素的拥有权限未开放。

## 原始失败

自写攻击第一轮3通过、2失败：Store捕获在Types阶段已明确Unsupported；整Package的ReadAddress为Verify E-VERIFY-ADDRESS。两项是独审断言对阶段/代码的预期错误，源码未放宽。失败原件和当时完整review测试输入保留；修正后6通过、0失败。

## 179合流与边界

只读比较确认179与180主要在holds/borrows/loans的邻近匹配分支相交：合流时须保留StoreBorrowMut传播和unique loan生成，同时保留179实际field root及其预算计费。独占Store窗口仍覆盖实际整个Store，不能按未知索引视作分离；父壳move/drop继续与所有字段相交。当前审核没有执行179+180合流程序，最终组合由集成树验证。

180源限定Linux x64；Win64由181接管。一般Store捕获、元素/派生地址返回、保留窗口与lending仍未开放。无阻断性生产问题；主树已补录旧175断言迁移写集（aba4ef7）。

## Rust四目标

限定安全与实际原地写有证据；没有同语义Rust运行/内存、维护成本或编译成本比较，不宣称产品级净突破。完整语言NotSatisfied、产品NotRun、B0 Doing。具体命令、文件、原始观察与边界见review.json；manifest.json绑定紧凑证据。执行程序、LLVM阶段dump和缓存未纳入本目录。

# 334：按当前源码复核十方面语言缺口

本片以已合入 main 的 `40fbcca6ef506f3cb8e1d40a4f8995c788627da0`
为源码基线，只更新真实源样本、能力观察和消费者优先级，不改变语言接受性、
公共 schema、ABI、预算、正式验收门或工单状态。

复核用户十行：基础类型/模式、泛型/trait、借用/拥有、Type/Syntax、动态抽象、
一般状态、返回后外部登记、挂起/并发、库/ZUI、待定语义与平台/成本验证。
保留历史 pending 路径与字节，并按当前实际结果判断；未支持的合法目标仍是
待实现能力，不改成永久拒绝测试。普通 Product 局部可变 helper 和 CT 普通
Copy 泛型已经实现；life-only、具体泛型 trait 实例及真实 Direct pthread
仅按各有限合同报告。原 a18 数组已修复；一般按值 ABI 由333独立实测，不在
本片先报崩溃或修复。329–333仅登记施工与承接，不提前计入已实现。

CLI沿用现有 release：原观察源码 `f2122be4deb5d6b00abbc3847f8d144a919035fe`，
二进制 SHA256 `89891e131177e2fdc8d5e9febb90fbb37521a9fc815869e76b56f74ae575df67`。
先核源码、Cargo及原生适配输入与当前基线同一性，再复跑旧25探针和覆盖十行
所必需的最小源。逐源记录类别、阶段、诊断、源hash与CLIhash，check至Verify
和真实native分别计数；不运行全量，不搬回原始 evidence。

完整语言、B0/ZUI、一般 SABI/Mutex/Transfer/Share 和Rust四目标保持原验收义务。
本片的后续排序以新增可表达的组件API和真实机器消费者为依据，不给完成百分比。

本次实际收集完成：52源中22 Success、18 Unsupported、12 SafeReject，旧25原字节
重放为8/7/10，正控与保护反例无回退。9源×API O0/O2的18次构建/执行均明确
backend=llvm-api、无fallback、main harness打印0且exit0；四个新增最小源仅为
Copy tuple通配正控、T/life混合及usize/u64链。前者原生成功，后三者当前未支持。
初始25源探索重放和一次tuple O0运行单列，不累加到最终覆盖数。

两份语言文档按十行重写，原25和大数组历史JSON不改。新collector不创建Cargo
target，机器产物仅存在临时目录。当前规范hash与派工一致；CLI来源收据与生产
输入同一性、每份源hash及真实类别/阶段在`docs/validation/language-gaps-334.json`。
本片没有生产修改、正式全量、Windows、弱内存或Rust同语义成本测量；Rust四目标
与完整语言仍未完成，也不把审计本身称为新的产品突破。

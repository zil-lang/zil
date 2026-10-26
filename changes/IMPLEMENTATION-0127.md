# 127 局部工厂 callable 与泛型 helper 组合

## 结果与实际消费者

泛型 helper 现在能直接调用已经解析并核验为 self-contained 的工厂结果，再把普通 Copy 结果交给外部回调。普通程序可写 `let local=make(2); return f(local(3))`。参数与局部实例可以来自同一工厂、共享隐藏 ClosureId，公共 helper trace 仍只记录参数的真实调用。

四模块 `examples/mainline/local_factory_helper` 组合外部拥有 Fn 与两个局部拥有 FnMut，并保留第三个未调用环境。正常路径输出 `PCBAST`；局部调用错误输出 `CBAT`；参数调用错误输出 `PCBAT`，后两者以错误退出1结束。这些顺序对应实际回调和逆序资源清理；不是手写 C 模型。O0/O2 两种机器构建都执行，正常程序还经真实 CLI 从四个文件解析 import、构建 O2 并运行。公开 helper 合同导出参数0次/1次两种完整 Return 轨迹。

## 实现及安全边界

Expand 从 opaque 返回的可见 Fn/FnMut/FnOnce 签名检查局部调用，不套用 F 的签名。FnMut 需要 var，FnOnce 标记消费且不回退。opaque impl 不授予 Copy，未实例化模板中的重复调用或移动后使用同样拒绝。克隆局部完整类型前先收费。

Types 在有界私有签名快照中确认 Call 的已解析工厂身份，然后通过局部/SSA 实际转移独立重建来源。Analyze 从已核工厂返回合同重新建立自己的有限来源表。Verify 不消费两者的结论，而由真实 Load/Store/TakeLocal/Borrow/Reborrow 构建转移图并用队列传播。三处均区分参数、工厂局部及未知来源，位置混合保持保守并集，不能用相同 ClosureId 或名称推导实际环境相同。

只把参数来源调用送入126的单一kind次数/exit重建。工厂局部调用使用自身调用种类，不计入 F；其完整资源、借用、移动、消费与清理仍走原有独立检查。工厂空外部 Hold/Call/writes、真实捕获与 Cleanup 先经 retained 核验，伪造标志不能授权。新的来源精度范围在 Types/Analyze/Verify 一致报 Unsupported；已经证实的错误 trace、伪造合同与缺权限继续语义拒绝。

局部任意 MakeClosure、额外外部借入捕获、多个 callable 参数、`&local` 后间接调用、helper 转发/递归及循环仍未开放。工厂 prepass 的未解析快照与直接/间接工厂依赖拒绝不变。生产仅五个私有模块；schema/wire17、ABI、后端布局、默认额度与规范未变。Types/Analyze 的来源传播是受工作额度约束的保守固定点，未声称所有输入的渐近复杂度改善。

## 验证、提交绑定与复用

最终生产/测试源码为 `8d08fbaf62550be112b1c8ab5d0c678ae6ccb582`。完整限定运行在 `efefc4e85860609ad05278ed161427029bf3e7b2`：77组driver（含12新增）、33组Verify helper、2组Expand generic，共112组通过；fmt与修改模块及新增测试的clippy `-D warnings`通过。

`d9af39a19d14e4916d9f0064cb6f513b44d22a32` 只给已有预算组补Expand边界，单独重跑该组、fmt/clippy通过，并执行真实CLI消费者。最终8d08相对d9af只将新增Analyze未知/混合/借入局部来源分支的诊断构造器改为Unsupported，同时把raw断言收紧到逐项类别；重新跑4组raw、四阶段预算、fmt/clippy全部通过。该更正没有改变已通过路径的求解、CFG、代码生成和清理，因此不再重复16次新源原生执行；复用范围明确，不把多次重跑相加冒充不同程序。

新增8组源测试实际保存23份CompileReport，其中16次O0/O2构建/执行、6个精确Expand最近反例，以及1次跨模块公共接口检查。手填公开 TypedModule 与 CandidatePlan 的4组测试包含2个正例和9个反例，分别执行Analyze、raw Verify与wire Verify，共33次观察；没有调用Types或Analyze生成该候选计划。覆盖同ID不同实例、参数别名错计、混合来源、借入局部、局部嵌套创建、缺返回合同、伪外部 envelope、伪 cleanup 与外部地址捕获。

最终边界：Expand604/603，Types294/293，Analyze1298/1297，Verify1641/1640。每阶段前者通过、后者ResourceLimit、恢复前者再通过。126节点额度诊断更正已同步；旧53bafad的原件与新更正分开保留。

最早20cc/8364候选的实际运行也保存；它们没有功能测试失败。完整原始命令、各次源码提交、阶段IR/接口、LLVM/对象/原生输出、raw/wire候选和精确诊断进入本单归档，逐文件SHA/长度/执行位读回复核。非作者审核和主树最终集成由root协调，作者不自行验收。

## Rust 四目标反思

安全方面，本轮提供同隐藏类型不同实例的独立来源核验与最近反例，范围仍是一个外部回调和有限自包含工厂。运行与内存方面只证明实际行为和清理，不据这些执行宣称相对Rust的时间或RSS同级。表达方面，现有普通工厂与泛型helper可以组合，不再要求把局部处理器移到调用者或手工展开调用；没有维护成本量化比较。编译成本方面新增工作已受逻辑额度约束，但没有新的冻结完整编译性能比较，也不声称快于Rust。该片是可复现的语言组合能力进展；完整四目标合取与产品级净突破仍未证明。

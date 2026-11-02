# 回调原值转交：251–256

已通过PR19合入实际main `9aeffc113971002fe788dc679c157a332b7dfcef`，整树等于最终候选 `19f8663c0d22953b5a24cd809bfa92cba9659b21`。拥有 Event → 公共 dispatch → 拥有 Reply 的246–250已通过PR18进入main；本批继续同一F实例的跨模块转交、局部移动与循环，恢复调用方声明模式，并补齐闭包正文内的拥有回调组合。schema/Wire31；公共合同见设计§9.4.2。

共同候选 b05a70b2ce834ff2ea93618035eec35ff136b622 的默认整仓3733通过、零失败/忽略。256消费者实际288次原生运行（原组256、补充32），其中214次分配注入、98次实际失败，最终live均为0；两组分别核对oracle与全部原件，运输往返通过。原嵌套源码另有O0/O2两次实际运行，不混入288。模块独审已过；这些不是完整语言或Rust产品比较的完成证据。

R1随后在测试代码的两处usize冗余转换处被Clippy拦下，总判仍Fail；修复只删除转换，限定测试1/1与该test Clippy已过，剩余正式门已在R3通过：reference3626/0、text-reference64/0及fmt/Clippy/迁移/运输全Pass；default3733为明示复用R1。最终两CI各自实际重跑3733/3626/64并核回完整归档。R0旧迁移断言及原归档脚本错误均保留真实Fail和修复链。原FnMut按值后返回i32的历史输入不在本批裸F返回范围，仍由B0承接；新FnMut返回F的只读调用反例已经按实际阶段和原因拒绝。

下一阶段集中完整Type/Syntax：跨模块Type与四类别Syntax传返、具名多provider、卫生与来源、反射及阶段依赖、#emit和普通代码同核。257–262已按冻结公共合同并行施工，尚未完成；长期外部回调生命周期随后。B0 Doing，完整1.4/ZUI NotSatisfied，产品及原生WindowsNotRun。

证据入口：evidence/integration/callable-forward-20261102/R1/，evidence/impl/256/final-R1/，evidence/reviews/252–256/。不同候选与default/reference、作者/独审/CI重放分别记账。

最终PR运行41455207131、41455207092均完整独立核回；primary artifact配额仍满，完整日志备用归档有效。main自动重放41461237336、41461237165已逐份独立核回，各默认3733/0、参考3626/0、文本64/0，精确checkout及全部4237项源码和原始归档一致；见[main回放验收](../evidence/integration/callable-forward-20261102/main-replay/ACCEPTANCE.json)。原始R1默认归档79,441项、R3参考归档72,116项全部核回后释放展开原件，压缩档保留。

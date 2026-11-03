# 336–339语言主线有限验收

共同受测源码为`874f071bb53e85727b5467a2769ada47b51edd98`。默认4196/0、reference4086/0，各422组、零忽略/过滤；双Clippy、fmt、LLVM文本65/0、迁移9/0、CI工具5/0及3/0、handoff工具29/0、计划/架构检查通过。精确命令、源码、工具链与非作者审查见[集成记录](validation/language-batch336-integration-20261104.json)。

| 切片 | 新增有限能力 | 独立复核与保留边界 |
| --- | --- | --- |
| 336 | trait默认体可返回真实词法模块内的普通静态函数值；完整签名和局部遮蔽保留。 | 实际参数/结果独立满足Copy，不能借同名泛型bound；需明确结果标注，无额外ABI/借用/拥有合同。一般限定路径及更多默认体组合仍待实现。 |
| 337 | life-only具体Direct函数/具体inherent方法的可变借用返回；声明上界与实际mutable输入权限分开核验。 | Analyze/Verify各自核实际来源；Verify另独立核完整raw指令和窗口。CopyT+mutable、HRTB/借用FP/storage/lending未开放；未用无life泛型的别名检查缺口保留。 |
| 338 | 普通大Copy Enum/ErrorUnion构造、tag/活动载荷、快照、调用/返回、fnptr及程序入口使用一致内存ABI和`.lc1`符号。 | 独审补齐实际名字/变体容器预付；a18、CGU/cache、入口两分支和C机器载荷消费者通过。大型closure/特殊helper/foreign仍有限。 |
| 339 | 静态`fn(&i32)->i32`实际读取父方共享对象，父body可同时合法读或写不相交对象，join后恢复使用。 | 两核分别核worker可达图、同时窗口及真实来源；helper返回的mutable捕获遗漏已修。调用/捕获前置必须实际保活原对象，不按参数号假定分离。Mutex/Atomic及完整异步仍缺。 |

336/338由非作者sol复核，337/339由Astra复核，另有独立339调用方来源对照。原失败、修复和最终提交分别绑定；作者、独审、全量重放计数不相加成覆盖率。所有生产源码和审查提交已进入本地main历史，GitHub PR为[32](https://github.com/zil-lang/zil/pull/32)，最终远端状态和恢复以备份收据为准。

release CLI从clean共同源码始末一致构建，SHA256为`f671b630b7c051ea0232833f523700ecee670eed87a2f410c8504003eb9af715`。实际新组合、原331组件及两份原CT输入共16次Clang/API×O0/O2执行通过；新组合验证默认fnptr→大型记录式Enum间接返回→mutable life→共享子线程，API实际多个CGU。既有清理消费者另16构建/128运行/56故障注入，原业务oracle保持，live allocations=0。

组合原源`tests/language-readiness/batch336/component.zil`仍因fnptr枚举结果缺少显式绑定类型而在Expand拒绝；只补`:Message`的独立文件`component-explicit.zil`已在上述四机器配置通过。原源不覆写、不计成功。root脚本两次入口/CGU预期错误保留并修正，最终runner按显式入口和实际配置判断；最终16次来自已提交的源路径。

小栈问题没有修复：合法`var f`大型借入Enum闭包在正常CLI四配置成功，但当前和保留旧C环境的2MiB Rust API均SIGABRT。最初`let f`输入本身存在FnMut调用错误，正常CLI在Types正确拒绝；其小栈诊断崩溃不能当作合法程序失败。API请求stop_after Verify不证明崩溃位于Verify，已更正为内部阶段未知、Codegen前。未增加编译器全局栈、默认预算或产品门。

原52输入当前仍27 Success/13 Unsupported/12 SafeReject，旧正控/保护反例无回退；原25源9/6/10。[历史334收据](validation/language-gaps-334.json)字节保持，新投影另存。完整缺口见[矩阵](LANGUAGE-REMAINING.md)。B0 Doing、完整语言/ZUI NotSatisfied、原生Windows和Rust同语义成本NotRun保持。

handoff的`current_acceptance`现在指向本批实际完整受测记录；仅这个当前指针变动，274条冻结历史及其引用/字节未改，220Unavailable/54Unreviewed不追认。legacy仍单列Fail。GitHub CI实际因账号支付/额度未启动、steps=0，本地通过不冒称远端CI通过。阶段远端源码及WIP分支、实际恢复说明见[备份](PROGRESS-BACKUP.md)。

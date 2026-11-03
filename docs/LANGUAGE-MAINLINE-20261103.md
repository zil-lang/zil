# life、trait、结构化线程与大型Copy值集成

本轮从PR28后的主线继续，保留全部施工树、未提交源码和旧失败，不运行reset/clean。完整编译器候选022303df8本地默认4097/0、reference3988/0，各422组、零忽略/过滤；文本接口65/0、迁移9/0、两配置全目标Clippy通过。最终profile候选ee0559611只增加规范措辞澄清、审核记录和一个既有目标内的独立测试，编译器源/Cargo逐字不变；该完整目标两配置各19/0、定向Clippy通过。重放不叠加到全仓计数。精确来源和复用说明见[验收JSON](validation/mainline-integration-20261103.json)。

322保留显式共享life到真实来源上界，Analyze/Verify独立从CFG核验。原始life源、具体Self、slice、CT重建、接口与缓存已有限验证；独审发现scalar增量复用遗漏Core life，实际12次raw声明绕过保留为失败，key域与完整声明身份、hit前重新验证及计费修复后关闭。源入口原本即拒绝，不声称该反例证明源级UB。

323贯通具体泛型trait实例、同Self多实参和inline/where静态调用，保留模块/名义/phantom/有序实参身份与未用定义检查。独审发现别名、模块、静态收集、发布预算链重置和重复扫描漏收费，已合并为真实剩余额度并逐访问预付；组合N3797/851/5及源导出的求和/耗尽下游控制通过。原局部成绩不代替修后累计结论。

324将至少4096物理字节的有限Copy聚合转为每Value独立栈快照，有界复制保持Copy、顺序、布局及索引trap。原a18输入262144字节与hash保持，Clang/API×O0/O2实际四组成功，无fallback。最初默认driver目标未启用API而误记四组已撤回；恢复后显式feature及非作者复验成立。一般巨型按值ABI和特殊SSA桥接仍未解决，新的by-value探针未执行。

325/328首次建立实际Linux x64 pthread创建/join纵切：纯标量i32子线程worker、父线程具体body、真实发布/完成HB、创建失败准确Drop、join失败abort。最终既有目标包含life+trait组合和Direct callable工厂，默认API配置28次真实线程执行；机器模型执行单列，附加独立源探针已保存到tests/fixtures/thread328-review。父闭包/参数helper入口仍Unsupported；返回callable的普通Direct工厂不因结果合同变成helper入口，已按独审发现明确措辞并保留原源四组合和Once反例。

326的CurrentPass只表示当前交接记录完整。274份冻结旧记录仍为220Unavailable/54Unreviewed；legacy-audit保留1924错误，不追认旧证据。327交付非作者323审核及真实源近邻，转328施工后不自审328；其他片分别由非作者审核。父B0仍Doing，完整语言/全部ZUI、FP/HRTB/lending、一般trait、完整async/SABI/Mutex/Atomic、一般大值ABI和Rust产品比较仍未完成。

发布构建的既有208消费者16次构建/128次机器观察（56次注入）通过，[精简观察记录](validation/mainline-consumer208-20261103.json)保留真实输出与输入身份。GitHub PR29工作流未启动的支付/额度问题与本地验证分开，不能称远端CI通过。源码分阶段推到独立进度及施工分支，逐SHA核验；恢复方法见[备份说明](PROGRESS-BACKUP.md)。

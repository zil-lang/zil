# 203 双声明 State 的五模块消费者

本例使用两份完整的 `State<Tag,F,G>` 元素。F 来自可重复 FnMut 工厂（Ticket + ByteBuffer），G 来自另一个实际 Fn 工厂（Ticket），但 State 声明将它限制为 FnOnce。两元素各自拥有独立 marker、环境和清理责任。

`main` 经过公开普通 T 的 identity/seed/push/reserve/pop/观察 helper；循环在 Store 实际字段窗口上按 ACCA 顺序调用 F。借入期间不调用 G；pop 后将完整 State 交给单 State 参数的 finish，`P`、G 输出、G 立即关闭、`Q` 区分声明消费是否发生，之后仍可调用剩余 F。两个 State 没有被伪装为两个同步 callback 参数。

输入 0/1/2/3/7 覆盖零轮、一轮、continue、多个轮次、break；6 覆盖容量满 push 的完整错误 payload 清理；9/EOF 是入口拒绝。每个 O0/O2 运行包括普通执行、独立分配 probe、每个实际申请失败点及最后申请之后的成功控制。runner 固定源码/编译器身份并核一个 Verify、五模块；Linux 与 Win64/Wine 分列。原生 Windows 不据 Wine 升级。

运行入口为 `python3 tools/language/203/consumer.py --help`。必须传成功且干净的冻结 CLI 构建收据、实际 SHA-256 和新输出目录。源码包不包含机器产物；证据后续单独封装。

当前作者源已完成，尚未宣称该完整消费者通过：`expand01` 曾发现 seed 返回拥有 Store 时漏写 give，原输入已保存；已按既有责任语义改为 `return give items`。`expand02` 准确暴露普通 main 中 callable State 生产者的完整 T 依赖不能提前 eager 实例化，正在补对应合同及实现。五模块正例保持成功预期，不把这条失败改写为支持边界或复制 202 成绩。

正式a355规范及Expand值推导补片后，`inferred02` 已实际通过五模块Expand。完整生产组合的Types/native/fault仍待运行；早期失败不重标。

Types作者 source09 真实经过 main identity/seed 后，在跨模块 `a.active` 处 E-FIELD-PRIVATE；这是消费者首次把字段访问移到main后遗漏public的源码错误。已只把State.active声明为公开API；once与tag仍通过既定State函数使用，未放宽编译器隐私门。原失败由Types作者保存。

CLI05阶段实测：clean `5e101737eea87074fe0531a2ab9371efc44062d4` 与匹配CLI在Linux和Win64/Wine分别完成128次执行，各16普通、112分配probe、88真实分配失败；每个平台O0/O2各一份完整五模块构建、Verify各1次。预期保持原样，所有probe最终live=0，Win64同时bad_operations=0。原生Windows仍NotRun。首次Linux启动仅因同步临时目录被clean-source门拒绝，编译/执行均0；目录自行消失后以同源新目录执行，原失败保留。

这组成绩只绑定CLI05，不计最终203验收：随后独审的未用定义While条件别名近邻另有修复，最终消费者将用新冻结CLI按相同源和oracle另行执行，原ZIP不覆盖或倒标。阶段小收据是`evidence/impl/203/consumer/CLI05_RESULT.json`，独立文本证据包约1.47MB，无执行产物。

CLI06最终作者重放已实际完成：clean消费者`7afbb6493cde414b042dcd0dfc67fc7d2579d63d`，生产与CLI来源`ee0ebb3f78d942a58c8cf3576654e937c5e35629`字节一致；Linux03和Wine02各128次执行、各88次真实分配失败全部符合原oracle。与CLI05的五源及oracle完全相同；两套成绩分开保存，不累计冒充一套独审。最终小收据是`evidence/impl/203/consumer/RESULT.json`，完整说明在`changes/203-consumer.md`，新文本证据包约1.47MB。原生Windows、完整语言和产品状态保持原界限。

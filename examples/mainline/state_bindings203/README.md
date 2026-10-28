# 203 双声明 State 的五模块消费者

本例使用两份完整的 `State<Tag,F,G>` 元素。F 来自可重复 FnMut 工厂（Ticket + ByteBuffer），G 来自另一个实际 Fn 工厂（Ticket），但 State 声明将它限制为 FnOnce。两元素各自拥有独立 marker、环境和清理责任。

`main` 经过公开普通 T 的 identity/seed/push/reserve/pop/观察 helper；循环在 Store 实际字段窗口上按 ACCA 顺序调用 F。借入期间不调用 G；pop 后将完整 State 交给单 State 参数的 finish，`P`、G 输出、G 立即关闭、`Q` 区分声明消费是否发生，之后仍可调用剩余 F。两个 State 没有被伪装为两个同步 callback 参数。

输入 0/1/2/3/7 覆盖零轮、一轮、continue、多个轮次、break；6 覆盖容量满 push 的完整错误 payload 清理；9/EOF 是入口拒绝。每个 O0/O2 运行包括普通执行、独立分配 probe、每个实际申请失败点及最后申请之后的成功控制。runner 固定源码/编译器身份并核一个 Verify、五模块；Linux 与 Win64/Wine 分列。原生 Windows 不据 Wine 升级。

运行入口为 `python3 tools/language/203/consumer.py --help`。必须传成功且干净的冻结 CLI 构建收据、实际 SHA-256 和新输出目录。源码包不包含机器产物；证据后续单独封装。

当前作者源已完成，尚未宣称该完整消费者通过：`expand01` 曾发现 seed 返回拥有 Store 时漏写 give，原输入已保存；已按既有责任语义改为 `return give items`。`expand02` 准确暴露普通 main 中 callable State 生产者的完整 T 依赖不能提前 eager 实例化，正在补对应合同及实现。五模块正例保持成功预期，不把这条失败改写为支持边界或复制 202 成绩。

正式a355规范及Expand值推导补片后，`inferred02` 已实际通过五模块Expand。完整生产组合的Types/native/fault仍待运行；早期失败不重标。

Types作者 source09 真实经过 main identity/seed 后，在跨模块 `a.active` 处 E-FIELD-PRIVATE；这是消费者首次把字段访问移到main后遗漏public的源码错误。已只把State.active声明为公开API；once与tag仍通过既定State函数使用，未放宽编译器隐私门。原失败由Types作者保存。

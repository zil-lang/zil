# 204 叶 Store 拥有捕获：五模块消费者

两份 `State<Tag,F,G>` 保存实际不同环境的 FnMut 与 FnOnce；各 State 经普通 StoreElement 模板完成 identity、seed、push、reserve、pop。F 捕获完整 Bag，其 Store 的叶 Item 含直接 Ticket 资源；G 直接捕获 Store，在调用内先 reserve，再 give 到局部后 reserve，最后仅由局部清理元素与分配。另一份 Fn 捕获子 Closure，子 Closure 拥有空 Store，两次实际调用保持独立的可重复入口。

循环借入外层 Store 的实际 F 字段，按 ACCA 顺序增长两份相同类型、不同值的内层 Store；0/1/2/3/7 覆盖未调用、一次、continue、多轮与 break。6 覆盖外层满容量 push 返回完整 State 错误值，9/EOF 为输入拒绝。pop 后才消费 G，随后仍可调用同 State 的 F。

G 的 `P D J 4 d Q` 标记分别来自 caller、body、give 后、局部 Store 叶资源、剩余环境和 caller 后续。reserve 在 give 前后各有真实申请失败点；对应失败必须保留准确的清理顺序、分配数及最终 live=0。F 的内层元素 `1`/`3` 和环境 `a`/`c`、外层 `X`/`Y` 标记区分两个同型实际值及其持有窗口。

新 runner 的 oracle 在执行前按源码和 x64 布局推导：内层 Item 步长 8、F/G 环境各 40、外层 State 步长 104。C probe 只拦截/观察 allocator，不能生成 oracle。计划每平台 O0/O2 共 148 次实际执行、108 次真实失败注入；这只是计划，尚无本消费者 Pass 成绩。必须绑定干净编译器构建收据、精确源码字节和实际 CLI SHA 后执行，不能继承 203 的结果。

运行：`python3 tools/language/204/consumer.py --help`。Linux 与 Win64/Wine 分列；原生 Windows、完整 1.4 与产品状态分别保持 NotRun、NotSatisfied、NotRun。源码包不含执行产物；证据后续单独打包。

CLI05 作者实际结果：同一 clean 消费者 `7bf69fe1`、同一冻结编译器 `7abbbac1`，Linux01 与 Wine02 各 148 次执行、108 次真实申请失败全部符合上述原 oracle，最终活跃分配为零；Wine bad_operations 为零。每平台 O0/O2 各构建一次、Verify 各 1。Wine01 的 IPC 启动失败另列保留，没有改源或预期来取得成功。详细身份与结果见 `changes/204-consumer.md` 及具名小收据。

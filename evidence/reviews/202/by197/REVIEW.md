# 202 非作者独立审核

裁决：Pass，限设计 §8.2.1 的普通 StoreElement 模板完整类型绑定及其已登记消费者。审核者为 /root/impl197；202 的 Core/CT/driver、Expand、Types 生产实现分别由 root、bundle_sync、roles196_frontend 完成，本审核者未修改 202 生产代码。

## 冻结身份

- 接受候选：b73ba8a764ee605c1ba8e3a651eb2c6baece8197。
- 实际独立执行源码：988da100191c163ed5afcd87e3186189cacaf468，执行开始时 Git clean。
- 实际消费者编译器源码：7698369cc8c4b6f50d5971ec975040417c92f841；CLI SHA256 为 0f41b91e1c34a6831b3e9a882642eef0df6c5d5b30520f76a7b9c8a77e4ecf2b。
- 设计 SHA256：bb8821ef1c88f2753d12dd668fef0638da6dc849f541c55767051389f6387aa3；normative hash：10bd65c22970a3479f436ceab8baf5f58aa014ff94f780ec0896e160fd5c4bb5；schema/Wire 26。

SOURCE.json 逐文件登记 177 个保守生产源码/manifest 路径、15 个审核输入及规范元数据，共 200 个对象。实际测试源码与接受候选在这些对象上逐字相同；全树仅多出 collector 工具。CLI 源码与实际执行树的生产路径也完全相同。此为只读等价核对，未声称重新在 b73 上执行独审。

## 实际执行及证据等级

最终 source05：19 组独立语言测试与 7 组 CT 测试全部通过。其中 CT 有 3 组审核者自加控制、4 组经测试适配器编入的既有/新增单元，分项保留。44 次源编译中，34 次完成 Verify 后在 Linux 原生 O0/O2 运行；另有 5 次 Expand SafeReject、1 次 Expand Unsupported、3 次 Verify 借用冲突 SafeReject、1 次 Verify 零布局 Unsupported，均为预期边界。编译预检、源前门拒绝与独立 Verify 不混记。

最终冻结的 35 份 raw Core 输入各经 single-module 和 program 两条公开 Types 入口：6 个正常控制、28 个 SafeReject/Unsupported 对手输入及 1 个 ResourceLimit 深类型输入。完整元数据、错目标、裸/嵌套 TypeApply、跨定义自由名字、不同名义/phantom 家族、未完成递归实例均有实际原始输入和结果。另保留 TypedIR/raw/Wire 控制、零布局约束拒绝以及 Wire 25 头部拒绝；预算搜索的重复调用不作为新增独立场景计数。

最终 consumer-linux02：同一冻结 CLI 编译原五模块源，O0/O2 共 126 次原生执行通过，其中 16 次普通执行、110 次探针执行、86 次真实分配失败。所有探针终态 live=0；最大峰值为 3 个分配、269 字节。预期来自逐次 ABBA 调用、Store 容量、两个独立回调缓冲区、前缀清理及 pop 后清理的独立推导，未从观测值反填。覆盖 continue、break、空回合、增长失败、满容量 push 返回完整错误载荷、pop 后再次通过 helper 调用、最后一次分配之后的故障序号，以及非法输入/EOF。5 份源文件在执行前逐字快照。

本审核者未运行 Wine、Native Windows、产品或正式全仓。root 的全仓和另一作者的 Wine 运行属于分别绑定其实际提交的证据，不计入上述独审数字。

## 审核发现及修复复验

1. 全局名义 T 与局部 callable T 碰撞时，正确源在 Types 之前形成错误的名义占位。仅删除无关全局声明的邻居真实运行，说明这不是合法源的资格拒绝。Expand 504141f1 复用原 AST 卫生处理，并继续使用同一剩余预算；原正确源在最终候选显式/推断形式均 O0/O2 成功。
2. 未调用 raw identity 声明捕获另一函数的 T、以及未调用模板体把 TypeApply 指向 putchar，原 single/program 都接受。原先 error-union/factory 近邻的较早拒绝不足以证明此门，已另外隔离并保留。Types 0098acdc 对每个普通模板做自身符号检查；精确原输入及未获字段/直接调用能力的邻居最终均拒绝。这是 Core 声明/元数据合同漏检，未把未发射的函数体描述为已执行内存安全漏洞。
3. 五模块消费者的普通 pop<T>()? 后再借回调字段给 helper，真实停于 Types 实例计划。ba4e4d07 分别保留错误/成功两侧，Try 只选择成功侧；带注解的局部和 ascription 也保留同一事实。普通联合整体没有被当成 callback 环境，错误侧 T!void 没有伪装成成功 Package。
4. 独立最小 pop-to-helper 源另外触发 Expand：调用者整个 void!i32 被误用于约束具体 i32 helper 返回。f200d5be 分开整体联合与实际载荷上下文，保留最终实际返回类型检查。最终独立 Left/Right、真实字段/phantom 两种原小例均 O0/O2 成功。

历史 helpers01 两份源始终保持 1655/1657 字节，SHA256 分别为 101580f8d78e661011d3b73f731c4f2f2ae376a408c1cc02c8aa072d4f627bf9、d8fd10675872542e99533ee23957db2c3150760a2f71ea8ff28150ad6c3a7a55。没有通过缩短或重写两份原输入获得成功。

## 完整 T、资格及效应边界

实例键按实际模板 ModuleId/NodeId 与完整实际 Type 核对，不按 ClosureId 或布局合并。独立源覆盖同一环境下不同普通名义族、phantom 类型、声明次序、同短名的两个模块，以及嵌套 T/U 与结果上下文。错误实际 TypeApply 不能选择另一个家族。未完成实例不能用临时 FunctionId 打破循环。

即使 identity<T:StoreElement> 完全没有 Store 操作，实际绑定仍产生精确对应 T 的 canonical OwnedStore<T> ResourceDefinition。独审逐值核对应，并确认没有 Store 运行时指令；零大小真实环境经 Verify 拒绝，捕获 i32 的非零邻居原生成功。此约束不创建运行时 Store 或责任，也不是授信位。任意 raw 程序删除未使用约束后仍可能是另一份安全 IR；审核不把源语义等价当作 Verify 的承诺。

经 shifted 的实际第 1 个参数、再经 middle/reserve 的结构写，保留了真实 Desc 效应；窗口活跃期间 reserve(4)、无增长 reserve(2)、失败 reserve(-1) 均实际到 Verify 并以 E-BORROW-CONFLICT 拒绝。前门未准入不算借用证明。CT 的纯函数控制实际经 VerifyCT/VM 得到 I32(7)，新模板/TypeApply 的直接、嵌套及 wrapper 执行均禁止；超大类型实参在准备 CT 查询之前耗尽预算，观察器调用次数为零。

## 预算及历史记录

最终新完整类型控制的 N 为 Expand 10837、Types 46169、Analyze 77259、Verify 174758；资格承载且无 Store 指令的 identity 控制为 Types 6148、Analyze 10759、Verify 13500。各阶段 N 成功、N−1 ResourceLimit、fresh N 恢复；后者 Types/Analyze 输出与默认限额结果逐值一致。

在单独保留的 e08e4c61 旧路径回归中，State Expand 1731、ordinary eager Store Expand 2255，以及上一阶段同一回调 Store 源的 2871/5241/16091/29902 四阶段 N 均保持并通过 N−1/fresh 检查。后续生产改动及相邻作者控制按各自提交登记；不将这几份样本扩大为所有旧输入的成本证明。

source01 的 Item<T,T> 是审核夹具改名笔误，source02 修正后才成为真实全局 T 碰撞反例；unused_raw01 是审核 Rust 辅助代码把不同结果类型放入同一数组的编译错误。两者均完整保留，未计为语言实现问题。早期 no-run、CT 回合、限定成功回合、未用元数据真实接受、消费者 Types 失败及 pop_helper Expand 失败都保留实际源码/规范身份，不把多轮数字相加成一次全仓通过。

原始命令、输入、Core/Typed/candidate/Wire、stdout/stderr、失败过程和最终 oracle 存于独立 202-review-by197-evidence.zip。执行文件及目标文件只保留长度与 SHA256。REVIEW.json、SOURCE.json、ARCHIVE.json 与三个只读迁移收据留在 Git。当前有限 202 审核无未决项；完整 1.4/B0、下一阶段独立 State binder、Native Windows 和产品验收均不由此宣布完成。

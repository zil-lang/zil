# 同步借入输入与拥有回调结果

普通具体 Fn、FnMut、FnOnce 可以以已有有限 Copy、Copy/slice 地址或完整封闭普通拥有数据的地址作为输入，返回完整封闭普通 Product、Enum、ErrorUnion 拥有数据。例如 `FnMut(&Item)->Reply` 中 `Reply` 可管理自己的 ByteBuffer。地址参数沿现有共享/独占窗口，不转交输入责任；同列 Copy 与完整封闭拥有数据参数保持各自原传递和责任规则，原纯拥有签名继续独立准入。

返回数据不得包含地址、Desc、具体或 opaque 闭包依赖；借入输入不因此成为 lending 返回或环境捕获权限。输入和返回值可以同型，但不能凭同型合并实例、移动输入字段或导出 private Created 资格。拥有结果按原 Incoming/Unknown 边界接收；显式 create/give 合同继续独立核实。

唯一 F 的普通自由 helper 可以声明该签名：Fn 使用共享或拥有 F，FnMut 使用独占 F，FnOnce 使用拥有 F。每个声明包含未使用模板都核完整参数/返回资格及声明调用模式；具体环境的更强模式不能替代声明约束。调用次数、顺序、实际环境、输入 loan、父窗口、责任及每个退出的清理由原 CFG、roles/traces 与独立内核核实。

调用先准备 callee 和从左到右实参。返回先完整准备拥有结果，再清理局部和剩余环境。结果责任与输入借用分别结算，已转入结果的叶不重复销毁。失败只清理已取得的真实责任。Fn/FnMut 可重复调用并产生不同拥有结果；FnOnce 仍只保证至多一次。

复用现有 ClosureDefinition、CallClosure 族、Package/Enum/ErrorUnion 及真实借用/清理指令，不增加公共字段或 Wire/ABI。Types、Analyze、Verify 分别核实际类型树；Verify 不消费前两核的准入结论。新树遍历预费，既有 Copy/slice、纯拥有 FnOnce、capture/State/Store/FFI 边界和默认限额保持。

本范围不授予多 F、普通 T、借入结果、opaque factory、callback 容器、thin/FFI、CT、异步或额外捕获资格；这些位置仍各用原独立资格核。跨模块真实 Source、独立 raw/Wire 最近反例、双后端 O0/O2、逐分配点失败清理及新旧预算边界是验收对象，有限范围不代表完整语言或产品门通过。

同步存储借入程序中的实际入口、普通本地 ByteBuffer 字段视图与完整消费者由[存储借入组合合同](stored-owned-callback-pipeline.md)限定，输入 loan 和结果责任继续分别验证。

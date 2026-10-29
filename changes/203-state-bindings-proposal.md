# 203：独立 State 声明绑定与有限双环境 Store

状态：由集成方基于已接受 main `94515c1c78c77c8bb9eb2300cdadce714fed7e19` 发布施工合同。实现、独立审核和机器成绩均待本单实际执行；202 的既有证据不改写。规范以新增 §8.2.2 和 interfaces 的 state_bindings27 为准。

## 问题与选定表示

202 已保留完整普通 T，但同一拥有 State 内的 F/G 仍缺独立声明绑定。一个工厂可产生两个不同拥有值，实际类型同为 C，而函数分别声明 F:Fn 和 G:FnOnce。不能以 C 的较强能力替代 G 的源级限制，也不能把一个 State 实参伪装成两个同步 callback 参数。

新增 `CoreFunction.state_parameters: Vec<CoreStateParameter>`，与已有 direct `callable_parameters` 分开：

```rust
pub struct CoreStateParameter {
    pub index: u32,
    pub bindings: Vec<CoreStateBinding>,
}
pub struct CoreStateBinding {
    pub generic_index: u32,
    pub generic_name: String,
    pub kind: CallableKind,
    pub parameters: Vec<Type>,
    pub result: Type,
    pub fields: Vec<u32>,
}
```

首片非空 State 表恰好一项、两个 ordered callable bindings；每项分别占用一个直接 callback 字段，总计两个。State 参数求值/拥有转交一次，真实参数号不是字段号。绑定按函数 callable 声明 index 0/1 排序；selectors 按真实字段索引排序、非空、无交叠、无遗漏，并与符号 Named binder 完全一致。F/G 构造器两个 direct 输入仍使用原表，拥有结果单独分类；不伪造联合 helper 事件合同。

新 State 操作不混合普通 retained type_parameters、direct callable_parameters、callable_return、foreign 或 receiver；可复用 197 已有限特化的普通 Copy T。旧同 F State 新表为空，既有模式、lending 和费用保持。新双 F/G 的两个 bound 结果先限 Copy。

## 身份、来源和资格

实际实例保留完整 T、真实定义 ModuleId/NodeId 和两个实际 ClosureId。绑定表顺序与名义字段顺序分开：具体名义 family 的 callback 向量按固定字段声明顺序，通过 selectors 从函数绑定取值；一次同时替换 F/G，不能连续剥除 `:closure` 后缀。同 `[C,C]` 保留两槽、两值、两份责任；普通 phantom T 和跨模块名义身份不得丢失。

Types 在擦除前构造只属于当前函数实例的有界声明来源。Name、move、let、字段取走、完整注解、普通 `identity<T>`、TypeApply、Store、错误联合两侧及 `?` 成功侧都保留来源。它只选择声明允许的调用模式，不制造拥有值或 loan。字段取出的 G 及 direct helper 实参继续核其声明能力；借入 G:FnOnce 拒绝，拥有 pop 后可消费。重复 NodeId 对应冲突来源应拒绝；来源缺失不能回退到实际 C 的较强 kind。进入当前片不能保真的 capture、opaque 返回、函数指针等擦除通道明确 Unsupported。跨函数进入另一份合法 F/G 声明后，使用被调用者自己的表；模式不是全局 Package 类型属性。

同批扩展有限 StoreElement：仍总计一或两个直接 callback 字段，允许最多两个不同的具体、自含 ClosureId，其他字段保持有限 Copy/直接 Managed。新异环境路径的两个实际 callable 结果也须 Copy；旧单根/同根 lending 保持。符号 F/G 依据声明即满足有限资格，不采用“恰好本实例 C=C 才合法”的延迟条件特化。

Types、Analyze、Verify 各自检查两个捕获根、未用资源定义、完整元素非零布局与既有捕获边界。Verify 从真实字段、capture DAG、live flags、对齐独立重建实际布局。类型布局可复用已检查子图；运行时字段/元素/值的责任不能去重。直接 Store、含 Store Package、递归拥有和未开放嵌套 callback Package capture 保持关闭。push/pop/reserve、窗口、父责任、callee 先于参数和失败清理沿既有规则执行。

## 消费者与边界

公共 schema/Wire/fingerprint 统一 27。Core 本来不属于当前 Typed wire payload，不因新增来源表编造运行时 opcode。CT runtime normalization 完整保存且先计费；CT 执行 State 模板及 wrapper 隐藏调用明确 E-CT-FORBIDDEN。CLI/IDE 共用生产 stages。raw Core 元数据预费后核完整头、签名、索引、字段映射和未调用定义；raw TypedIR 只有具体实际类型，不能被报告为恢复了弱源 bound 的证明。

新遍历、名字、类型、selectors、实例比较/复制、队列、双根图均先计费。旧无新表/单根路径不额外全树扫描；旧预算和新 N/N−1/fresh 分列。默认额度与产品门保持。

## 分工与验收

root：正式规范/派工、公共 Core/schema、机械迁移、CT/driver/IDE、Analyze/Verify 双根资格与布局、最终集成。frontend：Types 完整绑定、声明来源、实例/推断/效果与有限资格。bundle_sync：Expand 独立 State 表、同步替换、family 与符号 StoreElement 资格、真实多模块消费者。impl197：独立源码、raw/Wire、预算及机器审核，不改生产。各自在203分支和worktree；root 独占正式全仓。

验收首先逐字运行 preflight203 的四个输入，记录真实身份；同 C 两模式、两个不同 C、声明/字段逆序、完整 phantom T、identity/TypeApply/Store/错误两侧、借入拒绝与拥有成功、错误第二捕获根/循环、未用定义、CT/raw 元数据、机器布局/多个元素/分配故障都需对照。非作者必须绑定精确候选。阶段交付采用标准 bundle；新执行文件与大证据不进源码 Git，证据单独小包。完整1.4、产品、原生Windows按实际状态记录，不因本合同发布升级。

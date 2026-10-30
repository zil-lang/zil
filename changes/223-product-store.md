# 223 完整Product作为Store元素

基线 a8e8831a9e28ac2ec83be6eac740a8f04a00af7d。承接206、214–217；本记录是施工合同，不是完成证据。

只开放§8.1.1完整有限Product作为Store元素：递归名义Package child、有限Copy伴随字段、直接Managed ByteBuffer或有限Copy头用户资源叶，至少一个拥有叶、非零实际布局。整个元素进入/退出初始化前缀，push失败归还完整元素，pop先移除尾部责任，reserve失败不改变前缀。Store既有协议需要的Product!Unit与Unit!Product壳同步贯通tag/完整责任/错误清理，不能把准入推广为任意嵌套Sum。

递归元素按每层字段声明序真实析构；类型大小/对齐/GEP/flags由具体布局重建。共享元素窗口只读；reserve/push/pop/give/drop/父壳移动与活跃窗口的冲突继续核验，最后使用后释放限制。Types、Analyze、Verify分别从实际类型/来源/指令构建资格，不调用另一层的安全证明。未使用定义、raw/Wire入口与捕获图同样核验。布局内联，不增加隐式Box/RC/锁/全局表。

Product内Store、递归Store、Closure元素树、拥有数组、任意Sum、长期借入/异步不随此片开放。普通&Product由224单独建资格；Store窗口准入不能反向授予普通借入权限。默认限额/产品门不改；旧路径费用保持，新工作真实记录N/N-1/fresh。原217 product-store-element逐字晋升；product-owning-store/nested-store-element分别保留边界。原生O0/O2必须覆盖不同元素、pop、空Store、逐分配点失败与递归清理顺序。

## 正式CI旧负例诊断迁移（2026-10-30）

首轮正式CI `41082643919` 在默认和参考模式均保留了原 `impl201_store::fields_require_direct_finite_shape_and_complete_resources` 的失败。原 raw `Nested { Managed ByteBuffer, Closure0 }` 不作任何修改；223移除普通Product Store的 blanket拒绝后，它进入实际Product子字段资格检查，仍由 Analyze 拒绝，但类别/消息从 `Unsupported / ordinary Package Product is not a Store element profile` 变为 `SafeReject / E-ANALYZE-IR / ordinary Product has an unsupported owning or borrowed child`。仅迁移该Nested分支的精确拒绝预期，其余隐藏Address、Array、Store和所有资源/环境近邻保持。没有新增接受行为或修改生产实现。

CI两种workspace命令追加 `--no-fail-fast`，继续运行各test binary以完整收集失败；任何测试失败仍使cargo及相应gate非零。测试集合、限额、产品门和后续验收要求不变。

第二轮 `41084362447` 的完整默认workspace为2858/3/0，保留原失败。214原 `fn main(){let items:Store<Root>=store_new()}` 的完整Product已经属于223准入范围；保持原源码及SourceUnit身份，将旧Expand/Types Unsupported断言迁移到Types成功并核真实Root Store资源定义。这份旧探针仍只证明Types，不冒称Verify或native。


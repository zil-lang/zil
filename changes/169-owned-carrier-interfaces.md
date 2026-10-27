# 169：有限拥有载体的共同接口

## 为什么

166–168分别把非Copy值放入错误联合、固定数组与闭包环境。解除前端Copy限制无法说明隐藏责任归谁、在哪一条路径取走、如何清理。当前raw schema/wire为18；本轮增加可独立核验的显式操作并升至19，旧版本按现有固定版本策略拒绝。

## 接口与范围

- 166：`TakeErrorUnion { value, is_error }`（70）消费一个完整拥有联合并取选中分支；即使选中分支是Copy，整个联合也只消费一次。`DropErrorUnion { local }`（71）按实际tag清理活动Package载荷并结算本地联合责任。现有MakeErrorUnion对非Copy载荷实际消费；ReadErrorUnion仅接受Copy联合。第一段分支限定现有Copy或完整有限Package，至少一侧Package；不猜测E!Resource内部模式。类型、tag路径、完整性与唯一责任由Analyze/Verify分别重建。机器沿用内联tag及两个固定载荷槽，非活动槽不产生责任。
- 167：沿用`Type::Array`，新增MakeOwnedArray（72）、ProjectOwnedArrayField（73）、TakeOwnedArrayElement（74）、DropOwnedArrayElement（75）、StoreOwnedArrayElement（76）、DropOwnedArray（77）。第一段是同质非空Package元素，静态索引，完整SSA/ABI、局部逐元素初始化标志。Copy字段读取值，Managed字段投影Desc；字段来源依赖真实数组元素，必要时保守提升整壳。移动/写入/清理与活动窗口冲突不能漏检。构造准备失败按逆成功顺序清理，完整数组按索引升序、元素内按既有声明序清理。
- 168：工厂转发和同工厂重复构造复用实际Call/TakeLocal及已核CallableReturnContract。源面opaque项按定义的Signature身份稳定缓存，不能合并不同定义身份或不同closure表达式。环境再捕获所需借入指令单独登记；只允许结构、责任与外借闭合的有限环境，不引入隐式堆盒或RC。原始Closure布局图须有预算及循环拒绝，不能依赖源码构造顺序假定无环。

以上均是现行规范§2、§3、§7–9、§15.1的有限实施，不新增源语法或默认预算。动态容器、一般头地址、外部保持/取消和完整dyn仍各需实际闭环，不由这批接口自动开放。

## 施工与核验

root单写IR枚举、opcode、序列化、指纹、版本与共同登记。接口按公共版本基础和每条语义线分成独立提交，施工者仅取本线接口并补齐自己的全部消费方与显式CT拒绝，最后串行合并完整集合。Verify不调用Types/Analyze求证函数。Wire往返、字段指纹变化、未知opcode/旧版本拒绝先在IR crate核对；语言安全和机器执行由各纵向实现及非作者审核另行证明。

当前`data/normative-inputs.json`继承旧摘要，与真实规范及接口文件不一致。本轮只在新的共同接口提交重新记录当前文件的真实摘要，并更新166–169的当前派工快照；不重写任何旧提交或旧审核，不把历史handoff缺失变成Pass。

## 新交付规则

源码包从Git受控文件导出，保留源码、测试夹具、配置、锁、文档与当前任务；排除evidence、执行产物、构建缓存及Git数据库。审计收据与恢复历史如需提供另包，优先小于50MB，不收录可重建的大型阶段产物；旧161–165交付不重打。

## 确认补充：闭包借入与同质回调元素

168确认 `BorrowCapture { index:u32, mutable:bool }` 使用opcode78；只借具体Closure真实环境地址，不转移责任。167确认 `BorrowOwnedArrayElement { local:LocalId,index:u32,mutable:bool }` 使用opcode79；只借静态槽内具体Closure，数组/元素仍负清理责任。对活动借用的移出、替换、整数组移动/清理必须拒绝。167先验证Package元素，再组合168递归清理支持同一工厂的具体回调元素；接口登记本身不代表该组合已验收。

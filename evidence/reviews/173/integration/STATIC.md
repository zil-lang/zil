# 173 独立集成审阅：静态预期

受审候选 `02cce89206e74cb5aa0050faf4885973288b9383`，实现作者 `/root`，本审阅者 `/root/dynamic_store`。只审 root 的合并连接、普通 owned-error signature 上下文补项、原 163 声明探针期望迁移、组合消费者和探针。172 实现本人编写，本审阅不审批该实现；172 作者结果亦不替代本次执行。

## 静态结果

- Analyze local_value 保留 171 的 shared-resource-head address 与 172 的 Store carrier 两侧准入；没有把两种形状混为同型。
- Graph bind 合并的是被计费的约束类型分类：原有 Copy/Package/array 与两侧新约束均保留；它不取代 Verify 的 capture/ABI 授权。
- Verify closures 合并保留 shared resource-head address、Store、170 array-field Package 等显式捕获禁入。
- context_signature 只补既有 errors::owned 外形；仍对签名完整 validate_type、callable/returned contract 预收费后才 clone。源 aggregate 错误载荷准入和 Verify errors/closures 检查没有删除。
- 163 变更源码字符串逐字不变，只将原已实现的声明探针的预期改为 Success/Verify=1；它没有被拿来替代 171 原生调用证据。

## 独立推导的析构与分配预期

main 将 Panel 初始 A/B 数组取出，取走 A，填入 C，再把 C/B 还给 Panel 并整体转交 saved。因此 saved 持有 Z/C/B，old 持有 A。Store 的 D.. 前缀被尾取，按倒序立即销毁；H 是后构造的保留回调资源，先于 saved 和 old 销毁。正常输出 reverse(D..count) + HZCBA。count=0 无分配；count>0 仅 Store reserve 分配一次，所有 model.item 的 ByteBuffer 均为空。该唯一分配失败发生在 callback 构造前，输出 ZCBA/exit1。无效输入9在任何拥有对象构造前退出9。

prepare_failure 先建好 Z/A/B，替换 RHS 的 C 成功、D 完成分配后显式失败；先清 D，再清 C 临时，然后保留的 Z/A/B，输出 DCZAB/exit1。分别在分配1/2/3/4失败时输出 AZ、BAZ、CZAB、DCZAB，成功分配数为0/1/2/3；正常显式失败已分配4。每个计数最终都必须恰好释放。

field_exit 取走 children 整个数组后提前返回，先销毁独立 children 的 A/B，再清 parent 剩余 Z，输出 ABZ/exit1。准备失败1/2与前述相同，为AZ/BAZ，成功分配0/1。

173 allocator_probe 只在构造器注册既有真实 realloc/free 记录器，让0次分配也报告实际零计数；未为消费者构造容器、制造分配或伪造stdout。FAIL_AT只命中真实realloc序号。

## 额外独立探针计划

不含 Store 的四模块 ordinary-abi 使用 P!i32 错误参数/返回、void!P 成功参数/返回与不相关保留 FnMut。独立预期 ABC/exit0，正常分配2/释放2；任一payload reserve失败被模型显式容忍，仍ABC/exit0，成功分配1/释放1。另测 E!Q 两侧均为不同拥有Package，预期DEC/exit0、零分配。最近禁区包括 owned-error move/borrow capture、闭包非Copy参数/结果、借入资源或嵌套owned-error payload、重复消费错误值和含错误捕获的闭包数组。必须记录实际Phase/Code，不把解析错误算成语义拒绝。

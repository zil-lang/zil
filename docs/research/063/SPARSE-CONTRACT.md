# 稀疏责任位置表示：下一实验的最小合同

这是draft13集成期间，独立核验作者对下一表示假设作出的静态理论复核。它不属于063冻结计时数据，也不是已经完成的稀疏求解器或安全证明。当前生产图算法裁决仍为NoGo；本合同仅规定下一实验怎样保持原有限query。

## 唯一位置漏掉的最小反例

设C有两个拥有型Managed捕获字段，原始指令为：

```text
v0: Managed<R> = MakeManaged<R>(...)
v1: Closure<C> = MakeClosure { closure: C, captures: [v0, v0] }
```

如果先把captures去重为token集合，再令`location[token(v0)]=carrier(v1)`，结果仍满足每个token最多位于一个carrier，却已经丢掉了重复转交的见证。一个闭包拥有多个不同token是正常情况，同一个token填入两个拥有字段则不是。因此原始操作数的顺序、出现次数及拥有字段位置不能先被集合化。

[063独立oracle](../../../tools/research/063/oracle.rs)的MakeClosure分支遍历原captures，以`!moved.insert(from.clone())`记录`duplicate-transfer`，随后才计算token集合。[生产Verify](../../../crates/zil-verify/src/resources.rs)按每次出现consume，第二次面对已移动责任而拒绝。这些已有检查是反例来源，本次没有新增或重新执行测试。

## 精确错误通道，而非一个Invalid位

原oracle按`fault:{raw_point}:{fault_label}`保存完整指令执行前状态的联合关系。原events还保留block/ordinal、result和完整原始指令，包括有序captures。下一表示必须保留失败位置、精确标签、该标签对应的全部pre-state，以及原事件映射；一个Invalid布尔不足以恢复这些信息。

现有合同没有单独的operand-index输出列。迁移检查仍须保存出现次数；若新增第几个实参的诊断，可作为扩展，并定义精确投影回原合同，不能宣称扩展输出与旧canonical原字节相同。

另一个细节是：oracle的alive从共同pre-state读取，重复出现由multiplicity检查记为duplicate-transfer。若新算法先依次更新状态，再把第二次出现记成use-after-move，接受集合可能相同，完整关系却不同。原指令级fault/abort保存pre-state，正常exit保存后态；不可统一替换成一个压缩后态。

## 第一版只替换关联坐标

建议只替换`incident(carrier, token)`，其余状态坐标及有限支持域保持原样：

- 唯一位置只约束正常传播态。允许多个不同token指向同一闭包；必须验证所有原始支持域初态成立，以及每条无fault迁移保持成立。
- absent不能合并EMPTY、MOVED、DONE、退休类别或fresh状态。否则第一次清理后的DONE与从未拥有的EMPTY可能混淆。
- Desc根、未来使用、初始化偏序、共享guard、错误与正常出口仍保留联合关系，不拆成独立边缘集合。
- 身份沿用原token/site和单活跃incarnation边界，不能退化成地址。重叠incarnation仍保留原UnsupportedMultiplicity。
- 在过滤正常后态前记录所有原fault/abort观察。不能只留首个错误，也不能先按唯一性删掉输入再报告无冲突。

生产条件清理合流与研究事件query已存在明确合同差别，第一版不能用生产接受性替代有限query等价。

## 编码、解码与逐步对应

令旧状态为S、稀疏状态为Ŝ。解码D只恢复关联坐标：

\[
incident(c,t)=[location(t)=c].
\]

其他旧坐标、原始位置和事件标签保持。对可表示旧状态，编码E须满足D(E(s))=s，初态集合也必须精确对应；不能把未经检查的原始初态合法性当成前提。

对每个位置、每种迁移变体和正常后继，要求集合解码D*满足：

\[
D_*(\widehat T(X))=T(D_*(X)).
\]

对每个fault、abort、unsupported与exit标签分别要求：

\[
D_*(\widehat O_\ell(X))=O_\ell(D_*(X)).
\]

这里要求相等，而非单向包含或只有最终接受性一致。初态对应、逐步对应、并集和单调性共同成立，才可推出各点最小固定点及观察关系相等；仅写D保持并集并不充分。

当前oracle在fault后不传播正常后态。因此可研究“稀疏正常态，加按原位置/标签保存的精确终止见证”。若逐条证明检查覆盖违规、失败后态确实不可观察，可能无需持久表示所有非法多重拥有矩阵；这仍须通过每条原始指令证明，不能先假定已核源码正确。

## 可检验的下一步

保持原063支持域、canonical schema、原events和全部关系输出；加入重复捕获、二次清理、同址重启、共享guard近邻。每一原始candidate经解码后的完整结果，必须与独立oracle逐项相同，原始失败保留。构造、检查、解码、材料化和输出成本全部计入，与`mono:owner:gc`强基线比较；ResourceLimit和Unsupported均非等价通过。

从T×C关联位改为约T×ceil(log2(C+1))位，仅是关联子表示的位数假设，不含其他生命周期/Desc/顺序坐标，也不保证BDD节点或总编译成本按同倍数下降。

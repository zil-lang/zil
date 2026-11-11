# 流式 partition layout key 独立源码审核

结论：**最终发布候选 `a60ccf924b43c1ac0ef98b9b0f3d1c270c5789bf` 的源码审核通过，未发现阻断正确性的问题。** 本报告只覆盖精确源码、编码边界与既有验证合同；没有运行构建、测试或性能采样，不把旧候选的测试/性能结果移作新 head 的结果。

## 精确身份

| 引用 | SHA |
|---|---|
| 原 main 基线 | `99035931d358add0eaed6d4535035c38413701dd` |
| 已审 set 投影候选 | `e9d392caa1e19bc722e276977165f2a7e5b33700` |
| 首个流式发布候选 | `8358d3c33de4a102e60db1acd29573b85be1597f` |
| 最终发布候选 | `a60ccf924b43c1ac0ef98b9b0f3d1c270c5789bf` |
| 最终 tree | `d260d616a23af8f9c716fbe26c38c200e0eeeb5f` |
| 同树作者提交 | `89cc46c8ef3781a4a1cd723cbcd4bfff478533f9` |
| 最终生产 layout.rs blob | `15504b8e9751bd76a96ba9dc19c279f60be3af1c` |

已独立通过 GitHub commit API 读取最终提交、父、tree、文件 patch 和 blob；本地作者 checkout tree 与生产 blob 逐项相同。最终提交父为 8358；唯一增量是把 Gep 的 indices.len 移到 ptr 符号投影之前，未有夹带修改。相对 main 仍仅 layout.rs 与 layout/tests.rs 两个路径。

发布提交：https://github.com/zil-lang/zil/commit/a60ccf924b43c1ac0ef98b9b0f3d1c270c5789bf

## 变化与 placement 读集

原投影每函数建立 Function::references 和 direct_calls 两套 BTreeSet<String>，global initializer 另建一套引用集合。新实现直接遍历当前完整 operand graph，把符号出现写入 BufferedHasher，避免这些临时集合和 String clone。Op.values 仍为已有的借用 iterator，没有另行复制 opcode 变体匹配，函数 signature/attrs、global/type headers、完整 linkage、定义/声明状态、machine cost/blockcount、LIR 每块/每位置 callee 与预算读集原样。

流式顺序、重复、Array/Gep 的容器结构可以区分原本相同的引用集合，因此可能保守增加 miss；它不允许不同的引用集合或 direct-call 集合变成同一个已解码符号事件流。planner 本身仍使用原集合算法、原 imports 上限、原 local identity 关系；改动的是 key 构造，不是 placement 规则或输出 body。

## 编码边界

最终编码可按以下事件明确读取；Hash 字符串使用 Rust Hash 的字符串边界，整数长度为固定宿主宽度，同 compiler identity 与目标上下文下保持一致。BufferedHasher 只缓冲既有写入，不依赖每次 write 的隐式边界。

| 事件 | 编码 |
|---|---|
| Global 符号出现 | tag0 + 完整 name |
| Array | tag1 + args.len + 子符号事件 + end2 |
| Gep | tag3 + indices.len + ptr/indices 的子符号事件 + end4 |
| 直接 Global callee | tag5 + 完整 callee name |
| function/global 引用区结束 | end6 |

固定 header 在可为空的符号子流之前，Array/Gep 有独立结束 tag，名称内容由字符串 Hash 的自身边界隔离；结束符号不会被误当作相邻 item 的 header。无引用叶可以全部省略，无需恢复 scalar 的数量和位置，因为 planner 只观察 refs/direct-calls；同一容器中的 ptr 与 index 位置本身也不决定 refs 集合。函数、global、相邻容器之间的边界保留，不能把一个定义的符号拼接进下一个定义。

8358 的 Gep 原先先写 ptr 投影再写裸 indices.len，ptr 投影可能为空，不宜依赖后验机器合法性限制来证明该字段边界；审核提出此疑虑后，最终 a60 只前置该长度。未声称已构造出实际错误机器布局或有效输入碰撞，也未把 8358 写成最终已审通过版本。

## 引用与直接调用覆盖

hash_symbols 对 Val 做穷尽匹配：Global 写 name；Array 对所有 Arg.value 递归；Gep 对 ptr 和所有 index Arg.value 递归。Local、Int、F32/F64、Bool、Null/Zero/Undef/Poison、Bytes、Invalid 的无符号处理与现有 Val::symbols 一致。新增 Val 变体将要求更新穷尽匹配，不会静默落入通配分支。没有根据当前样例裁剪 nested Array/Gep 或省略 index 引用。

函数仍遍历每个 block 的每条 instruction，再消费原 Op.values 的所有 operand。它与 Function::references 的原入口相同，包含 load/store、atomics、call callee/args、phi incoming、return、Gep indices 等既有语义读集；没有手写较窄的 opcode operand 列表。

直接调用额外要求原式 `Op::Call { callee: Val::Global(name), .. }`，先写 tag5，再照常由 Op.values 写 tag0 引用。间接调用即使通过其他 operand 观察同一 Global 地址，也没有 tag5；因此 direct-call 与 address-only 不会因引用集合相同而合并。直接调用多次或引用多次会留下多次事件，属于保守绑定。

## 验证、限额与测试

除了上述 key 序列和测试增补，没有改 ResourceHelpers::prepare、ordinary planner、Layout::valid/replay、声明校验、machine model、native builder、driver Verify、对象 key 或发布规则。ResourceHelpers 每请求先 fresh prepare、成功不使用 ordinary cache 的边界保持。完整 External 枚举判断、`.lc1` 精确符号名、cost/maxcopy 与 LIR early-break fuel 仍由原投影覆盖。没有新增 unsafe、提高限额或缩减当前验证；引用递归与原 Val::symbols 的结构递归一致。

新增测试构造两层 Array 内的 Gep：只改无符号整数 index 保持 key，改变深层 ptr Global 必须改变 key。它保留直接/间接同 refs、地址变化、linkage、cost 超限等原用例。已有核心正例继续用变化后的当前 Module 跑未缓存 planner 并比较完整输出 unit，未替换为镜像 key 的 oracle。

本审核只静态阅读测试，不把 root 报告的 8358 8/0 测试结果当成 a60 的运行结果。最终 head 仍需独立构建、相同测试及冻结 screen；原 set 候选 84 请求的同 ELF 与未证净益结果继续单独保留，不为新流式实现预先认领收益。

最终结论仅为精确 a60/tree d260 的源码可继续验证；性能是否改善由当前真实请求数据决定。

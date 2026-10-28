# 205：拥有 State 定义的实际借址检查点

以9ecae62a发布的规范 `9813f43ee7c86d786f197f0ed4b8e65938ee354d94b2cdaec858e09e7871ce5e` 施工；新旧运行各保实际身份。新私有账本沿已有参数化正文 walk 建立，不从具体 ClosureId 或 Fn 类型槽授予权限。

真实 local id 与直接字段槽组成位置。每次借址分配独立身份，take、替换和 whole State 转交使旧身份失效；refill 不复活旧地址。局部地址传播、条件引用、父子 reborrow、分支、while条件/continue回边按实际来源检查。callee 与已准备的参考参数保持到当前调用完成，后续参数不能移动它们依赖的字段。纯 reference 局部赋值只替换指针，不能因为旧 referent 已失效而禁止建立新借址。所有新集合、类型扫描、容量增长与祖先复制有前置计费，未遇借址的旧定义不建立账本。

原02普通T和原04非T未用定义在冻结204 CLI上实际Success，分别省略于Core或保留Core后不实例化到TypedIR。两者均没有native执行，Verify1只核实际main，不能称已执行内存错误。旧混合原01此前仅被表头Unsupported挡住，不把它算别名安全证明。原件与全部阶段文件保存在205-expand的baseline01和baseline-non-t01。

作者限定 `alias05` 为新规范下 b21214f0 加实际补丁，10组Expand测试通过；原01/02/04均具名 SafeReject，最后使用、sibling、fresh borrow、条件引用、父子reborrow与循环刷新指针的正例通过。源码、阶段输出、诊断与实际命令分别保留。`alias01` 7/1来自作者whole-local refill控制先被既有moved-place规则拒绝；该输入保留，最近whole-move反例改为直接访问旧view。`alias03` 9/1记录刷新reference局部时误读旧referent，随后由assignment-place规则修正。早期未保存完整working补丁的轮次仅作为诊断过程，不代替干净候选复验。

旧same-source N1731与普通Store N2255在无新账本的clean54546563上实测保持；新账本 `alias05` 的同旧State源真实 N1938，新混合表头 N503，未用T/last-use源 N688，均N/N−1/fresh。这些是本检查点精确数，后续若生产改变须重新测量；旧硬断言迁移与更大限定回归尚待收口，不能宣称最终205通过。

此片只提供源定义/窗口和计费的有限证据。TypedIR、Analyze、Verify、CT、平台消费者及非作者审核独立；没有运行/RSS、维护或编译性能的Rust比较结论，没有产品级净突破，默认额度和product-gates保持。

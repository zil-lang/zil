# 206：完整嵌套拥有Product

205已按最终61c2d252标准bundle交付基线固定；本片依据设计§8.1.1与interfaces.package_product206。实施登记不计通过。

原120字节package-direct-nested SHA8f82dcba19831acf7417ee2fd4c852b8086d2dc377c8fc470131f87a603d2dba及child-only、三个Created/Incoming源族、unknown普通Call边界共9源，在205 CLI04上均实际Types Unsupported、Verify0、未native。旧探针与本次基线分别保持实际compiler/norm身份。

开放普通完整Package Product：Copy与Managed叶、递归完整child，至少一transitive manages叶，允许无direct manages的自然父wrapper。当前Managed叶限ByteBuffer及有限Copy头用户资源；新内部Store/具体Closure/拥有Sum/owning-array/借入组合仍闭，旧入口不反关。既有有限普通Copy资格保持，包括满足原资格的Enum、Copy ErrorUnion、FunctionPointer及ForeignCallback；不新增这些类型的借入/拥有权限。嵌入child始终完整，父持一直接debt；取出到独立局部后才能局部部分移动。无新deep Place/schema/ABI，无隐式分配。

Types需分普通Product准入和旧capture/Store/array/sum资格；aggregates resolve_indexed/Resolver、普通signature/inference/Graph节点及child读/赋值覆盖。Analyze需独立有界叶来源，在当前local_value复用位置隔离上下文；父child debt与深Managed叶Created/Incoming分开。Verify独立核未用类型、真实SSA/Local、复制/覆写/部分形状、过期Desc和每一叶的新鲜性，不消费前端结论。MakePackage或take child不制造Created，普通unknown call无逐叶返回合同仍Incoming。Lower/LLVM递归布局/声明序清理与父一flag复用，仅表示已核完整值；不以store后设flag补安全证明。

新增叶/子树表、路径复制、状态克隆/合流、布局展开/GEP/清理路径在增长前预费。旧flat Package无新表、原精确N保留；任何真实旧费用/诊断变化先具名登记并保原输入、基线与实际Fail，不猜新值或提高默认额度。

预冻结输入与oracle来源：206-source-drafts/MANIFEST.json及run-logs/206-preparation/consumer01/PREPARATION.json（均scratch准备来源，实施时逐字纳入具名fixtures/源码）。五模块portable/leaf/product/transitions/main覆盖三层与Root-only-child、独立局部部分move/refill、完整child替换/失败/早返/whole relay；资源marker、实际数据与allocator逐失败点一起核对。原预期发生夹具错误时另留修正，不为实际输出改oracle。

必需近邻：Created/Incoming同child、same-type两个实际child、CFG混合、未知普通call、duplicate SSA、Take后Load、缺Drop覆盖、partial child嵌入/partial parent return、同布局错名义、错父槽、未用malformed raw/Wire、旧Desc不复活、last-use正例和旧资格关闭。raw/Wire尽量从真实合法源派生；手填IR按协议层报告，不替代源码。非法程序不native。

分工：types205_finish独占Types/Analyze及impl206_source.rs、impl206_analyze.rs、impl206_budget.rs、impl206_fixtures；root独占Verify及impl206_verify.rs；stage205_finish独占Lower/LLVM、examples/mainline/package_product206和tools/language/206消费者；review205_finish独占impl206_independent.rs与独立fixtures/收据，不改生产。正式全仓与主树归root。有限阶段通过、平台/Wine与原生Windows、作者/独审/root均分列，完整1.4与产品不由本片删除。

# 241 Analyze 只读准备

准备源树 /workspace/zil-core，观察HEAD e92508be996a190fa188d08319d37cc8b522bae6；未编辑生产，未运行cargo/测试，等待root精确241 baseline与冻结合同。IR239四op尚未在此观察树出现。

## 必要写集（仅Analyze）

- budget.rs：独立普通有限shape准入 Copy|Managed|Product|Sum；先原始深度/宽度/字段身份/路径和目标表界限预检再递归资格。不要把ordinary qualification替换capture/shared/StoreElement/array/resource_head资格。新的普通Store Product进入Sum须校验真实module resource定义，不凭Managed(OwnedStore)形状。四op字段/path逐项付费和边界预检，所有新增state/index/leaf表/clone/join/CFG边/比较先计费。保无新profile旧路径收费。
- resources/sums.rs（新）：有限active variant关系、整值生命期、当前tag证据、逐variant/field/deep child来源，原始制造/移动/整值Store/Call/Return/Drop和Unpack变换。不可扁平把全部variant同时活跃；Copy variant仍有整值是否可用状态。Sum路径只是只读标签/Copy末端，不授权owner/view/描述。来源保Created/Incoming和实际Store叶类型，不能用union wrapper统一Incoming代替。
- resources.rs：shape/owned_error/package_owned_field、MakeVariant/MakeErrorUnion/现有ErrorUnion ops、uses/reverse/live_sources、Unpack原子读写、DropEnum，以及主CFG分支精化/join。与旧普通Product/Managed的Fact/出生表衔接。Unpack预先完成source exact-live/真实tag/字段类型/目标vacant互异/活borrow+Desc检查，才一次全消源并逆fields构造目标；不继承源构造年龄。目标Sum/Product子责任作为完整值转移，后续新Take/Store照真实执行另建年龄。
- resources/product.rs：有限普通Product中Sum准入、完整值叶路径/来源递归迁移，保旧Capture/Product/StoreElement限制。跨Sum路径要带variant信息，不能以同field ordinal合并不同variant；可把新的tagged-path来源放sums模块，让旧无Sum路径原样。
- lib.rs + incremental.rs：初始化新原子Unpack（读已初始化source、移除source、加入全部target）；ReadVariantLocal/VariantTagLocal是Read；DropEnum是Kill。缓存Input必须包含Unpack源与完整有序目标表，不能归Other；若Operation含Vec则不再Copy，Input克隆费用由明确preflight预付，不能无费分配。独立验证扫描的atomic语义与transfer必须一致。
- scopes.rs：四op的source及所有Unpack目标都在当前storage activation；Sum触发显式scope要求；Copy路径不产生新storage视图。
- holds.rs：Unpack/DropEnum与整值Sum Store/Take对真实borrow/Desc窗口的影响；contains_owned_store递归普通Sum时不借机拓宽capture资格。ReadVariantLocal仅Copy、无派生地址根。目标已有Hold/描述不能被覆盖。所需数据流必须与资源核实际来源一致，不从前端标志授权。
- defers.rs：Sum owning分类；DropEnum在允许清理suffix内，按真实注册年龄核reverse；Unpack源移出forget并对owning目标按逆fields construct，最终pattern发布Take/Store按原实际动作年龄，不能提前forget新债。无Register的旧函数保持原费用。
- dynamic_store.rs/retained.rs/callable_fields/helpers/helper_flow：预计仅新四op exhaustiveness或原始uses walkers机械接入；不扩各自资格。必须以精确baseline穷举结果决定，不能预先宽化Sum capture/State/array。resources/summary_graph可能仅边/效果依赖机械接入。
- resource_tests或新sum_tests + lib module：raw IR（可含Wire roundtrip仅作raw输入）与预算N/N-1/fresh/incremental cache mutation测试；正式source/native由Types/root/独审协调，不作为Analyze单核自己实施语言。

约9–12生产文件+1新模块+1测试模块；机械穷举额外文件以baseline实际变化为准。

## tag/版本证明的最低要求

当前resources两次固定点向所有successor复制同state，无tag guard事实。新模块必须从实际VariantTagLocal结果→比较constant→Branch边导出must tag（或实现239最终冻结的等价可核动态检查合同），在join仅保各边均成立事实。每层path的variant都需当前值证据，Unpack也需真实source variant。所有whole/field可改变内容操作必须杀掉旧tag证据；local写位点编号本身不是运行时generation（loop重执行相同Store会重复编号），因此不能只比较“静态Store序号相等”而容许旧SSA tag在回边后复用。可把“当前内容仍允许使用的tag SSA token集合”作为must事实，任何修改清空，读取时重新加入；跨边传播与intersection必须有预算和终止上界。嵌套path检查不能跳过父tag/完整live。即使返回Copy，也不允许部分/已move的source经旧tag读取。

## 最近反例/正例

- active enum仅持其variant的多个owner；不同variant join不制造同时债。空/纯Copy variant也受wholemove失效。
- Unpack repeated/source=target/重复目标/目标已有owner/错arity/错类型/错variant/没有guard/目标storage已结束：全拒，原子前失败不改变任何责任。
- 深path缺父tag、旧tag后wholeStore/Unpack/fieldmutation、loop相同静态写点后旧tag、branch join非共同guard：拒。只Copy末端可读，无owner/view/Desc旁路。
- Product内Sum、ErrorUnion中含实际Store Product、enum多个owned字段，Created/Incoming区别经pack/unpack/return保持；不能create承诺转发incoming叶。
- guard失败只清剩余完整责任；全pattern guard成功后逆pattern Take→Store；nested拆temps按逆fields年龄，与defer注册及普通局部Drop共同逆成功构造。
- 活Store view/borrow/Desc跨Unpack/Drop拒；Copy快照和已结束窗口保正例。
- old capture/shared/array/StoreElement继续负例，不能仅因为ordinary Sum闭合就新授存储资格。
- N/N-1/fresh + warm AnalysisSession等价；只变Unpack目标表必须cache miss或不同准确query，旧未初始化路径不能缓存成成功。

状态：只读准备；Implementation/Test/Independent/Full均NotRun。

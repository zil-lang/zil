# 严格完整可见性对照：非作者精确源码静审

## 结论与固定身份

**当前冻结源码静审无有效输入阻断，可进入统一构建和五项proof控制验证；尚未执行任何门。** 本审查者不是该候选作者。结论只覆盖源码接入、闭包/费用证明和有限测试设计，不代报正在由Root执行的release构建，不证明测试通过、运行质量或编译提速，也不替代wrapper/protocol的独立审查。

| 对象 | 身份 |
| --- | --- |
| 作者源码HEAD | `49979808c9f50afea2646242c68776fa376c7e60` |
| 发布源码 | `2549cbec8e1f3096d4c8c8a960459c83095ad13d`（Root提供并核同tree；本审实际读取作者精确Git树） |
| 同一tree | `ebb993128355fed8228627c6d7b37d5cb0a90d45` |
| 唯一parent | `eb5b150903b67d8b9011f6adc2d3ac67b1c419a6` |
| 实际工作树 | `/workspace/scratch/abf690ce880c/thinlto-complete-visibility` |
| 写集 | `zil-backend-llvm/src/build.rs`、`machine_cgu.rs`一行module接线、新`machine_cgu/complete_visibility.rs` |

已核HEAD/tree/parent、干净状态与完整parent差分，`git diff --check HEAD^`无输出。只读源码，没有改author树、编译、测试、native或采样。作者报告后续尝试强化patch原子失败且零源码修改，Root要求保持该tree冻结；两项维护建议在本报告末尾保留，不为它们另改构建身份。

## 接入边界与权限来源

1. `build.rs:220–248`读取新COMPLETE开关，仅值1且已有显式THINLTO_RECEIPT才接受；原exporter guard继续要求Clang/O2/incremental-o2/after、Linux Main、8units/2jobs、无closed/object cache、全新绝对receipt路径。环境变量只允许走研究分支，不赋予Verify或完整规划权限。
2. 当前lower仍接受VerifiedModule并走原exact LIR binding（emit.rs的validate_binding检查实际function/closure/resource/layout，不仅fingerprint）；随后原machine先完整`validate_module`，执行原checked/scalar fold，fold后partition输入再次完整`validate_module`。Proof::prepare位于这个第二次预检之后，不能让raw machine的坏类型/SSA/Invalid operand借proof通过。
3. `Proof::prepare`从当前machine clone实际调用原`partition_view_cached`、原ResourceHelpers/Complete policy、同entry/count/split=true，cache_root=None；没有从外部JSON、环境字符串或人工名单反序列化规划。`from_original`私有，测试可构造Plan是模块内部逻辑控制，不是生产授权入口。
4. 实际产物继续走canonical Disabled导出。`finish`位于实际每unit完整预检之后、写receipt之前，错误返回infra拒绝；只有构造并校验成功的proof才进入receipt。原copy budget和身份分区没有被更改或替换成force flag。
5. COMPLETE未设时不创建/执行Proof，既有研究编译管线仍是原Disabled exporter；完全未开启研究receipt时，原生产编译分支无新规划/验证/导入差分。**不是声称旧研究receipt原字节相同**：旧exporter开、COMPLETE关时，本次JSON多了`visibility_proof:null`字段；需要由研究consumer接受其明确null，不把它当有proof。没有构建或同码执行，不能把静态路径相同写成实测ELF相同。

## 闭包、费用与owner证明

### 当前模型的纯范围拒绝

`from_original`只接受External source定义和main wrapper；其他compiler-owned/local定义拒绝，任何Global item拒绝，closures在prepare拒绝。已验证的declaration还必须属于getchar/putchar/llvm.trap或显式overflow intrinsic名字集合，陌生声明拒绝；calls目标必须在当前facts或实际foreign declaration集合内，不能仅因找不到定义就当外部。

`calls`只把精确`Op::Call { callee: Val::Global(..) }`当直调，对每个参数独立检查；其他callee一律视为indirect并拒绝。其余Op走现有穷尽`Op::values`迭代器，递归检查Array与Gep的ptr和indices。因此同symbol既直调又作为参数地址不会被集合差减掩盖；没有新增一份漏opcode的operand表。当前Global occurrence在任意已遍历角色都会拒绝。

### 真实clone名单与精确名称

原plan中已知canonical body形成每unit的canonical集合；其他body必须能解析为`zil.import.<usize>.<exact canonical>`，目标必须属于原sources，linkage必须Internal，cost必须等于原canonical cost且不能重复。名字使用split_once只剥owner前缀，保留`zil.fn.1.lc1`完整后缀，不按最后一段数字猜目标。原plan来自内部真实planner，因此本审没有把“知道某个前缀字符串”本身当原规划证明。

### 唯一owner映射与闭包完备

finish从Disabled各unit实际body恢复canonical定义集合，拒未知/重复/跨unit重复owner和空unit，要求所有当前facts恰好出现一次、两种plan的unit数相同。每个Disabled集合必须唯一匹配原plan的canonical集合，matched保证双射；没有按两个Vec的相同index假定owner相同，故Complete启用small_caller_components造成的真实分组差异会拒绝。

对每个unit从全部owned定义出发，依当前原machine的直调图遍历，visited按函数去重，foreign终止；任何cross-owner source target进入required，cross-owner non-source拒绝。遍历可以经过别的owner再回本owner，循环/自递归由visited终止。同owner定义本已全部作为起点，因此不会丢其出边。`required == allowed`同时拒缺失和额外原规划import，能抓住原partition_view_recorded吞Complete预算失败后返回ordinary8unit/fallbackNone的情况。

### 原费用没有被prelink大小替代

source_cost只沿当前LIR非foreign source_name映射的原canonical function.cost求和，不加入main wrapper/helper/declaration给分母扩容。每unit的每个allowed目标按原function.cost收费一次；不同unit重复导入同目标分别收费。所有加法checked_add，累计copy_cost一旦大于source_cost即拒绝。它保持的是原machine instructions+blocks费用合同，不声称预链接优化后的LLVM指令数不膨胀。

该Rust证据尚不保证LLVM实际导入情况：wrapper还必须独立重算实际prelink Required、检查Required⊆Allowed、Required⊆.3 definitions、所有实际跨owner/AE及其他新增源body⊆Allowed，并处理外部无summary的force诊断。当前源码只产生有限许可，不凭字符串声称wrapper已做到以上事项。

## 五项proof控制的准确语义

所有五项均位于新模块，**本审未执行**。它们直接调用私有from_original来隔离proof逻辑，不通过真实planner制造Plan，不能把测试数量说成“原Complete planner已端到端验证”。生产prepare真实planner接入的静态依据另如上。

| 控制 | 实际覆盖 | 不覆盖/边界 |
| --- | --- | --- |
| `original_budgeted_visibility_preserves_exact_lc1_name` | 两owner、一个真实费用的许可，要求输出保留完整lc1 canonical名字和闭包标志 | 两unit是内部逻辑控制，不能当8unit exporter/native通过；没有独立解析全部receipt费用字段 |
| `silent_copy_fallback_with_same_units_and_no_fallback_is_rejected` | 伪造ordinary同unit数、fallbackNone、缺import，要求由required!=allowed拒绝 | 不实际触发原planner的预算回退；证明的是识别此形状的后置条件 |
| `changed_canonical_owners_and_extra_import_are_rejected` | 移动canonical定义造成owner集合不同必须拒；额外非所需import也拒 | 未运行所有unit permutation/duplicate形状；静态双射与全局owners负责这些拒绝 |
| `unknown_target_and_nested_address_are_rejected` | 未声明unknown direct target拒；同callee symbol藏在Array参数仍被逐occurrence拒 | **现Array参数是Ty::Ptr+Val::Array，callee仍零形参，本身不是合法typed machine**。它是raw checker分支控制，不能声称已证明“合法address输入先过preflight再由pure scope拒绝”；本轮也没有独立Gep合法控制 |
| `recursive_closure_terminates_and_copy_cost_is_per_unit` | 三owner循环，visited终止；每unit导入另外两定义，累计复制超过原source_cost，要求拒绝 | 不等于实际planner N/N−1预算/native执行；没有独立usize overflow实跑，checked_add为静态依据 |

静读各factory/类型/借用/集合推导未见明显Rust编译阻断；这不是“编译通过”。有些raw输入故意不先validate，是为了把proof自身分支与完整model预检分开；对此严格限定证据范围。

## 保留的维护建议与未测边界

- `no_address`当前`_ => true`也包含Val::Invalid；**在本次生产顺序里Invalid已被完整preflight拒绝**（preflight.rs:331），当前有效输入不存在由此绕过的路径。但未来新增Val variant会默认true，建议生产化前穷尽列出已知scalar/local/bytes并令Invalid false，让新enum变体触发编译提醒。本轮Root明确维持冻结源码，不因这个未来风险重build。
- 最近合法地址反例还应在后续适当阶段用匹配Array/Gep类型与callee参数签名、先独立validate_module成功，再要求proof拒绝；当前nested raw测试不能承担此主张。现源码逐occurrence检查本身没有发现当前漏检。
- foreign许可只存当前validated declaration名字，Rust proof不在receipt里序列化完整函数签名；exact声明/签名应由wrapper按实际input IR交叉核，不能泛化成“任何同名extern都已证明此两case语义”。本轮fixture和工具/source绑定由冻结protocol负责。
- 新Proof会clone当前machine并重新运行原planner，是研究质量对照的额外工作；当前不能据prelink复用就宣称编译提速。
- 还未执行新5项、完整backend/API、fullO2、2case/54native或任何预算/地址native控制；它只允许继续有限研究验证，不是产品采用、全语言安全或运行质量验收。

# 施工9：有限生成类型与普通泛型 where Copy

## 当前验收状态

228、229已合入并完成正式主线验收。共同实现为 04fbd476fdbf02fbeaee374d2a8895f16f23c954（P），PR候选为 faf7c8e2c713d81cedaf2b2a74a738045f093cd0（R）。两个切片的交叉非作者审核均为Pass，作者和原失败记录保持原身份。

| 正式验收 | 实际提交 | GitHub run |
| --- | --- | --- |
| PR11完整验收 | ee5333028cb0c959dc6b73e43de000c66e4ace3f | [41154458005](https://github.com/zil-lang/zil/actions/runs/41154458005) |
| main合并后完整验收 | e60f9e2242a1d1ac3bfc14e082e627d7f7bc8952 | [41156023933](https://github.com/zil-lang/zil/actions/runs/41156023933) |

两次运行的22个声明步骤及3个自动步骤均成功，共25个实际步骤：默认workspace 2905/0/0、reference 2863/0/0，均零过滤；格式与Clippy通过，text-reference 36/0、migration 9项合成控制通过。每次机器State/分配故障控制均独立核对16次构建、40个API builder对象、128次执行、88条allocator观察、56次注入故障及145条command记录。各轮结果不相加为独立覆盖。

PR与实际main整树不同；直接分层核对确认仅225、226工单及施工8日志三个文档blob改变，生产、测试、示例、配置和工具相同。主线结果从本次原始收据重新计数，未把PR结果改标为main。main之后至e463b7ee、再至21058d0e的并行更新仍只涉及文档/规划/证据，已逐SHA保留；全部CI输入不变。

两次CI的原始小ZIP、完整job日志、独立审计和最终恢复包的非bundle成员已保存于evidence/integration/228-229/ci41154458005及ci41156023933。大native ZIP未在本地另行逐文件下载核验，判断依据为完整manifest、GitHub摘要元数据、成功上传及清理前inventory核对。consumer209的工作流保存完整summary；审计重核其全部观察，原始命令目录和可执行文件未由该工作流上传。源码bundle另由环境恢复代理导入并在无alternates的隔离对象库中验证，收据见recovery目录；不追改两份CI审计收据当时未展开bundle的声明。

施工9从远端6ba0b04e建立独立工作区，并保留施工8的225–227及编译研究各自的进度和身份。

## 新增语言能力

### 228：有限 #for 类型声明生成

在已有有限源级 `#for` 中，允许生成无泛型、无方法的普通 `struct` 和无泛型普通类型别名。类型声明可与函数处于同一生成批次；批内先绑定名字，再改写引用，因而支持同批前向引用。类型和值分别维护卫生名字，函数泛型参数正确遮蔽外部同名类型，私有生成类型在不同迭代和模块之间仍保留名义身份。

迭代标量可进入字段、默认值、数组长度、别名及签名的已有表达式位置。例如本轮消费者实际使用：

```zil
fn duplicate<T>(value: T) -> (T, T) where T: Copy {
    return (value, value)
}

#for (lanes in [2, 3]) {
    type Payload = [i32; lanes]
    struct Event { kind: i32; payload: Payload }
    fn score(event: Event) -> i32 {
        var total = event.kind
        for i in 0..lanes { total += event.payload[i] }
        return total
    }
    #if (lanes == 2) {
        pub fn pointer_event() -> i32 {
            let pair = duplicate(Event { kind: 65, payload: [4, 5] })
            return score(pair.0)
        }
    } else {
        pub fn touch_event() -> i32 {
            let pair = duplicate(Event { kind: 66, payload: [6, 7, 8] })
            return score(pair.1)
        }
    }
}
```

上面是从消费者简化的说明片段；完整实际执行字节以 `examples/mainline/generated_events228/combined/` 和证据归档为准。完整消费者还比较两份副本的结果，并检查输入模式、输出和退出状态。

生成内容不回流改变ConfigReady。私有名义身份、公开重名、保留名字和既有预算限制仍在编译流程中验证。这一片不开放泛型类型、带方法类型、resource、package、enum、trait、impl、完整Type/Syntax或 `#emit` 生成。

### 229：普通自由函数 where T: Copy

普通泛型自由函数现在接受 `where T: Copy`，正规化到既有内联 `T: Copy` 能力检查；重复Copy约束幂等。约束目标必须是本函数直接声明的普通类型参数。

本片不开放StoreElement、callable、自定义trait、投影、生命周期、const参数、方法或impl上的一般where约束。未实例化的泛型函数体仍接受符号检查，不能利用外部同名结构体获得字段或构造能力。含where的新路径按where→别名→特化传递同一剩余预算；未触及该名义碰撞缺陷的无where旧路径保留原行为与费用；碰撞路径的拒绝修复和新增费用单独记录。

## 独审发现与返修

| 问题 | 原始观察 | 最终行为与证据 |
| --- | --- | --- |
| 228新增模块扫描未计费 | 原实现每个生成批次无条件扫描imports，连旧纯函数路径也新增了未计费工作 | 类型标志随原预费materializer遍历产生；只有实际类型批次才逐模块预费查询。旧纯函数阈值不变 |
| 保留函数名经卫生改名逃逸 | 普通 `fn bytes_len` 拒绝；旧纯函数生成和新混合生成错误接受 | 在原拼写被改写前复用普通函数 `primitive_name/primitive_name_error` 检查；三个原源均在Expand拒绝。合法 `Copy`、`bool` 函数控制保持接受 |
| 泛型参数借用同名普通类型的字段能力 | 未调用的 `fn bad<T>(x:T)->i32 where T:Copy=>x.x` 可从同名 `struct T` 偷取成员；旧inline和无bound形态也有该问题 | 仅在真实普通binder名字碰撞时先执行既有名义身份保护，再完整符号检查。原攻击拒绝，合法同名普通类型及泛型封装仍可原生执行 |

后两个问题包含既有路径缺陷，不能全部归因为新语法引入。原Fail、审查fixture的语法/字段可见性错误、消费者首次错误类别断言均单独保留；生产错误与测试构造错误没有混为一个统计。

消费者最初把修后 `Unsupported / Expand / E-UNSUPPORTED` 写成了必须 `SafeReject`，产生三项测试断言失败。修正只更改oracle，未改源码或生产代码；再次执行精确类别检查通过。没有把这次测试判定错误描述为编译器仍误接受。

## 精确身份与验证

| 身份 | 提交或收据 |
| --- | --- |
| 228最终生产 | `c1578e319d606d745835a7c3c609a3bb738b3c15` |
| 228作者证据 | `1082b50ec3221ccf0550255fb1a6675500266055` |
| 229最终生产 | `d8c71880928c2e0497ffe078834382503960256b` |
| 229作者证据 | `026ea29beb8d83c382ad04bd417606679769afa3` |
| 共同实现P | `04fbd476fdbf02fbeaee374d2a8895f16f23c954` |
| 228非作者审查 | `/root/mainline_recovery`，`evidence/reviews/228/joint/RESULT.json` |
| 229非作者审查 | `/root/scope_audit`，`evidence/reviews/229/joint-P/RESULT.json` |
| 独立事件消费者 | `/root/environment_recovery`，`evidence/reviews/228-consumer/FINAL-REVIEW.json` |
| root受影响集成门 | `evidence/integration/228-229/local/RESULT.json` |

合并后的driver生产树与228最终作者字节完全一致；expand四个229生产文件与d8完全一致，共享lib与c157完全一致。共同P的driver树为 `5cad5e35d03c16291e4cd45adbef8c769b5cab54`，expand树为 `01c3690f47779962a87665e725353cffd361eb33`。所有作者和审查提交以no-ff保留，冲突修正按已冻结作者的完整文件取值，未重写作者身份。

最终有限执行记录如下。各轮大量重叠，不能相加为独立覆盖，也不能替代正式整仓CI。

| 验证范围 | 真实结果 |
| --- | --- |
| 228作者限定 | 7/7新测试；相关19/19；22次LLVM API O0/O2构建与执行 |
| 229作者限定 | 72/0及限定Clippy通过 |
| 228在共同P的独立复核 | 6/0/0，18次LLVM API O0/O2构建与执行 |
| 229单片独立审查 | 8/8及3项精确预算控制；10次LLVM API O0/O2构建与执行 |
| 229共同P必要组合复核 | 3/3；2次LLVM API O0/O2构建与执行 |
| 四组双模块事件消费者 | 7个消费者测试加1个保留名测试通过；8次构建、24次原生执行 |
| root共同P受影响回归 | 26/0/0；格式检查通过。之前10项旧回归为重叠执行 |

四组消费者为手写+inline、生成+inline、手写+where、生成+where；同输入下都验证 `0 → J / exit0`、`1 → W / exit0`、`x → 空输出 / exit7`。LLVM API报告确认23.1.4、builder输入、`parse_ns=0`，O0/O2均验证并执行。早期CLI控制保持原候选身份，不改标为最终修复版的执行。

旧 `#for(i in 0..1){struct S{x:i32}}fn main()->i32=>0` 原字节从旧拒绝列表迁入正例，标签仍为generated-type；其余旧反例未删改。该迁移已同时进入本次PR与main两套完整配置验收。

## 预算合同

以下是逻辑工作/节点额度，不能当作编译速度或相对Rust性能。

| 具名源路径 | 最终阈值 | 对比 |
| --- | --- | --- |
| 222作者core/full/nodes | 71 / 138 / 751 | 不变 |
| 222独审core/full/nodes | 77 / 96 / 1084 | 不变 |
| 219core/full/nodes | 127 / 205 / 645 | 不变 |
| 228作者nodes/work | 616 / 1811 | 模块查询补费前为614 / 1809 |
| 228独立嵌套work/nodes | 2651 / 472 | 最终共同P fresh、warm、N−1及恢复均通过 |
| 229无碰撞plain work/nodes | 589 / 220 | 不变 |
| 229无碰撞alias work/nodes | 952 / 233 | 不变 |
| 229 where累计work/nodes | 1651 / 488 | 不变 |
| 229碰撞inline work/nodes | 837 / 188 | 修复前281 / 131；真实额外保护工作已登记 |
| 229碰撞where work/nodes | 936 / 230 | 修复前380 / 173；真实额外保护工作已登记 |

规范补充 `changes/229-nominal-shadow-repair.md` 显式记录碰撞修复费用，未将它伪装为旧预算不变。默认额度未提高；上述具名N−1控制均在Verify前因ResourceLimit终止，失败后活动查询为零。

## 范围与接续

本轮使有限生成器可以生成有真实类型的事件载荷，并用普通where Copy泛型消费它们。完整Type/Syntax、`#emit`、一般trait/lending、普通defer、长期外部回调关闭/注销/重入、挂起/取消/并发等继续按各自工单推进。

225–227不随本轮晋升；B0仍Doing、181仍施工，完整1.4与完整ZUI仍NotSatisfied，Rust产品门及原生Windows仍NotRun。没有建立四目标合取或可重复产品净突破。历史handoff记录完整性Fail也不因本轮局部语言验证而消失。

## 下一轮优先候选

可并行推进有限非泛型enum生成（只含已有Unit/Tuple变体）与已有名义trait bound的where正规化。前者主要占用ct_selection；后者需要generic_where及modules中真实where路径的模块身份解析。它们目前只是只读梳理后的候选，没有新派编号、没有修改规范，也未计入已实现能力；225–227仍由其施工线维护。

## 本轮实际交接记录检查

在已提交元数据候选 `3a1aca37a53c310da961c0a54d9c59f7aa35f50b` 的main工作树实际运行 `node tools/handoff.mjs`，全局 **Fail / 1379项记录完整性错误 / 0 warnings**，退出码1。228、229均为已合入且各零错误。相对本轮6ba起点真实基线1372项，错误多重集新增7、减少0；新增记录涉及225、226、227，未改写其他施工工单来抹平错误。这些是工单/证据记录完整性问题，与编译器CI失败不同。原始输出及逐项差分见[handoff-final-D2/RESULT.json](../evidence/integration/228-229/closeout/handoff-final-D2/RESULT.json)。`data/handoff.json`保存上述D提交的实际快照，不冒称包含快照自身的后续提交已重新执行检查。首次元数据D检查有1381项，其中本轮两项是消费者收据status字段混入范围说明；原始D输出保留为Fail。原消费者复核者在新CONSUMER-ACCEPTANCE.json中分离严格状态与有限范围，绑定原始报告摘要和原执行身份，未改原报告、检查器或任何测试结果；本次相对该D新增0、减少2项。

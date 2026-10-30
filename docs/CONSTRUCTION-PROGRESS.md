# 当前语言施工进度：zil施工8

222–224完成有限实现、原失败返修、共同候选的三份非作者范围审查及本地组合检查。正式GitHub验收与合入按下列实际身份记录；这些结果不把完整语言或产品门升级为完成。

| 验收身份 | 实际提交 | GitHub run |
| --- | --- | --- |
| PR 完整验收 | `4d3df5891aafebdd8337337180a7e2559daec361`（实际 checkout） | [run 41088376020](https://github.com/zil-lang/zil/actions/runs/41088376020) |
| main 合并后完整验收 | `ccae8c60c96a2c174b137b2fab084e9e8a97a02f`（实际 merge） | [run 41106864272](https://github.com/zil-lang/zil/actions/runs/41106864272) |

最终 main 默认 workspace **2861 passed**，reference workspace **2826 passed**；正式通过还要求其余必需 workflow 步骤完成并成功、原始证据上传完整，不能只看默认 workspace 一项绿色。

PR9 最初合入 `39e52ef784d41f5744b909a9e6702ab16c1b1227` 后，[main run 41101248493](https://github.com/zil-lang/zil/actions/runs/41101248493) 保持 **Fail**：默认2860/1/0，reference2826/0/0。唯一失败发生在 `review223_machine` 向子进程stdin写入时的 `BrokenPipe`，早于退出码/stdout/stderr的采集；该次消费者输出没有被验证为通过。原 `entry_error.zil` 不读取stdin，修复 `e38ce27fda432e7a55b4a203f4fd047756d9512e` 将无输入样例改用空stdin且不写入；需要模式字节的样例仍严格检查写入。相对PR候选，生产源码、规范和源fixtures保持原字节，退出/输出/分配清理oracle不变。上表最后main是非作者复核该测试驱动修复后的新合并身份，没有把原Fail改名为Pass；PR与最后main的完整文件树不同，差异限于该测试驱动、223派工和修复证据，见 `evidence/integration/222-224/closeout/POSTMERGE-REPAIR-BINDING.json`。

| 切片 | 本批工作与保留边界 |
| --- | --- |
| 222 | 有限源级 `#for`、普通函数/语句生成、既有类型标量槽替换、卫生与累计预算；未开放类型声明生成和通用Type/Syntax，普通defer lowering仍Unsupported。 |
| 223 | 完整Product作为Store元素、真实递归布局与清理、Store与普通地址资格分离；Product内Store、递归Store、拥有捕获Store<Product>及Product独占窗口未开放。 |
| 224 | 同步共享 `&Product`、直接子包/资源字段地址、完整责任与NLL；借入返回/逃逸、&Product闭包捕获（含现有立即调用形状）及可变嵌套Product未开放。 |

本地clean组合132/0/0与共同实现 `4d7bbb762eb34987ffd05c9ab23a6ad6bc6fae41` 的19项受影响回归有重叠，不能相加。旧218–221的2817/2785仍只属于旧验收；作者、独审和正式CI结果分别记账。完整范围见 [LANGUAGE-222-224.md](LANGUAGE-222-224.md)，机器入口为 [construction-progress.json](../data/construction-progress.json)。

PR阶段两次原CI失败不改写：`41082643919` 默认/reference各在未完成整仓时记录64通过、1失败；`41084362447` 默认2858通过、3失败、0忽略，reference Incomplete、后续门NotRun，未收尾原因未证。91609072临时运行及e3e231b / run `41088078934`由后继修补替代，均为Superseded，不计最终Pass或Fail。完整原输出与限定返修回执保留在本批evidence目录。

原29份readiness探针为 **15成功/12 Unsupported/2原语法无效**，旧成功零回退。晋升的 `product-error-union` 原输入仅含未调用的 `fn make()->void!Root`，只证明精确Unit/Product形式到达Verify。29次check没有native执行，也没有 `#for` / 普通Product共享借入输入；原样本、阈值与原失败均保留。

完整语言1.4与完整ZUI保持NotSatisfied，B0为Doing、181为施工；Rust产品门及原生Windows为NotRun。现有机器证据属于Linux x86_64相应受测路径，不把历史Win64/Wine结果当作本轮原生Windows验收。四目标合取与可重复产品净突破未成立，编译性能研究分支独立保留。

本轮只在 [zil-log](https://chatgpt.com/space/page_5799fccae05c8191b25b68c26ec7a4ab) 更新 **zil施工8** 一行，使用北京时间。一次发布调用长时间未返回造成心跳超出10分钟，恢复后由活跃审查代理更新；该事实保留。阶段结束明确标记，时间戳不作为会话结束后后台继续工作的承诺。

# 结构化sink非作者源码独审

结论：**当前冻结差分静态审查无阻断。** 此结论只覆盖源码与测试设计，没有执行构建、Rust测试、LLVM API测试、native或性能采样，不代表运行质量、编译收益或生产准入通过。

## 固定身份与写集

被审worktree：`/workspace/scratch/abf690ce880c/structured-emission`。

- HEAD：`924076ca11c75e88bb1bcb9df98c58d0dae1bbd2`。
- tree：`ad37913cf49a1bfec28e66e7abdcf225dbcafed5`。
- 唯一parent：main `99035931d358add0eaed6d4535035c38413701dd`。
- 工作树clean，`git diff --check main..HEAD`无输出。
- 差分恰为5文件：`zil-backend-llvm/src/emit.rs`及其`scoped_callback_tests.rs`、`scoped_thread_tests.rs`，`zil-llvm-api/src/model.rs`和新`item_builder_tests.rs`；没有分区、import、LLVM pipeline、Verify、cache key或native FFI变更。

## 逐项审查

| 关注点 | 源码判断与依据 |
| --- | --- |
| 消除的工作 | `FunctionEmitter.text`从`Code(Vec<Event>)`改为`ItemBuilder`，每个event立即移动到owned Function/Block/Inst，`emit_with_cleanup`直接返回`Vec<Item>`；`lower_function_batch`去掉旧`.into_items()`二次消费。省的是每函数event tape的积累/遍历及与最终结构的短暂共存，不能夸成完全无Event或消掉所有fragment分配。 |
| `copy_event`准入 | `emit.rs::event`仍只调用未改变的`copy_aggregates.rs::copy_event`。后者先检查snapshot参数和result、必要时置unsupported并拒绝result，然后才`self.text.push(event)`；sink替换没有绕过这条准入，也没有重新执行一次copy检查。`copy_aggregates.rs`原本直接写text的物理内存操作仍直接push，未新增改道。 |
| `line`不多转写 | `emit.rs::line`依然`self.text.append(code)`。新append只移动fragment里的events并逐个sink.accept；没有经`self.event`再次走copy_event，没有render/parse或clone。原line即直接append Code，因此准入语义保持。 |
| 结构规则 | `ItemBuilder::accept`逐分支搬运旧`Code::into_items`状态机：Begin仅active为空时接受并重置blocks；Label要求active；Instruction要求最后block；End要求active并在该位置push Function；NamedType/Global/Declare仅在无active时接受。错误字符串及function完成后item追加时机相同。 |
| 首个结构error | `push`只在无error时调用accept，首次错误保存`Option<&'static str>`，之后events被消费丢弃但不再修改结构；`finish`先返回该error，随后才检查unterminated。旧Code转换也会返回第一个错误。没有后来的error覆盖首错。 |
| emitter错误优先级 | sink结构error不通过`push/append`即时抛出。`emit_with_cleanup`中的原`?`、helper错误、最终`unsupported_large_copy`检查均先于新增`finish`。因此原先先完成emitter、再检查event结构的可见错误优先级保持。新callback测试特意先喂unmatched End，再制造snapshot Return：有snapshot时仍优先报原Unsupported，无snapshot时才报unmatched function end。 |
| helper与item顺序 | 主函数End之后仍按原source instruction顺序生成registration/thread/cell/callback helpers。sink只在End追加已完成函数，不提前排序或交换helper。`lower_function_batch`现有foreign/scalar-summary/leaf快路、task_symbol收集、scalar_locals::promote与body.push顺序未改；intrinsics仍是原BTreeSet。 |
| 公开Code API | `pub struct Code(pub Vec<Event>)`、derive、`new/append(impl Into<Code>)/push`及From实现保持；公开`into_items(self)->Result<Vec<Item>,String>`签名未变，仅委托ItemBuilder。FunctionEmitter是模块内部实现，其返回类型变化已对应调用点。新增ItemBuilder没有替换Code tuple入口。 |
| 两个旧LLVM形状测试 | prelude继续经Code.into_items验证，再extend已经finish过的emitter items，以Module结构直接构造。它们没有取消真实clang形状检查或原断言；不再需要把最终items倒转为Event。此审没有执行这些测试。 |

### 测试是否为独立oracle

`item_builder_tests.rs::streamed_fragments_preserve_complete_items_and_helper_order`先手写完整期望Items（NamedType、Global、Declare、main、helper以及各Block/Inst），再输入跨函数边界的append/push混合event；比较期望结构、顺序及helper位置，期望值没有调用ItemBuilder构造，属于独立的有限结构oracle。

`malformed_streams_retain_the_first_error_until_finish`以手写字符串覆盖block外Label、block外Instruction、孤立End、unterminated、nested Begin、active中的Declare及首错不覆盖，字符串没有取自实现结果。新增large-copy优先级控制使用原copy_event实际检查，且期望是独立构造的Diagnostic，覆盖本次最易意外改变的错误顺序。

同一测试中`Code(events).into_items()`现也委托ItemBuilder，因此**Code对照不是独立旧实现差分oracle**，不能把两条新实现路径相等夸成基线等价证明；有效独立部分是上述手写Items/错误/Diagnostic及保留的现有LLVM/native控制。有限测试不穷尽任意event stream；源码上状态转移与原版逐分支一致，是本次静态无阻断的另一依据。

## 门与结论边界

本审未执行任何门。root正在进行的构建/测试及之后的同码、Verify/preflight、运行与编译性能结果应以各自冻结收据为准；本文件不代报其结果。尤其不能仅凭少一个Vec<Event>就宣称完整请求更快或RSS下降，也不能将本候选与ThinLTO研究门混合归因。完整default O2、当前独立Verify与完整机器预检，以及既有运行质量门没有在此差分中降低。

# 严格完整导入与默认ThinLTO：既有IR数据复核

## 范围与结论

本节先于新增remarks诊断写入。只读正式default/strict两次已完成NoGo的产物，用protocol pin的llvm-dis解码；没有新编译、重链、native或性能采样。严格对照正式6clean/54native/36formal已由Root完成且输出正确；n64 B/A=1.6181061285，n256=2.0561126545，均越1.10。与旧窗口均值不能当配对因果估计，但每次都未过其独立固定门。

**完整Required已导入，跨owner value仍不inline；显式range却成功消掉root大多数overflow，不能笼统归因为AE使所有范围证明失效。** 严格结果尚未证明inliner具体cost或拒绝原因，下列阶段形状是可直接核对的事实。

## 输入与解码身份

正式默认根`out/next-optimization/thin-runtime-v1`，严格根`out/next-optimization/thin-complete-runtime-v1`。严格源码发布2549cbec8e1f3096d4c8c8a960459c83095ad13d/tree ebb993128355fed8228627c6d7b37d5cb0a90d45；strict protocol记录actual force-import-all，前后visibility witness均通过，不能把严格结果当默认零导入重复。

`decoded-strict-v1/decoded-index.json`包含两case×八unit×.3/.4共32个原bitcode路径、原SHA、解码argv/exit及输出SHA；逐项对saved receipt核原SHA、解码后再次核源SHA。`ir-analysis.json`保存完整函数call目标/属性/向量typed行/overflow统计；`prelink-shape-analysis.json`保存fn14/23三阶段body hash和实际调用/声明文本。原raw未改动，decoded目录不是新运行结果。

## 热点实际差别

| 事实 | 默认n64 | 严格n64 | 默认n256 | 严格n256 |
| --- | ---: | ---: | ---: | ---: |
| unit0 .3 AE value定义 | 0 | 56 | 0 | 224 |
| .3 root leaf调用 | 63 | 63 | 255 | 255 |
| .4 hot函数leaf调用 | 56 | 56 | 224 | 224 |
| .4 hot函数 | main | main | zil.fn.2 | main |
| .4 hot函数IR指令行 | 741 | 478 | 2113 | 1351 |
| .4 hot函数typed-vector行 | 154 | 189 | 225 | 147 |
| .4 root累加/主循环sadd.overflow | main64 | main2 | root254+main2 | main1 |

这些IR行数不是机器指令、SIMD执行数或pass remarks。严格.4中全部AE body已被剥为声明，残留call仍指向原跨owner value；原same-owner的7/31个value已inline，原跨owner集合未减少。严格n256连root和源main都已并入最终main，不能按旧独立root函数不存在就误判其工作消失。

具体`fn14`的.3头为`define available_externally range(i32 -2000, 2001) i32 @zil.fn.14(i32 %arg.0)`，.4仍为同range的declare；call实际为`tail call i32 @zil.fn.14(i32 range(i32 -250, 251) %v.15.i)`。fn23同样返回range且残留call。因此range被保留而且caller参数也已获得区间；root checked-add大量消失与此一致。仍保留的跨函数调用隔开其他owner算术主体，跨leaf组合不等于range证明，当前质量缺口不能只靠return attrs解释。

## 预链接造成的形状变化：已证与未证

两case的fn14/fn23均观察到：

| 函数 | canonical input | CGU prelink后 | 实际.3 import |
| --- | --- | --- | --- |
| fn14 | 45条IR，调用fn6…13八个helper | 178条IR，helper调用为0 | AE body178条；body文本与其prelink完全相同 |
| fn23 | 45条IR，调用fn15…22八个helper | 156条IR，helper调用为0 | AE body156条；body文本与其prelink完全相同 |

fn14预链接后仍有7个smul.overflow和8个sadd.overflow，fn23仍有4个smul.overflow和5个sadd.overflow。这些helper在没有root输入区间的CGU阶段先被inline；这些膨胀后body随后通过force导入，最终仍留下value调用。**阶段膨胀确实发生，尚不能仅凭45→178就断言它是最终拒绝inline的唯一原因。** 非exact参数跟踪、inliner顺序/cost、caller大小也可能参与，需要真实remarks。

Blaze ae1a7a7的LLVM prelink模块只lower当前own function，其他callee依请求声明，不含本例八个LLVM helper定义；因此不会在该LLVM阶段以相同方式inline八个外部定义。它保存一次prelink own/export再用于postlink，这一点已由精确源码核实。但Blaze的MIR inliner在O2/O3可能已经把callee built_in展开到当前own IR，其driver键也显式计入built_in依赖；**不能推出Blaze永不膨胀、一定保45条或一定更快**，也不能拿上游行为当本项目新方案运行证据。即使改为每函数prelink，后续postlink仍可能先inline helpers到value再拒绝value到root；这需要机制证据，不能跳过诊断直接实施。

## 现成remarks与后续诊断边界

冻结wrapper未配置opt-remarks-filename，当前保存材料未提供可直接证明fn14 cost/refusal的LLVM inline remarks；IR自身不能给出inliner阈值。Root随后只授权一次N64诊断重链：恢复严格窗口八份真实prelink bitcode并逐SHA核等、同原actual_command/CPU0,1/jobs2、空新cache/output，仅增加官方inline remarks输出选项。不native，不计为正式或性能样本，不改任何阈值、函数属性或输入；诊断结果将另写文件，不回填成此前已知原因。

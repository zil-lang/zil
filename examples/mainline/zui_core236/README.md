# 236：普通同步语言前置消费者（等待227/235）

这是五模块真实ZIL源码，不是ZUI框架。初始基线`a68398ad7`尚未实现227/defer及235/普通Product拥有Store child；本目录不能据源码存在声称已通过。具体执行身份和结果以`evidence/impl/236/`及仓外原始回执为准。

`Session`内联拥有`SubscriptionSet`和`NodeSet`两个完整child，各自管理真实Store和每项ByteBuffer。普通跨模块参数/返回、child取出/回填、扩容失败保持、先准备后替换与声明序关闭共同使用。没有新Session捕获、`Store<Session>`、拥有Sum、dyn或外部userdata。

输入为连续两个ASCII数字：数量`0..3`及模式`0..4`。其他输入或EOF退出9且不创建资源。数量0、1、2、3都实际执行；这不是任意规模图或性能验收。

| 模式 | 操作 | 数量2的无故障输出 | 退出 |
| --- | --- | --- | --- |
| 0 | 分支作用域正常落出，随后return | `SNLQabAB` | 0 |
| 1 | 提前return | `SELQabAB` | 0 |
| 2 | `?`传播Unit错误 | `SFLQabAB` | 1 |
| 3 | 实际扩容后，三次激活分别continue/continue/break | `SixixixNLQabAB` | 0 |
| 4 | 准备新订阅集合，清旧集合并提交 | `SabRNLQmnAB` | 0 |

`L`是defer之后成功构造的真实资源析构，`Q`是无捕获同步defer动作，a/b是订阅析构，A/B是节点析构。末尾`LQabAB`验证局部逆构造、动作履行与Product字段声明序的交织。`ix`验证每轮的局部析构和defer。`Q`不代表OS注销、drain或no-future协议完成。

`tools/language/236/oracle.py`独立按源码操作预测输出、退出及实际分配调用/释放，绝不读取编译器或机器输出。`ORACLE.json`冻结所有20个有效输入及4个无效入口，每个实际分配点失败一次，另含fail_at=0及末次申请之后控制：每个优化等级24次普通运行、151次分配探针。准备失败前缀、未插入的失败项、旧集合保留、新集合提交、注册前失败和注册后扩容/替换失败都有独立预期。字节峰值由探针记录，不猜机器布局或冒称RSS。

最近反例由限定测试按完整正例作两处修改：省略child回填后整值返回，预期Analyze拒绝；持有child的Store窗口后移动child并继续读旧窗口，预期Verify `E-BORROW-CONFLICT`。这两条预期尚待依赖合流后执行，不以其他早期Unsupported代替相应拒绝。

`leaf_control.zil`是另外的三模块既有叶形状控制；`defer_control.zil`是两模块基线观察。它们不计入联合五模块程序。`controls.py`只确认基线Unsupported及原API对象的真实分配ABI，成功不表示236或227/235通过。

合流后限定运行：

```sh
. /workspace/.zil-env/env.sh
export CARGO_TARGET_DIR=/workspace/.zil-env/target-236
export CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0
cargo test -p zil-driver --features llvm-api --test impl236_zui_core -- --test-threads=1
python3 -B tools/language/236/run.py --repo /absolute/clean/consumer \
  --cli /absolute/zil --compiler-build-receipt /absolute/compiler-build.json \
  --clang /absolute/clang --out /absolute/fresh-output
```

运行器使用原CLI产物作普通执行；故障探针捕获同次构建的真实LLVM API对象，再与既有realloc/free wrapper重链接，不用手写C/IR替代ZIL。构建收据须含`status=Pass`、`exit_code=0`、`git_status=""`、精确`source_commit`及`cli_sha256`。不构建Rust、不跑全仓、不改生产代码或产品门。

Rust四目标：本片期望补足自然完整拥有结构和同步退出义务的表达与有限安全/清理证据；未执行前这些均为待验证。没有Rust运行/内存、维护或编译成本配对测量，也不宣称产品级突破、完整1.4或完整ZUI。

后继普通API按真实依赖推进：拥有Sum/Option及variant切换；公共组件需要的关联项、高阶参数/结果与异构注册；Type/Syntax、生成trait/impl及真实`ui!`；最后是retained注册失败、停止接收、注销排空/no-future及最终清理。已有文本/Store/静态trait/固定回调家族不重列为全缺，不在236扩展这些范围。

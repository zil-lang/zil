# Manual native prelink 2321：非作者精确源码审查

## 结论

**无当前固定pure、无cache研究范围的blocking issue，可以继续准备并执行原两case协议。** 这不是产品采用、运行质量或编译收益结论；不要求因非现存风险扩大功能矩阵或再重建。本审没有运行工具链、LLVM/native或性能计时。

审查目录为独立clean checkout `manual-prelink-validation`，实核HEAD `2321e74897975978925dc7d2a611f7d672f716e1`、tree `03fa3d83dc5ba80d7849f7148b97b7d12449700b`，与作者local96e2同tree而非同commit。

| 精确文件 | SHA256 |
| --- | --- |
| `crates/zil-llvm-api/src/native/research_prelink.rs` | `09fe7a8d0bc76687c6eaa09455a5ed8fb699e2f6f78471e63204e049ab558c16` |
| `crates/zil-llvm-api/src/lib.rs` | `d9283a5608b32271c8a072c3b34710a5815885efd64e930952de55273f80e986` |
| `crates/zil-llvm-api/src/native.rs` | `e8def5f676e698f8dfdc46fed67c5c82b07a3cfb2d65f9dc5389b97e2573a6c9` |

## C ABI与所有权

对照本轮已pin LLVM23.1.4的`llvm-c/Core.h`、`BitReader.h`、`Linker.h`静读，新增Handle/指针、size_t/usize、unsigned/c_uint、LLVMBool/c_int签名吻合。外部只返回owned bytes/facts/receipt，不导出LLVM指针，也未引入第二份LLVM库。

`LLVMParseBitcodeInContext2`为eager parse，buffer由独立Buffer RAII保留至parse返回后释放；模块在同一Environment Context构造/解析，失败时Module RAII处理已返回指针。与消费buffer的文本ParseIR没有混用。`LLVMLinkModules2`无论成功失败都消费source；调用前`mem::replace(donor.0,null)`正确解除其RAII，失败返回整次错误，不继续使用被部分修改的dest。目的Module均晚于Environment声明，先销毁；Environment内machine/layout/context的drop顺序不会让Module存活过Context。

metadata entries数组用专门Drop销毁，拒绝分支也释放；递归MDNode带visited防环，允许标量/string，其他Value逃逸拒绝。调用控制只接受真实CallInst、bundles为零、operands=args+1、当前Use就是末尾callee Use、called Function准确且`LLVMGetCalledFunctionType==LLVMGlobalGetValueType(F)`；没有仅以opaque ptr类型相同替代真实函数签名。Invoke/CallBr/indirect/constant地址不走宽松分支。

## typed许可与真实图

backend同一次原`Proof`走`finish_checked`，原owner双射、完整source closure等于Allowed、原cost与checked-add预算算法及错误顺序保持；JSON只输出，不回读。无必要把预算再传入native重算第三次。新API接收typed Allowed但不授予ZIL Verified能力，当前唯一backend调用链来自这份当前CheckedProof。

`PrelinkedUnit`的BC、facts、signature/foreign/intrinsic/target字段均crate私有，无外部constructor/cache/deserializer，getter只借用facts。prelink先完整当前machine preflight/build/LLVMverify、标准prelink O2、再verify及实际scan；canonical definitions集合必须原样相等，新增非intrinsic或改签名拒绝。事实与序列化bytes来自同一实际module。

目的编译复用这些sealed实际事实建立唯一owner、全图及跨module签名表；不再为每个目的无条件parse全部8个BC。Required从全部own canonical定义递归求闭包，visited处理环，缺body/未知callee拒绝；跨ownerRequired必须属于当前Allowed。foreign只允许exact getchar/putchar签名或实际LLVM intrinsic ID，非`llvm.*`前缀猜测。解析仅own及有Required的donor，实际parse后scan必须与sealed defs/decls/calls/signature/intrinsic完全相等。

## GlobalDCE、链接及私有化

source按真实owner选Required，非selected定义暂Internal再GlobalDCE，前后verify；actualdefs精确等selected，不能静默保留额外helper或丢选中根。每次同Context链接后verify并核defs恰等own加已选imports，最终又核own∪Required。先对全部imports gather、核碰撞与逐Use，再统一原地name/Internal；不新增canonical声明、不手工copy/加强attrs、不加alwaysinline、不改阈值或预算。递归Function对象引用随rename保持；其他canonical owner不改。

此实现没有承诺任意Global/metadata/address/COMDAT/异常/PGO/CFI输入合法：当前model/pure proof、builder与scan拒绝这些范围。module asm/GC等不是当前structured model能注入的字段，不能从scan没有通用LLVM文本沙箱检查推导现存pure调用链有漏洞。将来扩大范围须独立审查。

最后verify→`thinlto<O2>`→verify→现有TargetMachine emitter，x86-64/空features/PIC/default/O2/after保持。新receipt明确manual、ordinary BC/no summary、非官方importer、无native cache和独立pipeline identity，不冒充旧default<O2>实际同管线或同码。

## backend及四项真实控制

显式研究门要求API/Linux x64/O2/incremental-after/8unit2jobs、fresh绝对receipt，拒旧exporter混用/closed/cache。双整图machine preflight、实际unit预检和原Complete planner clone保留；普通请求及旧exporter仍走原入口。两phase完整join，只跨线程共享不可变owned BC/facts；同jobs预算，不嵌套叠加worker。排序只算一次unit成本，结果按unit顺序收错，取消/错误不发布；对象先写原staging，最终仍走旧native link和发布路径。Config/Receipt/model未改，Cargo仅unsafe边界注释调整，无依赖/预算变更。

新增四项test源码已逐读，均不mock LLVM：合法typed模型及LLVMverify后，普通直调一个callee Use通过、同CallInst兼参数地址两个Use仅callee通过；8个真实prelink unit完成私有导入与ELF对象生成；实际Required缺Allowed拒绝；真实无效bitcode与重复强定义link失败覆盖buffer/source释放路径。它们不等同于完整递归、metadata、所有链接冲突或通用地址矩阵，不作这种额外宣称。

已有执行证据是Root统一窗口产生的`out/next-optimization/manual-prelink-build-2321-v1/binding.json`：64 native API tests（含新4）、6 selected proof tests、no-native check、release均Pass，binary SHA256 `ce90681f261c109c412bb9d513e3c0ff58680a2e593d798cdf21b3e27daa407a`。本审只读取该收据及源码，没有复跑或大binary hash；研究字段dead_code warnings不影响当前准入。

## 仍待正式实证

两case实际完整请求及原1.10/.90运行门尚由新冻结协议检验。普通BC手工链接和无summary postlink、donor DCE/解析次数、prelink阶段顺序仍可能影响质量和编译成本；当前源码安全边界通过不能预填收益，也不授权many-edit/cache/并发或生产合入。

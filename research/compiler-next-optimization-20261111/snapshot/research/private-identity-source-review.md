# 请求内工具身份复用：指定源码静审

## 结论与范围

初审指定 `common.py` SHA256 `10241bf71de0058ee53a720424ee1166fb7e45de8ae4eb10ed8664f9d463f160`，补审作者新增exact9role后的 `5800eef2ac10c31463bc10cf69c7b0ffd3b342700fd835609b14bfc1178b53f2`；`clang_wrapper.py` 保持 SHA256 `9006574e40259d3dc514d2f68939ac0669b6ea5ce42bcfb5a8cdbb72025f66ba`。均按实际字节读取、与旧private版本比较。**在固定角色协议、每个B请求来自全新输出目录、不resume的受控runner范围内，未发现本次身份去重的现存执行阻断。** 本报告并补审 `compile_screen.py` 精确 SHA256 `13d8c5e7b843c3bfcbfcaf05fb10748f4035bbbf01890ce5fabb1c8c2c581856`。未执行任何控制、编译、LLVM或native。

## 源码确认

1. wrapper流程仅将入口`validate(p)`换为`validate_request(p)`；32次外部LLVM verify、真实prelink/.3可见性、完整原budget/owner proof、exact8插件收据及before/after全文比较未改变。首个wrapper在完整ZIL请求启动后才进入首核，没有将首次大文件SHA移出B计时。
2. `validate_request`先清除本解释器旧`_REQUEST_IDENTITIES`，实际hash当前protocol与Rustreceipt，context绑定protocol SHA、receipt绝对path及SHA。同名旧receipt变更、换protocol或同目录跨request复用会因context不等拒绝；没有以旧stamp替代当前机器proof。
3. `flock(LOCK_EX)`串行首核；首个持锁者保留旧`validate`全部读取，包括同clang alias的重复读取。每个登记file的hash前后stat一致、同路径多次hash记录一致，完整validate及末端全工具guard成功后才以exclusive-create发布stamp。后续持锁者检查format/context/path集合/completed/digest格式及全部stat，再运行原`validate`，只有固定登记path的SHA读取变为stat guard。
4. `sha`只查精确`abspath`字典key，无目录前缀或后缀匹配。普通protocol/scripts/buildlog/CPP/currentproof/IR/BC/output仍走`read_bytes`真实SHA；它们只在错误地被配置成工具path时才可能命中缓存。记录包含target的dev/ino/mode/size/mtime_ns/ctime_ns、最终路径分量自身lstat及resolved path，能拒绝一般原地改写、替换、symlink retarget；只恢复mtime不足以逃过ctime检查。
5. 每个实际工具`run`前后执行全部固定工具guard。后guard失败前raw已经保存，另写`fixed_tool_after_guard=Failed`并抛错。工具异常/timeout沿旧owned-group流程拒绝且保存失败证据。前guard失败不启动工具，wrapper异常日志保存原因，不应宣称存在一次工具进程attempt。`process-attempt.json`与raw可能在后guard注释上不同，判定必须看完整请求退出及raw，不能只读attempt的exit=0。
6. 首核异常通过finally清capture；fd上下文退出释放flock。写stamp中断可留下截断文件，后续JSON读取拒绝，不删除重试或静默重建。并发首核正常只发布一次；异常后可以有另一个wrapper重新开始完整校验，但本次编译仍有失败子任务，完整请求不能算Pass。

## 需要在最终freeze明确的两个边界

**登记来源依赖fresh runner，现已显式保证。** common本身接受已经存在且context/stat一致的stamp；`full_validate_completed`等字段不是密码学签名，不能独立证明它来自当前进程首核。上述13d8 runner使用全新out、每round/arm独立fresh process-log root，并在启动B前明确拒绝已存在wrapper目录；旧run.py也补同一断言。正常路径不会预存wrapper/stamp，足以界定这次研究。runner在完整请求返回后核stamp context、full_hash_calls、bytes及每tool digest。若将来允许旧log目录、预置stamp、目录恢复或不明来源输入，common不能单独授权复用，必须拒绝或重新完整SHA。

**工具角色现已固定。** 5800版本的`_tool_paths`先要求exact baseline/exporter/rustc/clang/lld/opt/bcanalyzer/dis/plugin九个角色，才取其path并登记plugin构建compiler；新增`proof`/`IR`角色会拒绝。plugin compiler按完整validate被绑定为clang SHA，实际协议中为clang与clang++两个invocation path、同一resolved文件，因此是**10次full hash调用、10个distinct invocation paths、9个distinct resolved physical files**。

这是固定可信本地研究工具的偶然修改检测，不是对恶意同用户伪造stamp、修改工具后恢复所有可见状态或运行时重定向路径的安全沙箱；原身份校验也不提供这种保证。无需为本轮额外引入权限/签名系统，但不得将此实现宣传为不受fresh边界限制的缓存能力。

## 13d8完整请求runner

固定两case各执行六个ABR全排列，每arm每case六个完整cold编译，总36；没有以编译器内部seconds替代外部完整进程wait4。CPU affinity在所有编译前固定为协议两CPU；A保持LLVM API、B保持Clang+原完整证明、R保持O2/overflow-checks/panic-abort/8CGU和同clang linker，没有Rust多给核。B首核仍在该请求计时内，runner事后验证首核10次/fullbytes，不能把上层计时外validate误充本次首核。

每产物实际执行00、ff、41三个stdin，共108native，其中41的36次为formal，其他72次只用于功能检查；两个功能启动先于该产物formal。按case/arm每格六个编译和六个formal运行求mean/p95等，编译比取B/A完整wall和CPU，runtime比独立以formal wall计算B/A≤1.10、R/B≥.90，任一输出错误也失败。没有把nested工具CPU再加到完整请求，没有丢outlier，没有虚构原weighted总门。不同编译产生的SHA作为集合记录，不强迫与A机器码相同，也没有因跨arm不同而省略native质量门。

完整请求raw/attempt由原process helper先保存，再解码和核证；通过验证的编译追加到compile-rows，native各自raw/record及jsonl保留。launch/timeout异常留下原process attempt和fatal/incomplete manifest，不能算完成格；完整执行但质量NoGo则保留Complete和runtime_gate_pass=false并退出1，这不是“不完整”。新鲜staging、明确无object cache及B每次独立empty native cache只支持cold编译声明，不支持many-edit或并发推断。实际六点p95是有限样本描述，不是稳健尾延迟证明。

## 最少控制建议（未执行，不要求新增矩阵）

最终screen本身会通过真实九个wrapper、真实validate和首核计数验证大部分路径，因此不再要求另造完整协议/mock校验控制。唯一较小且有判别力的补充是ROOT外真实临时文件的stat guard反例：登记后等长度改写再恢复mtime，ctime必须使guard拒绝；它只证明guard分支，不能冒充真实首核。下面是初审提出的覆盖点，前三项中首核计数/跨请求绑定主要已由真实screen证据覆盖，未另写或执行控制。

- **首次与并发复用：** 两个进程对同fresh请求调用实际`validate_request`；首核stamp只创建一次，full_hash_calls恰为10（按固定images列表计；10 invocation paths对应9 resolved physical files），bytes精确，双方通过；串行第三次不重写stamp。此项验证flock，不只检查源码中出现flock。
- **固定工具变更拒绝：** 登记后实际改写一个同长度小tool文件，并用utime恢复mtime；stat guard仍须因ctime/其他字段拒绝。可用同bytes另inode替换作为同一控制的子项。
- **动态数据仍实SHA及跨请求拒绝：** 登记后改一个未登记IR文件，普通`sha(IR)`必须读出新digest；改当前Rustreceipt或换protocol/context后，不得复用原stamp。工具role注入动态文件应由上述exact角色schema拒绝。
- **首核/发布失败不变成功：** 真实错误buildlog digest使完整validate失败，不能产生completed stamp；预置截断stamp必须拒绝且保留，不自动回退为成功。修复fixture后另起fresh请求才可成功。

工具stat复用有效性和质量/完整编译收益是不同门。本报告不把首次读取量减少当作wall/CPU实测，也不更改原质量限制、CPU总预算或编译screen规模。


## 最终协议与发布freeze复核

- 发布source `3d0dc3e1f420ebea68aff66ebbcd86111251bffc`，tree `dbbc630ebb80319fd62aaab438e44e7f80ee9307`；独立读取Git中`research/compiler-thinlto-private-identity/`的15个blob，均与本地15文件freeze逐SHA一致。
- `research/thinlto-private-identity-source-freeze-v1.json` SHA256 `802ef50b27d5efa4889b96aa47fad0e9e3a23658dfd873ab16eaa1845e18c45a`：15文件全一致。其中README是第15个文件，protocol的scripts按原规则只收14个.py/.cpp，集合和digest均准确一致，没有缺测脚本。
- 实际protocol `out/next-optimization/private-identity-protocol-v1/protocol.json` SHA256 `722a146dd2bee2cbf5e4f1550532bf4404829bfc8aed46a956925590e4989215`。整份cases、tools、build_receipts、plugin_build_receipt、orders、limits、cpus、jobs、source_claims均与原private208协议对应字段完全相等；不是只比source hashes。
- Path.resolve实算10个distinct invocation paths与9个distinct resolved physical files：clang与插件构建compiler clang++为同一物理文件的两个调用路径。此前“9 unique paths”措辞已更正；首核10次full hash合同及计入B请求的约1.031GB读取不变。

本次最终复核只读文件/Git blob并更新报告，没有prepare/编译/LLVM/native/计时，也未新增控制。当前无运行前阻断；随后暂停工具，等待36请求唯一窗口结束。报告不预填任何尚未执行的结果。

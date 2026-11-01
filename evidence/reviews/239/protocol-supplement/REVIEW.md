# 239 IR/Wire/CT 补充只读审计

审计点 `ca1520d1b4ac827d81ea8c43e85bff6ab07486ab`，/workspace/zil-239 工作树clean，作者最终source SHA逐项核对一致。未修改任何生产、已交241树、卡或规范；未审自己的241。最终共同候选尚待root冻结，本结果不代替244整体独审。

结论：当前未发现239范围阻塞。

- lib.rs 2300–2330逐项编码四opcode90–93、source local、path长度/每层variant+index、Unpack variant和完整有序fields、Drop local；指纹域V30。实际补探针9种独立操作数/次序变异均改变fingerprint并精确保真往返。
- wire.rs Writer/Reader两端按max_call_depth限制path，max_locals限制Unpack outputs。Reader count先检查上界、累计扣nodes、checked_mul最小记录字节并校remaining，再with_capacity；Writer count共享nodes且先限长/扣费后逐项编码。此为原始传输，保留重复/越界LocalId是正确的，不替代Verify别名/类型/存活检查。
- 树外公开codec实际补充四op组合N=17，N−1 ResourceLimit，fresh N成功，读写一致；深度/宽度上界与恶意u32::MAX长度均拒。未知94..254共161值全部E-WIRE-OPCODE；255为既有显式Unsupported，不能误称未知。追加伪cap字节E-WIRE-TRAILING。
- 旧外层Wire version27/28/29实际E-WIRE-VERSION；原v27/v28/v29独立fingerprint编码源及旧攻击wire/json/ORACLE共7份与dispatch79ed逐字节相同，详frozen-legacy.json。raw TypedModule里的schema字段仍是待核数据，不据codec成功授权；真正旧外层版本明确拒绝。
- VerifiedModule.limits和uses_owned_sum_extension是私有字段，唯一构造在check_module成功分支，limits只克隆本次请求、presence只接核内派生返回；公开方法仅&Limits/bool，无可写访问。raw TypedModule/CandidatePlan/Wire无这些字段，也不能传输Verified wrapper；因此额度快照与优化presence不制造原始权限。
- CT VM对4op有显式Forbidden分支；CT独立检查先普通Verify，再serializable和closed opcode白名单，4op均不在白名单。已读作者最终3组测试及原始收据：90/91合法Copy局部观察先ordinary raw/Wire Verify成功，然后精确E-CT-FORBIDDEN；92/93使用全类型非Copy而活动Empty的真实拥有Enum，先ordinary raw/Wire Verify成功，再CT Type gate精确Forbidden。后者通过CT类型门拒，不声称VM执行新拥有操作。旧SSA VariantTag/ReadVariant两例仍由真实CT VM返回0/7。未重复作者整套测试，未把protective Unsupported记CT成功。
- declaration generation内的Enum tuple字段两个driver walker只改field.ty，保留manages/span；这不是运行时资源CT执行。239已有生成/手写比较与源码指纹记录，不在本补审中重跑机器。

新增执行仅 /tmp/review239-protocol/src/main.rs 的公开IR/Wire小probe，依赖base/ir，不调用Analyze或Verify，也没有native语义/完整安全声称。精确命令/来源hash见RESULT.json，stdout/stderr在run01.log。作者原有IR65、CT3等是阅读证据，未计为本审执行次数。Formal/full/product和共同SHA绑定均未执行。

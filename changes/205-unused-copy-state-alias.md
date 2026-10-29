# 205：未实例化普通Copy T State的旧别名定义检查

作者与非作者分别用冻结204 CLI05（source7abbbac1f6479c52f4967653acfab74fc56c945d，CLI SHA baa0b5757871b5f7bc31713f61c0a91042d1396087d9bddc65d6872c82a4312d）重放原02。实际生产输入与204正式4fa100e4一致，旧规范92c5身份保留，不倒标新8d6规范。

原源SHA256：`2601332cc0d78658ac96f105e83fea18ce8544bccafe45f2e1ccc8e0254b2d62`。原样输入：

```zil
package State<T,F>{manages bytes:ByteBuffer;a:F;b:F;tag:T}
fn recycle<T:Copy,F:Fn()->i32>(state:State<T,F>)->State<T,F>{
    var next=state
    let view=&next.a
    let replacement=next.a
    next.a=replacement
    view()
    return next
}
fn main(){}

```

实际编译Success、Verify1，但未调用的T:Copy函数被Expand省略，Core只有可达/具体内容；这不是原坏正文得到借用安全证明，更不是已执行的use-after-free。负例未native。作者与独审原结果各在run-logs/205-expand/baseline01与run-logs/205-review-by197/baseline01保留。

集成方授权在既已登记generic_check/generic_places/generic_loop_control/generic_aliases写集中修复该源定义漏检。适用于新205混合入口及实证旧单State定义中的实际借址/别名操作；恢复既有窗口与未用定义规则，不新增安全权限，不改变规范hash。完整State/field真实位置和代次必须保留，take/refill不能复活旧alias，分支/循环/条件和callee-before-arguments均按实际使用检查。无此新操作的旧路径与具名精确费用保持，不能用全局保守拒绝代替last-use/sibling近邻。

原01/03在旧204只遇混合头Unsupported，不能计作alias核通过。修复须用相同字节重新检查02，并以真实last-use、兄弟字段、不冲突转交、take/refill旧epoch和调用准备负例闭合。Types与raw Verify独立入口另核，不能以Expand提前拒绝冒充。完整1.4/B0/产品状态不变。

## 非T最近对照与预算补充

去除普通T后的最近原样顺序同样实际Success/Verify1。Core保留recycle，但Typed只main，仍未native；原source SHA256 `7a88eac73129444f5035e476ce4c406beb12b902b37f63bff466dd98f54c718a`，固定副本见evidence/impl/205/preflight/04_single_state_non_t_alias.zil，原阶段/命令见run-logs/205-expand/baseline-non-t01。该实证否定了“非T保留Core即已检查旧alias”的假设。

修复覆盖有限owning State定义中的真实borrow/alias，而不只含普通T。先实跑并保留原N1731控制；若新位置/别名检查带来真实新增工作，按同输入记录新N及N−1/fresh N，不用旧数字阻挡正确检查。此为具名实际缺陷的预算规则补充，默认limits/product阈值不变；设计§8.2.3及接口同批更新，新规范`9813f43ee7c86d786f197f0ed4b8e65938ee354d94b2cdaec858e09e7871ce5e`，所有旧运行仍保持原规范身份。

## 两个公开Types入口的独立责任

独审legacy-raw01在clean71a3b24a12788a87a9f81c30ae1fcc7af2a4e5bb实际0/1。以旧非T单State最后使用合法近邻经Expand得到的Core为控制，保留旧空state_parameters和单callable_parameters头，仅将原view() AST从take前移至refill后，保持NodeId/类型；合法控制两入口通过，坏未用Core也被两入口接受且Typed只有main。此为raw Core定义检查漏检，未执行内存漏洞。重放测试是crates/zil-driver/tests/impl205_independent.rs::raw_legacy_single_state_alias_cannot_depend_on_expand_having_checked_the_body。

Types必须沿真实既有有限单owning State头独立建立私有声明形状和实际Position/epoch账本；不伪造公共State表，也不依赖Expand已检查。两同F字段不能沿generic_index位合为一个消费值。新增遍历/快照/比较全部预费；具名旧State输入因这次修复增加的实际成本按现9813规范保留同源旧N/原Fail及新边界。未涉及该修复的旧路径保持。

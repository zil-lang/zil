# G3.4.1-methods 实施合同

Goal G3.4.1；父合同 `_zil_design.md` §18.1/V10 与原 `tests/language-readiness/terminal/342/slice_iter.zil`。派工 root，实施 general_priority_review；基线 main c63001c8e。规范摘要由本单执行收据记录，实施者不兼任独审、不合入。

写集 generic_inherent.rs、generics.rs 必要注册/入口、generic_expr_call.rs 必要调用钩子、generic_stored_nominal.rs 名义实例记录钩子、generic_source_life.rs 必要 enclosing scope；本 changes、限定测试/收据。与 life 作者 lifetimes.rs/Option<&T>/Types 分离，公共 schema/锁/正式性能门不写。

先 `cargo check -p zil-expand -j1 --offline --locked`，重链接及 driver 限定消费者由 root/life 协调独占。保留真实源码、未用定义正反控、预算 N/N−1/fresh，原 SliceIter 原字节合流。正常提交/push，非作者精确审核与正式门由集成方裁决。Rust 四目标：新增普通方法组合语义；同语义运行/RSS、编译和维护成本均未测。

规范 spec_hash e235b19010325a125338f764f9e75b8ab6dd612f75ea4915e4761edd4a7e0b7a；normative_hash e938daf9f797c6790549c578e25d474d6eafeba20bf50ee51b282fbaa653f80b。依赖 life 词法提交 e48ca12809b971039dc387ee24b10d6986c50cfd（本枝 cherry-pick 5a0a12e8f）。

方法先 lift 为有完整 impl binder 的原成员普通函数，再进入 generic_source_life 验证；无需取消其 ambient impl 保护。generic_check 的 scoped key 仅对真 nominal class 应用启动，Copy 继续完整检查。限定 Expand generic_inherent 初版3/3；最新source-class钩子 no-link已过，合流SliceIter/机器/预算与独审未完成。

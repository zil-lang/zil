# Independent complete-parent CT probes

Reuse existing terminal/344/anonymous cases for static-member-instance, member-context, generated-member, mixed-runtime-instance and phantom-fixed-context. Only generic-host.zil is new: a concrete Maker<bool> static mixed CT factory. This is a Source consumer, not synthetic IR.

Baseline release executable SHA256 91ba589a33517724e20f4ab9645389850effb9dfef86cf641b08ad4b7e4bbccf is bound by the prior PR45 complete.json to source 0fee93aa7eb7fa0fe2f1fe9e86fb3f63f019566b; its production crates diff to accepted fd261b22 is empty. Static member, ordinary member and fixed-context cases already succeed through Verify. Generated-member rejects in Expand for missing explicit lineage, mixed-runtime-instance rejects in CT for unknown type name, generic-host rejects in CT for lacking a concrete ordinary callee. These are baseline observations, not new implementation acceptance. Logs remain outside source under the root phase output.

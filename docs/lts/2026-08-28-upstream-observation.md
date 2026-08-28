# grok-build upstream observation — 2026-08-28

Status: **observed and classified; not integrated**

本记录固定 2026-08-28 的 public source 状态刷新。它不改变 integrated source baseline、
产品版本、binary、Release、安装或运行环境。前次观察
[`2026-08-23`](2026-08-23-upstream-observation.md) 的三树分析和分类仍然有效。

## 固定坐标

| 项目 | 值 |
|---|---|
| XAICode integrated upstream | `8a14c91d88875a831a38b3a066b1683116bcb31c` / crate `1.0.0` / `SOURCE_REV` `27b3c66635e2c0bf213429a36ab916f25d59df20` |
| Previous fixed public observation | `07b2f7144fd5c5c9d3dd1966937a87852d2dbdb8` / crate `1.0.8` |
| Current public target | `9684fa3cdbf2995e30ea8b9b637f1db008f144fc` |
| Observation / target commit time | `2026-08-28T12:00:00Z` / `2026-08-27T13:30:20Z` |
| Target crate / `SOURCE_REV` | `1.0.10` / `70ec060ec3d28e77b9c4593be43c2ab0128bcd21` |
| npm stable `latest` | `1.0.5`, published `2026-08-16T00:25:35.078Z`, `gitHead` `5115b46bc909ae5c7f5fc064455197440e796b6b` |
| Source/distribution mapping | `unmapped`; npm `gitHead` does not equal the public target |

Public target is a descendant of the integrated baseline and previous observation. It is
16 public commits ahead of `8a14c91` (4 new commits since the `07b2f714` observation).

## 4 个新提交的变更摘要

所有 4 个提交都是 `Synced from monorepo` 批量同步：

| 提交 | 日期 | 重点 |
|---|---|---|
| `07b2f714` | `2026-08-23` | 已在前次观察中记录 |
| `c2ad97f` | `2026-08-24` | plugin marketplace CTA、MCP server-name merge、compaction timing、hunk tracker off |
| `77cd7eb` | `2026-08-25` | subagent sampling gate、shell history fix、startup telemetry、status line minimal mode |
| `9684fa3` | `2026-08-27` | **442 files** — hooks prompt gate、sandbox io_uring fix、security Bearer leak fix、auth AuthBackend trait、computer-hub BotRelay、dashboard workspace、MCP elicitation、websocket 0.28 unify、xai-grok-home→xai-dirs rename、new crates |

## 增量分类（仅 4 个新提交相对于 `07b2f714`）

### 安全修复 — 纳入下一迁移切片

| 变更 | 理由 |
|---|---|
| `sandbox: block io_uring child-network bypass` | 沙盒网络逃逸向量 |
| `security: stop installer sending deploy key as Bearer to attacker-settable URL` | 严重凭证泄露 |
| `sandbox: fix Path import on Darwin enforce builds` | macOS 构建修复 |

### 构建兼容性 — 下一切片编译时可能需要

| 变更 | 理由 |
|---|---|
| `unify websocket crates on 0.28` | 消除重复依赖栈 |
| `derive_more` adds `as_ref` feature | 被新代码依赖 |
| `xai-grok-home` → `xai-dirs` rename | XAICode 已无此 crate，不需要处理 |
| `xai-grok-workspace default-features=false` | sandbox-enforce 拆分 |

### 行为变更 — 推迟到功能切片

| 变更 | 理由 |
|---|---|
| `hooks: UserPromptSubmit blocks + queue hold` | 新功能，非安全修复 |
| `auto-mode: auto-allow mkdir/touch` | 权限行为变更 |
| `shell: headless sessions default always-allow` | 无头模式权限默认值变更 |
| `auth: AuthBackend trait` | 认证架构重构，高风险 |
| `shell: gate mcpServers on folder trust` | 新信任门控逻辑 |
| MCP `elicitation`/`owned_clients` 新模块 | MCP 新功能 |
| `pager: reconstruct turn-end markers on resume` | 功能修复 |
| `tools: wake task waits on child exit` | 进程管理修复 |

### 拒绝 — 托管控制面

| 变更 | 理由 |
|---|---|
| `chat/gateway: identity stamp + rehydration` | xAI 聊天网关 |
| `computer-hub: BotRelay connection manager` | 远程 relay |
| `dashboard: workspace members/adoption` | 托管 Dashboard |
| `workspace OIDC proactive refresh` | 托管认证 |
| `tracing: detach turn-end uploads` | xAI 遥测上传 |
| `TUI: retarget model slugs to grok-4.6` | 第一方模型 |
| BotRelay protocol (Swift/Kotlin, ~7000 lines) | 托管 Bot relay |
| `process_identity()` + `set_identity`/`set_release_channel` | 遥测身份标记 |

## intake stage 更新

`2026-08-23` 观察中建议的 5 个 intake stage 不变。本刷新增加的 4 个提交属于
stage 5 (`19d42e3 -> latest`) 的增量，不创建新 stage。安全修复应在下一迁移切片的
最前段优先吸收。

## 当前结论

- 本刷新仅更新 provenance 记录，不导入上游源代码。
- 下一个迁移切片应首先处理 3 个安全修复和构建兼容性变更。
- 行为变更推迟到独立功能切片，按既有 LTS 分类规则逐段审查。

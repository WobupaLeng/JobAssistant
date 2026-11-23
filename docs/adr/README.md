# ADR 索引

| 编号 | 标题 | 状态 |
|---|---|---|
| [ADR-003](ADR-003-frontend-leptos-to-vue.md) | 前端由 Leptos 0.8 WASM 改为 Vue 3 + Vite + TypeScript | Accepted（2026-11-22 用户裁决） |
| [ADR-004](ADR-004-legacy-chat-orchestration.md) | 聊天编排回归冻结 SPEC——否决整页化，落实同屏常驻与就地展开 | Accepted（2026-11-23 用户裁决「方案 1」） |
| [ADR-005](ADR-005-legacy-token-accumulation.md) | 在 LlmClient 边界累计 token 计数（tokens_total/greet/score）——TokenMeter 装饰器 + 单事务原子自增 | Accepted（2026-11-23 Chief 派单，存量架构设计裁定） |

实施图纸见 [`docs/specs/`](../specs/)；UI 冻结真源见 [`docs/ui/UI-SPEC-chat-detach-v3.md`](../ui/UI-SPEC-chat-detach-v3.md)。
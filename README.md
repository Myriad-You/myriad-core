# Myriad core

Myriad 的共用核心：决定「她是谁、她能做什么」的规则只写在这里一次，各端照同一份规则运行，同一输入得出同一结果。

- **纯规则 crate**：`myriad-merope`（心情、说话、作息、群、游戏、rig 的语义……）、`myriad-agent-rules`（回想、提示里的不可信围栏……）、`myriad-persona-sync`（同步她的格式、规范字节、时钟、导出文件）、`myriad-persona-log`（多端的日志、合并、折叠、执笔租约）、`myriad-nodes`（她的节点：三种节点、头和身子、关系、折叠、放置和整理的输入输出、整理队列）、`myriad-her-link`（她的连接的帧）、`myriad-perception-filter`（看屏幕时的宿主过滤）。
- **`myriad-core-ffi`**：把上面的规则包成一个静态库，对外只有一个 JSON 进、JSON 出的 C 入口，给不用 Rust 的端（今天是 macOS 原生 App）。
- **`crates/conformance/`**：各端编排同一份规则时都要过的一致性对照。
- **`shared/`**：这些规则读的契约（她的连接、rig、表演、来源表、群）。

谁在用：[Myriad](https://github.com/Myriad-You/Myriad)（站点，后端直接依赖 crate）、Myriad 的 macOS 原生 App（经 C 入口）。各端都按一个提交钉住本仓库。

说明见 [docs/SHARED_CORE.md](docs/SHARED_CORE.md)，工作规则见 [AGENTS.md](AGENTS.md)。

```sh
cargo test --workspace
cargo test -p myriad-merope --features test-games
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p myriad-core-ffi
```

本仓库 2026-10-26 带着历史从 Myriad 拆出来，之前的提交记录照原样保留。

## 许可

AGPL-3.0-only，见 [LICENSE](LICENSE)。

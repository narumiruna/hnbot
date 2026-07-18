# hnbot Rust 分階段改寫計畫

## Goal

在同一個 repository 內以 Rust 完整重寫 `hnbot`，先與 Python 版本並存並完成離線行為對照，再切換 Docker Compose；確認 Rust 服務穩定後移除 Python、uv、PyPI 與 Logfire。完成後唯一受支援的執行介面仍是 `hnbot serve`。

## Context

- 採用使用者選定的「分階段切換」：Rust 未達功能與測試對等前，Python 仍是正式部署版本。
- 最終發布方式為 Docker Compose only；不發布 PyPI、crates.io 或 GitHub binary。
- 可觀測性改為 Rust `tracing` 結構化 stdout 日誌，不保留 Logfire/`LOGFIRE_TOKEN`。
- 遷移期間禁止 Python 與 Rust 同時對正式 Telegram/Telegraph/Redis 執行有副作用的流程，避免 `exists`/`set` 間的競態造成重複通知。

## Architecture

在 repository root 新增 Rust binary crate（edition 2024，`rust-version = 1.88`），Rust 原始碼放在既有 `src/` 下的 `.rs` 模組；遷移期間可與 `src/hnbot/` 並存，最終刪除 Python package。

主要模組責任：

- `config`：以 `dotenvy` + typed parser 載入並驗證環境變數。
- `rss`：以 `reqwest` + `feed-rs` 取得及解析 HNRSS，維持 entry 反轉順序、points/comments/id/date contract。
- `http`：共用 transient retry、`Retry-After`、exponential jitter 與 HN request pacer。
- `article`：以 `serde`/`schemars` 定義 `Article`/`Section`，實作 Unicode-safe chunking 與遞迴摘要。
- `openai`：直接透過 `reqwest` 呼叫 Responses API JSON-schema structured output；支援 `OPENAI_BASE_URL`。
- `telegraph`：直接呼叫 createAccount/createPage API，將允許的 HTML subset 轉為 Telegraph Node JSON。
- `telegram`：直接呼叫 Bot API `sendMessage`，維持 HTML escaping 與訊息格式。
- `store`：以 async `redis` crate 保存既有 `hnbot:entry:{id}` key/value contract。
- `app`：以 Tokio task、兩組 semaphore、batch isolation、signal cancellation 與 client cleanup 編排服務。
- `main`：以 `clap` 保留 `hnbot serve [--poll-interval >= 1]`；裸 `hnbot` 顯示 help，`main` 無效。

主要 crates：`tokio`、`clap`、`serde`、`serde_json`、`schemars`、`reqwest`（rustls/json）、`redis`（Tokio）、`feed-rs`、`chrono`、`url`、`regex`、`html2md`、`html5ever`、`dotenvy`、`tracing`、`tracing-subscriber`、`thiserror`、`tokio-util`、`futures`、`rand`。測試使用 `wiremock`、`assert_cmd` 與 Tokio test utilities。所有版本由 `Cargo.lock` 固定。

## Important behavior and interface contracts

- 保留以下設定名稱與現有程式預設值：`OPENAI_API_KEY`、`OPENAI_BASE_URL`、`OPENAI_MODEL=gpt-5-mini`、`BOT_TOKEN`、`CHAT_ID`、`ARTICLE_LANG`、Redis、HTTP、concurrency、chunk、feed、batch sleep 與 polling 設定；`FEED_POINTS` 程式預設維持目前的 `200`。
- 啟動時明確驗證 OpenAI、Telegram 必填設定及所有數值範圍；未知環境變數忽略。
- Feed 失敗經 retry exhaustion 後使該 batch 失敗，但 service 記錄錯誤並於 interval 後繼續。
- Comment 取得、OpenAI 不可處理輸入、文章/Telegraph 失敗只略過該 entry；成功送出 Telegram 後才寫 Redis。
- 同一 batch 等待所有 sibling tasks 結束後才回報未處理例外；batch 不重疊。
- HN comment request starts 維持全域最小間隔，429 會延長全域 cooldown。
- 不在 CI 呼叫真實 OpenAI、Telegram、Telegraph、HNRSS 或 Redis；外部 API 以固定 request/response fixture 與 mock server 驗證。

## Non-Goals

- 不改變文章 prompt、Telegram 文案、Redis schema、feed 篩選語意或 polling 模型。
- 不新增 Web UI、管理 API、dry-run production mode、資料庫 migration 或多 instance locking。
- 不發布 Rust crate/binary；只建置 Docker image。
- 不保留 Python extension/plugin 相容性；`html_to_markdown` 等 Python-level API 在最終切換後移除。

## Assumptions

- Linux Docker Compose 是唯一正式執行環境。
- 現有 Redis volume 必須原地沿用，不清除、不改 key。
- Rust stdout tracing 足以取代目前可選的 Logfire；最終從設定與文件移除 `LOGFIRE_TOKEN`。
- OpenAI Responses、Telegram Bot 與 Telegraph 使用官方 HTTP API contract，不依賴特定 Rust SDK。
- 改寫計畫儲存為 `docs/plans/2026-07-18_rust-rewrite-plan.md`，執行時依完成證據逐項更新並在全部完成後封存。

## Plan

- [x] 建立並保存本計畫，記錄 Python baseline contract 與目前 `just all`、CLI help、Docker Compose config 結果；Python baseline `just all`（58 tests）與兩個 CLI help 成功，Compose file 未改動且後續由 GitHub CI run `29637092472` 的 `docker compose config --quiet` 補驗成功。
- [x] 新增 root `Cargo.toml`、`Cargo.lock`、`rust-toolchain.toml`、Rust modules 與 Rust CI job；edition 2024/MSRV 1.88 已固定，Rust format、strict clippy、48 tests 全部通過，`just python-all` 亦以 59 tests 通過。
- [x] 實作 typed settings、domain models 與 `clap` CLI contract；所有既有 defaults/validation、`.env`、poll override、裸命令 help 與 `main` rejection 已由 config/CLI unit tests、3 個 `assert_cmd` tests 及 live help smoke 證明。
- [x] 建立 `tests/contracts/parity.json` 與既有 RSS XML 共用 fixtures；Python/Rust 同時驗證 retry-after、HTML-to-Markdown、article rendering、Telegraph sanitizer、Telegram escaping 與 RSS contract，雙方 fixture tests 通過。
- [x] 實作共用 HTTP retry、數字/HTTP-date `Retry-After`、exponential jitter、global pacer/cooldown、RSS 與 comment fetch；Wiremock、paused Tokio time 與真實 RSS fixture 覆蓋 429/503、三次 exhaustion、spacing、malformed feed、entry order 與 metadata。
- [x] 實作 HTML-to-Markdown、Unicode-safe character chunking、`Article`/`Section` schema、原 prompt、constraint validation 與遞迴摘要；空內容、單/多 chunk、Unicode boundary 與 invalid constraints 均有離線 tests。
- [x] 實作 OpenAI Responses HTTP adapter；Wiremock 已驗證 URL、Bearer auth、model/base URL、strict schema payload、parsed article 及 400/429/5xx/invalid-output mapping，settings/Telegram transport tests 證明 secrets 不進入 debug/error text。
- [x] 實作 Telegraph sanitizer/Node JSON、createAccount/createPage 與 Telegram sendMessage；golden/shared fixtures 和 Wiremock 覆蓋 tag remap、attribute allowlist、malformed nesting、escaping、metadata omission、payload 與 error response。
- [x] 實作 async Redis dedupe store 與 Tokio `App`；fake adapters/paused time 已驗證兩組 semaphore（fetch max 1/pipeline max 3）、send 後 set、失敗不 set、siblings join、sequential 3 batches 與 cancellation，production SIGINT/SIGTERM handler 由 cancellation token 驅動。
- [x] 新增完全 mock 的 `tests/service.rs`，以 production HTTP adapters 跑過 RSS → comments → OpenAI → Telegraph → Telegram → fake Redis，第二輪驗證 dedupe；全部 Cargo tests 不需外網、services 或 secrets。
- [x] 完成 Python/Rust parity review：settings/CLI/RSS/retry/pacing/prompt/schema/sanitizer/message/Redis/concurrency/failure/cancellation 均由 shared fixtures、兩套 gates與 module tests 對照；刻意差異只有 JSON stdout tracing 取代 Logfire及 Docker-only 發行。
- [x] 切換 build/deployment code：Rust multi-stage/non-root Dockerfile、原 Compose Redis volume/`serve` command、Rust+Docker CI 與 Rust 文件均已完成；GitHub CI run `29637092472` 已通過 Rust gates、Docker build、Compose config 與 container `serve --help`，Python rollback source依計畫保留至受控部署通過。
- [ ] 執行受控切換：停止 Python service 後以同一 Redis volume 啟動 Rust image，禁止雙跑；觀察恰好 3 個完成的 feed batches，驗證 process 未退出、batch 不重疊、既有 Redis entries 被略過、每個新成功通知只有一次 send 與對應 Redis key。任一條件失敗即部署前一個 Python image 並保留 Rust code 修正。
- [ ] 受控切換通過後移除 Python runtime/tooling：刪除 `src/hnbot/`、Python tests/scripts、`pyproject.toml`、`uv.lock` 與 Python CI/publish/bump workflow；將 justfile/pre-commit/Dependabot/README/DESIGN_DOC/AGENTS.md 改成 Cargo + Docker Compose，保留共享 contract fixtures與 RSS sample。驗證 repository 不再引用 Python、uv、PyPI、Logfire 或已刪路徑。
- [ ] 執行最終 gate：`cargo fmt --check`、strict clippy、所有 Rust tests、Docker build、Compose config、container CLI smoke、secret scan、`git diff --check` 全部成功；完成計畫 checklist 後移至 `docs/plans/archived/`。

## Test cases and verification scenarios

- Settings：required/missing secrets、所有 defaults、env override、NaN/inf/negative/boundary values。
- CLI：唯一 `serve` command、poll interval config/override/minimum、裸命令與 `main` 無 side effect。
- RSS/HTTP：真實 fixture、反轉順序、malformed feed、timeout、429、5xx、Retry-After 秒數/日期、三次 exhaustion。
- Pacing/concurrency：序列/並行 request spacing、global cooldown、fetch concurrency 1、pipeline concurrency 3、batch 不重疊。
- Content：HTML link/image stripping、whitespace、empty/single/multi chunk、Unicode boundary、structured output parse failure。
- Telegraph/Telegram：tag/attribute allowlist、malformed nesting、text/link escaping、optional points/comments/domain、HTTP failure。
- State/failure：processed skip、failed entry 不 set、send 成功後 set、siblings 全部 await、service batch recovery、SIGTERM cleanup。
- End-to-end：所有外部 adapters mock 的成功、部分失敗、重試與第二輪 dedupe。

## Risks

- Rust HTML/Markdown parser 輸出可能改變 LLM input；以共享 fixtures 固定 contract-level 結果，不要求無意義的 parser byte parity。
- OpenAI Responses 與 Telegraph payload 容易因 SDK 差異出錯；採直接 HTTP、mock request snapshots 與切換前 staging smoke 控制風險。
- Python/Rust 雙跑可能重複通知；正式環境明確禁止並行，切換時先停 Python。
- Rust 寫入 Redis 的時機或 key 若改變會破壞 dedupe；沿用原 key/value 並以既有 volume 做受控驗證。
- 最終刪除 Python 後回復成本提高；只有在 3 個 Rust batches 驗收通過後才移除，且保留前一個 image/commit。

## Rollback / Recovery

- Rust 尚未切換 Docker 前，回退單一 Rust PR 不影響正式 Python service。
- 切換期間若 health、batch、通知或 Redis 驗證失敗，停止 Rust，部署前一個 Python image，使用同一 Redis volume；不得清空 dedupe state。
- Python source 最終刪除後仍可從切換前 tag/commit 重建 image；Rust 沒有資料 migration，rollback 不需轉換 Redis。

## Completion Checklist

- [x] Rust `hnbot serve` 在 CLI、設定、資料流、retry/pacing、concurrency、failure boundaries、shutdown 與 Redis contract 上達到已記錄的 Python parity，並由共享 fixtures及 Rust tests 證明。
- [ ] 正式 Compose 已只執行 Rust image，使用原 Redis volume，且受控 3-batch 驗收全部通過並有 deployment evidence。
- [ ] Repository 已移除 Python、uv、PyPI、Logfire 與其 workflows/config，驗證方式為 repository search、tracked file review 與 Rust-only CI。
- [ ] `tracing` stdout 不包含 secrets，且 service/batch/entry/retry/cancellation 事件可由 mock tests與受控部署 logs 驗證。
- [ ] Docker image 以 non-root user 執行，`hnbot serve --help` smoke、Compose config 與 service startup 均通過。
- [x] Cargo format、strict clippy、全部 tests、Docker build、pre-commit 與 GitHub CI 全部通過（pre-removal gate；移除 Python 後須再跑一次 final gate）。
- [ ] 文件只描述 Rust + Docker Compose 的現行操作，且完成計畫已封存至 `docs/plans/archived/2026-07-18_rust-rewrite-plan.md`。

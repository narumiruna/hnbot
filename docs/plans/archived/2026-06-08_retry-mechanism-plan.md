## Goal

強化自動 retry，讓 cron 遇到暫時性 HTTP 錯誤（例如 `https://hnrss.org/newest?points=200` 的 `httpx.ConnectTimeout`）時會先重試，而不是單次 timeout 就讓整個 job 失敗。成功條件是 feed 抓取與 HN comments 抓取共用一致的 transient HTTP retry policy，且測試與文件能證明 retry 行為。

## Context

- Cron #437 在抓取 `hnrss.org` RSS feed 時遇到 `ConnectTimeout`，目前 `src/hnbot/rss.py:get_hn_feed()` 沒有 retry，所以 `App._run_feed_batch()` 直接中止。
- `src/hnbot/app.py:CommentFetcher._fetch_with_retry()` 已用 tenacity 對 HN comments GET 重試 `httpx.RequestError`、HTTP 429、HTTP 5xx，並支援 `Retry-After`。
- `DESIGN_DOC.md` 已列出「Feed-level fetch is not wrapped in retry logic inside `App._run_feed_batch()`」為已知限制。

## Architecture

- 將 transient HTTP retry 判斷、`Retry-After` wait、exponential jitter 與 logging 抽成共用模組（例如 `src/hnbot/http_retry.py`），避免 feed 與 comment fetch 各自維護一份 policy。
- retry 範圍先限制在 idempotent HTTP GET：HN RSS feed 與 HN comments page。不要把整個 GitHub Actions step 做粗粒度重跑，以免在 Telegraph/Telegram 已成功但 Redis 尚未標記時造成重複訊息。

## Non-Goals

- 不在本階段 retry Telegram send、Telegraph page creation、OpenAI generation；這些流程有重複副作用或成本，應另行設計 idempotency 後再處理。
- 不改變 Redis dedupe key 的語意或 TTL。

## Plan

- [x] 新增 feed fetch retry 的 regression tests 到 `tests/test_rss.py`，覆蓋第一次 `httpx.ConnectTimeout` 後成功，以及連續 transient failure 後仍會丟出最後錯誤；已用 `uv run pytest tests/test_rss.py -v` 驗證（9 passed）。
- [x] 抽出 `src/hnbot/http_retry.py`，提供共用的 transient HTTP 判斷、`Retry-After` 解析、wait strategy 與 retry logging helper；已用 `uv run pytest tests/test_app.py tests/test_rss.py -v` 驗證既有 comment retry 行為不變（17 passed），並用 `uv run pytest tests/test_http_retry.py tests/test_rss.py tests/test_app.py -v` 驗證共用 helper（20 passed）。
- [x] 更新 `src/hnbot/rss.py:get_hn_feed()`，把 `client.get(url)` 與 `raise_for_status()` 包在共用 retry policy 內，使 HNRSS 的 `ConnectTimeout`、其他 `httpx.RequestError`、HTTP 429、HTTP 5xx 會自動重試；已用 `uv run pytest tests/test_rss.py -v` 驗證（9 passed）。
- [x] 更新 `src/hnbot/app.py:CommentFetcher` 改用同一份 retry policy，保留每個 entry 的 log context（entry id、attempt number、exception）；已用 `uv run pytest tests/test_app.py -v` 驗證 429 retry、retry exhausted、單筆 entry skip 行為不變（8 passed）。
- [x] 更新 `DESIGN_DOC.md` 的 Reliability and Failure Handling 與 Known Limitations，移除 feed-level retry 限制並記錄新的 retry scope；已用 `rg -n "Feed-level fetch|retry policy|Retry-After|HNRSS feed fetch|OpenAI generation" DESIGN_DOC.md` 檢查，輸出只保留新 retry scope 與非 retry 範圍。
- [x] 視實作是否新增設定，更新 `.env.example`、`README.md`、`tests/test_settings.py`；本次未新增設定，沿用固定 3 attempts / exponential jitter，已用 `uv run pytest tests/test_settings.py -v` 驗證既有設定（12 passed）且 `git diff -- .env.example README.md tests/test_settings.py` 無輸出。
- [x] 跑完整品質 gate，確保 retry 強化沒有破壞格式、lint、type-check、測試；已用 `just all` 驗證（ruff format/check、ty check、38 tests 全部通過）。

## Risks

- 已緩解：retry wait 使用 3 attempts 與 exponential jitter 上限（`max=8`），避免無限重試。
- 已緩解：retry scope 只套到 idempotent HNRSS/HN comments GET，未對 Telegram、Telegraph、OpenAI 做整體重跑。
- 已緩解：`Retry-After` 格式錯誤時會回退 default wait；`tests/test_http_retry.py::test_retry_after_seconds_ignores_invalid_header` 已驗證。

## Completion Checklist

- [x] HNRSS feed transient timeout 會自動 retry，並由 `uv run pytest tests/test_rss.py -v` 的 regression tests 驗證（9 passed）。
- [x] HN comments transient retry 行為仍維持原有語意，並由 `uv run pytest tests/test_app.py -v` 驗證（8 passed）。
- [x] `DESIGN_DOC.md` 與必要設定文件反映新的 retry scope，並由 `rg -n "Feed-level fetch|retry policy|Retry-After|HNRSS feed fetch|OpenAI generation" DESIGN_DOC.md` 與設定檔無 diff 驗證。
- [x] 全專案品質 gate 通過，並由 `just all` 的成功輸出驗證（ruff format/check、ty check、38 tests 全部通過）。

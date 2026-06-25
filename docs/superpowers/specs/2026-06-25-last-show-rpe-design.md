# Last 顯示加入 RPE — Design

Date: 2026-06-25
Branch: `feat/last-show-rpe`

## Problem

在 workout 頁面選擇某個 exercise 時，`.pr-info` 的「Last」資訊條目前只顯示上次的 weight 與日期
（`Last: 100 kg @ 2026-06-20`）。使用者要靠上次的 RPE（自覺強度）來判斷這次要不要加重，但 RPE 沒有顯示出來。

## Goal

讓「Last」資訊條在 weight 與日期之外，也顯示上次該 exercise 最近一筆紀錄的 RPE，以 chip 形式呈現。
若上次沒填 RPE，顯示「RPE —」佔位，讓使用者明確知道「沒紀錄」而非「漏看」。

最終樣式（方案 B + 佔位）：

```
有 RPE：  Last: 100 kg  [RPE 8]  2026-06-20   [Fill]
無 RPE：  Last: 100 kg  [RPE —]  2026-06-20   [Fill]
```

`[RPE 8]` 為金色外框 chip（沿用 `.rpe-chip`），佔位版為虛線、淡化。

## Scope

最小改動，4 個檔案：

1. **SQL** — `src/repositories/workout_repo.rs::get_last_weight_per_exercise_by_user`
   - SELECT 多帶 `wl.rpe`。
   - 現有查詢以 `GROUP BY wl.exercise_id` + `MAX(wl.created_at)` 取每個 exercise 最近一筆。
     依 SQLite 的 bare-column 規則，當 SELECT 含 `MAX()` 聚合時，其他裸欄位（`weight`、`rpe`）取自
     該 `MAX` 所在的同一列，因此 `rpe` 與既有的 `weight` 來自同一筆「最新」紀錄，語意一致。

2. **Model** — `src/models/personal_record.rs::LastExerciseWeight`
   - 新增欄位 `rpe: Option<i32>`。
   - 更新 `from_row` 對應新的 SELECT 欄位順序。

3. **Template + JS** — `templates/workouts/show.html`
   - server-side 產生的 `exerciseLastWeights[exerciseId]` 物件多塞 `rpe`（值或 `null`）。
   - `showLastWeightInfo()` 在 weight 與日期之間插入 RPE chip：
     - `rpe` 有值 → `<span class="rpe-chip">RPE {n}</span>`
     - `rpe` 為 `null` → `<span class="rpe-chip rpe-chip-empty">RPE —</span>`
   - `fillLastWeight()` 行為不變：只填 weight，不動 RPE 欄位。

4. **CSS** — `templates/base.html`
   - 新增 `.rpe-chip`：金色外框（`border: 1px solid var(--gold)`、`color: var(--gold)`、
     `font-family: var(--font-display)`、`font-size: var(--font-xs)`、`padding: 1px var(--sp-2)`、
     `border-radius: var(--radius)`）。
   - 新增 `.rpe-chip-empty`：`border-style: dashed`、降低不透明度，表示「無紀錄」。

## Tests

- **整合測試**（`tests/` 對應 workout repo）：`get_last_weight_per_exercise_by_user` 回傳的
  `LastExerciseWeight` 含正確 `rpe`，涵蓋兩種情況：
  - 最近一筆有填 RPE → `Some(n)`。
  - 最近一筆沒填 RPE → `None`。
- **E2E**（`tests/e2e/`）：若既有 last-weight 場景存在，擴充斷言 `.pr-info` 內出現 RPE chip；
  否則新增一個場景：建立一筆有 RPE 的 set，重新選同一 exercise，驗證 chip 文字為 `RPE 8`。

## Out of Scope

- Fill 不會自動填入 RPE（維持最小改動）。
- 不顯示上次的 reps（使用者選 B 而非 C）。
- 不改動既有 RPE 的輸入 / 編輯 / stats 顯示。

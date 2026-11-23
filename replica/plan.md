# Plan — рабочий план реализации (после eng-review 2026-11-24)

Полные решения и находки: отчёт `C:\Users\User\.gstack\projects\meetingus\main-eng-review-20261124-022626.md`.
Входные данные: `replica/recon.md`, `replica/features.csv`, `docs/`.

## Ключевые решения (D1–D15)

| # | Решение |
|---|---|
| D1→D12 | **Google Calendar → волна 2** (OAuth-верификация = внешний блокер); детект встреч локальный |
| D2 | Раскладка Original: 12 Rust-модулей + features/* React |
| D3 | **MVP = Windows 11 only**; Linux = волна 2 |
| D4→D11 | Эхо: **text-dedup me/them в MVP** + наушники (рекомендация); AEC — волна 2 |
| D5→D10 | Латентность: **min-spec + per-tier таблица** из спика; hard gate = Assist e2e ≤2 с; partial ≤1.5 с = soft target |
| D6 | API-ключ MiMo → **OS keyring** (keyring crate), поле в Settings |
| D7 | Релиз: **ручной GitHub release, unsigned**; подпись/CI/автоапдейт — NOT in scope |
| D8 | Тесты: **cargo test + vitest + @testing-library**; E2E/click-through — Playwright |
| D9 | **Deepgram вырезан** — только локальный whisper |
| D13 | Онбординг S08 = **in-app wizard**: mic check → ключ → скачивание модели (base/small/turbo) |
| D14 | Детект встречи: **loopback energy + process title** (Zoom/Teams/Meet/Slack/Webex) |
| D15 | RAG = **SQLite FTS5** (chunks+FTS5 в схеме); embeddings — волна 2 |
| OV-7 | Скриншот: **сокрытие своих окон** при capture; privacy README (audio/screen → MiMo) |
| OV-9 | Фаза 5 = чекпоинты **5a→5b→5c→5d** |

## Фазы

- [x] **Фаза 0 — recon**: `replica/recon.md` (S01–S10, F01–F10), `features.csv` (59 строк), 6 скриншотов
- [ ] **Фаза 1 — AI-спик (T1)** → `replica/ai-stack.md`:
  - [ ] STT: замер RTFx/частичная задержка на моделях tiny/base/small/turbo (min-spec: i5-12400F/16GB/RTX4060)
  - [ ] MiMo: TTFT flash/pro (streaming), проверка цен/доступности/free-tier
  - [ ] Vision: есть ли у MiMo vision → иначе OCR-fallback (тумблер скриншотов)
  - [ ] Фиксация выбранной модели + per-tier таблицы (hard gate: Assist e2e ≤2 с)
- [ ] **Фаза 2 — architect** → `replica/architecture.md` + `replica/schema.sql`:
  - 12 модулей src-tauri (audio, vad, asr, llm, capture, db, session, state, commands, overlay, tray, main)
  - Схема: meetings, transcript_segments, meeting_notes, chat_messages, modes, files, settings, **chunks+FTS5** (D15)
  - SQLite WAL + single writer (A1); IPC-контракт; батч событий ≤250 мс (A2); data-flow ASCII
- [ ] **Фаза 3 — design** → токены Cluely + компонентные спеки + contrast.py (свои ассеты)
- [ ] **Фаза 4 — vertical slice**: оверлей + Ctrl+Enter + 2 потока аудио → стриминг-транскрипт (dedup D11) → Assist (MiMo flash). Verify: тест-звонок, ответ ≤2 с
- [ ] **Фаза 5 — чекпоинты** (каждый = вертикальный срез с verify):
  - [ ] **5a**: widget/chat (S04/S05): кнопки Assist/What should I say/Follow-up/Recap, Smart, Hide, хоткеи, tray
  - [ ] **5b**: post-call + dashboard (S01/S02/S03): заметки, next steps, транскрипт, copy/edit/delete, RAG-FTS5, детект (D14) → попапы S06
  - [ ] **5c**: settings/modes/onboarding (S07/S08/S10): wizard (D13), keyring (D6), кейбинды, язык, тема, файлы, инструкции
  - [ ] **5d**: polish: кейс-крайты, пустые состояния, batch-IPC, preload модели
- [ ] **Фаза 6 — test**: TDD по ходу; exit = 0 S1+S2 по Test Plan (`user-main-eng-review-test-plan-*.md`); severity S1(blocker)/S2(major)/S3(minor)/S4(cosmetic)
- [ ] **Фаза 7 — diff**: parity.py + imgdiff.py
- Позже: entrepreneur, brand (sweep.py), релиз (T15-чеклист)

## Задачи ревью

- [ ] **T1** (P1) Спик: min-spec + WER/латентность per-tier + цены MiMo → `replica/ai-stack.md`
- [ ] **T2** (P1) Text-dedup me/them в пайплайне (unit-тест)
- [ ] **T3** (P1) In-app wizard онбординга (mic + keyring + модель)
- [ ] **T4** (P2) Детект: loopback energy + process title
- [ ] **T5** (P2) chunks + FTS5 в схеме
- [ ] **T6** (P2) Скрытие своих окон при скриншоте
- [ ] **T7** (P2) README privacy (что уходит в MiMo)
- [x] **T8** (P2) Баннер superseded в docs/cluely-rea-findings.md
- [x] **T9** (P2) Чекпоинты 5a–5d в плане (этот файл)
- [ ] **T10** (P2) Батчинг IPC-событий ≤250 мс
- [ ] **T11** (P2) WAL + single writer + батч-запись сегментов
- [ ] **T12** (P2) Preload модели + таблица размеров в wizard
- [ ] **T13** (P1) Тест-сьют: cargo + vitest + Playwright
- [ ] **T14** (P3) Трейт-границы Windows-специфики (под Linux-волна 2)
- [ ] **T15** (P3) Чеклист релиза (unsigned, SmartScreen)

## NOT in scope (волна 2 и позже)

Linux-сборка, Google Calendar, Deepgram/облачный STT, AEC, embeddings, подпись/CI/автоапдейт, mobile/macOS/Enterprise, share-ссылки, briefs, Undetectability, биллинг.

## Стандарты кода

karpathy (явные предположения, минимальный код, surgical, верифицируемые цели) · ub-mode (make it work → right → practice, TDD, один писатель, идемпотентность, лог действий) · чистая комната (раскладка/флоу 1в1, бренд Cluely — нет; sweep.py до публичного релиза).

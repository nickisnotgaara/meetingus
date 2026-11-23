# Architecture: meetingus (a rebuild of Cluely's core features)

Scope: Windows-first desktop MVP, local-first, answers via Xiaomi MiMo only. Решения: replica/plan.md (D1–D16).

## Stack

| layer | choice | why |
| --- | --- | --- |
| shell | Tauri 2 (Rust) | пиксель-1в1 UI на web + Rust-ядро; прозрачный always-on-top оверлей проверен (Windows flags, D3) |
| UI | React 19 + TypeScript + Vite | 1в1 раскладка Cluely на HTML/CSS; vitest + testing-library (D8) |
| database | SQLite через sqlx, WAL | одна локальная БД, ноль серверов; WAL даёт параллельные чтения при одном писателе |
| auth | нет (локальный профиль) | без аккаунтов по решению (recon out-of-scope) |
| payments | нет | бесплатный опенсорс (D7) |
| email | нет — черновик письма локально как текст | заметки и follow-up генерируются, отправляет пользователь руками |
| jobs | in-process tokio-задачи (notes generator, FTS indexer, watchdog, idle timer, detection poller) | нет внешнего воркера; всё в одном бинаре |
| hosting | нет; релизы — руками на GitHub (D7) | unsigned .exe/.msi, SmartScreen-варнинг задокументирован |
| ASR | whisper-rs (whisper.cpp), CUDA если есть; small (GPU) / base (CPU-only) | спик: GPU 53× RTFx, CPU base 2.4× — укладываемся в бюджет (ai-stack.md) |
| LLM | MiMo API (reqwest + SSE): flash = Assist/заметки/vision, pro = Smart | спик: TTFT 1.1–2.5 s, vision работает, копейки за запрос (ai-stack.md) |
| key storage | keyring crate (Windows Credential Manager) | D6: ключ никогда не в plain-text/репо |
| overlay | Tauri WebviewWindow: transparent, always_on_top, skip_taskbar, focusable(false) | verified Windows-флаги (tarnished-tactician#3) |
| hotkeys | tauri-plugin-global-shortcut | Ctrl+Enter/Ctrl+\ из любого окна |
| capture | Windows Graphics Capture / GDI→PNG (порт legacy/screenshot.rs) | скриншот-контекст; свои окна скрываются при capture (OV-7) |
| detection | sysinfo (процессы Zoom/Teams/Meet/Slack/Webex) + энергия loopback | D14: два локальных слоя, календарь — волна 2 |

Монорепа (раскладка Original, D2):
```
src-tauri/src/   main.rs overlay.rs tray.rs commands.rs state.rs session.rs
                 audio.rs vad.rs asr.rs llm.rs capture.rs db.rs
src/             App.tsx lib/(ipc,types) features/{widget,chat,dashboard,settings,notifications}/
replica/         recon.md features.csv plan.md ai-stack.md architecture.md schema.sql design/ screens/
legacy/          архив egui-MVP (только справочно)
```

## Schema

Tables: 8 + FTS5-индекс. Access rules: **local single-user** — данные в `%APPDATA%\meetingus\`, никакой сетевой экспозиции; владелец = ОС-пользователь (RLS не применима; «права» = права файловой системы). Схема целиком: `replica/schema.sql`.

Ключевые таблицы: `sessions`, `transcript_segments` (role me/them), `meeting_notes`, `chat_messages`, `modes`, `files`, `settings` (kv; API-ключа здесь нет — keyring), `chunks` + `chunks_fts` (FTS5, D15).

## API (IPC-контракт: Tauri commands + события)

| method (invoke) | does | input | output | flow |
| --- | --- | --- | --- | --- |
| `session_start` | старт сессии (идемпотентно: ongoing → вернуть существующую) | source, detection_name? | session | F02 |
| `session_end` | стоп → запуск генерации заметок + FTS-чанкинг | session_id | session | F05 |
| `session_resume` | продолжить ended-сессию | session_id | session | F05 |
| `session_state` | текущее состояние для UI (reconnect после crash webview) | — | state snapshot | F01 |
| `assist_ask` | вопрос/дефолтное действие → streaming | text?, action, smart, use_screen | — (события `assist://delta/done/error`) | F02/F03/F04 |
| `chat_clear` | очистить диалог (Ctrl+R) | — | ok | F03 |
| `list_meetings` / `get_meeting` | список/деталь из Activity | filter? | rows / session+notes+segments | F06 |
| `notes_update` | ручное редактирование summary/manual_notes | session_id, fields | ok | F06 |
| `meeting_delete` | удалить встречу | session_id | ok | F06 |
| `search_meetings` | FTS5-поиск + «спроси о встрече» (D15) | query, session_id? | rows / answer stream | F07 |
| `settings_get` / `settings_set` | настройки kv | key?, key, value | value / ok | F09 |
| `api_key_set` / `api_key_clear` | ключ в keyring (D6) | key | ok | S07 |
| `keybind_start_record` / `keybinds_set` | запись кейбинда (блокирует остальные) | action | combo / ok | F09 |
| `model_status` / `model_download` | статус/скачивание whisper-модели (D13) | size? | status / события `model://progress` | F01 |
| `onboarding_state` / `onboarding_complete` | wizard: mic-check, ключ, модель (D13) | — | state / ok | F01 |
| `mode_*` / `file_*` | CRUD Modes/файлов | … | … | F10 |
| `screenshot_toggle` / `capture_now` | тумблер + принудительный capture (скрытие своих окон — OV-7) | — | png base64 | F04 |
| `overlay_hide_show` / `overlay_move` | Hide Ctrl+\, move ±80 | — | ok | S04 |
| `export_transcript` | копия транскрипта/summary | session_id | text | F06 |

События Rust→JS: `session://state`, `transcript://segment`, `audio://health`, `assist://delta|done|error`, `notes://progress`, `model://download-progress`, `detection://meeting`.

Webhooks in/out: **нет** (нет сервера). Jobs (in-process): notes_generator (по `session_end`), fts_indexer (батч сегментов), audio_watchdog (10 с/30 с, legacy), idle_timer (10 мин → idle), detection_poller (1 Гц: процессы + loopback-энергия).

## The parts that bite

- **time zones:** хранить UTC epoch ms, показывать локально; сегменты — offset от старта сессии (не абсолютное время) → переживают смену TZ.
- **idempotency:** `assist_ask` — один in-flight запрос (guard в state; повторный Ctrl+Enter = фокус/повтор, не дубль); `session_start` возвращает ongoing-сессию; сегменты пишутся с монотонным start_ms.
- **races:** **один писатель** — вся запись через mpsc-очередь в db-task (ub-mode); UI читает через отдельное read-соединение (WAL); события в webview батчами ≤250 мс (A2).
- **audio device loss:** watchdog → событие `audio://health` → toast + пауза (не тихий сбой); legacy-тайминги 10 с/30 с сохраняем (regression contract).
- **эхо (D11):** text-dedup me/them на уровне сегментов до записи; наушники — в README (T7).
- **crash webview:** ядро Rust владеет сессией и пишет сегменты живьём (F6); UI при рестарте зовёт `session_state` и продолжает.
- **model download:** resumable (HTTP Range), прогресс в UI, offline-режим = ручное размещение (README).
- **prompt cache:** system-промпт и префикс транскрипта — стабильные (кэш MiMo почти бесплатен).
- **DB growth:** ~300 сегментов/час встречи (~50 КБ) — тривиально; `PRAGMA incremental_vacuum` по расписанию не нужен.
- **GDPR/приватность:** экспорт всех данных в JSON + полное удаление (кнопки в Settings); README-раздел «что уходит в MiMo» (T7).
- **Windows-only абстракции (T14):** audio/hotkey/detection — за трейтами (`AudioBackend`, `HotkeyBackend`) с заглушками для волны 2 (Linux).

## Build order

1. **Vertical slice (Фаза 4):** каркас Tauri+React → оверлей S04 (transparent, drag, Hide) → Ctrl+Enter → два аудиопотока (WASAPI loopback + mic) → VAD → ASR small (CUDA) → сегменты в SQLite → `assist_ask` → MiMo flash streaming с индикатором (D16). Verify: тест-звонок, partial ≈0.5 s, первый токен ≤2.5 s.
   Таблицы: sessions, transcript_segments, chat_messages, settings. IPC: minimal set выше.
2. **Must-haves (features.csv, чекпоинты 5a–5d):** навбар/действия/Smart/Hide (5a) → заметки+дашборд+транскрипт+copy/delete+FTS5+детект (5b) → wizard+keyring+кейбинды+язык+тема+Modes/файлы (5c) → пустые состояния, preload модели, батчи, крайты (5d).
3. **Should-haves:** по features.csv (Resume, auto-end, ambient — как приоритеты после must).
4. **Fixes from replica-entrepreneur:** после Фазы 7 (отзывы пользователей Cluely).

## Failure modes (→ тесты Фазы 6)

Отключение аудио-устройства → toast+пауза; MiMo недоступен → error в стриме+retry; БД lock → WAL+один писатель (прозрачно); crash webview → рестарт без потери; модель не скачена → wizard блокирует; дубли эха → dedup. **Critical gaps: 0** (каждый — planned test + видимый UX).

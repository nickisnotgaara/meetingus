# Статус проекта — 2026-11-24

Бесплатный open-source клон Cluely (оверлей AI-помощника для встреч) на
Windows: Tauri 2 + React/TS, ядро на Rust, ответы — Xiaomi MiMo, распознавание
речи — локальный whisper.cpp. Облачных сервисов и аккаунтов нет.

Конвейер и ключевые сценарии работают и проверены живьём; UI и onboarding
находятся на середине пути. Ниже — что именно сделано, чем подтверждено и что
известно плохого.

## Что сделано

### Phase 0 — рекогносциция (готово)
- Разбор Cluely: экраны, потоки, компоненты, модель данных — `replica/recon.md`,
  матрица функций — `replica/features.csv` (59 строк), 6 скриншотов.
- Чистовая переработка плана после eng-review — `replica/plan.md`,
  `replica/architecture.md`, `replica/schema.sql` (8 таблиц + FTS5),
  `replica/ai-stack.md` (замеры RTFx моделей на RTX 4060).

### Ядро (готово, покрыто тестами)
- Захват: микрофон («me») + системный loopback через WASAPI («them»),
  ресемплинг в 16 kHz моно — `src-tauri/src/audio.rs`.
- VAD (webrtc-vad), транскрипция whisper.cpp (GPU/CUDA, предзагрузка модели
  ~1.3 с на старте), текстовый дедуп эха — `vad.rs`, `asr.rs`, `dedup.rs`.
- Хранение: SQLite WAL, сессии/сегменты/чат/настройки/FTS5 — `db.rs`.
- Assist: стриминг ответов MiMo (`mimo-v2.6-flash` в Fast, `mimo-v2.6-pro` в
  Smart, в Fast отключён reasoning), промпты действий Assist / What should I
  say / Follow-up / Recap — `assist.rs`, `llm.rs`.
- Ключ API: env-переопределение или OS keyring (D6), панель ввода ключа.
- Горячие клавиши: глобальные Ctrl+Enter (Assist) и Ctrl+\ (скрыть/показать).
- Наблюдаемость: health-вотчдог (тишина >10 с), логи session_start/end.

### UI (базовый готов, дальше — полировка)
- Виджет из дизайн-токенов Cluely — `src/App.tsx`, `src/App.css`,
  спецификации — `replica/design/tokens.json`, `components.md`.
- Состояния: Idle/Recording, индикатор стрима, карточка ответа с Stop,
  чип Fast/Smart, панель ключа, баннер ошибок.

### Проверки, которые реально прогнаны
| Проверка | Результат |
| --- | --- |
| `cargo test` (unit) | 48 passed, 0 failed |
| Headless pipeline E2E (30 с wav → VAD → whisper GPU → дедуп → SQLite) | 6 сегментов за 2.23 с, все `them`, без дублей |
| Live MiMo Assist (TTFT) | fast mode 1423 ms, ответ без цитат |
| UI E2E через CDP WebView2 (все кнопки, Enter, Smart, прерывание, Stop, End) | 16/16 |
| Без ключа → ошибка → вставка ключа → keyring → ответ | 4/4, ключ лежит в Credential Manager (`mimo.meetingus`) |
| Ctrl+\ hide/show, Ctrl+Enter после End session | работают |
| Дубли сегментов в UI (утечка подписок StrictMode) | найден и исправлен |

Скриншоты прогона — `C:\Users\User\AppData\Local\Temp\opencode\shots\`.
E2E-скрипты (CDP) — там же: `ui-e2e.js`, `short-session.js`, `key-test.js`.

## Известные проблемы и открытые вопросы

1. **Обратная связь 24.11: «основные функции не работают, UI выглядит
   сомнительно».** В dev-прогоне конвейер и кнопки отработали (таблица выше),
   поэтому нужен конкретный сценарий воспроизведения: что именно не работает
   (какая кнопка/поток), на какой сборке, скриншот UI. До выяснения считать
   качество UI и «основных функций» неудовлетворительным.
2. **Эхо не подавлено на уровне звука** — только текстовый дедуп (D11).
   Рекомендуются наушники; AEC — волна 2.
3. **Onboarding**: backend готов (каталог/загрузка моделей, mic-метр,
   статус/финиш), **UI-визард не сделан** — свежая установка показывает виджет
   без модели и не проводит пользователя.
4. **Не проверялось вживую**: реальные звонки Zoom/Meet (поведение loopback,
   ducking, эхо), микрофон-гарнитура, CPU-only сборка, длинные сессии (>1 ч),
   восстановление после краша, повторный запуск с уже сохранённым ключом.
5. **Только Windows 11 x64 + NVIDIA CUDA** в этой итерации; CPU-сборка не
   собиралась, Linux — волна 2.
6. Релиза нет: нет инсталлятора, подписи, авто-обновления (по решению D7 —
   вручную и без CI), README для релиза не написан.

## Как собирать и проверять

```powershell
# все cargo-команды только через build.ps1 (в PATH нет cargo)
.\build.ps1 dev cuda          # dev: Vite на :1420 + приложение
.\build.ps1 test cuda         # юнит-тесты
# E2E (ignored-тесты):
.\build.ps1 test cuda -- --ignored wav_runs_through --nocapture
.\build.ps1 test cuda -- --ignored live_assist --nocapture   # нужен MIMO_API_KEY
```

Данные приложения: `%APPDATA%\com.meetingus.app\meetingus.db`,
модели в `models/` репозитория (dev) или `%APPDATA%\com.meetingus.app\models\`.

Оставшиеся задачи и их порядок — в `replica/roadmap.md`.

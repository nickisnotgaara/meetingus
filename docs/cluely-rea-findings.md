# Cluely: результаты реверс-инжиниринга (REA)

Инструмент: `rea-agents` CLI, операция `analyze_javascript_application` по извлечённому `app.asar` (Cluely 2.0.200, `resources/app.asar`, ~27 МБ, 3299 файлов). Полный отчёт: `%TEMP%\opencode\rea-cluely.json` (303 МБ). Ниже — углублённый разбор `dist-electron/main.js` (552 КБ, 17 080 строк) и `dist/assets/transcription-*.js`.

## Окна (main.js, домены window/control/chat/dashboard/notification)

- Общий конфиг окон: `acceptFirstMouse`, `webPreferences: { preload }`; все окна — `setContentProtection(isInvisible || isCapturingScreenshot)` (прямой аналог нашего `WDA_EXCLUDEFROMCAPTURE`), `skipTaskbar`, поверх всех окон.
- **Control** — пилюля 163×50, перетаскивание мышью (`control-window-drag`), хоткеями ±80 px.
- **Chat** — 540 px шириной, без рамки/тени, высота 350–500 регулируется (`chat-window-resize`), сворачивается (`chat-clear-input`), `hiddenInMissionControl`.
- **Dashboard** — 1050×700 (min 800×600), тёмная/светлая тема (`#09090B`/`#FFFFFF`), маршруты `/dashboard`.
- **MeetingNotification** — 360 px, правый верхний угол рабочей области.

## Аудио (домен native-audio.ts)

- **Микрофон**: `node-record-lpcm16` + `sox` (14.4.1-win32 в extra-resources) → WAV 48 кГц моно → `broadcastIpc("chat-audio-data", { role: "me", chunk })`. Health-мониторинг: таймер первого чанка 10 с, stall-детект 30 с (2×15 с проверки), `hasRecentChunk()` — окно 5 с.
- **Системный звук (them)**: `audiotee` (бинарь, **только macOS**) — на Windows альтернатива: рендерер через `getDisplayMedia` c `audio: "loopback"` (`setDisplayMediaRequestHandler`), событие `chat-display-media-sources-changed` при смене дисплеев. Роли `me`/`them` разделены.
- 48 кГц — общая частота потока (`48e3`).

## VAD и STT (dist/assets/transcription-*.js)

- VAD: **@ricky0123/vad-web** (Silero v5/legacy ONNX) через onnxruntime-web (WASM/WebGPU), грузится с `cdn.jsdelivr.net`, который main-процесс перехватывает кастомным протоколом и отдаёт из `extra-resources/vad`. Два независимых конвейера VAD по ролям (`me`, `them`), кадры копятся в очереди и обрабатываются `frameProcessor.process()`.
- STT: **серверный** — `transcription.transcribe({ wavBase64, language })` (API `https://api.v2.cluely.com`), язык из `session.audioInputLanguage`. Очередь с счётчиком параллельности, статусы `ready`, транскрипт пишется в `session.transcript`.
- LLM/чат: серверный (api.v2.cluely.com), UI удалённый SPA (`renderer.v2.cluely.com`).

## Скриншот (домен screenshot.ts)

- `capture()`: `isCapturingScreenshot: true` (окна скрываются) → `desktopCapturer.getSources({types:["screen"]})` с thumbnail ≤ 1920 px на дисплее под Chat-окном → PNG `{data, contentType: "image/png"}`, ретраи 3×500 мс. Используется как визуальный контекст для LLM.

## Состояние (домен state.ts)

- Общее состояние (`shared-state.json`, zod-валидация, миграция V1): `session`, `isInvisible`, `noFocusWhenInvisible`, `hideChatHidesControlWindow`, `isAmbientEnabled`, `showChat`, `showDashboard`, `chatWindowIsExpanded`, `isRecordingShortcut`, `meHasVoiceActivity`, `themHasVoiceActivity`, `showSessionInactivityModal`, `meetingNotification`, `isCapturingScreenshot`, `openAtLogin`, `autoUpdate`, `permissions`, `authRefreshCount`, `dashboardFocusCount`, `shortcuts`, `appVersion`, `systemInfo`…
- Патч состояния → обновление всех доменов → `broadcastIpc("shared-state-updated")`.

## Сессия и хоткеи

- Сессия: `control-start-session` / `control-end-session`, таймер **10 минут** бездействия → модалка `showSessionInactivityModal` + открытие Dashboard.
- Глобальные хоткеи (переназначаемые): `toggleVisibility`, `ask` (отправить ввод), `clear`, `toggleSession`, `moveUp/Down/Left/Right` (±80 px), `scrollUp/Down` (±240 px). Режим записи нового хоткея блокирует все остальные.
- Tray: шаблонные иконки (active/inactive), на Windows клик открывает Dashboard; меню Start/Stop Listening, Settings, Quit. Автозапуск (`setLoginItemSettings`).
- Прочее: `electron-updater` (`update-available/downloaded`, `restart-to-update`), deep-link авторизация, single-instance (`second-instance`), permissions (microphone/screen/accessibility), системная тема.

## Что перенесено в наш Rust-MVP

| Фича Cluely | Наш реализм |
|---|---|
| Скрытность окна (`setContentProtection`) | `stealth.rs`, `WDA_EXCLUDEFROMCAPTURE`, авто через 1.5 с |
| Микрофон 48 кГц + health-мониторинг | `audio.rs` + watchdog (10 с/30 с) |
| Системный звук `them` | WASAPI loopback (полноценнее, чем на Windows у Cluely) |
| VAD по ролям me/them | `vad.rs`, два канала webrtc-vad |
| Индикаторы `meHasVoiceActivity`/`themHasVoiceActivity` | точки Me/Them в заголовке |
| STT wav→text с языком | whisper.cpp CUDA локально (`stt.rs`) |
| Скриншот-контекст для LLM | `screenshot.rs` (GDI→PNG→base64 vision-сообщение) |
| Сессия + 10-мин таймер бездействия | watchdog → «session idle» |
| Глобальные хоткеи (перемещение ±80 и др.) | global-hotkey: Ctrl+Alt+H/A/S/L/стрелки |
| Перетаскивание окна | drag заголовка (`ViewportCommand::StartDrag`) |
| Tray с меню | tray-icon: Show/Hide, Toggle listening, Screenshot, Quit |
| Старт/стоп прослушки | кнопка «Listening: ON/OFF» + хоткей + трей |
| Persist состояния | `shared-state.json` (invisible/auto_answer/listen_them) |
| Chat: ввод, стриминг ответов | egui чат + SSE-стриминг, фильтр think-тегов |

Не переносилось: облачные аккаунты/биллинг Clerk, удалённый SPA UI, updater, deep-link auth, multi-window (dashboard/notification), переназначаемые хоткеи, meeting notifications.

## Ограничения анализа

- Поведение UI-продукта в asar отсутствует (SPA с `renderer.v2.cluely.com`); оценка UX чата/оверлея — наш дизайн.
- STT/LLM у Cluely серверные; их промпты и параметры из asar не извлекаются.

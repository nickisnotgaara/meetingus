# Recon map: Cluely (desktop, Windows 11 + macOS → clone targets Windows + Linux)

Scope: десктопное приложение целиком — оверлей-виджет во время сессии, Assist-чат, генерация заметок, дашборд с историей, настройки. Без мобилки, без биллинга, без undetectability (решения заказчика).
For: бесплатный опенсорсный клон, локально-first, ответы через Xiaomi MiMo.
Date: 2026-11-24

## Sources

| # | source | URL | notes |
| --- | --- | --- | --- |
| 1 | homepage (ядро-фичи, кнопки виджета) | https://cluely.com/ | Assist / What should I say? / Follow-up questions / Recap / Smart; 12+ языков, 300 мс заявлено; совместимость Zoom/Slack/Webex/Teams/Meet |
| 2 | pricing (планы, лимиты free) | https://cluely.com/pricing | Starter free / Pro $19.99 / Pro+Undetectability $149.99; free: 5–10 AI-сообщений/день (не публикуется), 3 файла |
| 3 | help: How to Use Cluely | https://support.cluely.com/en/articles/12294306-how-to-use-cluely | виджет (HUD после 1.88.6): старт/стоп сессии, Assist Ctrl+Enter, Smart, Hide Ctrl+\, открытие дашборда |
| 4 | help: Dashboard App | https://support.cluely.com/en/articles/12101340-how-to-use-cluely-s-dashboard-app | Activity, meeting item: metrics/resume/share/delete/next steps/edit, экспорт транскрипта, старт сессии |
| 5 | help: Settings | https://support.cluely.com/en/articles/12418681-how-to-use-cluely-s-settings | вкладки General/Calendar/Keybinds/Profile/Language; хоткеи (toggle, ask, clear, session, move ±80, scroll ±240); только Google Calendar |
| 6 | help: Using Cluely for Free | https://support.cluely.com/en/articles/11127977-using-cluely-for-free | дневные лимиты, сброс 24 ч, AI chat ≠ dashboard chat |
| 7 | docs: changelog | https://docs.cluely.com/changelog | виджет 1.88.7 (упрощённый), транскрипт переехал в дашборд (1.90.1), Audio Settings, Meeting Note Templates, Ambient, отмена Live Insights (1.73) в пользу Assist |
| 8 | docs: Meeting Notes | https://docs.cluely.com/feature/postcall | Activity default view; summary/next steps/transcript/share/resume/delete/edit |
| 9 | docs: Live Insights (устаревший слой UI) | https://docs.cluely.com/feature/liveinsights | навбар (drag/logo/audio/end timer/X), дефолтные действия, динамические инсайты — частично retired |
| 10 | публичные скриншоты (референсы) | mintcdn.com/cluely | 6 изображений в `replica/screens/` — только для сравнения раскладки, никогда не шипятся |
| 11 | собственные REA-наблюдения пользователя | `docs/cluely-rea-findings.md`, `docs/competitive-features.md` | поведенческие факты (окна, размеры, роли аудио, сессии); код оттуда не переиспользуется |
| 12 | обзоры третьих сторон | interviewwhisperer.app, favtutor.com, copilotinterview.com | флоу «before/during/after», лимиты, 10 сообщений/день в UI |

Не использовано: исходники/бандлы приложения, приватные API, чужие аккаунты. Скриншоты и тексты — референс, не ресурс для копирования.

## Core loop

Начать сессию → слушать разговор (mic = me, системный звук = them) → по Ctrl+Enter получить мгновенный ответ из транскрипта и экрана → закончить сессию → получить заметки (summary, next steps, transcript) и найти их позже в дашборде.

## Screens

| ID | screen | route / how to reach | purpose | key components | states seen |
| --- | --- | --- | --- | --- | --- |
| S01 | Dashboard: Activity (home) | лого виджета / трей / старт приложения | список прошлых встреч + предстоящие + старт | карточки upcoming (Join, brief), список meeting items, кнопка Start Cluely, dropdown Mode, поиск, аватар-меню (Customize/Help/Billing/Settings/Logout) | empty (нет встреч), loading, filled, лимит free |
| S02 | Dashboard: Meeting note | S01 → клик по item | разбор прошлой встречи | вкладки Summary/Transcript, редактируемый summary, Next Steps, метрика даты, Resume/Share/Delete, поле «Ask Cluely about this meeting», Copy-иконки | generating notes, filled, пустой транскрипт |
| S03 | Dashboard: live transcript активной сессии | во время сессии → дашборд | живой транскрипт со спикерами (из виджета переехал в 1.90.1) | список сегментов me/them, waveform-иконка, real-time обновление | loading, live |
| S04 | Widget (во время сессии) | старт сессии (кнопка/алерт) | живая помощь во время звонка | навбар: drag-иконка (6 точек), лого→дашборд, audio toggle, синяя End session с таймером, X; чат-область: кнопки Assist / What should I say? / Follow-up questions / Recap, инпут, Smart-тумблер, превью скриншота, Hide (Ctrl+\) | collapsed, hidden (Hide), нет сессии |
| S05 | Assist chat panel | Ctrl+Enter / ввод текста | Q&A со стриминговым ответом | инпут, Submit, streaming markdown, Clear (Ctrl+R), отображение reasoning, лимит сообщений | empty, streaming, error, limit-reached |
| S06 | Meeting notification | календарный алерт / детект встречи | предложить записать/получить заметки | попап 360 px справа сверху: «Zoom meeting detected → Take notes», «Done with your meeting → Get notes», upcoming card → Join | dismissed, expired |
| S07 | Settings | аватар → Settings | конфигурация | вкладки General (invisibility — skip, auto-launch, тема, update, Audio Settings: уровень микрофона + превью транскрипта), Calendar (Google), Keybinds (режим записи блокирует остальные), Profile, Language (output / meeting audio) | filled, recording-keybind, permission denied |
| S08 | Onboarding / sign-in | первый запуск | аккаунт + разрешения | sign up/in (email/Google), промпты mic/screen/accessibility, обучающий тур | signed-out, permissions denied |
| S09 | Session inactivity modal | 10 мин без активности | предупредить + открыть дашборд | модалка showSessionInactivityModal | - |
| S10 | Modes (Customize) | дашборд → Modes | свой контекст для ответов | список modes, редактор инструкций, загрузка файлов (3 free), email-шаблон заметок | empty, uploading, limit |

Состояния, которые обязаны быть у нас, потому что не записал их — не построишь: empty/limit/error/permission-denied на S01, generating на S02, streaming/error/limit на S05.

## Flows

```
F01 Первая установка
    S08 -> (разрешения mic/screen) -> S01
    happy path clicks: 3
    edge: отказ в разрешениях, нет интернета для входа (у нас входа нет — локально)

F02 Старт сессии и живой Assist
    S01 -> Start Cluely -> S04 -> Ctrl+Enter -> ввод/Submit -> ответ
    happy path clicks: 3 (или 0 через хоткей из любого окна)
    edge: микрофон молчит, loopback недоступен, ошибка захвата аудио

F03 Дефолтное действие
    S04 -> клик «What should I say?» (или Tab на первое) -> ответ
    happy path clicks: 1
    edge: пустой транскрипт, лимит сообщений

F04 Скриншот-контекст
    S04 -> вопрос про экран (скриншот автоматом, превью «Sent with screenshot») -> ответ
    happy path clicks: 1
    edge: скриншот выключен в настройках, несколько мониторов

F05 Завершение сессии и заметки
    S04 -> End session -> Generating notes... -> S01 (item появился) -> клик -> S02
    happy path clicks: 3
    edge: сессия < 2 мин, длинный транскрипт (медленные ответы), автостоп по календарю/смене микрофона

F06 Обзор прошлой встречи
    S01 -> item -> S02 -> Copy transcript / редактировать summary / Delete
    happy path clicks: 2
    edge: заметки ещё генерируются, транскрипт пуст

F07 Вопрос по сохранённой встрече
    S02 -> «Ask Cluely about this meeting...» -> ответ
    happy path clicks: 2
    edge: нет транскрипта, слишком длинная встреча

F08 Календарное уведомление
    S06 -> Take notes / Join meeting -> S04
    happy path clicks: 1
    edge: календарь не подключён, встреча отменена, алерт устарел

F09 Настройки: кастомный хоткей
    S07 -> Keybinds -> запись нового сочетания -> сохранение
    happy path clicks: 3
    edge: сочетание занято системой, режим записи блокирует другие хоткеи

F10 Customize (Modes + файлы)
    S01 -> Modes -> S10 -> инструкция/файл -> активный mode виден в S04 dropdown
    happy path clicks: 4
    edge: лимит 3 файла (free), файл не обработан
```

## Components

| component | variants | states | used on |
| --- | --- | --- | --- |
| Widget nav bar | drag, logo, audio toggle, end-session (timer), close | active, idle, hidden | S04 |
| Chat action button | Assist, What should I say, Follow-up questions, Recap | default, hover, loading, disabled (нет транскрипта) | S04, S05 |
| Chat input | text, submit (Ctrl+Enter), Smart toggle | empty, filled, streaming, limit | S04, S05 |
| Streaming answer block | markdown, reasoning, screenshot-attachment preview | streaming, done, error, copied | S05 |
| Meeting item row | title, date, actions (resume/share/delete) | normal, hover, generating | S01, S02 |
| Meeting card (upcoming) | participants, Join, brief | normal, soon, cancelled | S01, S06 |
| Notification popup | detected/done/upcoming | visible, dismissed, expired | S06 |
| Tabs | Summary, Transcript (+ Usage у оригинала) | active, disabled | S02 |
| Toggle | on/off (тема, auto-launch, screenshot, audio) | on, off, disabled | S07 |
| Keybind recorder | press-to-record | idle, recording, conflict | S07 |
| Settings tab nav | General, Calendar, Keybinds, Profile, Language | active | S07 |
| Mode dropdown / editor | preset, custom | open, empty, saving | S04, S10 |
| Empty / limit / error states | no meetings, generating, limit reached, audio error | — | S01, S02, S05 |
| Tray menu | show/hide, start/stop, quit | active/inactive | системное |

## Inferred data model

```
User        id, email, name, output_language, audio_language, theme,
            keybinds (json), auto_launch, screenshot_enabled, plan
            evidence: S07 вкладки, help: Settings, pricing
            confidence: high (но у нас: локальный профиль, без аккаунта)

Session     id, title, started_at, ended_at, status (ongoing|ended|idle),
            source (manual|calendar), calendar_event_id?, idle_autoended
            evidence: S04 End timer, F05, help: Dashboard (Resume Session)
            confidence: high

TranscriptSegment  session_id, role (me|them), text, start_ts, language
            evidence: REA-наблюдения (роли me/them), homepage (транскрипт в реальном времени)
            confidence: high (диаризация внутри «them» — guess, отдельных спикеров не видели)

MeetingNote session_id, summary, next_steps, key_insights, email_draft,
            manual_notes, generated_at
            evidence: S02, docs: Meeting Notes
            confidence: high

ChatMessage id, session_id?, role (user|assistant), content,
            screenshot_attached (bool), smart_mode (bool), created_at
            evidence: S05, help: Assist (Smart), changelog (reasoning tokens)
            confidence: high

Mode        id, name, instructions, email_template, is_default
            evidence: S10, help: Modes/Meeting Note Templates
            confidence: high

File        id, mode_id?, name, content, processed (bool)
            evidence: pricing (3 файла free), competitive-features.md
            confidence: medium (формат хранения — guess: локальный текст)

CalendarConnection  provider (google only), calendars[], visible[], sync_state
            evidence: help: Settings→Calendar, changelog (real-time sync)
            confidence: high

Quota       daily_ai_messages_used, daily_notes_used (только для скопированного UI-лимита)
            evidence: pricing/free help (5–10/день)
            confidence: medium — у нас не применяется (skip)
```

Relationships: User 1-n Session, Session 1-n TranscriptSegment, Session 1-1 MeetingNote, Session 1-n ChatMessage, Mode 1-n File, User 1-1 CalendarConnection.

## Feature matrix

См. `features.csv`. Must: 21, Should: 24, Could: 14 (итого 59 строк); clone=skip: 8 (из них 3 помечены must — это must-have для оригинала, но исключены из клона решением заказчика).

## Out of scope (cannot or should not be cloned)

- **Undetectability / скрытие из screen share** — решение заказчика: полностью исключено.
- **Мобильное приложение (iOS) и macOS** — только Windows + Linux; iOS на Rust невозможен (Swift).
- **Биллинг, подписки, лимиты free-плана, аккаунты, device limit, облачная синхронизация** — наш клон бесплатный, локальный, без входа.
- **Enterprise** (knowledge base sync, CRM/ATS-интеграции, SSO, admin) — не входит в MVP.
- **Публичные share-ссылки на заметки** — требуют хостинга; заменяем локальным экспортом.
- **Pre-call briefs / «Who am I talking to»** (исследование профилей людей) — внешнее обогащение данных, риск ToS; skip.
- **Бренд Cluely** (имя, логотип, копирайт, шрифты, тексты)** — чистая комната: раскладка и флоу — 1в1, ассеты и слова — свои; `sweep.py` перед релизом.

## Size

Screens 10, flows 10, entities 10. Hard parts: ① параллельный захват mic+loopback без эха и со стриминговым ASR, ② прозрачный always-on-top оверлей поверх звонка (и его поведение на Wayland), ③ генерация заметок и RAG «ask about this meeting» на локальной БД. Size: **L** (недели; ядро MVP — vertical slice из фазы 4).

Next step: `/replica-architect` (после AI-спика фазы 1 — замер ASR и выбор модели MiMo).

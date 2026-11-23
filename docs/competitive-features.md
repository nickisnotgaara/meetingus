# Функции Cluely и конкурентов — исследование (ноя 2026)

Источники: cluely.com (+pricing), finalroundai.com, techscreen.app/articles/cluely-alternatives-2026, duckduckgo-обзоры (interviewfox, theinterviewcopilot, saner.ai, acedly), REA-разбор app.asar (строки UI рендерера подтверждают поведение).

## Cluely — полный набор функций

**Ядро (из REA + сайта):**
- Реалтайм-транскрипт (12+ языков, заявлено 300 мс / 95%), роли me/them
- Assist (`Cmd/Ctrl+Enter`) — «What should I say?» — мгновенный ответ на последний вопрос
- Кнопка **Follow-up questions** — ИИ предлагает, что спросить дальше
- Кнопка **Recap** — пересказ разговора
- **«Ask about your screen or conversation»** — вопрос с контекстом экрана (скриншот) и разговора
- **Smart**-тумблер (smartMode) — умный режим ответов
- Hide (скрыть окно), перетаскивание «под глаза», окно всегда поверх

**Сессии (из строк рендерера):**
- `Untitled Session`, `Resume Session`, «Cannot resume mobile session» — сессии с возобновлением, кросс-девайс (iOS/десктоп)
- `Finish to see notes...` → `Generating notes...` → **AI-заметки по завершении** (markdown)
- `Copy summary`, `Copy transcript`, `Transcript copied` — экспорт в буфер
- `Write your notes here...` — ручные заметки к сессии
- `Ask about this meeting...` и `Saved-session answer` — **вопросы по сохранённым сессиям**
- `Resumed session did not activate in desktop state`, `Session ended with unfinished transcription` — состояния сессии
- Meeting-уведомления (окно, «Ask about this meeting», detach)

**Прочее:**
- Кастомные инструкции + **загрузка файлов** (3 файла Free / безлимит Pro) — контекст из документов (резюме, JD)
- Кастомные хоткеи, темы (system/light/dark), onboarding, permissions
- Амбиент-режим (isAmbientEnabled — оверлей без активной сессии)
- Не заходит в встречу ботом; невидимость для screen share — только тариф $149.99/мес

## Конкуренты — что есть сверх Cluely

| Фича | Где есть |
|---|---|
| AI mock-интервью (вслух, с персонажем-интервьюером) | Final Round AI (8 сценариев: coding, system design, behavioral, salary) |
| Debrief после сессии (что получилось / что улучшить) | Final Round AI |
| Резюме + JD + материалы как контекст ответов | Final Round AI (Goals), Cluely (files), Parakeet |
| Screen Help: чтение задачи с экрана + полное решение | Final Round AI, Interview Coder (LeetCode/HackerRank/CoderPad) |
| Стелс из дока/taskbar/Activity Monitor, 20+ механик | Interview Coder |
| Загрузка документов (pricing, objection notes) | Parakeet AI |
| Заметки без бота, интеграции (Notion, HubSpot, Zapier), SSO | Granola ($14/user), Otter |
| 76 языков распознавания | Final Round AI |
| Токены/кредиты вместо подписки, lifetime-лицензия | TechScreen, Interview Coder ($799), LockedIn, Parakeet |
| Локальный запуск, local RAG, BYOK, без подписок | Natively (open-source, github topic cluely-alternative) |

## Цены (окт 2026)

Cluely: Free (лимиты) / Pro $19.99 / Pro+Undetectability $149.99. Interview Coder $299/мес или $799 lifetime. Final Round AI от $25/мес. Granola $14/user. Otter от $8.33.

## Что есть у нас vs что добавить

| Фича | У нас | Статус |
|---|---|---|
| Стелс из screen share | WDA_EXCLUDEFROMCAPTURE (бесплатно!) | есть |
| Транскрипт me/them, тогл прослушки | есть | есть |
| Assist / ответ на вопрос | Answer last | есть |
| Скриншот-контекст | Screenshot → vision | есть |
| Языки en/ru/uz | есть | есть |
| Перемещение окна, хоткеи, трей | есть | есть |
| **Сессии: save/resume/finish** | — | **делаем** |
| **AI-заметки/summary по завершении** | — | **делаем** |
| **Вопросы по сохранённым сессиям** | — | **делаем** |
| **Recap / Follow-up questions** | — | **делаем** |
| Ручные заметки к сессии | — | **делаем** |
| Копировать summary/transcript | — | **делаем** |
| Загрузка файлов (резюме/JD) в контекст | — | дальше |
| Mock-интервью с ИИ | — | дальше |
| Debrief (что улучшить) | — | дальше |
| Стелс из taskbar/dock | skipTaskbar нет | дальше |
| Meeting-уведомления, календарь | — | потом |

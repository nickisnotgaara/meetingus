# meeting-copilot (MVP, всё на Rust)

Аудио → VAD → локальный STT (whisper.cpp, CUDA) → LLM (Ollama) → оверлей с чатом, скрытый из screen capture. Windows-only. Клон Cluely по результатам реверс-инжиниринга.

## Состав

| Слой | Реализация |
|---|---|
| Микрофон + ресемплинг 16 кГц | `cpal` + health-мониторинг |
| Системный звук (собеседник) | WASAPI loopback (`wasapi`) |
| Детекция речи (me/them раздельно) | `webrtc-vad` (20 мс кадры) |
| Распознавание | `whisper-rs` (whisper.cpp CUDA), `large-v3-turbo-q5_0` |
| LLM | Ollama `/v1` (`qwen3:8b`), vision: `qwen2.5vl:3b` |
| Оверлей и чат | `eframe`/`egui` — перетаскиваемый, поверх всех окон |
| Скриншот → LLM | GDI → PNG → vision-сообщение |
| Скрытность | `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` |
| Хоткеи/трей | `global-hotkey`, `tray-icon` |

## Требования

- Rust 1.94+ (MSVC), VS Build Tools (C++), `pip install cmake libclang`.
- CUDA Toolkit 12.x (сборка через conda-тулчейн в `C:\cuda-env` или установленный CUDA; см. `build.ps1`).
- Модель Whisper:

```powershell
curl.exe -L -o models\ggml-large-v3-turbo-q5_0.bin https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin
```

- Ollama: `ollama pull qwen3:8b` (+ `ollama pull qwen2.5vl:3b` для скриншотов).

## Сборка и запуск

```powershell
.\build.ps1
ollama serve
.\target\release\meeting-copilot.exe
```

## Управление

- **Мышь**: заголовок окна — перетаскивание; кнопки `Answer last`, `Screenshot`, `Listening: ON/OFF` (прослушка собеседника), `Auto-answer`.
- **Глобальные хоткеи** (работают из любого окна):
  - `Ctrl+Alt+H` — скрыть/показать окно (невидимость для шаринга экрана)
  - `Ctrl+Alt+A` — ответить на последний вопрос собеседника
  - `Ctrl+Alt+S` — скриншот экрана → спросить ИИ (нужна vision-модель)
  - `Ctrl+Alt+L` — вкл/выкл прослушку собеседника
  - `Ctrl+Alt+стрелки` — переместить окно (±80 px)
- **Трей**: Show/Hide, Toggle listening, Screenshot, Quit; клик по иконке — показать/скрыть.
- **Консоль**: `h`/`a`/`s`/`l` как выше, `q` — выход, любой текст — сообщение в чат.
- Проверка STT: `.\target\release\meeting-copilot.exe --transcribe-wav sample.wav`

## Настройки (`.env`)

| Переменная | По умолчанию | Назначение |
|---|---|---|
| `STT_MODEL_PATH` | `models/ggml-large-v3-turbo-q5_0.bin` | файл модели Whisper |
| `STT_THREADS` | `8` | потоки CPU-декодирования |
| `LANGUAGE` | `auto` | язык (`auto`, `ru`, `en`) |
| `LLM_BASE_URL` | `http://127.0.0.1:11434/v1` | OpenAI-совместимый LLM |
| `LLM_MODEL` | `qwen3:8b` | модель ответов (для скриншотов — `qwen2.5vl:3b`) |
| `OPENAI_API_KEY` | `ollama` | ключ (для локального LLM любой) |
| `PERSONA` | generic assistant | system-промпт |
| `SYSTEM_AUDIO` | `1` | `0` — отключить loopback |

Состояние (скрытость, авто-ответ, прослушка) сохраняется в `shared-state.json` между запусками.

## Реверс-инжиниринг Cluely

Полные результаты REA: [docs/cluely-rea-findings.md](docs/cluely-rea-findings.md).

## Ограничения MVP

- Эхо при динамиках — лучше наушники.
- Скриншот требует vision-модель (`qwen2.5vl:3b`).
- Нет переназначения хоткеев, истории, обновлений и multi-window (dashboard).

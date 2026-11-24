import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./App.css";

type Segment = {
  role: "me" | "them";
  text: string;
  start_ms: number;
  end_ms: number;
};

type Answer = {
  id: string;
  label: string;
  text: string;
  thinking: boolean;
  streaming: boolean;
  error?: string;
  elapsedMs?: number;
  model?: string;
};

const ACTIONS: ReadonlyArray<{ key: string; label: string; primary?: boolean }> = [
  { key: "assist", label: "Assist", primary: true },
  { key: "what_to_say", label: "What should I say?" },
  { key: "followup", label: "Follow-up questions" },
  { key: "recap", label: "Recap" },
];

const LABELS: Record<string, string> = {
  assist: "Assist",
  what_to_say: "What to say",
  followup: "Follow-ups",
  recap: "Recap",
  custom: "You asked",
};

const mmss = (s: number) =>
  `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;

const newId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(16).slice(2)}`;

export default function App() {
  const [running, setRunning] = useState(false);
  const [segments, setSegments] = useState<Segment[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [input, setInput] = useState("");
  const [smart, setSmart] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const [answer, setAnswer] = useState<Answer | null>(null);
  const [copied, setCopied] = useState(false);
  const [stalled, setStalled] = useState<string[]>([]);
  const [keyStatus, setKeyStatus] = useState<{ configured: boolean; source?: string } | null>(null);
  const [keyInput, setKeyInput] = useState("");
  const [keySaving, setKeySaving] = useState(false);

  const listRef = useRef<HTMLDivElement>(null);
  const answerRef = useRef<HTMLDivElement>(null);
  const startedAt = useRef<number | null>(null);
  const activeId = useRef<string | null>(null);
  const askRef = useRef<(action: string, question?: string) => void>(() => {});
  const segmentsRef = useRef<Segment[]>([]);
  const runningRef = useRef(false);

  segmentsRef.current = segments;
  runningRef.current = running;

  const refreshKeyStatus = useCallback(async () => {
    try {
      setKeyStatus(await invoke<{ configured: boolean; source?: string }>("api_key_status"));
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refreshKeyStatus();
  }, [refreshKeyStatus]);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let cancelled = false;
    (async () => {
      const subs = await Promise.all([
        listen<Segment>("transcript://segment", (e) => {
          setSegments((s) => [...s, e.payload]);
          setStalled([]);
        }),
        listen<{ running: boolean }>("session://state", (e) => {
          setRunning(e.payload.running);
          if (e.payload.running) {
            startedAt.current = Date.now();
          } else {
            startedAt.current = null;
            setElapsed(0);
          }
        }),
        listen<{ message: string }>("app://error", (e) => setError(e.payload.message)),
        listen<{ role: string; stalled: boolean }>("audio://health", (e) => {
          setStalled((roles) => {
            const next = roles.filter((r) => r !== e.payload.role);
            return e.payload.stalled ? [...next, e.payload.role] : next;
          });
        }),
        listen<{ request_id: string; text: string }>("assist://delta", (e) => {
          if (e.payload.request_id !== activeId.current) return;
          const delta = e.payload.text;
          setAnswer((a) =>
            a && a.id === e.payload.request_id
              ? { ...a, text: a.text + delta, thinking: false, error: undefined }
              : a,
          );
        }),
        listen<{ request_id: string; chars: number }>("assist://thinking", (e) => {
          if (e.payload.request_id !== activeId.current) return;
          setAnswer((a) =>
            a && a.id === e.payload.request_id ? { ...a, thinking: true } : a,
          );
        }),
        listen<{ request_id: string; text: string; elapsed_ms: number; model: string }>(
          "assist://done",
          (e) => {
            if (e.payload.request_id !== activeId.current) return;
            setAnswer((a) =>
              a && a.id === e.payload.request_id
                ? {
                    ...a,
                    text: e.payload.text,
                    thinking: false,
                    streaming: false,
                    elapsedMs: e.payload.elapsed_ms,
                    model: e.payload.model,
                  }
                : a,
            );
          },
        ),
        listen<{ request_id: string; message: string }>("assist://error", (e) => {
          if (e.payload.request_id !== activeId.current) return;
          if (e.payload.message.includes("API key")) void refreshKeyStatus();
          setAnswer((a) =>
            a && a.id === e.payload.request_id
              ? { ...a, thinking: false, streaming: false, error: e.payload.message }
              : a,
          );
        }),
        listen("assist://hotkey", () => askRef.current("assist")),
      ]);
      if (!cancelled) {
        unlisteners.push(...subs);
      } else {
        // StrictMode unmounted before subscribe resolved: drop them now.
        subs.forEach((u) => u());
      }
      try {
        const s = await invoke<{ running: boolean }>("session_state");
        setRunning(s.running);
        if (s.running) startedAt.current = Date.now();
      } catch (e) {
        setError(String(e));
      }
    })();
    const timer = setInterval(() => {
      if (startedAt.current) {
        setElapsed(Math.floor((Date.now() - startedAt.current) / 1000));
      }
    }, 1000);
    return () => {
      cancelled = true;
      unlisteners.forEach((u) => u());
      clearInterval(timer);
    };
  }, [refreshKeyStatus]);

  useEffect(() => {
    listRef.current?.scrollTo({ top: listRef.current.scrollHeight });
  }, [segments]);

  useEffect(() => {
    answerRef.current?.scrollTo({ top: answerRef.current.scrollHeight });
  }, [answer?.text, answer?.thinking]);

  const ask = useCallback(
    async (action: string, question?: string) => {
      const trimmed = (question ?? "").trim();
      if (action === "custom" && !trimmed) return;
      if (
        action !== "custom" &&
        !runningRef.current &&
        segmentsRef.current.length === 0
      ) {
        setError("Start a session first, or type a question below.");
        return;
      }
      const id = newId();
      activeId.current = id;
      setCopied(false);
      setError(null);
      setAnswer({
        id,
        label: LABELS[action] ?? "Assist",
        text: "",
        thinking: true,
        streaming: true,
      });
      try {
        const res = await invoke<{ model: string }>("assist_ask", {
          requestId: id,
          action,
          question: trimmed || null,
          smart,
        });
        setAnswer((a) => (a && a.id === id ? { ...a, model: res.model } : a));
      } catch (e) {
        const message = String(e);
        if (message.includes("API key")) void refreshKeyStatus();
        if (message.includes("No conversation yet")) {
          setError(message);
          setAnswer(null);
        } else {
          setAnswer((a) =>
            a && a.id === id ? { ...a, thinking: false, streaming: false, error: message } : a,
          );
        }
      } finally {
        if (action === "custom") setInput("");
      }
    },
    [smart, refreshKeyStatus],
  );

  askRef.current = ask;

  const stopAnswer = async () => {
    try {
      await invoke("assist_cancel");
    } catch {
      /* ignore */
    }
    setAnswer((a) => (a ? { ...a, streaming: false, thinking: false } : a));
  };

  const copyAnswer = async () => {
    if (!answer?.text) return;
    try {
      await navigator.clipboard.writeText(answer.text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch (e) {
      setError(String(e));
    }
  };

  const toggleSession = async () => {
    try {
      if (running) {
        await invoke("session_end");
      } else {
        setSegments([]);
        setAnswer(null);
        activeId.current = null;
        await invoke("session_start");
      }
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  const saveKey = async () => {
    if (!keyInput.trim()) return;
    setKeySaving(true);
    try {
      const status = await invoke<{ configured: boolean; source?: string }>("set_api_key", {
        key: keyInput,
      });
      setKeyStatus(status);
      setKeyInput("");
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setKeySaving(false);
    }
  };

  const hideWindow = () => {
    void getCurrentWindow().hide();
  };

  const hasContext = running || segments.length > 0;

  return (
    <div className="widget">
      <header className="w-header">
        <div className={`status ${running ? "on" : ""}`}>
          <span className="dot" />
          <span className="label">{running ? `Recording ${mmss(elapsed)}` : "Idle"}</span>
        </div>
        <div className="header-right">
          <span className={`chip ${smart ? "gold" : ""}`} title="Model">
            {smart ? "Smart · pro" : "Fast · flash"}
          </span>
          {stalled.length > 0 && (
            <span className="chip warn" title="No audio on this input for 10s">
              {stalled.join("/")} silent
            </span>
          )}
          <button className="ghost" onClick={hideWindow} title="Hide (Ctrl+\)">
            Hide
          </button>
          <button className={`pill ${running ? "stop" : "start"}`} onClick={toggleSession}>
            {running ? "End session" : "Start session"}
          </button>
        </div>
      </header>

      {error && (
        <div className="banner" onClick={() => setError(null)}>
          {error}
        </div>
      )}

      {keyStatus && !keyStatus.configured && (
        <div className="keybar">
          <span>MiMo key</span>
          <input
            type="password"
            value={keyInput}
            placeholder="paste API key"
            onChange={(e) => setKeyInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") void saveKey();
            }}
          />
          <button onClick={saveKey} disabled={keySaving || !keyInput.trim()}>
            {keySaving ? "…" : "Save"}
          </button>
        </div>
      )}

      <div className="transcript" ref={listRef}>
        {segments.length === 0 && (
          <div className="empty">
            {running
              ? "Listening — say something…"
              : "Start a session to capture the conversation"}
          </div>
        )}
        {segments.map((s, i) => (
          <div key={i} className={`seg ${s.role}`}>
            <span className="who">{s.role === "me" ? "You" : "Them"}</span>
            <span className="text">{s.text}</span>
          </div>
        ))}
      </div>

      {answer && (
        <div className={`answer ${answer.error ? "has-error" : ""}`}>
          <div className="answer-head">
            <span className="answer-label">{answer.label}</span>
            {answer.model && (
              <span className="answer-meta">
                {answer.model.replace("mimo-v2.6-", "")}
                {answer.elapsedMs ? ` · ${(answer.elapsedMs / 1000).toFixed(1)}s` : ""}
              </span>
            )}
            <span className="answer-actions">
              {answer.streaming && (
                <button className="mini" onClick={stopAnswer} title="Stop">
                  Stop
                </button>
              )}
              {!answer.streaming && answer.text && (
                <button className="mini" onClick={copyAnswer}>
                  {copied ? "Copied" : "Copy"}
                </button>
              )}
              <button
                className="mini close"
                onClick={() => setAnswer(null)}
                title="Close"
              >
                ×
              </button>
            </span>
          </div>
          <div className="answer-body" ref={answerRef}>
            {answer.thinking && !answer.text && !answer.error && (
              <span className="thinking">
                Thinking<span className="dots">…</span>
              </span>
            )}
            {answer.text && <span className="answer-text">{answer.text}</span>}
            {answer.streaming && answer.text && <span className="caret" />}
            {answer.error && <span className="answer-error">{answer.error}</span>}
          </div>
        </div>
      )}

      <div className="actions">
        {ACTIONS.map((a) => (
          <button
            key={a.key}
            className={`action ${a.primary ? "primary" : ""}`}
            onClick={() => void ask(a.key)}
            disabled={!hasContext}
            title={!hasContext ? "Start a session first" : undefined}
          >
            {a.label}
          </button>
        ))}
      </div>

      <div className="inputrow">
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && input.trim()) void ask("custom", input);
          }}
          placeholder="Ask anything, or about the meeting…"
        />
        <button
          className={`smart ${smart ? "on" : ""}`}
          onClick={() => setSmart(!smart)}
          title="Smart mode uses MiMo pro"
        >
          Smart
        </button>
        <button
          className="send"
          onClick={() => void ask("custom", input)}
          disabled={!input.trim()}
          title="Send (Enter)"
        >
          ↵
        </button>
      </div>

      <footer className="hint">Ctrl+Enter Assist · Ctrl+\ Hide</footer>
    </div>
  );
}

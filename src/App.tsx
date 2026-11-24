import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type Segment = {
  role: string;
  text: string;
  start_ms: number;
  end_ms: number;
};

const mmss = (s: number) =>
  `${String(Math.floor(s / 60)).padStart(2, "0")}:${String(s % 60).padStart(2, "0")}`;

export default function App() {
  const [running, setRunning] = useState(false);
  const [segments, setSegments] = useState<Segment[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [input, setInput] = useState("");
  const [smart, setSmart] = useState(false);
  const [elapsed, setElapsed] = useState(0);
  const listRef = useRef<HTMLDivElement>(null);
  const startedAt = useRef<number | null>(null);

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let cancelled = false;
    (async () => {
      const subs = await Promise.all([
        listen<Segment>("transcript://segment", (e) =>
          setSegments((s) => [...s, e.payload]),
        ),
        listen<{ running: boolean }>("session://state", (e) => {
          setRunning(e.payload.running);
          if (e.payload.running) {
            startedAt.current = Date.now();
          } else {
            startedAt.current = null;
            setElapsed(0);
          }
        }),
        listen<{ message: string }>("app://error", (e) =>
          setError(e.payload.message),
        ),
        listen<{ role: string; stalled: boolean }>("audio://health", (e) =>
          setError(
            e.payload.stalled
              ? `${e.payload.role} audio stalled for >10s`
              : null,
          ),
        ),
      ]);
      if (!cancelled) unlisteners.push(...subs);
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
  }, []);

  useEffect(() => {
    listRef.current?.scrollTo({ top: listRef.current.scrollHeight });
  }, [segments]);

  const toggleSession = async () => {
    try {
      if (running) {
        await invoke("session_end");
      } else {
        setSegments([]);
        await invoke("session_start");
      }
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="widget">
      <header className="w-header">
        <div className={`status ${running ? "on" : ""}`}>
          <span className="dot" />
          <span className="label">
            {running ? `Recording ${mmss(elapsed)}` : "Idle"}
          </span>
        </div>
        <button
          className={`pill ${running ? "stop" : "start"}`}
          onClick={toggleSession}
        >
          {running ? "End session" : "Start session"}
        </button>
      </header>

      {error && (
        <div className="banner" onClick={() => setError(null)}>
          {error}
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

      <div className="actions">
        <button className="action primary" disabled title="wire in unit 8">
          Assist
        </button>
        <button className="action" disabled title="wire in unit 8">
          What should I say?
        </button>
        <button className="action" disabled title="wire in unit 8">
          Follow-up questions
        </button>
        <button className="action" disabled title="wire in unit 8">
          Recap
        </button>
      </div>

      <div className="inputrow">
        <input
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Ask a question, or about your screen…"
          disabled
        />
        <button
          className={`smart ${smart ? "on" : ""}`}
          onClick={() => setSmart(!smart)}
          title="Smart mode"
        >
          Smart
        </button>
        <button className="send" disabled title="wire in unit 8">
          ↵
        </button>
      </div>
    </div>
  );
}

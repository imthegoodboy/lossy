import { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { api, type Item, type Status } from "./api";
import "./style.css";

function SavedItem({ item }: { item: Item }) {
  const [full, setFull] = useState<Item | null>(null);
  const [error, setError] = useState("");
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    let alive = true;
    setFull(null);
    const observer = new IntersectionObserver(([entry]) => {
      if (!entry.isIntersecting) return;
      observer.disconnect();
      api<Item>({ op: "get", id: item.id }).then(value => {
        if (alive) { setFull(value); setError(""); }
      }).catch(e => { if (alive) setError(String(e)); });
    }, { rootMargin: "300px" });
    if (ref.current) observer.observe(ref.current);
    return () => { alive = false; observer.disconnect(); };
  }, [item.id, item.revision]);
  const value = full || item;
  return <article ref={ref} aria-label={item.heading || "Saved item"}>
    <header><span>{item.source}</span><time dateTime={new Date(item.updated).toISOString()}>{new Date(item.updated).toLocaleString()}</time></header>
    <h2>{item.heading || (item.kind === "image" ? "Copied image" : "Untitled")}</h2>
    {item.kind === "image" ? <img src={`data:image/png;base64,${value.text}`} alt={item.heading || "Saved clipboard image"} /> : <p className="saved-text">{value.text}</p>}
    {error && <p className="error" role="alert">{error}</p>}
  </article>;
}

function App() {
  const [items, setItems] = useState<Item[]>([]);
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState("");
  const [ready, setReady] = useState(false);
  const [more, setMore] = useState(false);
  const [starting, setStarting] = useState(false);
  const [pageCount, setPageCount] = useState(1);
  const sentinel = useRef<HTMLDivElement>(null);
  const busy = useRef(false);
  const refresh = useCallback(async () => {
    if (busy.current) return;
    busy.current = true;
    try {
      const state = await api<Status>({ op: "status" });
      setStatus(state);
      // Avoid moving or replacing content underneath an active text selection.
      if (!window.getSelection()?.toString()) {
        const collected: Item[] = [];
        let hasMore = false;
        for (let page = 0; page < pageCount; page++) {
          const batch = await api<{ items: Item[]; more: boolean }>({ op: "list", offset: page * 60 });
          collected.push(...batch.items); hasMore = batch.more;
          if (!hasMore) break;
        }
        setItems([...new Map(collected.map(item => [item.id, item])).values()]);
        setMore(hasMore);
      }
      setError(""); setReady(true);
    } catch (e) { setError(String(e)); }
    finally { busy.current = false; }
  }, [pageCount]);
  useEffect(() => { void refresh(); const timer = setInterval(refresh, 2000); return () => clearInterval(timer); }, [refresh]);
  useEffect(() => {
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting && more && !busy.current) setPageCount(count => count + 1);
    });
    if (sentinel.current) observer.observe(sentinel.current);
    return () => observer.disconnect();
  }, [more, items.length]);
  async function enable() {
    if (!status || starting) return;
    setStarting(true);
    try {
      const prefs = { ...status.prefs, enabled: true, paused: false, autostart: true };
      await api({ op: "settings", prefs });
      setStatus({ ...status, prefs });
      await refresh();
    } catch (e) { setError(String(e)); }
    finally { setStarting(false); }
  }
  return <main aria-label="Saved text and images">
    {status && !status.prefs.enabled && !status.error && <label className="consent"><input type="checkbox" checked={false} disabled={starting} onChange={enable} /><span>Save supported text and clipboard items locally, and start quietly with Windows.</span></label>}
    {(error || status?.error) && <p className="error" role="alert">{error || status?.error}</p>}
    {status?.prefs.paused && <p className="notice">Saving is paused. Resume from the system tray.</p>}
    {!ready && !error && <p className="empty">Loading saved items…</p>}
    {ready && items.length === 0 && <p className="empty">Saved text and images will appear here.</p>}
    {items.map(item => <SavedItem key={item.id} item={item} />)}
    <div ref={sentinel} className="sentinel" aria-hidden="true" />
  </main>;
}

createRoot(document.getElementById("root")!).render(<App />);

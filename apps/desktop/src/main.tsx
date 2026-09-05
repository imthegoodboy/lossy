import { useCallback, useEffect, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { api, type Item, type Status } from "./api";
import "./style.css";

function SavedItem({
  item,
  onOpen,
}: {
  item: Item;
  onOpen: (item: Item) => void;
}) {
  return (
    <article
      className={`card tone-${item.color || "paper"}`}
      role="button"
      tabIndex={0}
      aria-haspopup="dialog"
      aria-label={`Open ${item.heading || "Saved item"}`}
      onClick={() => {
        if (!window.getSelection()?.toString()) onOpen(item);
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpen(item);
        }
      }}
    >
      <header>
        <span>{item.source}</span>
        <time dateTime={new Date(item.updated).toISOString()}>
          {new Date(item.updated).toLocaleString()}
        </time>
      </header>
      <h2>
        {item.pinned && (
          <span className="pin-mark" aria-label="Pinned">
            ◆{" "}
          </span>
        )}
        {item.heading || (item.kind === "image" ? "Copied image" : "Untitled")}
      </h2>
      {item.kind === "image" ? (
        <img
          className="preview-image"
          src={`data:image/png;base64,${item.text}`}
          alt={item.heading || "Saved clipboard image"}
        />
      ) : (
        <p className="saved-text preview-text">{item.text}</p>
      )}
    </article>
  );
}

function FullItem({
  item,
  onClose,
  onChange,
}: {
  item: Item;
  onClose: () => void;
  onChange: () => void;
}) {
  const [full, setFull] = useState<Item | null>(null);
  const [error, setError] = useState("");
  const [copying, setCopying] = useState(false);
  const [copied, setCopied] = useState(false);
  const [editing, setEditing] = useState(false);
  const [heading, setHeading] = useState("");
  const [text, setText] = useState("");
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [notice, setNotice] = useState("");
  const [history, setHistory] = useState(false);
  const [revision, setRevision] = useState(1);
  const [discard, setDiscard] = useState(false);
  const dirty =
    editing && !!full && (heading !== full.heading || text !== full.text);
  function close() {
    if (saving) return;
    if (dirty) setDiscard(true);
    else onClose();
  }
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    let alive = true;
    const element = dialog.current!;
    element.showModal();
    api<Item>({ op: "get", id: item.id })
      .then((value) => {
        if (alive) setFull(value);
      })
      .catch((e) => {
        if (alive) setError(String(e));
      });
    return () => {
      alive = false;
      element.close();
    };
  }, [item.id]);
  async function copy() {
    if (!full || copying) return;
    setCopying(true);
    setError("");
    try {
      await api({ op: "copy", id: full.id, revision: full.revision });
      setCopied(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setCopying(false);
    }
  }
  async function color(value: string) {
    if (!full || saving) return;
    setSaving(true);
    setError("");
    try {
      await api({ op: "color", id: full.id, color: value });
      setFull({ ...full, color: value });
      onChange();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }
  async function pin() {
    if (!full) return;
    try {
      await api({ op: "pin", id: full.id, pinned: !full.pinned });
      setFull({ ...full, pinned: !full.pinned });
      onChange();
    } catch (e) {
      setError(String(e));
    }
  }
  async function save() {
    if (!full || saving) return;
    setSaving(true);
    setError("");
    try {
      const updated = await api<Item>({
        op: "save",
        heading: heading.trim() || "Untitled",
        text,
        ...(full.kind === "note"
          ? { id: full.id, revision: full.revision }
          : {}),
      });
      if (full.color && full.color !== "paper") {
        await api({ op: "color", id: updated.id, color: full.color });
        updated.color = full.color;
      }
      setFull(updated);
      setEditing(false);
      setCopied(false);
      setNotice(
        full.kind === "note"
          ? "Changes saved"
          : "Recovery copy saved. Original kept.",
      );
      onChange();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }
  async function remove() {
    if (!full || saving) return;
    setSaving(true);
    try {
      await api({ op: "delete", id: full.id, revision: full.revision });
      onChange();
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }
  async function loadRevision() {
    if (!full) return;
    try {
      const older = await api<Item>({ op: "revision", id: full.id, revision });
      setHeading(older.heading);
      setText(older.text);
      setEditing(true);
      setError("");
    } catch {
      setError(
        "That revision is unavailable. Try revision 1 or a recent revision.",
      );
    }
  }
  return (
    <dialog
      ref={dialog}
      aria-labelledby="full-heading"
      onCancel={(event) => {
        event.preventDefault();
        close();
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) {
          const bounds = event.currentTarget.getBoundingClientRect();
          if (
            event.clientX < bounds.left ||
            event.clientX > bounds.right ||
            event.clientY < bounds.top ||
            event.clientY > bounds.bottom
          )
            close();
        }
      }}
    >
      <div className="popup-heading">
        <div>
          <p className="source">{item.source}</p>
          <h2 id="full-heading">
            {full?.heading || item.heading || "Saved item"}
          </h2>
        </div>
        <button className="close" aria-label="Close popup" onClick={close}>
          ×
        </button>
      </div>
      <div className="popup-content">
        {full ? (
          full.kind === "image" ? (
            <img
              src={`data:image/png;base64,${full.text}`}
              alt={full.heading || "Saved clipboard image"}
            />
          ) : editing ? (
            <div className="edit-fields">
              <label>
                Heading
                <input
                  value={heading}
                  maxLength={120}
                  onChange={(e) => setHeading(e.target.value)}
                />
              </label>
              <label>
                Text
                <textarea
                  value={text}
                  onChange={(e) => setText(e.target.value)}
                />
              </label>
            </div>
          ) : (
            <p className="saved-text">{full.text}</p>
          )
        ) : (
          !error && <p className="notice">Loading full item…</p>
        )}
      </div>
      {full && (
        <div className="item-options">
          <fieldset className="colors" disabled={saving}>
            <legend>Box colour</legend>
            {["paper", "rose", "peach", "lavender", "sage", "blue"].map(
              (value) => (
                <label
                  className={`swatch tone-${value}`}
                  title={value}
                  key={value}
                >
                  <input
                    type="radio"
                    name="box-color"
                    aria-label={`${value} box colour`}
                    checked={(full.color || "paper") === value}
                    onChange={() => color(value)}
                  />
                  <span aria-hidden="true" />
                </label>
              ),
            )}
          </fieldset>
          <label className="pin-option">
            <input type="checkbox" checked={full.pinned} onChange={pin} />
            Keep pinned
          </label>
        </div>
      )}
      {full && (
        <details className="more-options">
          <summary>More options</summary>
          <div className="option-links">
            {full.kind !== "image" && (
              <>
                <button
                  onClick={() => {
                    if (dirty) return;
                    setHeading(full.heading);
                    setText(full.text);
                    setEditing(!editing);
                    setHistory(false);
                  }}
                >
                  {editing ? "Stop editing" : "Edit text"}
                </button>
                <button
                  onClick={() => {
                    setHistory(!history);
                    setRevision(full.revision);
                  }}
                >
                  Revision history
                </button>
              </>
            )}
            <button className="delete-link" onClick={() => setDeleting(true)}>
              Delete item
            </button>
          </div>
        </details>
      )}
      {history && full && (
        <div className="revision-row">
          <label>
            Revision{" "}
            <input
              aria-label="Revision number"
              type="number"
              min={1}
              max={full.revision}
              value={revision}
              onChange={(e) => setRevision(Number(e.target.value))}
            />
          </label>
          <span>of {full.revision}</span>
          <button onClick={loadRevision}>Load revision</button>
        </div>
      )}
      {deleting && (
        <div className="confirm">
          <p>
            Delete this item from the archive? Existing backups may retain it
            until rotation.
          </p>
          <button disabled={saving} onClick={remove}>
            Delete permanently
          </button>
          <button onClick={() => setDeleting(false)}>Keep item</button>
        </div>
      )}
      {discard && (
        <div className="confirm">
          <p>Your edits have not been saved.</p>
          <button onClick={onClose}>Discard edits</button>
          <button onClick={() => setDiscard(false)}>Keep editing</button>
        </div>
      )}
      {error && (
        <p className="error" role="alert">
          {error}
        </p>
      )}
      <div className="popup-footer">
        <span role="status">
          {notice ||
            (copied
              ? "Copied to clipboard"
              : dirty
                ? "Unsaved edits"
                : full
                  ? "Saved on this device"
                  : "")}
        </span>
        {editing ? (
          <button className="copy" disabled={saving || !dirty} onClick={save}>
            {saving
              ? "Saving…"
              : full?.kind === "note"
                ? "Save changes"
                : "Save recovery copy"}
          </button>
        ) : (
          <button className="copy" disabled={!full || copying} onClick={copy}>
            {copying ? "Copying…" : copied ? "Copy again" : "Copy"}
          </button>
        )}
      </div>
    </dialog>
  );
}

function App() {
  const [items, setItems] = useState<Item[]>([]);
  const [status, setStatus] = useState<Status | null>(null);
  const [error, setError] = useState("");
  const [ready, setReady] = useState(false);
  const [more, setMore] = useState(false);
  const [starting, setStarting] = useState(false);
  const [pageCount, setPageCount] = useState(1);
  const [selected, setSelected] = useState<Item | null>(null);
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
          const batch = await api<{ items: Item[]; more: boolean }>({
            op: "list",
            offset: page * 60,
          });
          collected.push(...batch.items);
          hasMore = batch.more;
          if (!hasMore) break;
        }
        setItems([
          ...new Map(collected.map((item) => [item.id, item])).values(),
        ]);
        setMore(hasMore);
      }
      setError("");
      setReady(true);
    } catch (e) {
      setError(String(e));
    } finally {
      busy.current = false;
    }
  }, [pageCount]);
  useEffect(() => {
    void refresh();
    const timer = setInterval(refresh, 2000);
    return () => clearInterval(timer);
  }, [refresh]);
  useEffect(() => {
    const observer = new IntersectionObserver(([entry]) => {
      if (entry.isIntersecting && more && !busy.current)
        setPageCount((count) => count + 1);
    });
    if (sentinel.current) observer.observe(sentinel.current);
    return () => observer.disconnect();
  }, [more, items.length]);
  async function enable() {
    if (!status || starting) return;
    setStarting(true);
    try {
      const prefs = {
        ...status.prefs,
        enabled: true,
        paused: false,
        autostart: true,
      };
      await api({ op: "settings", prefs });
      setStatus({ ...status, prefs });
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setStarting(false);
    }
  }
  return (
    <main aria-label="Saved text and images">
      {status && !status.prefs.enabled && !status.error && (
        <label className="consent">
          <input
            type="checkbox"
            checked={false}
            disabled={starting}
            onChange={enable}
          />
          <span>
            Save supported text and clipboard items locally, and start quietly
            with Windows.
          </span>
        </label>
      )}
      {(error || status?.error) && (
        <p className="error" role="alert">
          {error || status?.error}
        </p>
      )}
      {status?.prefs.paused && (
        <p className="notice">Saving is paused. Resume from the system tray.</p>
      )}
      {!ready && !error && <p className="empty">Loading saved items…</p>}
      {ready && items.length === 0 && (
        <p className="empty">Saved text and images will appear here.</p>
      )}
      <section className="matrix" aria-label="Saved items">
        {items.map((item) => (
          <SavedItem key={item.id} item={item} onOpen={setSelected} />
        ))}
      </section>
      <div ref={sentinel} className="sentinel" aria-hidden="true" />
      {selected && (
        <FullItem
          key={selected.id}
          item={selected}
          onChange={() => {
            void refresh();
          }}
          onClose={() => setSelected(null)}
        />
      )}
    </main>
  );
}

createRoot(document.getElementById("root")!).render(<App />);

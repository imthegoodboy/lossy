import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type DragEvent,
} from "react";
import { createRoot } from "react-dom/client";
import {
  ArrowUpRight,
  Check,
  Copy,
  DotsSixVertical,
  Image as ImageIcon,
  PushPin,
  TextAlignLeft,
  X,
} from "@phosphor-icons/react";
import {
  api,
  setupBrowser,
  type Item,
  type Status,
  type Preferences,
} from "./api";
import { CaptureSetup } from "./CaptureSetup";
import "./style.css";

function shortTime(value: number) {
  const date = new Date(value);
  return date.toDateString() === new Date().toDateString()
    ? date.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })
    : date.toLocaleDateString([], { month: "short", day: "numeric" });
}

function ItemGlyph({ kind }: { kind: string }) {
  const Glyph =
    kind === "image" ? ImageIcon : kind === "clipboard" ? Copy : TextAlignLeft;
  return <Glyph size={16} weight="regular" aria-hidden="true" />;
}

function SavedItem({
  item,
  onOpen,
  dragging,
  dropTarget,
  onDragStart,
  onDragEnd,
  onDragOver,
  onDrop,
  onMoveKey,
}: {
  item: Item;
  onOpen: (item: Item) => void;
  dragging: boolean;
  dropTarget: boolean;
  onDragStart: (event: DragEvent, id: string) => void;
  onDragEnd: () => void;
  onDragOver: (event: DragEvent, id: string) => void;
  onDrop: (event: DragEvent, id: string) => void;
  onMoveKey: (id: string, key: string) => void;
}) {
  return (
    <article
      className={`card tone-${item.color || "paper"}${dragging ? " dragging" : ""}${dropTarget ? " drop-target" : ""}`}
      data-card-id={item.id}
      role="button"
      tabIndex={0}
      aria-haspopup="dialog"
      aria-label={`Open ${item.heading || "Saved item"}`}
      aria-describedby="arrange-help"
      onDragOver={(event) => onDragOver(event, item.id)}
      onDrop={(event) => onDrop(event, item.id)}
      onClick={() => {
        if (!window.getSelection()?.toString()) onOpen(item);
      }}
      onKeyDown={(event) => {
        if (
          event.altKey &&
          ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(
            event.key,
          )
        ) {
          event.preventDefault();
          onMoveKey(item.id, event.key);
          return;
        }
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpen(item);
        }
      }}
    >
      <header
        draggable
        onDragStart={(event) => onDragStart(event, item.id)}
        onDragEnd={onDragEnd}
        title="Drag to rearrange. Or focus this card and use Alt + arrow keys."
      >
        <DotsSixVertical className="drag-grip" size={15} aria-hidden="true" />
        <span className="card-source" title={item.source}>
          <ItemGlyph kind={item.kind} />
          <span>{item.source}</span>
        </span>
        {item.pinned && (
          <PushPin
            className="pin-mark"
            size={15}
            weight="fill"
            aria-label="Pinned"
          />
        )}
      </header>
      <div className="card-body">
        <h2>
          {item.heading ||
            (item.kind === "image" ? "Copied image" : "Untitled")}
        </h2>
        {item.kind === "image" ? (
          <img
            draggable={false}
            className="preview-image"
            src={`data:image/png;base64,${item.text}`}
            alt={item.heading || "Saved clipboard image"}
          />
        ) : (
          <p className="saved-text preview-text">{item.text}</p>
        )}
      </div>
      <footer className="card-footer">
        <time
          dateTime={new Date(item.updated).toISOString()}
          title={new Date(item.updated).toLocaleString()}
        >
          {shortTime(item.updated)}
        </time>
        <span className="card-kind">
          {item.kind === "image"
            ? "Image"
            : item.kind === "note"
              ? "Note"
              : item.kind === "clipboard"
                ? "Clipboard"
                : "Draft"}
        </span>
        <ArrowUpRight className="open-hint" size={15} aria-hidden="true" />
      </footer>
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
      className={`tone-${full?.color || item.color || "paper"}`}
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
          <p className="source">
            <ItemGlyph kind={full?.kind || item.kind} />
            {full?.source || item.source}
          </p>
          <h2 id="full-heading">
            {full?.heading || item.heading || "Saved item"}
          </h2>
        </div>
        <button className="close" aria-label="Close popup" onClick={close}>
          <X size={19} />
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
                  disabled={saving}
                  maxLength={120}
                  onChange={(e) => setHeading(e.target.value)}
                />
              </label>
              <label>
                Text
                <textarea
                  value={text}
                  disabled={saving}
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
            <input
              type="checkbox"
              checked={full.pinned}
              disabled={saving}
              onChange={pin}
            />
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
                  disabled={dirty || saving}
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
            {copied ? <Check size={17} /> : <Copy size={17} />}
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
  const [dragged, setDragged] = useState<string | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  const [arrangementNotice, setArrangementNotice] = useState("");
  const dragId = useRef<string | null>(null);
  const arranging = useRef(false);
  const interactionVersion = useRef(0);
  const suppressClickUntil = useRef(0);
  const matrix = useRef<HTMLElement>(null);
  const sentinel = useRef<HTMLDivElement>(null);
  const busy = useRef(false);
  const updatingPreferences = useRef(false);
  const refresh = useCallback(async () => {
    if (
      busy.current ||
      dragId.current ||
      arranging.current ||
      updatingPreferences.current
    )
      return;
    busy.current = true;
    const version = interactionVersion.current;
    try {
      const state = await api<Status>({ op: "status" });
      if (version !== interactionVersion.current) return;
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
        if (version !== interactionVersion.current) return;
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
  function endDrag() {
    dragId.current = null;
    setDragged(null);
    setDropTarget(null);
    suppressClickUntil.current = Date.now() + 350;
  }
  async function moveCard(id: string, target: string) {
    if (arranging.current || id === target) return;
    const from = items.findIndex((item) => item.id === id);
    const to = items.findIndex((item) => item.id === target);
    if (from < 0 || to < 0) return;
    const previous = items;
    const next = [...items];
    next.splice(to, 0, next.splice(from, 1)[0]);
    arranging.current = true;
    interactionVersion.current++;
    setItems(next);
    setArrangementNotice("Saving arrangement…");
    try {
      await api({ op: "reorder", ids: next.map((item) => item.id) });
      setArrangementNotice(`Moved to position ${to + 1}. Arrangement saved.`);
    } catch (e) {
      setItems(previous);
      setArrangementNotice("");
      setError(
        `Could not save arrangement. Previous order restored. ${String(e)}`,
      );
    } finally {
      arranging.current = false;
    }
  }
  function moveWithKeys(id: string, key: string) {
    if (!matrix.current) return;
    const columns = getComputedStyle(matrix.current).gridTemplateColumns.split(
      " ",
    ).length;
    const offset =
      key === "ArrowLeft"
        ? -1
        : key === "ArrowRight"
          ? 1
          : key === "ArrowUp"
            ? -columns
            : columns;
    const target = items[items.findIndex((item) => item.id === id) + offset];
    if (target) void moveCard(id, target.id);
  }
  async function updatePreferences(changes: Partial<Preferences>) {
    if (!status || starting) return;
    updatingPreferences.current = true;
    interactionVersion.current++;
    setStarting(true);
    try {
      const prefs = {
        ...status.prefs,
        ...changes,
      };
      await api({ op: "settings", prefs });
      setStatus({ ...status, prefs });
    } catch (e) {
      setError(String(e));
      throw e;
    } finally {
      setStarting(false);
      updatingPreferences.current = false;
    }
  }
  function enable() {
    void updatePreferences({
      enabled: true,
      paused: false,
      autostart: true,
    }).catch(() => {});
  }
  return (
    <main
      aria-label="Saved text and images"
      onDragOver={(event) => event.preventDefault()}
      onDrop={(event) => event.preventDefault()}
    >
      <header className="archive-heading">
        <h1>
          Saved for later
          <span className="item-count">
            {" "}
            {ready ? `${items.length}${more ? "+" : ""}` : ""}
          </span>
        </h1>
        <p>Just on this device</p>
      </header>
      {status && !status.prefs.enabled && !status.error && (
        <label className="consent">
          <input
            type="checkbox"
            checked={false}
            disabled={starting}
            onChange={enable}
          />
          <span>
            Enable local saving for the selected apps and start quietly with
            Windows. Nothing is captured until you enable this. Choose your apps
            in Capture setup below.
          </span>
        </label>
      )}
      {status && !status.error && (
        <CaptureSetup
          status={status}
          busy={starting}
          update={updatePreferences}
          setupBrowser={setupBrowser}
        />
      )}
      {(error || status?.error) && (
        <p className="error" role="alert">
          {error || status?.error}
        </p>
      )}
      {status?.prefs.paused && (
        <p className="notice">
          Saving is paused. Resume in Capture setup or from the system tray.
        </p>
      )}
      {!ready && !error && (
        <section
          className="matrix loading"
          aria-label="Loading saved items"
          aria-busy="true"
        >
          {[0, 1, 2, 3].map((i) => (
            <div className="card skeleton" key={i}>
              <span />
              <span />
              <span />
            </div>
          ))}
        </section>
      )}
      {ready && items.length === 0 && (
        <div className="empty">
          <TextAlignLeft size={32} weight="light" aria-hidden="true" />
          <h2>A place for your unfinished thoughts.</h2>
          <p>
            Text and images from supported apps will appear here.
            <br />
            Click any saved item to pick up where you left off.
          </p>
        </div>
      )}
      <section ref={matrix} className="matrix" aria-label="Saved items">
        {items.map((item) => (
          <SavedItem
            key={item.id}
            item={item}
            onOpen={(value) => {
              if (!dragId.current && Date.now() > suppressClickUntil.current)
                setSelected(value);
            }}
            dragging={dragged === item.id}
            dropTarget={dropTarget === item.id && dragged !== item.id}
            onDragStart={(event, id) => {
              if (arranging.current) {
                event.preventDefault();
                return;
              }
              event.dataTransfer.setData("application/x-lossy-card", id);
              event.dataTransfer.effectAllowed = "move";
              const card = event.currentTarget.closest("article");
              if (card) event.dataTransfer.setDragImage(card, 28, 22);
              interactionVersion.current++;
              dragId.current = id;
              setDragged(id);
              setArrangementNotice("");
            }}
            onDragEnd={endDrag}
            onDragOver={(event, id) => {
              if (!dragId.current) return;
              event.preventDefault();
              event.dataTransfer.dropEffect = "move";
              setDropTarget(id);
            }}
            onDrop={(event, id) => {
              event.preventDefault();
              const from = dragId.current;
              endDrag();
              if (from) void moveCard(from, id);
            }}
            onMoveKey={moveWithKeys}
          />
        ))}
      </section>
      {items.length > 1 && (
        <p className="arrange-help" id="arrange-help">
          Drag a card’s top edge to rearrange.{" "}
          <span>Alt + arrow keys works too.</span>
        </p>
      )}
      <p className="sr-only" role="status">
        {arrangementNotice}
      </p>
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

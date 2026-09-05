import React, { useState, useEffect, useRef, useCallback } from "react";
import { createRoot } from "react-dom/client";
import { createPortal } from "react-dom";
import {
  Heart,
  SquaresFour,
  PushPin,
  FileText,
  ClipboardText,
  Image as ImageIcon,
  Plus,
  MagnifyingGlass,
  Gear,
  ShieldCheck,
  Pause,
  Play,
  Copy,
  X,
  Trash,
  ArrowCounterClockwise,
  FolderOpen,
  Check,
  ArrowLeft,
  ArrowRight,
  DownloadSimple,
} from "@phosphor-icons/react";
import {
  api,
  openFolder,
  setupBrowser,
  type Item,
  type Preferences,
  type Status,
} from "./api";
import "./style.css";

const filters = [
  ["all", "Everything", SquaresFour],
  ["draft", "Drafts", FileText],
  ["note", "My notes", Heart],
  ["clipboard", "Clipboard", ClipboardText],
  ["image", "Images", ImageIcon],
  ["pinned", "Pinned", PushPin],
] as const;
const defaults: Preferences = {
  enabled: false,
  paused: false,
  clipboard: true,
  autostart: false,
  retention_days: 30,
  allowed_apps: ["notepad.exe"],
  browser_capture: true,
};
function time(ms: number) {
  return new Date(ms).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}
function Modal({
  title,
  children,
  onClose,
  wide = false,
}: {
  title: string;
  children: React.ReactNode;
  onClose: () => void;
  wide?: boolean;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const dialog = ref.current!;
    dialog.showModal();
    return () => dialog.close();
  }, []);
  return (
    <dialog
      ref={ref}
      className={wide ? "modal wide" : "modal"}
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="modal-head">
        <h2>{title}</h2>
        <button className="icon" aria-label="Close dialog" onClick={onClose}>
          <X size={21} />
        </button>
      </div>
      {children}
    </dialog>
  );
}
function App() {
  const [status, setStatus] = useState<Status | null>(null),
    [items, setItems] = useState<Item[]>([]),
    [filter, setFilter] = useState("all"),
    [search, setSearch] = useState(""),
    [offset, setOffset] = useState(0),
    [more, setMore] = useState(false),
    [loading, setLoading] = useState(true),
    [error, setError] = useState(""),
    [toast, setToast] = useState("");
  const [selected, setSelected] = useState<Item | null>(null),
    [settings, setSettings] = useState(false),
    [newNote, setNewNote] = useState(false),
    [busy, setBusy] = useState(false);
  const requestVersion = useRef(0),
    toastTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const notify = useCallback((message: string) => {
    setToast(message);
    clearTimeout(toastTimer.current);
    toastTimer.current = setTimeout(() => setToast(""), 3200);
  }, []);
  const refresh = useCallback(async () => {
    const version = ++requestVersion.current;
    try {
      const result = await api<{ items: Item[]; more: boolean }>({
        op: "list",
        filter,
        search,
        offset,
      });
      if (version === requestVersion.current) {
        setItems(result.items);
        setMore(result.more);
        setError("");
      }
    } catch (e) {
      if (version === requestVersion.current) setError(String(e));
    } finally {
      if (version === requestVersion.current) setLoading(false);
    }
  }, [filter, search, offset]);
  useEffect(() => {
    const timer = setTimeout(refresh, 160);
    return () => clearTimeout(timer);
  }, [refresh]);
  useEffect(() => {
    let alive = true;
    let running = false;
    const poll = async () => {
      if (running) return;
      running = true;
      try {
        const s = await api<Status>({ op: "status" });
        if (alive) setStatus(s);
      } catch (e) {
        if (alive) setError(String(e));
      } finally {
        running = false;
      }
    };
    poll();
    const timer = setInterval(poll, 1500);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, []);
  useEffect(() => {
    if (status?.last_saved) refresh();
  }, [status?.last_saved, refresh]);
  async function open(item: Item) {
    try {
      setSelected(await api<Item>({ op: "get", id: item.id }));
    } catch (e) {
      notify(String(e));
    }
  }
  async function copy(item: Item) {
    try {
      await api({ op: "copy", id: item.id });
      notify(
        item.kind === "image"
          ? "Image copied"
          : "Copied. Pick up where you left off.",
      );
    } catch (e) {
      notify(String(e));
    }
  }
  async function pause() {
    if (!status) return;
    setBusy(true);
    try {
      const prefs = { ...status.prefs, paused: !status.prefs.paused };
      await api({ op: "settings", prefs });
      setStatus({ ...status, prefs });
    } catch (e) {
      notify(String(e));
    } finally {
      setBusy(false);
    }
  }
  const onboarding = status && !status.prefs.enabled && !status.error;
  return (
    <div className="app-shell">
      <aside>
        <a
          className="brand"
          href="#"
          onClick={(e) => {
            e.preventDefault();
            setFilter("all");
          }}
          aria-label="Lossy home"
        >
          <span className="brand-mark">
            L<span />
          </span>
          lossy<span className="brand-period">.</span>
        </a>
        <p className="sidebar-caption">Your words, kept close.</p>
        <button className="primary new-button" onClick={() => setNewNote(true)}>
          <Plus weight="bold" size={18} /> New note
        </button>
        <nav aria-label="Saved item filters">
          {filters.map(([key, label, Icon]) => (
            <button
              key={key}
              aria-current={filter === key ? "page" : undefined}
              className={filter === key ? "nav-item active" : "nav-item"}
              onClick={() => {
                setFilter(key);
                setOffset(0);
              }}
            >
              <Icon weight={filter === key ? "fill" : "regular"} size={21} />
              {label}
              {key === "all" && items.length > 0 && (
                <span className="nav-count">
                  {items.length}
                  {more ? "+" : ""}
                </span>
              )}
            </button>
          ))}
        </nav>
        <div className="sidebar-bottom">
          <div className="local-note">
            <ShieldCheck size={25} weight="duotone" />
            <div>
              <strong>Just yours.</strong>
              <span>Stored on this device.</span>
            </div>
          </div>
          <button className="nav-item" onClick={() => setSettings(true)}>
            <Gear size={21} /> Preferences
          </button>
        </div>
      </aside>
      <main>
        <header>
          <div className="breadcrumb">
            YOUR LITTLE ARCHIVE <span>/</span>{" "}
            {filters.find((f) => f[0] === filter)?.[1]}
          </div>
          <button
            disabled={!status?.prefs.enabled || busy}
            className={`capture-toggle ${status?.prefs.paused ? "is-paused" : ""}`}
            onClick={pause}
          >
            <span className="status-dot" />
            {status?.error
              ? "Needs attention"
              : !status?.prefs.enabled
                ? "Setup needed"
                : status.prefs.paused
                  ? "Saving paused"
                  : "Saving locally"}
            {status?.prefs.paused ? (
              <Play size={14} weight="fill" />
            ) : (
              <Pause size={14} weight="fill" />
            )}
          </button>
        </header>
        <section className="page-heading">
          <div>
            <div className="eyebrow">A SOFTER LANDING FOR YOUR THOUGHTS</div>
            <h1>
              {filter === "all"
                ? "Nothing good gets lost."
                : filters.find((f) => f[0] === filter)?.[1]}
            </h1>
            <p>
              {filter === "all"
                ? "Half-written thoughts. Copied things. Ready when you are."
                : "A little less searching. A little more picking up where you left off."}
            </p>
          </div>
          <div className="paper-stack" aria-hidden="true">
            <div className="paper-back" />
            <div className="paper-front">
              <Heart size={26} weight="fill" />
              <i />
              <i />
              <i />
              <span>kept safe</span>
            </div>
          </div>
        </section>
        <div className="toolbar">
          <label className="search">
            <MagnifyingGlass size={20} />
            <input
              aria-label="Search saved items"
              placeholder="Find that thing you were writing…"
              value={search}
              onChange={(e) => {
                setSearch(e.target.value);
                setOffset(0);
              }}
            />
            {search && (
              <button
                className="icon"
                aria-label="Clear search"
                onClick={() => setSearch("")}
              >
                <X size={16} />
              </button>
            )}
          </label>
          <span className="archive-count">
            {items.length}{" "}
            {items.length === 1 ? "little thing" : "little things"}
            {more ? " +" : ""}
          </span>
        </div>
        {(error || status?.error) && (
          <div className="error-banner" role="alert">
            {error || status?.error}
            <button onClick={refresh}>Try again</button>
          </div>
        )}
        {loading ? (
          <div className="empty">
            <Heart size={36} />
            <h2>Opening your archive…</h2>
          </div>
        ) : items.length === 0 ? (
          <div className="empty">
            <div className="empty-icon">
              <FileText size={38} weight="duotone" />
            </div>
            <h2>
              {search
                ? "No matches just yet."
                : "Your next thought belongs here."}
            </h2>
            <p>
              {search
                ? "Try a different word or clear your filters."
                : "Create a note, copy something in an allowed app, or start a draft in a supported editor."}
            </p>
            <button className="secondary" onClick={() => setNewNote(true)}>
              <Plus size={17} />
              Write a little something
            </button>
          </div>
        ) : (
          <div className="card-grid">
            {items.map((item) => (
              <article
                className={`item-card ${item.kind === "note" ? "note-card" : ""}`}
                key={item.id}
              >
                <button
                  className="card-open"
                  onClick={() => open(item)}
                  aria-label={`Open ${item.heading}`}
                >
                  <div className="card-meta">
                    <span className={`type-icon ${item.kind}`}>
                      {item.kind === "image" ? (
                        <ImageIcon size={17} />
                      ) : item.kind === "clipboard" ? (
                        <ClipboardText size={17} />
                      ) : (
                        <FileText size={17} />
                      )}
                    </span>
                    <span>
                      {item.kind === "note"
                        ? "My note"
                        : item.kind === "clipboard"
                          ? "Copied text"
                          : item.kind === "image"
                            ? "Copied image"
                            : "Saved draft"}
                    </span>
                    {item.pinned && (
                      <PushPin
                        className="pinned-mark"
                        weight="fill"
                        size={15}
                      />
                    )}
                  </div>
                  <h2>{item.heading || "Untitled"}</h2>
                  {item.kind === "image" ? (
                    <div className="image-preview">
                      {item.text ? (
                        <img
                          src={`data:image/png;base64,${item.text}`}
                          alt="Copied image preview"
                        />
                      ) : (
                        <ImageIcon size={44} weight="duotone" />
                      )}
                    </div>
                  ) : (
                    <p className="snippet">{item.text}</p>
                  )}
                </button>
                <div className="card-footer">
                  <span title={item.source}>{item.source}</span>
                  <button
                    className="icon"
                    onClick={() => copy(item)}
                    aria-label={`Copy ${item.heading}`}
                  >
                    <Copy size={17} />
                  </button>
                </div>
                <div className="card-time">{time(item.updated)}</div>
              </article>
            ))}
          </div>
        )}
        {(offset > 0 || more) && (
          <div className="pagination">
            <button
              className="secondary"
              disabled={offset === 0}
              onClick={() => setOffset(Math.max(0, offset - 60))}
            >
              <ArrowLeft />
              Previous
            </button>
            <span>Page {offset / 60 + 1}</span>
            <button
              className="secondary"
              disabled={!more}
              onClick={() => setOffset(offset + 60)}
            >
              Next
              <ArrowRight />
            </button>
          </div>
        )}
        <footer>
          <ShieldCheck size={15} />
          No cloud. No account. Just a little peace of mind.
          <span>Made for the unfinished.</span>
        </footer>
      </main>
      {(selected || newNote) && (
        <Editor
          item={selected}
          onClose={() => {
            setSelected(null);
            setNewNote(false);
          }}
          onChange={() => refresh()}
          notify={notify}
          onCopy={copy}
        />
      )}
      {(settings || onboarding) && (
        <Settings
          initial={status?.prefs || defaults}
          onboarding={!!onboarding}
          onClose={() => setSettings(false)}
          onSave={async (prefs) => {
            await api({ op: "settings", prefs });
            setStatus((s) => (s ? { ...s, prefs } : null));
            setSettings(false);
            notify("Preferences saved");
          }}
          notify={notify}
        />
      )}
      {toast &&
        createPortal(
          <div className="toast" role="status">
            <Check size={18} />
            {toast}
          </div>,
          ((selected || newNote || settings || onboarding) &&
            document.querySelector("dialog[open]")) ||
            document.body,
        )}
    </div>
  );
}

function Editor({
  item,
  onClose,
  onChange,
  notify,
  onCopy,
}: {
  item: Item | null;
  onClose: () => void;
  onChange: () => void;
  notify: (s: string) => void;
  onCopy: (i: Item) => void;
}) {
  const [heading, setHeading] = useState(item?.heading || ""),
    [text, setText] = useState(item?.text || ""),
    [saved, setSaved] = useState(item),
    [busy, setBusy] = useState(false),
    [history, setHistory] = useState(false),
    [historyRevision, setHistoryRevision] = useState(item?.revision || 1),
    [confirmDelete, setConfirmDelete] = useState(false),
    [dirty, setDirty] = useState(false),
    [confirmClose, setConfirmClose] = useState(false),
    [saveError, setSaveError] = useState("");
  const latestInput = useRef({ heading, text });
  latestInput.current = { heading, text };
  const image = item?.kind === "image";
  const close = () => {
    if (dirty) setConfirmClose(true);
    else onClose();
  };
  // Start a checkpoint as soon as React observes an edit. Serialize writes and save the
  // newest snapshot next if typing continues during the durable disk commit.
  useEffect(() => {
    if (dirty && !busy && !saveError && !image && !confirmClose)
      void save(false);
  }, [heading, text, dirty, busy, saveError, image, confirmClose]);
  async function save(announce = true) {
    if (busy) return;
    setBusy(true);
    setSaveError("");
    try {
      const payload: Record<string, unknown> = {
        op: "save",
        heading:
          heading.trim() ||
          text
            .split("\n")
            .find((line) => line.trim())
            ?.slice(0, 72) ||
          "Untitled note",
        text,
      };
      if (saved?.kind === "note") {
        payload.id = saved.id;
        payload.revision = saved.revision;
      }
      const next = await api<Item>(payload);
      setSaved(next);
      setDirty(
        latestInput.current.heading !== heading ||
          latestInput.current.text !== text,
      );
      if (announce)
        notify(
          item && item.kind !== "note"
            ? "Recovery copy saved to My notes"
            : "Note saved",
        );
      onChange();
    } catch (e) {
      setSaveError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function remove() {
    if (!saved) return;
    setBusy(true);
    try {
      await api({ op: "delete", id: saved.id, revision: saved.revision });
      onChange();
      onClose();
      notify("Removed from your archive");
    } catch (e) {
      notify(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function pin() {
    if (!saved) return;
    try {
      await api({ op: "pin", id: saved.id, pinned: !saved.pinned });
      setSaved({ ...saved, pinned: !saved.pinned });
      onChange();
    } catch (e) {
      notify(String(e));
    }
  }
  async function restore(revision: number) {
    if (!item) return;
    try {
      const old = await api<Item>({ op: "revision", id: item.id, revision });
      setHeading(old.heading);
      setText(old.text);
      setHistoryRevision(revision);
      setDirty(true);
    } catch {
      notify(
        "That intermediate revision was compacted. Try the first or a recent revision.",
      );
    }
  }
  return (
    <Modal
      title={
        image
          ? "A picture, kept close."
          : item
            ? "Pick up the thought."
            : "A fresh little note."
      }
      onClose={close}
      wide
    >
      <div className="editor-meta">
        <span>{saved?.source || item?.source || "My notes"}</span>
        <span>{saved ? time(saved.updated) : "Only on this device"}</span>
      </div>
      <label className="field-label">
        Heading
        <input
          className="heading-input"
          placeholder="Give this thought a name"
          value={heading}
          maxLength={120}
          disabled={image}
          onChange={(e) => {
            setHeading(e.target.value);
            setDirty(true);
          }}
        />
      </label>
      {image ? (
        <div className="full-image">
          <img
            src={`data:image/png;base64,${item!.text}`}
            alt={item!.heading}
          />
        </div>
      ) : (
        <label className="field-label editor-text">
          Your words
          <textarea
            placeholder="Start anywhere…"
            value={text}
            onChange={(e) => {
              setText(e.target.value);
              setDirty(true);
            }}
            spellCheck
          />
        </label>
      )}
      <div className="editor-info">
        <span>
          {image
            ? "Original image"
            : `${text.length.toLocaleString()} characters`}
        </span>
        <span>
          {dirty || busy
            ? "Saving changes…"
            : saved
              ? "Saved on this device"
              : "Save to keep this note"}
        </span>
      </div>
      {saveError && (
        <p className="error-banner" role="alert">
          {saveError} Your changes are still in this window. Try Save again.
        </p>
      )}
      {history && item && (
        <div className="history">
          <label>
            Revision{" "}
            <input
              type="number"
              min={1}
              max={item.revision}
              value={historyRevision}
              onChange={(e) => setHistoryRevision(Number(e.target.value))}
            />
            <span>of {item.revision}</span>
          </label>
          <button
            className="secondary"
            onClick={() => restore(historyRevision)}
          >
            Load revision
          </button>
          <p>
            The first and recent revisions are kept. Loading a revision
            preserves the original.
          </p>
        </div>
      )}
      {confirmDelete ? (
        <div className="inline-confirm">
          <p>Delete this item and its saved revisions?</p>
          <button className="danger" disabled={busy} onClick={remove}>
            Delete permanently
          </button>
          <button className="secondary" onClick={() => setConfirmDelete(false)}>
            Keep it
          </button>
        </div>
      ) : (
        <div className="editor-actions">
          <div>
            {saved && (
              <>
                <button
                  className="icon"
                  title="Pin item"
                  aria-label="Pin item"
                  onClick={pin}
                >
                  <PushPin
                    size={20}
                    weight={saved.pinned ? "fill" : "regular"}
                  />
                </button>
                <button
                  className="icon"
                  aria-label="Delete item"
                  onClick={() => setConfirmDelete(true)}
                >
                  <Trash size={20} />
                </button>
                {!image && (
                  <button
                    className="icon"
                    aria-label="Revision history"
                    onClick={() => setHistory(!history)}
                  >
                    <ArrowCounterClockwise size={20} />
                  </button>
                )}
              </>
            )}
          </div>
          <div>
            {saved && (
              <button
                className="secondary"
                disabled={dirty}
                onClick={() => onCopy(saved)}
              >
                <Copy size={17} />
                Copy
              </button>
            )}
            {!image && (
              <button
                className="primary"
                disabled={busy || (!dirty && !!saved)}
                onClick={() => save()}
              >
                {busy
                  ? "Saving…"
                  : item && item.kind !== "note"
                    ? "Save recovery copy"
                    : "Save note"}
              </button>
            )}
          </div>
        </div>
      )}
      {confirmClose && (
        <div className="inline-confirm">
          <p>
            Some changes are still waiting to save. Earlier autosaves are kept.
          </p>
          <button className="danger" onClick={onClose}>
            Close without waiting
          </button>
          <button className="secondary" onClick={() => setConfirmClose(false)}>
            Keep editing
          </button>
        </div>
      )}
    </Modal>
  );
}

function Settings({
  initial,
  onboarding,
  onClose,
  onSave,
  notify,
}: {
  initial: Preferences;
  onboarding: boolean;
  onClose: () => void;
  onSave: (p: Preferences) => Promise<void>;
  notify: (s: string) => void;
}) {
  const [prefs, setPrefs] = useState({
      ...initial,
      autostart: onboarding ? true : initial.autostart,
    }),
    [apps, setApps] = useState(initial.allowed_apps.join(", ")),
    [busy, setBusy] = useState(false),
    [message, setMessage] = useState("");
  const toggle = (
    key: "paused" | "clipboard" | "autostart" | "browser_capture",
  ) => setPrefs((p) => ({ ...p, [key]: !p[key] }));
  async function save() {
    setBusy(true);
    setMessage("");
    try {
      await onSave({
        ...prefs,
        enabled: true,
        allowed_apps: apps
          .split(",")
          .map((a) => a.trim().toLowerCase())
          .filter(Boolean),
      });
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function action(op: string) {
    setBusy(true);
    try {
      await api({ op });
      notify(
        op === "backup"
          ? "Encrypted backup created"
          : "Archive verification passed",
      );
    } catch (e) {
      setMessage(String(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Modal
      title={onboarding ? "Let’s keep your words." : "Make yourself at home."}
      onClose={onboarding ? () => {} : onClose}
    >
      <p className="modal-intro">
        {onboarding
          ? "Lossy saves supported drafts and clipboard items in an encrypted archive on this Windows account. Choose where it can listen."
          : "Your archive stays on this device. You’re always in control of what gets kept."}
      </p>
      <div className="setting-row">
        <div>
          <strong>Start quietly with Windows</strong>
          <p>No window opens when you sign in.</p>
        </div>
        <input
          aria-label="Start with Windows"
          type="checkbox"
          checked={prefs.autostart}
          onChange={() => toggle("autostart")}
        />
      </div>
      <div className="setting-row">
        <div>
          <strong>Keep clipboard text & images</strong>
          <p>From allowed apps. Lossy’s own copies are skipped.</p>
        </div>
        <input
          aria-label="Keep clipboard"
          type="checkbox"
          checked={prefs.clipboard}
          onChange={() => toggle("clipboard")}
        />
      </div>
      <label className="field-label settings-field">
        Allowed native apps
        <input
          aria-label="Allowed apps"
          value={apps}
          onChange={(e) => setApps(e.target.value)}
          placeholder="notepad.exe, mspaint.exe"
        />
        <small>
          Comma-separated executable names. Password fields and known password
          managers are excluded. Start with Notepad to test capture.
        </small>
      </label>
      <div className="setting-row">
        <div>
          <strong>Browser companion</strong>
          <p>For individual Claude, Codex and WhatsApp Web drafts.</p>
        </div>
        <input
          aria-label="Browser companion"
          type="checkbox"
          checked={prefs.browser_capture}
          onChange={() => toggle("browser_capture")}
        />
      </div>
      <p className="settings-help">
        Browsers and WhatsApp Desktop are excluded from native capture because
        conversations and private windows can’t be identified reliably there.
        Install the included browser companion for web chats. See the companion
        guide in the project.
      </p>
      <button
        className="secondary"
        disabled={busy}
        onClick={() =>
          setupBrowser()
            .then(setMessage)
            .catch((e) => setMessage(String(e)))
        }
      >
        Set up browser companion
      </button>
      <label className="setting-row">
        <div>
          <strong>Keep unpinned items for</strong>
          <p>Pinned items stay until you delete them.</p>
        </div>
        <select
          value={prefs.retention_days}
          onChange={(e) =>
            setPrefs({ ...prefs, retention_days: Number(e.target.value) })
          }
        >
          <option value={7}>7 days</option>
          <option value={30}>30 days</option>
          <option value={90}>90 days</option>
          <option value={365}>1 year</option>
        </select>
      </label>
      {!onboarding && (
        <div className="settings-tools">
          <button
            className="secondary"
            disabled={busy}
            onClick={() => action("backup")}
          >
            <DownloadSimple />
            Back up now
          </button>
          <button
            className="secondary"
            disabled={busy}
            onClick={() => action("verify")}
          >
            <ShieldCheck />
            Verify archive
          </button>
          <button
            className="secondary"
            onClick={() => openFolder().catch((e) => setMessage(String(e)))}
          >
            <FolderOpen />
            Data folder
          </button>
        </div>
      )}
      {message && (
        <p className="error-banner" role="alert">
          {message}
        </p>
      )}
      <div className="settings-bottom">
        <span>
          <ShieldCheck size={17} />
          No cloud or analytics
        </span>
        <button className="primary" onClick={save} disabled={busy}>
          {busy
            ? "Setting things up…"
            : onboarding
              ? "Start keeping my words"
              : "Save preferences"}
        </button>
      </div>
    </Modal>
  );
}
createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);

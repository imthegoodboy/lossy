import { useEffect, useState } from "react";
import { openFolder, type Preferences, type Status } from "./api";

const APP_PATTERN = /^[^\\/:*?"<>|]+\.exe$/i;

/** Trim and lowercase an executable name for consistent app selections. */
function cleanApp(value: string) {
  return value.trim().toLowerCase();
}

/** Render capture health and controls for saving, app selection, and browser setup. */
export function CaptureSetup({
  status,
  busy,
  update,
  setupBrowser,
}: {
  status: Status;
  busy: boolean;
  update: (changes: Partial<Preferences>) => Promise<void>;
  setupBrowser: () => Promise<string>;
}) {
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [newApp, setNewApp] = useState("");
  const [apps, setApps] = useState<string[]>(status.prefs.allowed_apps);
  const [connecting, setConnecting] = useState(false);
  const prefs = status.prefs;
  const recommended = status.recommended_apps ?? [];
  const state = !prefs.enabled
    ? "Saving is off"
    : prefs.paused
      ? "Saving is paused"
      : prefs.clipboard
        ? "Saving on · clipboard on"
        : "Saving on · clipboard off";

  useEffect(() => {
    setApps(prefs.allowed_apps);
  }, [prefs.allowed_apps]);

  /** Persist preference changes, exposing failures in the panel and returning success. */
  async function change(value: Partial<Preferences>) {
    setError("");
    try {
      await update(value);
      return true;
    } catch (e) {
      setError(String(e));
      return false;
    }
  }

  /** Validate and deduplicate app names, restoring the prior selection if saving fails. */
  async function saveApps(next: string[]) {
    const normalized = [...new Set(next.map(cleanApp).filter(Boolean))];
    if (normalized.length > 100) {
      setError("Keep the selected-app list to 100 apps or fewer.");
      return;
    }
    if (normalized.some((app) => !APP_PATTERN.test(app))) {
      setError("Use executable names such as orca.exe.");
      return;
    }
    const previous = apps;
    setApps(normalized);
    if (!(await change({ allowed_apps: normalized }))) setApps(previous);
  }

  /** Validate the entered executable and add it once to the selected apps. */
  function addApp() {
    const value = cleanApp(newApp);
    if (!value) return;
    if (!APP_PATTERN.test(value)) {
      setError("Use an executable name such as orca.exe.");
      return;
    }
    setNewApp("");
    if (!apps.includes(value)) void saveApps([...apps, value]);
  }

  return (
    <details className="capture-setup">
      <summary>
        Capture setup <span>{state}</span>
      </summary>
      <div className="capture-content">
        <p>
          Lossy saves supported editable fields, not every keystroke. Try a
          harmless sentence in Notepad, then return here to check the last save.
        </p>
        <dl className="capture-health">
          <div>
            <dt>Last saved this session</dt>
            <dd>
              {status.last_saved
                ? new Date(status.last_saved).toLocaleTimeString()
                : "No text or image saved yet"}
            </dd>
          </div>
          <div>
            <dt>Last desktop field checked</dt>
            <dd>
              {status.native?.checked_at
                ? `${status.native.app || "Windows"} · ${status.native.state} (${new Date(status.native.checked_at).toLocaleTimeString()})`
                : "Waiting for a desktop field"}
            </dd>
          </div>
          <div>
            <dt>Last clipboard check</dt>
            <dd>
              {status.clipboard_status?.checked_at
                ? `${status.clipboard_status.app || "Windows"} · ${status.clipboard_status.state}`
                : "No clipboard change checked yet"}
            </dd>
          </div>
        </dl>
        <fieldset disabled={busy}>
          <legend className="sr-only">Capture preferences</legend>
          <p className="setup-heading">Saving</p>
          <label>
            <input
              type="checkbox"
              checked={prefs.enabled}
              onChange={(e) => void change({ enabled: e.target.checked })}
            />
            Save supported text locally
          </label>
          <label>
            <input
              type="checkbox"
              checked={prefs.paused}
              onChange={(e) => void change({ paused: e.target.checked })}
            />
            Pause saving everywhere
          </label>
          <label>
            <input
              type="checkbox"
              checked={prefs.autostart}
              onChange={(e) => void change({ autostart: e.target.checked })}
            />
            Start quietly at Windows sign-in
          </label>
          <p className="setup-heading">What to keep</p>
          <label>
            <input
              type="checkbox"
              checked={prefs.clipboard}
              onChange={(e) => void change({ clipboard: e.target.checked })}
            />
            Save copied text and images from any app except password managers
          </label>
          <label>
            <input
              type="checkbox"
              checked={prefs.browser_capture}
              onChange={(e) =>
                void change({ browser_capture: e.target.checked })
              }
            />
            Allow drafts from sites enabled in the browser companion
          </label>
          <p className="setup-heading">Desktop apps</p>
          <label>
            <input
              type="checkbox"
              checked={prefs.all_desktop_apps ?? false}
              onChange={(e) =>
                void change({ all_desktop_apps: e.target.checked })
              }
            />
            Allow all supported desktop apps, including apps installed later
          </label>
          <p className="capture-warning">
            Broader capture may save sensitive text in ordinary fields. Known
            browsers, password managers and terminal hosts stay excluded.
            Unknown apps cannot always be classified; leave this off and use the
            selected-app list for tighter privacy control.
          </p>
          {!prefs.all_desktop_apps && (
            <div className="app-picker">
              <div className="app-chips" aria-label="Selected desktop apps">
                {apps.length === 0 && (
                  <span className="app-empty">No desktop apps selected yet.</span>
                )}
                {apps.map((app) => (
                  <span className="app-chip" key={app}>
                    {app}
                    <button
                      type="button"
                      aria-label={`Remove ${app}`}
                      onClick={() => void saveApps(apps.filter((x) => x !== app))}
                    >
                      ×
                    </button>
                  </span>
                ))}
              </div>
              <form
                className="app-add"
                onSubmit={(e) => {
                  e.preventDefault();
                  addApp();
                }}
              >
                <input
                  aria-label="Add a desktop app"
                  value={newApp}
                  onChange={(e) => setNewApp(e.target.value)}
                  placeholder="orca.exe"
                />
                <button type="submit">Add app</button>
                {recommended.length > 0 && (
                  <button
                    type="button"
                    onClick={() =>
                      void saveApps([...apps, ...recommended])
                    }
                  >
                    Add common apps
                  </button>
                )}
              </form>
            </div>
          )}
        </fieldset>
        <p>
          Chrome / Edge websites, including WhatsApp Web, need the companion
          plus per-site permission. Clipboard copies work separately from draft
          capture and are skipped when a password field is focused or the source
          is a password manager. Known browsers, terminals and WhatsApp Desktop
          stay excluded from passive draft capture.
        </p>
        <div className="setup-actions">
          <button
            disabled={connecting}
            onClick={async () => {
              setConnecting(true);
              setError("");
              try {
                setMessage(await setupBrowser());
              } catch (e) {
                setError(String(e));
              } finally {
                setConnecting(false);
              }
            }}
          >
            Set up browser companion
          </button>
          <button
            type="button"
            onClick={() => {
              setError("");
              void openFolder().catch((e) => setError(String(e)));
            }}
          >
            Open data folder
          </button>
        </div>
        {message && <p role="status">{message}</p>}
        {error && (
          <p className="error" role="alert">
            {error}
          </p>
        )}
      </div>
    </details>
  );
}

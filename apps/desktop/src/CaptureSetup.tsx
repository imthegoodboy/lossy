import { useState } from "react";
import type { Preferences, Status } from "./api";

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
  const [apps, setApps] = useState<string | null>(null);
  const [connecting, setConnecting] = useState(false);
  const prefs = status.prefs;
  const state = !prefs.enabled
    ? "Saving is off"
    : prefs.paused
      ? "Saving is paused"
      : "Saving enabled · coverage varies by field";
  async function change(value: Partial<Preferences>) {
    setError("");
    try {
      await update(value);
    } catch (e) {
      setError(String(e));
    }
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
          <label>
            <input
              type="checkbox"
              checked={prefs.clipboard}
              onChange={(e) => void change({ clipboard: e.target.checked })}
            />
            Save copied text and bitmap images from enabled desktop apps
          </label>
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
            <form
              onSubmit={(e) => {
                e.preventDefault();
                const values = (apps ?? prefs.allowed_apps.join(", "))
                  .split(/[,\n]/)
                  .map((v) => v.trim().toLowerCase())
                  .filter(Boolean);
                if (values.some((v) => !/^[^\\/:*?"<>|]+\.exe$/i.test(v))) {
                  setError(
                    "Use executable names such as orca.exe, separated by commas.",
                  );
                  return;
                }
                void change({ allowed_apps: [...new Set(values)] });
              }}
            >
              <label className="app-list">
                Selected desktop apps
                <input
                  aria-label="Selected desktop apps"
                  value={apps ?? prefs.allowed_apps.join(", ")}
                  onChange={(e) => setApps(e.target.value)}
                  placeholder="notepad.exe, orca.exe, cursor.exe"
                />
              </label>
              <button type="submit">Save app list</button>
            </form>
          )}
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
        </fieldset>
        <p>
          Chrome / Edge websites, including WhatsApp Web, need the companion
          plus per-site permission. Desktop app permission alone does not enable
          a website. Custom editors, terminal prompts and elevated apps may not
          be accessible. Clipboard copies are skipped when Windows cannot verify
          the source app.
        </p>
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

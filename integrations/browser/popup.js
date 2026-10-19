const status = document.querySelector("#status");
const enableButton = document.querySelector("#enable");
const disableButton = document.querySelector("#disable");
let current = null;

/** Resolve the active normal HTTP(S) tab, origin pattern, and content-script ID.
 * @throws {Error} If the active tab is private, missing, or uses an unsupported scheme.
 */
async function site() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab?.url || tab.incognito)
    throw Error("Open a normal website tab first.");
  const url = new URL(tab.url);
  if (!["https:", "http:"].includes(url.protocol))
    throw Error("This page cannot be captured.");
  const origin = url.origin + "/*";
  const id =
    "lossy-" +
    Array.from(new TextEncoder().encode(url.origin))
      .map((n) => n.toString(16))
      .join("");
  return { tab, origin, id };
}

/** Refresh site permission and registration state, disabling controls for invalid tabs. */
async function refresh() {
  try {
    current = await site();
    const [registered, permitted] = await Promise.all([
      chrome.scripting
        .getRegisteredContentScripts({ ids: [current.id] })
        .catch(() => []),
      chrome.permissions.contains({ origins: [current.origin] }),
    ]);
    const enabled = registered.length > 0 && permitted;
    enableButton.disabled = enabled;
    disableButton.disabled = !enabled;
    status.textContent = enabled
      ? "Drafts on this site are saved locally by Lossy."
      : "Not enabled here. Lossy only saves sites you choose.";
  } catch (e) {
    current = null;
    enableButton.disabled = true;
    disableButton.disabled = true;
    status.textContent = e.message;
  }
}

/** Request site access, register and inject the capture script, then refresh the controls. */
enableButton.onclick = async () => {
  try {
    const { tab, origin, id } = current || (await site());
    if (!(await chrome.permissions.request({ origins: [origin] }))) return;
    await chrome.scripting.unregisterContentScripts({ ids: [id] }).catch(() => {});
    await chrome.scripting.registerContentScripts([
      {
        id,
        matches: [origin],
        js: ["content.js"],
        runAt: "document_idle",
        allFrames: false,
      },
    ]);
    await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      files: ["content.js"],
    });
    await refresh();
    status.textContent = "Enabled. Your next edits will be saved locally.";
  } catch (e) {
    status.textContent = e.message;
  }
};

/** Remove future script registration and site access, then refresh the controls. */
disableButton.onclick = async () => {
  try {
    const { origin, id } = current || (await site());
    await chrome.scripting.unregisterContentScripts({ ids: [id] }).catch(() => {});
    await chrome.permissions.remove({ origins: [origin] });
    await refresh();
    status.textContent =
      "Disabled. Reload this tab to stop the existing listener.";
  } catch (e) {
    status.textContent = e.message;
  }
};

void refresh();

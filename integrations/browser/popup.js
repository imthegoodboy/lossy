const status = document.querySelector("#status");
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
document.querySelector("#enable").onclick = async () => {
  try {
    const { tab, origin, id } = await site();
    if (!(await chrome.permissions.request({ origins: [origin] }))) return;
    await chrome.scripting
      .unregisterContentScripts({ ids: [id] })
      .catch(() => {});
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
    status.textContent = "Enabled. Your next edits will be saved locally.";
  } catch (e) {
    status.textContent = e.message;
  }
};
document.querySelector("#disable").onclick = async () => {
  try {
    const { origin, id } = await site();
    await chrome.scripting
      .unregisterContentScripts({ ids: [id] })
      .catch(() => {});
    await chrome.permissions.remove({ origins: [origin] });
    status.textContent =
      "Disabled. Reload this tab to stop the existing listener.";
  } catch (e) {
    status.textContent = e.message;
  }
};

let native;
const pending = new Map();
let requestId = 0;
let profilePromise;
let ordered = Promise.resolve();
function connect() {
  if (native) return native;
  native = chrome.runtime.connectNative("app.lossy.companion");
  native.onDisconnect.addListener(() => {
    native = null;
    for (const reply of pending.values())
      reply({ error: "Open Lossy and set up the browser companion first." });
    pending.clear();
  });
  native.onMessage.addListener((message) => {
    const entry = pending.entries().next().value;
    if (entry) {
      pending.delete(entry[0]);
      entry[1](message);
    }
  });
  return native;
}
chrome.runtime.onMessage.addListener((message, sender, reply) => {
  if (
    message.op !== "browser_capture" ||
    !sender.tab ||
    sender.tab.incognito ||
    sender.frameId !== 0
  ) {
    reply({ error: "Capture unavailable" });
    return;
  }
  if (pending.size > 100) {
    reply({ error: "Lossy is busy" });
    return;
  }
  ordered = ordered
    .then(async () => {
      const origin = new URL(sender.url).origin + "/*";
      if (!(await chrome.permissions.contains({ origins: [origin] }))) {
        reply({ error: "Site capture disabled" });
        return;
      }
      profilePromise ||= chrome.storage.local
        .get(["profile"])
        .then(async (data) => {
          if (data.profile) return data.profile;
          const profile = crypto.randomUUID();
          await chrome.storage.local.set({ profile });
          return profile;
        });
      const profile = await profilePromise;
      const id = ++requestId;
      pending.set(id, reply);
      try {
        connect().postMessage({
          ...message,
          context: `${profile}|${sender.tab.id}|${message.context}`,
          private: false,
        });
      } catch {
        pending.delete(id);
        reply({ error: "Lossy is unavailable" });
      }
    })
    .catch(() => reply({ error: "Capture unavailable" }));
  return true;
});

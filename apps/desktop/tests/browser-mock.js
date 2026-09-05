// Synthetic browser-only visual harness. Never imported by production.
(() => {
  const seed = [
    [
      "A small idea for tomorrow",
      "A gentle reminder: build things that make the day feel a little lighter.\n\nStart with the unfinished thought.",
      "Claude · Synthetic conversation A",
      "draft",
    ],
    [
      "The weekend plan",
      "Saturday, a slow morning. Coffee at ten, a walk by the lake, and absolutely no rushing.",
      "WhatsApp · Synthetic conversation B",
      "draft",
    ],
    [
      "Little things worth keeping",
      "A book recommendation. A line from a conversation. A thought that arrived at the wrong time.",
      "My notes",
      "note",
    ],
    [
      "The recipe I meant to save",
      "Flour, a little patience, and the good olive oil. Rest for thirty minutes before baking.",
      "Clipboard · Synthetic Notepad",
      "clipboard",
    ],
  ].map(([heading, text, source, kind], i) => ({
    id: String(i + 1).padStart(32, "0"),
    revision: 1,
    heading,
    text,
    source,
    kind,
    pinned: i === 2,
    updated: Date.now() - i * 180000,
  }));
  let items =
    JSON.parse(sessionStorage.getItem("lossy-smoke-items") || "null") || seed;
  let prefs = {
    enabled: true,
    paused: false,
    clipboard: true,
    autostart: false,
    retention_days: 30,
    allowed_apps: ["notepad.exe", "mspaint.exe"],
    browser_capture: true,
  };
  let stamp = 1;
  const persist = () => {
    sessionStorage.setItem("lossy-smoke-items", JSON.stringify(items));
    stamp++;
  };
  window.__TAURI_INTERNALS__ = {
    invoke: async (command, { payload: r } = {}) => {
      if (command !== "request") return "Synthetic companion setup complete";
      const item = items.find((i) => i.id === r.id);
      switch (r.op) {
        case "status":
          return {
            prefs,
            last_saved: stamp,
            error: null,
            data_dir: "Synthetic test folder",
          };
        case "list": {
          const all = items.filter(
            (i) =>
              (!r.filter ||
                r.filter === "all" ||
                r.filter === i.kind ||
                (r.filter === "pinned" && i.pinned)) &&
              (!r.search ||
                `${i.heading} ${i.text}`
                  .toLowerCase()
                  .includes(r.search.toLowerCase())),
          );
          return {
            items: all.slice(r.offset || 0, (r.offset || 0) + 60),
            more: false,
          };
        }
        case "get":
        case "revision":
          if (!item) throw "Item not found";
          return { ...item };
        case "save": {
          if (item && r.revision !== item.revision) throw "Version conflict";
          const next = {
            id: item?.id || crypto.randomUUID().replaceAll("-", ""),
            revision: (item?.revision || 0) + 1,
            heading: r.heading,
            text: r.text,
            kind: "note",
            source: "My notes",
            updated: Date.now(),
            pinned: item?.pinned || false,
          };
          items = items.filter((i) => i.id !== next.id);
          items.unshift(next);
          persist();
          return { ...next };
        }
        case "delete":
          items = items.filter((i) => i.id !== r.id);
          persist();
          return true;
        case "pin":
          item.pinned = r.pinned;
          persist();
          return true;
        case "color":
          item.color = r.color;
          persist();
          return {...item};
        case "settings":
          prefs = r.prefs;
          return true;
        case "copy":
          window.__lastSyntheticCopy = item?.text;
          return true;
        case "backup":
        case "verify":
          return true;
        default:
          throw "Unexpected test operation";
      }
    },
  };
})();

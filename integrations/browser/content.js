(() => {
  if (window.__lossyCompanion) return;
  window.__lossyCompanion = true;
  const ids = new WeakMap();
  const sent = new WeakMap();
  const documentId = crypto.randomUUID();
  let next = 0,
    unknownChat = crypto.randomUUID(),
    lastHeader = "";
  const blocked =
    /password|passcode|one.?time|otp|credit.?card|card.?number|cvv|cvc|secret|api.?key|token|security.?code/i;
  function editor(target) {
    return target instanceof Element
      ? target.closest(
          'textarea,input,[contenteditable="true"],[role="textbox"]',
        )
      : null;
  }
  function safe(el) {
    return (
      el &&
      !el.disabled &&
      !el.readOnly &&
      el.getAttribute("aria-readonly") !== "true" &&
      !(
        el instanceof HTMLInputElement &&
        !["text", "search", "url", "email", ""].includes(el.type)
      ) &&
      !blocked.test(
        [
          el.id,
          el.getAttribute("name"),
          el.getAttribute("autocomplete"),
          el.getAttribute("aria-label"),
          el.getAttribute("placeholder"),
        ].join(" "),
      )
    );
  }
  function context(el) {
    if (!ids.has(el)) ids.set(el, ++next);
    let entity = location.pathname + location.search + location.hash;
    if (location.hostname === "web.whatsapp.com") {
      const panel = document.querySelector("#main");
      const title =
        panel?.querySelector("header [title]")?.getAttribute("title") ||
        "WhatsApp conversation";
      if (title !== lastHeader) {
        lastHeader = title;
        unknownChat = crypto.randomUUID();
      }
      const stable = document
        .querySelector('[aria-selected="true"][data-id]')
        ?.getAttribute("data-id");
      entity = stable || unknownChat;
    }
    return `${location.origin}|${documentId}|${entity}|editor-${ids.get(el)}`;
  }
  function save(target) {
    const el = editor(target);
    if (!safe(el) || !document.hasFocus() || document.hidden) return;
    const text = "value" in el ? el.value : el.innerText;
    if (typeof text !== "string" || text.length > 200000) return;
    const key = context(el);
    const previous = sent.get(el);
    if (previous?.key === key && previous.text === text) return;
    const snapshot = { key, text };
    sent.set(el, snapshot);
    const retry = () => {
      if (sent.get(el) === snapshot) sent.delete(el);
    };
    chrome.runtime
      .sendMessage({
        op: "browser_capture",
        context: key,
        text,
        source: `${location.hostname} · ${document.title.slice(0, 160)}`,
        secure: false,
      })
      .then((reply) => {
        if (!reply?.ok) retry();
      })
      .catch(retry);
  }
  document.addEventListener("input", (e) => save(e.target), true);
  document.addEventListener("compositionend", (e) => save(e.target), true);
  document.addEventListener("focusin", (e) => save(e.target), true);
  // Observe programmatic clearing after Send as well as keyboard-generated input.
  setInterval(() => save(document.activeElement), 200);
  // WhatsApp does not expose a stable public conversation API. Without a selected internal ID,
  // isolate each chat selection; preserving an extra card is safer than merging two contacts.
  document.addEventListener(
    "pointerdown",
    (e) => {
      if (
        location.hostname === "web.whatsapp.com" &&
        e.target instanceof Element &&
        e.target.closest("#pane-side")
      )
        unknownChat = crypto.randomUUID();
    },
    true,
  );
})();

// Keyside overlay for OBS browser sources. Shows text the companion fills in and streams
// to the page. The companion styles the page from the address (?size=96&color=ffcc00…);
// this script only handles ?hide=1 and passes a timer ?format on.
(() => {
  const q = new URLSearchParams(location.search);
  const out = document.getElementById("out");
  const timer = location.pathname.match(/^\/overlay\/timer\/([a-z0-9_-]{1,24})\/?$/);
  const format = q.get("format") ? `&format=${encodeURIComponent(q.get("format"))}` : "";
  const src = timer ? `/overlay/events?timer=${timer[1]}${format}` : `/overlay/events?t=${encodeURIComponent(q.get("t") ?? "")}`;
  const hide = q.get("hide") === "1";

  const events = new EventSource(src);
  events.onmessage = (e) => {
    let d;
    try {
      d = JSON.parse(e.data);
    } catch {
      return;
    }
    out.textContent = typeof d.text === "string" ? d.text : "";
    const hidden = hide && (timer ? !d.running : !out.textContent.trim());
    out.style.visibility = hidden ? "hidden" : "visible";
  };
})();

// The sample deck on the landing page. Pretends to be OBS.

const icons = {
  monitor: '<rect x="2" y="3" width="20" height="14" rx="2"/><path d="M8 21h8M12 17v4"/>',
  mic: '<path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2M12 19v3"/>',
  "mic-off":
    '<path d="m2 2 20 20M18.89 13.23A7.12 7.12 0 0 0 19 12v-2M5 10v2a7 7 0 0 0 12 5M15 9.34V5a3 3 0 0 0-5.68-1.33M9 9v3a3 3 0 0 0 5.12 2.12M12 19v3"/>',
  volume: '<path d="M11 5 6 9H2v6h4l5 4V5Z"/><path d="M15.5 8.5a5 5 0 0 1 0 7M19 5a10 10 0 0 1 0 14"/>',
  "volume-off": '<path d="M11 5 6 9H2v6h4l5 4V5Z"/><path d="m22 9-6 6M16 9l6 6"/>',
  music: '<path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>',
  "music-off": '<path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/><path d="m2 2 20 20"/>',
  rec: '<circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="4" fill="currentColor"/>',
  live: '<path d="M4.9 19.1C1 15.2 1 8.8 4.9 4.9M7.8 16.2a6 6 0 0 1 0-8.5M16.2 7.8a6 6 0 0 1 0 8.5M19.1 4.9C23 8.8 23 15.1 19.1 19"/><circle cx="12" cy="12" r="2"/>',
  replay: '<path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8"/><path d="M3 3v5h5M12 7v5l4 2"/>',
};

function setIcon(key, name) {
  key.querySelector(".ic").innerHTML =
    `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${icons[name]}</svg>`;
}

function part(key, cls) {
  let el = key.querySelector("." + cls);
  if (!el) {
    el = document.createElement("span");
    el.className = cls;
    key.querySelector(".ic").after(el);
  }
  return el;
}

const clock = (ms) => {
  const s = Math.floor(ms / 1000);
  const pad = (n) => String(n).padStart(2, "0");
  return `${pad(Math.floor(s / 3600))}:${pad(Math.floor(s / 60) % 60)}:${pad(s % 60)}`;
};

const deck = document.getElementById("deck");
const keys = [...deck.querySelectorAll(".key")];
for (const key of keys) setIcon(key, key.querySelector(".ic").className.match(/ic-([\w-]+)/)[1]);

// Scenes: one is on air at a time.
const scenes = keys.filter((k) => k.dataset.scene);
for (const key of scenes) {
  key.addEventListener("click", () => {
    for (const k of scenes) {
      const on = k === key;
      k.classList.toggle("on", on);
      k.setAttribute("aria-pressed", String(on));
      part(k, "badge").textContent = on ? "ON AIR" : "";
    }
  });
}

// Inputs: mute toggles.
const muteIcons = { mic: "mic", desk: "volume", music: "music" };
const muted = { mic: false, desk: false, music: true };
function showMute(key) {
  const id = key.dataset.mute;
  key.classList.toggle("off", muted[id]);
  key.setAttribute("aria-pressed", String(muted[id]));
  part(key, "badge").textContent = muted[id] ? "MUTED" : "";
  setIcon(key, muted[id] ? muteIcons[id] + "-off" : muteIcons[id]);
}
for (const key of keys.filter((k) => k.dataset.mute)) {
  showMute(key);
  key.addEventListener("click", () => {
    muted[key.dataset.mute] = !muted[key.dataset.mute];
    showMute(key);
  });
}

// Record and stream: toggles with a running clock, also shown as status pills.
const outputs = {
  rec: { since: Date.now() - 754_000, idle: "Record", busy: "Stop recording" },
  live: { since: null, idle: "Go live", busy: "End stream" },
};
function showOutput(key) {
  const o = outputs[key.dataset.output];
  const on = o.since !== null;
  key.classList.toggle("off", on);
  key.setAttribute("aria-pressed", String(on));
  key.querySelector(".label").textContent = on ? o.busy : o.idle;
  document.querySelector(`[data-pill="${key.dataset.output}"]`).hidden = !on;
  tick();
}
for (const key of keys.filter((k) => k.dataset.output)) {
  showOutput(key);
  key.addEventListener("click", () => {
    const o = outputs[key.dataset.output];
    o.since = o.since === null ? Date.now() : null;
    showOutput(key);
  });
}
function tick() {
  for (const [id, o] of Object.entries(outputs)) {
    const key = deck.querySelector(`[data-output="${id}"]`);
    const text = o.since === null ? "Off" : clock(Date.now() - o.since);
    key.querySelector(".sub").textContent = text;
    key.querySelector(".sub").classList.toggle("mono", o.since !== null);
    document.querySelector(`[data-clock="${id}"]`).textContent = text;
  }
}
setInterval(tick, 1000);

// Replay buffer: confirms the save for a moment.
const replay = deck.querySelector("[data-replay]");
let saved;
replay.addEventListener("click", () => {
  clearTimeout(saved);
  replay.classList.add("ok");
  part(replay, "badge").textContent = "SAVED";
  replay.querySelector(".sub").textContent = "Last 30 seconds";
  saved = setTimeout(() => {
    replay.classList.remove("ok");
    part(replay, "badge").textContent = "";
    replay.querySelector(".sub").textContent = "Buffer on";
  }, 1800);
});

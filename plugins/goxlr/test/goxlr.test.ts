import { createTestHost } from "@keyside/sdk/testing";
import { test } from "node:test";
import assert from "node:assert/strict";
import plugin, { applyPatch } from "../src/main.ts";
import { fakeServer, eventually } from "../../_lib/fake.ts";

const SERIAL = "S210500000AAA";

function mixer(mini = false) {
  const fader = (channel: string) => ({ channel, mute_type: "All", scribble: null, mute_state: "Unmuted" });
  return {
    hardware: { device_type: mini ? "Mini" : "Full", serial_number: SERIAL },
    fader_status: { A: fader("Mic"), B: fader("Music"), C: fader("Chat"), D: fader("System") },
    levels: { volumes: { Mic: 255, Music: 128, Chat: 200, System: 255, Game: 255, Console: 0, LineIn: 0, Sample: 255, Headphones: 200, MicMonitor: 0, LineOut: 255 } },
    router: { Music: { BroadcastMix: true, Headphones: true } },
    cough_button: { is_toggle: true, mute_type: "All", state: "Unmuted" },
    effects: mini
      ? null
      : {
          is_enabled: false,
          active_preset: "Preset1",
          preset_names: { Preset1: "Clean", Preset2: "Robot voice" },
          current: { megaphone: { is_enabled: false }, robot: { is_enabled: false }, hard_tune: { is_enabled: false } },
        },
    sampler: mini ? null : { active_bank: "A", banks: { A: { TopLeft: { is_playing: false } } } },
    profile_name: "Streaming",
    mic_profile_name: "SM7B",
  };
}

/** Stands in for the GoXLR Utility: answers commands and sends patches like the real one. */
async function fakeUtility(opts: { mini?: boolean } = {}) {
  const status: any = { config: {}, mixers: { [SERIAL]: mixer(opts.mini) }, files: { profiles: ["Streaming", "Chill"], mic_profiles: ["SM7B"] } };
  const m = status.mixers[SERIAL];
  const base = `/mixers/${SERIAL}`;
  const commands: any[] = [];
  const fake = await fakeServer((ws, req) => {
    const reply = (data: unknown) => ws.send(JSON.stringify({ id: req.id, data }));
    // The real Utility sends u64::MAX, which JavaScript can't hold exactly.
    const patch = (ops: unknown[]) => ws.send(`{"id":18446744073709551615,"data":{"Patch":${JSON.stringify(ops)}}}`);
    if (req.data === "GetStatus") return reply({ Status: status });
    const [serial, cmd] = req.data?.Command ?? [];
    if (serial !== SERIAL) return reply({ Error: "Device not found" });
    commands.push(cmd);
    const [name] = Object.keys(cmd);
    const v = cmd[name];
    reply("Ok");
    switch (name) {
      case "SetFaderMuteState":
        return patch([{ op: "replace", path: `${base}/fader_status/${v[0]}/mute_state`, value: v[1] }]);
      case "SetCoughMuteState":
        return patch([{ op: "replace", path: `${base}/cough_button/state`, value: v }]);
      case "SetVolume":
        return patch([{ op: "replace", path: `${base}/levels/volumes/${v[0]}`, value: v[1] }]);
      case "SetRobotEnabled":
        return patch([{ op: "replace", path: `${base}/effects/current/robot/is_enabled`, value: v }]);
      case "SetRouter":
        return patch([{ op: "replace", path: `${base}/router/${v[0]}/${v[1]}`, value: v[2] }]);
      case "LoadProfile":
        return patch([{ op: "replace", path: `${base}/profile_name`, value: v[0] }]);
      case "PlayNextSample":
        // A patch the plugin can't follow, so it has to fetch the status again.
        m.sampler.banks.A.TopLeft.is_playing = true;
        return patch([{ op: "move", from: "/x", path: `${base}/sampler` }]);
    }
  });
  return Object.assign(fake, { commands });
}

test("follows the GoXLR and sends commands", async () => {
  const u = await fakeUtility();
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: u.port } });
  await h.start();
  await h.sync([
    { id: "f", action: "fader-mute", settings: { fader: "B" } },
    { id: "v", action: "volume", settings: { channel: "Music" } },
    { id: "r", action: "effect", settings: { effect: "robot", mode: "hold" } },
    { id: "p", action: "preset", settings: { preset: "Preset1" } },
    { id: "o", action: "route", settings: { input: "Music", output: "BroadcastMix" } },
    { id: "l", action: "profile", settings: { profile: "Chill" } },
  ]);
  await eventually(() => h.status()?.state === "connected");
  assert.equal(h.status()?.detail, "GoXLR");
  await eventually(() => h.view("f")?.meta === "Music");
  assert.equal(h.variable("profile"), "Streaming");
  assert.equal(h.variable("mic_profile"), "SM7B");
  assert.equal(h.view("v")?.meta, "50%");
  assert.equal(h.view("p")?.badge, "ACTIVE");
  assert.equal(h.view("p")?.meta, "Clean");
  assert.equal(h.view("o")?.badge, "ON");

  await h.press("f", "fader-mute", { fader: "B" });
  await eventually(() => h.view("f")?.badge === "MUTED");
  assert.deepEqual(u.commands.at(-1), { SetFaderMuteState: ["B", "MutedToX"] });
  await h.press("f", "fader-mute", { fader: "B", to: "all" });
  assert.deepEqual(u.commands.at(-1), { SetFaderMuteState: ["B", "Unmuted"] });

  await h.slide("v", "volume", 1, { channel: "Music" });
  await eventually(() => h.view("v")?.meta === "100%");

  await h.press("r", "effect", { effect: "robot", mode: "hold" });
  await eventually(() => h.view("r")?.badge === "ON");
  await h.release("r", "effect", { effect: "robot", mode: "hold" });
  await eventually(() => h.view("r") === null);

  await h.press("o", "route", { input: "Music", output: "BroadcastMix" });
  assert.deepEqual(u.commands.at(-1), { SetRouter: ["Music", "BroadcastMix", false] });

  assert.deepEqual(await h.options("profile", "profile"), [
    { value: "Chill", label: "Chill" },
    { value: "Streaming", label: "Streaming" },
  ]);
  assert.equal((await h.options("preset", "preset"))[1].label, "2: Robot voice");
  await h.press("l", "profile", { profile: "Chill" });
  await eventually(() => h.view("l")?.badge === "LOADED");
  assert.equal(h.variable("profile"), "Chill");
  await h.stop();
  await u.close();
});

test("refetches after a patch it can't apply", async () => {
  const u = await fakeUtility();
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: u.port } });
  await h.start();
  await h.sync([{ id: "s", action: "sample", settings: { bank: "A", button: "TopLeft" } }]);
  await eventually(() => h.status()?.state === "connected");
  await h.press("s", "sample", { bank: "A", button: "TopLeft" });
  assert.deepEqual(u.commands.at(-1), { PlayNextSample: ["A", "TopLeft"] });
  await eventually(() => h.view("s")?.badge === "PLAYING");
  await h.press("s", "sample", { bank: "A", button: "TopLeft" });
  assert.deepEqual(u.commands.at(-1), { StopSamplePlayback: ["A", "TopLeft"] });
  await h.stop();
  await u.close();
});

test("refuses settings it doesn't know and effects on a Mini", async () => {
  const u = await fakeUtility({ mini: true });
  const h = createTestHost(plugin, { connection: { host: "127.0.0.1", port: u.port } });
  await h.start();
  await h.sync([{ id: "e", action: "effect", settings: { effect: "fx" } }]);
  await eventually(() => h.status()?.detail === "GoXLR Mini");
  await eventually(() => h.view("e")?.disabled === true);
  await assert.rejects(h.press("e", "effect", { effect: "fx" }), /full-size/);
  await h.slide("v", "volume", 0.5, { channel: "__proto__" });
  await assert.rejects(h.press("f", "fader-mute", { fader: "E" }), /Unknown setting/);
  await assert.rejects(h.press("l", "profile", { profile: "../../etc/passwd" }), /no profile/);
  assert.equal(u.commands.length, 0);
  await h.stop();
  await u.close();
});

test("patches can't reach outside the status", () => {
  const doc: any = { a: { b: 1 }, list: [1, 2] };
  assert.equal(applyPatch(doc, [{ op: "replace", path: "/a/b", value: 2 }]), true);
  assert.equal(applyPatch(doc, [{ op: "add", path: "/list/-", value: 3 }]), true);
  assert.equal(applyPatch(doc, [{ op: "remove", path: "/list/0" }]), true);
  assert.deepEqual(doc, { a: { b: 2 }, list: [2, 3] });
  assert.equal(applyPatch(doc, [{ op: "add", path: "/__proto__/polluted", value: 1 }]), false);
  assert.equal(applyPatch(doc, [{ op: "add", path: "/a/constructor/x", value: 1 }]), false);
  assert.equal(applyPatch(doc, [{ op: "replace", path: "/missing/x", value: 1 }]), false);
  assert.equal(({} as any).polluted, undefined);
});

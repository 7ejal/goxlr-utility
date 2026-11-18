import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { runInNewContext } from "node:vm";

test("microphone bandwidth UI stages changes and hides unsupported controls", async () => {
  class Element extends EventTarget {
    children = [];
    get valueAsNumber() { return Number(this.value); }
    append(...children) { this.children.push(...children); }
  }
  const panel = new Element();
  const status = new Element();
  const sent = [];
  let socket;
  class TestSocket extends EventTarget {
    constructor(url) {
      super();
      assert.equal(url.href, "ws://localhost:14564/api/websocket");
      socket = this;
    }
    send(message) { sent.push(JSON.parse(message)); }
  }
  const source = await readFile(
    new URL("../daemon/web-content/mic-bandwidth/app.mjs", import.meta.url), "utf8",
  );
  runInNewContext(source, {
    URL, URLSearchParams, WebSocket: TestSocket,
    location: new URL("http://localhost:14564/mic-bandwidth/index.html?device=test-device"),
    document: {
      createElement: () => new Element(),
      getElementById: (id) => id === "bands" ? panel : status,
    },
  });
  const receive = (id, data) => {
    socket.dispatchEvent(new MessageEvent("message", { data: JSON.stringify({ id, data }) }));
  };
  const bandNames = ["31Hz", "63Hz", "125Hz", "250Hz", "500Hz", "1KHz", "2KHz", "4KHz", "8KHz", "16KHz"];
  const raw = Object.fromEntries(bandNames.map((band) => ["Equalizer" + band, 16]));
  const mixer = {
    mic_profile_name: "test-profile",
    mic_status: {
      eq_bandwidth: { raw, live_supported: false },
      equaliser: { frequency: { Equalizer1KHz: 1000 }, gain: { Equalizer1KHz: 6 } },
    },
  };
  const response = { Status: { mixers: { "test-device": mixer } } };
  socket.dispatchEvent(new Event("open"));
  assert.deepEqual(sent.pop(), { id: 1, data: "GetStatus" });
  receive(1, response);
  assert.equal(panel.children.length, 10);
  assert.match(status.textContent, /Profile only/);

  const [, output, input] = panel.children[5].children;
  assert.deepEqual([input.min, input.max, input.step, output.value], [0, 32, 1, 16]);
  assert.match(input.title, /Modeled Q 1.41/);
  input.value = 32;
  input.dispatchEvent(new Event("change"));
  assert.deepEqual(sent.pop(), {
    id: 2, data: { Command: ["test-device", {
      SetMicEqBandwidth: ["Equalizer1KHz", 32, "test-profile"],
    }] },
  });
  assert.equal(input.disabled, true);
  receive(2, "Ok");
  assert.deepEqual(sent.pop(), { id: 1, data: "GetStatus" });
  raw.Equalizer1KHz = 32;
  receive(1, response);
  assert.equal(input.title, "");

  delete mixer.mic_status.eq_bandwidth;
  receive(1, response);
  assert.equal(panel.hidden, true);
  assert.match(status.textContent, /unavailable/);
});

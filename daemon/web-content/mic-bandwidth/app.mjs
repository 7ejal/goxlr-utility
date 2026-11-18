const bands = new Map([
  ["Equalizer31Hz", "31 Hz"], ["Equalizer63Hz", "63 Hz"],
  ["Equalizer125Hz", "125 Hz"], ["Equalizer250Hz", "250 Hz"],
  ["Equalizer500Hz", "500 Hz"], ["Equalizer1KHz", "1 kHz"],
  ["Equalizer2KHz", "2 kHz"], ["Equalizer4KHz", "4 kHz"],
  ["Equalizer8KHz", "8 kHz"], ["Equalizer16KHz", "16 kHz"],
]);
const modeledQAt1kHz = new Map([[0, 14.0845], [8, 4.4248], [16, 1.4144], [24, 0.3675]]);
const device = new URLSearchParams(location.search).get("device");
const status = document.getElementById("status");
const panel = document.getElementById("bands");
const url = new URL("/api/websocket", location.href);
url.protocol = location.protocol === "https:" ? "wss:" : "ws:";
const socket = new WebSocket(url);
const controls = new Map();
let current;
let pending;
let error;

for (const [band, name] of bands) {
  const label = document.createElement("label");
  const output = document.createElement("output");
  const input = document.createElement("input");
  Object.assign(input, { type: "range", min: 0, max: 32, step: 1, disabled: true });
  input.addEventListener("input", () => { output.value = input.value; });
  input.addEventListener("change", () => {
    if (!current || pending) return;
    pending = true;
    error = undefined;
    for (const control of controls.values()) control.input.disabled = true;
    socket.send(JSON.stringify({
      id: 2, data: { Command: [device, {
        SetMicEqBandwidth: [band, input.valueAsNumber, current.mic_profile_name],
      }] },
    }));
  });
  label.append(name, output, input);
  panel.append(label);
  controls.set(band, { input, output });
}

function refresh() {
  socket.send(JSON.stringify({ id: 1, data: "GetStatus" }));
}

socket.addEventListener("open", refresh);
socket.addEventListener("message", (event) => {
  const { id, data } = JSON.parse(event.data);
  if (id === 2) {
    error = data.Error;
    pending = false;
    refresh();
  } else if (!pending && data.Status) {
    current = data.Status.mixers[device];
    const bandwidth = current?.mic_status.eq_bandwidth;
    panel.hidden = !bandwidth;
    status.textContent = error ?? (bandwidth
      ? bandwidth.live_supported
        ? "Raw 0–32 · Live device control · Save Mic Profile to keep changes"
        : "Raw 0–32 · Profile only · Save Mic Profile to keep changes"
      : "Microphone EQ bandwidth is unavailable on this device or daemon.");
    if (bandwidth) {
      for (const [band, { input, output }] of controls) {
        input.value = bandwidth.raw[band];
        output.value = input.value;
        const eq = current.mic_status.equaliser;
        const q = band === "Equalizer1KHz" && eq.frequency[band] === 1000 && eq.gain[band] === 6
          ? modeledQAt1kHz.get(input.valueAsNumber) : undefined;
        input.title = q === undefined ? "" : "Modeled Q " + q.toFixed(2) + " at +6 dB and 48 kHz";
        input.disabled = false;
      }
    }
  } else if (!pending && data.Patch?.some(({ path }) =>
    path === "/mixers" || (path.startsWith("/mixers/") &&
      /mic_profile_name|eq_bandwidth|equaliser\/|^\/mixers\/[^/]+$/.test(path)))) {
    refresh();
  }
});
socket.addEventListener("close", () => {
  panel.hidden = true;
  status.textContent = "Disconnected from GoXLR Utility. Reopen the Microphone tab to reconnect.";
});

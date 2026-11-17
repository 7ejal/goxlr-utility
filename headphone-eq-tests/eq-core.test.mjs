import test from "node:test";
import assert from "node:assert/strict";
import {FS,FREQS,flat,compile,apo,bank,preset,validate,quantize,coeff,db,HARDWARE_HEADPHONE_EQ_SUPPORTED} from "../daemon/web-content/headphone-eq/eq-core.mjs";

test("hardware headphone-only EQ remains blocked",()=>{
  assert.equal(HARDWARE_HEADPHONE_EQ_SUPPORTED,false);
  assert.equal(compile(flat()).isFlashableFirmware,false);
});
test("20 stereo bands and independent channels",()=>{
  const p=flat(); assert.equal(FREQS.length,20);
  p.left[11].gain=6;p.right[11].gain=0;
  const m=compile(p,2048);
  assert.notDeepEqual(m.q[0][11],m.q[1][11]);
  assert.ok(Math.abs(db(m.q[0][11].map(v=>v/2**21),1000)-6)<.08);
  assert.ok(m.preamp< -5.5);
  assert.ok(m.estimatedAfterPreamp<.1);
});
test("GXEQ DATA ONLY bank has correct 988 byte shape and CRC",()=>{
  const out=bank(flat()),dv=new DataView(out.buffer);
  assert.equal(out.length,988);
  assert.equal(new TextDecoder().decode(out.subarray(0,4)),"GXEQ");
  assert.equal(dv.getUint32(4,false),1);
  assert.equal(dv.getUint32(8,false),FS);
  assert.equal(dv.getUint16(12,false),2);
  assert.equal(dv.getUint16(14,false),20);
  let crc=0xffffffff;
  for(let i=0;i<984;i++){crc^=out[i];for(let j=0;j<8;j++)crc=(crc>>>1)^(crc&1?0xedb88320:0);}
  assert.equal(dv.getUint32(984,false),(crc^0xffffffff)>>>0);
});
test("APO text identifies Windows endpoints and stream caveat",()=>{
  const p=flat();p.endpoints=["Game","Chat"];p.left[11].gain=4;
  const s=apo(p,compile(p));
  assert.match(s,/Device: Game TC-HELICON GoXLR/);
  assert.match(s,/Device: Chat TC-HELICON GoXLR/);
  assert.doesNotMatch(s,/Device: Music/);
  assert.match(s,/BroadcastMix/);
  assert.match(s,/Channel: L/);assert.match(s,/Channel: R/);
  assert.match(s,/Filter 12: ON PK Fc 1000\.00 Hz Gain 4\.00 dB Q 1\.000/);
});
test("invalid profile values are rejected",()=>{
  for(const mod of [
    p=>p.left.pop(),p=>p.left[0].gain=100,p=>p.left[0].q=0,
    p=>p.right[0].freq=24000,p=>p.left[0].on="true",
    p=>p.name="x".repeat(1000),p=>p.endpoints=["Game","../../a"],
    p=>p.preamp=NaN,p=>p.enabled=1
  ]){
    const p=flat();mod(p);assert.throws(()=>validate(p));
  }
});
test("flat, warm and bypass behave consistently",()=>{
  assert.equal(preset("Warm").left[0].gain,2.5);
  const p=flat();p.enabled=false;p.left[0].gain=12;
  const m=compile(p),out=bank(p,m),dv=new DataView(out.buffer);
  assert.equal(m.preamp,0);assert.equal(dv.getUint32(16,false),0);
  assert.equal(dv.getUint32(24,false),0);
});
test("Q3.21 filters stable across normal frequency/gain/Q grid",()=>{
  for(const f of [31,250,1000,8000,16000])
    for(const gain of [-12,-6,0,6,12])
      for(const q of [.4,1,4]){
        const c=quantize(coeff({freq:f,gain,q}));
        assert.ok(Math.abs(db(c.map(v=>v/2**21),f)-gain)<.15);
      }
});
test("Research compiler fixture matches the original verified Python GXEQ",async()=>{
  const {createHash}=await import("node:crypto");
  const p=flat();p.left[11].gain=6;p.left[11].q=.8;p.right[15].gain=-3;
  const bytes=bank(p,compile(p,2048));
  assert.equal(createHash("sha256").update(bytes).digest("hex"),
    "9c8910c1fe50a58258acaf3c6ab5375320c09a62064bb3fbf43d114dbfb898ee");
});
test("App is locally served and never sends device mutation commands",async()=>{
  const {readFile}=await import("node:fs/promises");
  const app=await readFile(new URL("../daemon/web-content/headphone-eq/app.mjs",import.meta.url),"utf-8");
  const main=await readFile(new URL("../daemon/web-content/index.html",import.meta.url),"utf-8");
  const page=await readFile(new URL("../daemon/web-content/headphone-eq/index.html",import.meta.url),"utf-8");
  assert.match(main,/href="\/headphone-eq\/index.html"/);
  assert.match(page,/app\.mjs/);
  assert.match(app,/JSON\.stringify\("GetStatus"\)/);
  assert.doesNotMatch(app,/\/firmware-upload|SetRouting|SetEqGain|RunFirmwareUpdate/);
});

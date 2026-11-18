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
  assert.match(main,/index-BJM830Pw-headphone-eq\.js/);
  assert.match(page,/app\.mjs/);
  assert.match(app,/JSON\.stringify\("GetStatus"\)/);
  assert.match(app,/data-field="freq"/);
  assert.match(app,/value<20\|\|value>20000/);
  assert.doesNotMatch(app,/\/firmware-upload|SetRouting|SetEqGain|RunFirmwareUpdate/);
});

test("Headphones EQ is an appended native tab and original bundled UI is unchanged outside insertion",async()=>{
  const {readFile}=await import("node:fs/promises");
  const base=new URL("../daemon/web-content/assets/",import.meta.url);
  const original=await readFile(new URL("index-BJM830Pw.js",base),"utf-8");
  const patched=await readFile(new URL("index-BJM830Pw-headphone-eq.js",base),"utf-8");
  const marker=',v(d,{id:"headphone-eq",name:"Headphones EQ"}';
  const at=patched.indexOf(marker);
  assert.ok(at>0);
  assert.equal(patched.indexOf(marker,at+1),-1);
  const end=patched.indexOf(')]),_:1},8,["onOnChange","label"])',at);
  // The complete injected tab is exactly the generated render expression.
  const insert=',v(d,{id:"headphone-eq",name:"Headphones EQ"},{default:z(()=>[k("iframe",{src:"/headphone-eq/index.html?embedded=1",title:"Headphones EQ",style:"display:block;width:100%;height: min(1100px, 85vh);min-height:480px;border:0;background:#0c0d11;border-radius:8px;"})]),_:1})';
  const bandwidth = ",n.isDeviceMini()?Le(\"\",!0):k(\"iframe\",{src:\"/mic-bandwidth/index.html?device=\"+encodeURIComponent(c.getActiveSerial()),title:\"Microphone EQ bandwidth\",style:\"display:block;width:100%;height:350px;border:0;\"})";
  assert.equal(patched.split(bandwidth).length, 2);
  assert.ok(patched.replace(bandwidth, "").replace(insert, "") === original.replaceAll("\r\n", "\n"));
  assert.ok(at>patched.indexOf('v(d,{id:"system",name:e.$t("message.navigation.system")}'));
  const render = new Function("v","d","z","k","return ["+insert.slice(1)+"]");
  const tab = render((type,props,slots)=>({type,props,slots}),"Tab",fn=>fn,(tag,props)=>({tag,props}))[0];
  assert.equal(tab.props.id,"headphone-eq");
  assert.equal(tab.slots.default()[0].tag,"iframe");
  assert.equal(tab.slots.default()[0].props.src,"/headphone-eq/index.html?embedded=1");
});

test("11-band stock layout exports two 180-byte banks with exact identity padding",async()=>{
  const {flat11,native11Data}=await import("../daemon/web-content/headphone-eq/eq-core.mjs");
  const p=flat11(),m=native11Data(p);
  assert.equal(m.flashable,false);assert.equal(m.hardware_apply,false);
  assert.equal(m.calls_per_channel,4);
  assert.equal(m.channels[0].u24be_hex.length,360);
  assert.equal(m.channels[0].u24be_hex,m.channels[1].u24be_hex);
  assert.match(m.channels[0].u24be_hex,/000000000000080000000000000000$/);
  assert.throws(()=>native11Data(flat()));
});
test("11-band stereo stock-layout model has bounded error and rejects invalid mode",async()=>{
  const {flat11,native11Data}=await import("../daemon/web-content/headphone-eq/eq-core.mjs");
  const p=flat11();
  [ [3,-2,2,-3,1.5,4,-2,1,-1,2,-1],[-2,1,3,-1,2,-3,1.5,-2,2,-1,1] ].forEach((g,i)=>{
    g.forEach((v,j)=>p[i?"right":"left"][j].gain=v);
  });
  const m=native11Data(p);
  assert.notEqual(m.channels[0].u24be_hex,m.channels[1].u24be_hex);
  const {createHash}=await import("node:crypto");
  assert.deepEqual(m.channels.map(x=>createHash("sha256").update(Buffer.from(x.u24be_hex,"hex")).digest("hex")),
   ["c83d8a91eab8d0880ea59233c1a17221b748c22e3549550e83a4b50769c642f5","1bc6bceeaf66792f7cef1bf4ab25d24195282a69b8228f67cae384eaea17bff5"]);
  assert.ok(m.channels.every(x=>x.max_section_model_error_db<=.1));
  assert.ok(m.preamp_db<0);
  p.right[11].on=true;assert.throws(()=>native11Data(p));
  p.right[11].on=false;p.autoHeadroom=false;assert.throws(()=>native11Data(p));
});

test("E11L packet is transport-specific and both channel banks are complete",async()=>{
  const {flat11,native11Data,labUpdatePacket}=await import("../daemon/web-content/headphone-eq/eq-core.mjs");
  const p=flat11();p.left[4].gain=3;p.right[8].gain=-2;
  const m=native11Data(p),b=labUpdatePacket(p),v=new DataView(b.buffer);
  assert.equal(b.length,376);
  assert.equal(new TextDecoder().decode(b.slice(0,4)),"E11L");
  assert.equal(v.getUint32(8,false),m.preamp_q23);
  assert.equal(Buffer.from(b.slice(12,192)).toString("hex"),m.channels[0].u24be_hex);
  assert.equal(Buffer.from(b.slice(192,372)).toString("hex"),m.channels[1].u24be_hex);
  let crc=0xffffffff;
  for(const x of b.slice(0,372)){crc^=x;for(let j=0;j<8;j++)crc=(crc>>>1)^(crc&1?0xedb88320:0);}
  assert.equal(v.getUint32(372,false),(crc^0xffffffff)>>>0);
  assert.throws(()=>labUpdatePacket(flat()));
});

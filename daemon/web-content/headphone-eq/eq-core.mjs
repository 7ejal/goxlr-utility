/*
 * GoXLR Utility fork — offline stereo headphone EQ lab, NEVER device firmware.
 * Based on the independently verified 20-band stereo GXEQ data-only research.
 * No USB or hardware DSP writes. Equalizer APO export affects Windows endpoints
 * and MAY also affect BroadcastMix; not an isolated physical headphone mix.
 */
export const FS = 48000;
export const FREQS = Object.freeze([25,35,50,71,100,141,200,282,399,564,798,1000,1594,2254,3187,4505,6370,9005,12732,18000]);
export const ENDPOINTS = Object.freeze(["Game","Music","Chat","System","Sample"]);
export const HARDWARE_HEADPHONE_EQ_SUPPORTED = false;
const Q21 = 2**21, Q23=2**23;
const finite = (v,lo,hi) => typeof v==="number" && Number.isFinite(v) && v>=lo && v<=hi;
export function flat() {
  const make=()=>FREQS.map(freq=>({freq,gain:0,q:1,on:true}));
  return {schema:"goxlr-eq-preview-v1",name:"Flat",enabled:true,
    preamp:-1,autoHeadroom:true,left:make(),right:make(),endpoints:[...ENDPOINTS]};
}
export function validate(p) {
  if(!p||typeof p!=="object"||Array.isArray(p)||p.schema!=="goxlr-eq-preview-v1")throw Error("Unsupported profile");
  if(typeof p.name!=="string"||p.name.length>80)throw Error("Invalid name");
  if(typeof p.enabled!=="boolean"||typeof p.autoHeadroom!=="boolean")throw Error("Invalid switches");
  if(!finite(p.preamp,-30,6))throw Error("Preamp outside -30 to 6 dB");
  for(const ch of ["left","right"]) {
    if(!Array.isArray(p[ch])||p[ch].length!==20)throw Error("20 bands per channel required");
    for(const b of p[ch]) {
      if(!b||typeof b!=="object"||!finite(b.freq,20,20000)
         ||!finite(b.gain,-18,18)||!finite(b.q,.2,12)||typeof b.on!=="boolean")
        throw Error("Invalid frequency, gain, Q or enable");
    }
  }
  if(p.native11!==undefined&&typeof p.native11!=="boolean")throw Error("Invalid native11 mode");
  if(p.native11&&["left","right"].some(ch=>p[ch].slice(11).some(b=>b.on)))throw Error("Extra native11 bands must remain disabled");
  if(!Array.isArray(p.endpoints)||new Set(p.endpoints).size!==p.endpoints.length
     ||p.endpoints.some(x=>!ENDPOINTS.includes(x)))throw Error("Invalid GoXLR Windows endpoints");
  return p;
}
export function coeff(b) {
  const A=10**(b.gain/40),w=2*Math.PI*b.freq/FS, alpha=Math.sin(w)/(2*b.q),c=Math.cos(w),den=1+alpha/A;
  return [(1+alpha*A)/den,-2*c/den,(1-alpha*A)/den,2*c/den,-(1-alpha/A)/den];
}
export function quantize(c) {
  const q=c.map(x=>{
    const n=Math.round(x*Q21);
    if(!Number.isSafeInteger(n)||n<-(2**23)||n>=2**23)throw Error("Q3.21 coefficient overflow");
    return n;
  });
  const a1=-q[3]/Q21,a2=-q[4]/Q21;
  if(!(Math.abs(a2)<1&&1+a1+a2>0&&1-a1+a2>0))throw Error("Quantized coefficient unstable");
  return q;
}
export function db(c,f) {
  const w=2*Math.PI*f/FS,r1=Math.cos(w),i1=-Math.sin(w),r2=Math.cos(2*w),i2=-Math.sin(2*w);
  const nr=c[0]+c[1]*r1+c[2]*r2,ni=c[1]*i1+c[2]*i2;
  const dr=1-c[3]*r1-c[4]*r2,di=-c[3]*i1-c[4]*i2;
  return 10*Math.log10(Math.max((nr*nr+ni*ni)/(dr*dr+di*di),1e-40));
}
export function compile(p,samples=512) {
  validate(p);
  if(!Number.isInteger(samples)||samples<64||samples>4096)throw Error("Invalid sampling");
  const q=[p.left,p.right].map(ch=>ch.map(b=>quantize(b.on?coeff(b):[1,0,0,0,0])));
  const channels=[p.left,p.right],response=[[],[]];
  let peak=-Infinity;
  for(let i=0;i<samples;i++) {
    const hz=20*1000**(i/(samples-1));
    for(let ch=0;ch<2;ch++) {
      const v=p.enabled?channels[ch].reduce((sum,b,j)=>sum+(b.on?db(q[ch][j].map(n=>n/Q21),hz):0),0):0;
      response[ch].push({hz,db:v});
      peak=Math.max(peak,v);
    }
  }
  const required=Math.min(0,-peak-.5);
  const preamp=p.enabled?(p.autoHeadroom?Math.min(p.preamp,required):p.preamp):0;
  if(preamp< -30)throw Error("Too much gain; required headroom below -30 dB");
  const preampQ23=Math.min(Q23,Math.max(0,Math.round(10**(preamp/20)*Q23)));
  return {q,response,peak,preamp,required,preampQ23,
    estimatedAfterPreamp:peak+preamp,isFlashableFirmware:false};
}
export function apo(p,m=compile(p,2048)) {
  validate(p);
  const o=[
    "# GoXLR Utility fork — software Windows endpoint EQ export, NOT hardware headphone-only EQ.",
    "# WARNING: EQ on Game/Music/Chat/System/Sample may also alter BroadcastMix/stream.",
    "# Audio from LineIn, Console, mic and exclusive WASAPI/ASIO may bypass this EQ.",
    "# Equalizer APO must be installed and attached to selected endpoints manually."
  ];
  for(const endpoint of p.endpoints) {
    o.push("","Device: "+endpoint+" TC-HELICON GoXLR");
    for(const [key,channel] of [["left","L"],["right","R"]]) {
      o.push("Channel: "+channel,"Preamp: "+m.preamp.toFixed(2)+" dB");
      if(!p.enabled){o.push("# EQ bypassed");continue;}
      p[key].forEach((b,i)=>{
        if(b.on)o.push("Filter "+(i+1)+": ON PK Fc "+b.freq.toFixed(2)+" Hz Gain "
          +b.gain.toFixed(2)+" dB Q "+b.q.toFixed(3));
      });
    }
  }
  return o.join("\n")+"\n";
}
export function bank(p,m=compile(p,2048)){
  validate(p);
  const bytes=new Uint8Array(988),dv=new DataView(bytes.buffer);
  const put32=(off,v)=>dv.setUint32(off,v>>>0,false);
  bytes.set([71,88,69,81]);put32(4,1);put32(8,FS);
  dv.setUint16(12,2,false);dv.setUint16(14,20,false);
  put32(16,p.enabled?1:0);put32(20,m.preampQ23);
  let idx=24;
  for(let ch=0;ch<2;ch++)for(let i=0;i<20;i++){
    const b=ch===0?p.left[i]:p.right[i];
    put32(idx,p.enabled&&b.on?1:0);idx+=4;
    for(const v of m.q[ch][i]){dv.setInt32(idx,v,false);idx+=4;}
  }
  if(idx!==984)throw Error("Bad data-bank layout");
  let crc=0xffffffff;
  for(let i=0;i<984;i++){
    crc^=bytes[i];
    for(let j=0;j<8;j++)crc=(crc>>>1)^(crc&1?0xedb88320:0);
  }
  put32(984,(crc^0xffffffff)>>>0);
  return bytes;
}
export function preset(name){
  const funcs={
    Flat:f=>0,Warm:f=>f<150?2.5:f>5000?-1:0,
    "Voice clarity":f=>f>=1000&&f<=4505?2.2:f<120?-1.5:0,
    "Gentle bass":f=>f<141?3:0,
    "Soft treble":f=>f>4505?-2.5:f<100?1:0
  };
  if(!(name in funcs))throw Error("Unknown preset");
  const p=flat();p.name=name;
  for(let i=0;i<20;i++)p.left[i].gain=p.right[i].gain=funcs[name](p.left[i].freq);
  return p;
}

// Native research mode: reuse the stock three-biquad DSP routine P:000D99.
// The host format is data only. No device command or firmware is generated.
export const FREQS11=Object.freeze([31,63,125,250,500,1000,2000,4000,8000,12000,16000]);
export function flat11(){
  const p=flat(); p.name="11-band native research"; p.native11=true;
  for(const ch of ["left","right"]){
    p[ch]=p[ch].map((b,i)=>i<11?{freq:FREQS11[i],gain:0,q:1,on:true}:{...b,gain:0,on:false});
  }
  return p;
}
function stableStock(c){return Math.abs(c[4])<1 && 1-c[3]-c[4]>0 && 1+c[3]-c[4]>0;}
const STOCK_SCALE=[.5,1,.25];
const STOCK_GRID=[20,25,31,35,45,50,63,75,100,125,160,200,250,315,400,500,630,800,1000,1250,1600,2000,2500,3150,4000,5000,6300,8000,10000,12500,16000,18000,20000];
function stockNorm(w,i){
  const s=STOCK_SCALE[i%3],v=w.map(n=>n/Q23);
  return [v[2]*4/s,v[1]*4/s,v[0]*2/s,-2*v[4],-v[3]];
}
function packStock(q,i){
  const s=STOCK_SCALE[i%3];
  const clipRound=v=>{
    const n=Math.round(v*Q23);if(n< -Q23||n>=Q23)throw Error("Stock Q23 coefficient out of bounds");return n;
  };
  if(q[0]===Q21&&q[1]===-q[3]&&q[2]===-q[4])return [0,0,clipRound(s/4),0,0];
  const [b0,b1,b2,f1,f2]=q.map(n=>n/Q21);
  let w=[s*b2/2,s*b1/4,s*b0/4,-f2,-f1/2].map(clipRound);
  const target=q.map(n=>n/Q21),expected=STOCK_GRID.map(f=>db(target,f));
  const score=t=>{
    const c=stockNorm(t,i);
    if(!stableStock(c))return Infinity;
    return Math.max(...STOCK_GRID.map((f,j)=>Math.abs(db(c,f)-expected[j])));
  };
  let best=score(w);
  for(let pass=0;pass<8;pass++){
    let progress=false;
    for(let j=0;j<5;j++){
      let winner=w[j],cost=best;
      for(const delta of [-8,-6,-4,-3,-2,-1,1,2,3,4,6,8]){
        const v=w[j]+delta;if(v< -Q23||v>=Q23)continue;
        const t=w.slice();t[j]=v;const k=score(t);
        if(k+1e-9<cost){winner=v;cost=k;}
      }
      if(winner!==w[j]){w[j]=winner;best=cost;progress=true;}
    }
    if(!progress)break;
  }
  return w;
}
export function native11Data(p){
  validate(p);
  if(!p.native11||["left","right"].some(ch=>p[ch].slice(11).some(b=>b.on)))
    throw Error("Create an 11-band research profile first; all extra bands must be disabled.");
  if(p.enabled&&!p.autoHeadroom)throw Error("Enable automatic headroom before exporting native research data.");
  const channels=[],dense=Array.from({length:2048},(_,i)=>20*1000**(i/2047));
  let peak=-Infinity;
  for(const ch of ["left","right"]){
    let words=[],maxError=0,norm=[];
    for(let i=0;i<12;i++){
      const band=p[ch][i];
      const q=(i<11 && p.enabled && band.on && band.gain!==0)?quantize(coeff(band)):[Q21,0,0,0,0];
      const w=packStock(q,i),target=q.map(n=>n/Q21),recovered=stockNorm(w,i);
      if(!stableStock(recovered))throw Error("Unstable packed filter");
      const error=Math.max(...dense.map(f=>Math.abs(db(recovered,f)-db(target,f))));
      if(!Number.isFinite(error)||error>.1)throw Error("Stock-layout response error >0.1 dB; adjust gain, frequency or Q");
      maxError=Math.max(maxError,error);words.push(...w);norm.push(recovered);
    }
    peak=Math.max(peak,...dense.map(f=>norm.reduce((s,c)=>s+db(c,f),0)));
    const hex=words.map(n=>(n&0xffffff).toString(16).padStart(6,"0")).join("");
    if(hex.length!==360)throw Error("Incorrect 180-byte channel data size");
    channels.push({channel:ch,coefficient_bytes:180,u24be_hex:hex,max_section_model_error_db:maxError});
  }
  const preamp=p.enabled?Math.min(0,p.preamp,-peak-.5):0;
  if(preamp< -30)throw Error("Required preamp below -30 dB");
  return {schema:"GXEQ11_OFFLINE_STOCK_LAYOUT_V1",flashable:false,hardware_apply:false,
    stock_kernel:"P:000D99",lab_wrapper:"P:005FF2",active_slots_per_channel:11,padding_slots_per_channel:1,
    calls_per_channel:4,sample_rate:FS,enabled:p.enabled,estimated_peak_db:peak,
    preamp_db:preamp,preamp_q23:Math.min(Q23,Math.round(10**(preamp/20)*Q23)),
    preamp_not_in_coefficients:true,channels,
    warning:"EMULATOR DATA ONLY. Not firmware. No physical headphone path, transport or device memory allocation established."};
}

// E11L is an explicit host-laboratory packet, never a GoXLR USB opcode.
export function labUpdatePacket(p){
  const m=native11Data(p),bytes=new Uint8Array(376),v=new DataView(bytes.buffer);
  bytes.set([69,49,49,76]);bytes[4]=1;bytes[5]=m.enabled?1:0;
  v.setUint32(8,m.preamp_q23,false);
  for(const [ch,bank] of m.channels.entries()){
    for(let i=0;i<180;i++)bytes[12+ch*180+i]=Number.parseInt(bank.u24be_hex.slice(2*i,2*i+2),16);
  }
  let crc=0xffffffff;
  for(let i=0;i<372;i++){
    crc^=bytes[i];
    for(let j=0;j<8;j++)crc=(crc>>>1)^(crc&1?0xedb88320:0);
  }
  v.setUint32(372,(crc^0xffffffff)>>>0,false);
  return bytes;
}

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

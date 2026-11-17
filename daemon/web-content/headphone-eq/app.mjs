import {flat,validate,compile,apo,bank,preset,ENDPOINTS} from "./eq-core.mjs";
const $=id=>document.getElementById(id);
const STORAGE="goxlr-headphone-eq-lab-v1";
let profile=flat(),side="linked",current=null,saveTimeout=0;
try{const data=JSON.parse(localStorage.getItem(STORAGE));if(data)profile=validate(data);}catch{localStorage.removeItem(STORAGE);}
const error=message=>{const el=$("error");el.textContent=message;el.hidden=false;setTimeout(()=>el.hidden=true,6500);};
const note=message=>$("localMessage").textContent=message;
const save=()=>{
  try{validate(profile);localStorage.setItem(STORAGE,JSON.stringify(profile));note("Saved locally in this browser.");}
  catch(ex){error(ex.message);}
};
function download(name,bytes,mime){
  const blob=new Blob([bytes],{type:mime}),url=URL.createObjectURL(blob),a=document.createElement("a");
  a.href=url;a.download=name;document.body.appendChild(a);a.click();a.remove();setTimeout(()=>URL.revokeObjectURL(url),1000);
}
function paintGraph(){
  const svg=$("response");
  const W=780,H=245,left=46,top=19,w=714,h=196;
  const x=f=>left+Math.log10(f/20)/3*w;
  const y=db=>top+(18-Math.max(-18,Math.min(18,db)))/36*h;
  let out="";
  for(const dbValue of [-18,-12,-6,0,6,12,18]){
    const yy=y(dbValue);const primary=dbValue===0;
    out+='<line x1="'+left+'" y1="'+yy+'" x2="'+(left+w)+'" y2="'+yy+'" stroke="'+(primary?"#64616e":"#34343e")+'" stroke-width="'+(primary?"1.3":".7")+'"/>';
    out+='<text x="7" y="'+(yy+4)+'" fill="#8b8b99" font-size="11">'+(dbValue>0?"+":"")+dbValue+'</text>';
  }
  for(const f of [20,50,100,200,500,1000,2000,5000,10000,20000]){
    const xx=x(f);out+='<line x1="'+xx+'" y1="'+top+'" x2="'+xx+'" y2="'+(top+h)+'" stroke="#2b2b34" stroke-width=".6"/>';
  }
  if(current){
    for(let ch=0;ch<2;ch++){
      const path=current.response[ch].map((point,i)=>(i===0?"M":"L")+x(point.hz).toFixed(2)+","+y(point.db+current.preamp).toFixed(2)).join(" ");
      out+='<path fill="none" stroke="'+(ch===0?"#ff5265":"#ffc066")+'" stroke-width="2.5" stroke-linecap="round" d="'+path+'"/>';
    }
  }
  svg.innerHTML=out;
}
function dataSide(){return side==="right"?"right":"left";}
function drawTable(){
  const node=$("bands");
  const bands=profile[dataSide()];
  node.replaceChildren();
  bands.forEach((band,i)=>{
    const row=document.createElement("div");
    row.className="bandrow";row.dataset.row=i;
    row.innerHTML='<div class="freqrow"><span class="bandnum">'+(i+1)+'</span>'
      +'<input aria-label="Band '+(i+1)+' frequency in hertz" data-field="freq" type="number" min="20" max="20000" step="1" value="'+band.freq+'"><span>Hz</span></div>'
      +'<div class="gainrow"><input aria-label="Band '+(i+1)+' gain" data-field="gain" type="range" min="-18" max="18" step=".5" value="'+band.gain+'">'
      +'<span class="value">'+(band.gain>0?"+":"")+band.gain.toFixed(1)+'</span></div>'
      +'<input aria-label="Band '+(i+1)+' Q" data-field="q" type="number" min=".2" max="12" step=".1" value="'+band.q+'">'
      +'<input aria-label="Enable band '+(i+1)+'" data-field="on" type="checkbox" '+(band.on?"checked":"")+'>';
    node.appendChild(row);
  });
}
function drawEndpoints(){
  const box=$("endpoints");box.replaceChildren();
  ENDPOINTS.forEach(endpoint=>{
    const label=document.createElement("label"),input=document.createElement("input");
    input.type="checkbox";input.checked=profile.endpoints.includes(endpoint);input.value=endpoint;
    input.addEventListener("change",()=>{
      profile.endpoints=ENDPOINTS.filter(x=>x===endpoint?input.checked:profile.endpoints.includes(x));save();
    });
    label.append(input,document.createTextNode(endpoint));box.appendChild(label);
  });
}
function refresh(){
  $("profileName").value=profile.name;$("enabled").checked=profile.enabled;
  $("autoHeadroom").checked=profile.autoHeadroom;$("preamp").value=profile.preamp;
  document.querySelectorAll("[data-side]").forEach(el=>el.classList.toggle("active",el.dataset.side===side));
  try{
    current=compile(profile,512);
    $("effective").textContent="Applied preamp "+current.preamp.toFixed(2)+" dB";
    $("peak").textContent="Peak boost "+current.peak.toFixed(2)+" dB";
  }catch(ex){current=null;error(ex.message);}
  drawTable();paintGraph();drawEndpoints();
}
$("bands").addEventListener("input",ev=>{
  const field=ev.target.dataset.field;
  if(field!=="gain")return;
  const row=ev.target.closest("[data-row]");if(!row)return;
  const i=Number(row.dataset.row),value=Number(ev.target.value);
  const sides=side==="linked"?["left","right"]:[side];
  for(const ch of sides)profile[ch][i].gain=value;
  row.querySelector(".value").textContent=(value>0?"+":"")+value.toFixed(1);
  try{current=compile(profile,512);paintGraph();
    $("effective").textContent="Applied preamp "+current.preamp.toFixed(2)+" dB";
    $("peak").textContent="Peak boost "+current.peak.toFixed(2)+" dB";
  }catch(ex){error(ex.message);}
  clearTimeout(saveTimeout);saveTimeout=setTimeout(save,250);
});
$("bands").addEventListener("change",ev=>{
  const field=ev.target.dataset.field,row=ev.target.closest("[data-row]");
  if(!row || !["on","q","gain","freq"].includes(field))return;
  const i=Number(row.dataset.row);
  let value=field==="on"?ev.target.checked:Number(ev.target.value);
  if(field==="q"&&(!Number.isFinite(value)||value<.2||value>12)){error("Q must be between 0.2 and 12.");refresh();return;}
  if(field==="freq"&&(!Number.isFinite(value)||value<20||value>20000)){error("Frequency must be between 20 and 20,000 Hz.");refresh();return;}
  for(const ch of side==="linked"?["left","right"]:[side])profile[ch][i][field]=value;
  save();refresh();
});
document.querySelectorAll("[data-side]").forEach(el=>el.addEventListener("click",()=>{side=el.dataset.side;refresh();}));
$("preset").addEventListener("change",ev=>{profile=preset(ev.target.value);save();refresh();});
$("enabled").addEventListener("change",ev=>{profile.enabled=ev.target.checked;save();refresh();});
$("autoHeadroom").addEventListener("change",ev=>{profile.autoHeadroom=ev.target.checked;save();refresh();});
$("preamp").addEventListener("change",ev=>{
  const v=Number(ev.target.value);
  if(!Number.isFinite(v)||v< -30||v>6){error("Preamp must be -30 to +6 dB.");refresh();return;}
  profile.preamp=v;save();refresh();
});
$("profileName").addEventListener("change",ev=>{profile.name=ev.target.value.slice(0,80);save();refresh();});
$("reset").addEventListener("click",()=>{profile=flat();side="linked";save();refresh();});
$("exportJson").addEventListener("click",()=>{
  try{validate(profile);download("GoXLR-headphone-EQ-profile.json",JSON.stringify(profile,null,2),"application/json");}
  catch(ex){error(ex.message);}
});
$("importJson").addEventListener("change",async ev=>{
  const file=ev.target.files[0];if(!file)return;
  if(file.size>128000){error("Profile file too large");ev.target.value="";return;}
  try{profile=validate(JSON.parse(await file.text()));save();refresh();note("Imported "+profile.name+" locally.");}
  catch(ex){error("Invalid EQ profile: "+ex.message);}
  ev.target.value="";
});
$("exportApo").addEventListener("click",()=>{
  try{const m=compile(profile,2048);download("GoXLR-Windows-endpoints-EQ.txt",apo(profile,m),"text/plain");}
  catch(ex){error(ex.message);}
});
$("copyApo").addEventListener("click",async ()=>{
  try{await navigator.clipboard.writeText(apo(profile,compile(profile,2048)));note("APO configuration copied. Device installation is manual.");}
  catch(ex){error("Copy failed: "+ex.message);}
});
$("exportData").addEventListener("click",()=>{
  try{download("DATA_ONLY_NOT_FLASHABLE.gxeq",bank(profile,compile(profile,2048)),"application/octet-stream");}
  catch(ex){error(ex.message);}
});
$("checkDevice").addEventListener("click",async()=>{
  $("deviceStatus").textContent="Checking…";
  try{
    const res=await fetch("/api/command",{method:"POST",headers:{"Content-Type":"application/json"},body:JSON.stringify("GetStatus")});
    if(!res.ok)throw Error("HTTP "+res.status);
    const reply=await res.json(),mixers=reply?.Status?.mixers || {};
    const devices=Object.values(mixers);
    if(!devices.length)throw Error("No mixer connected");
    const hardware=devices[0]?.hardware || {};
    const version=hardware.versions?.firmware;
    const ver=Array.isArray(version)?version.join("."):"unknown";
    $("deviceStatus").textContent="GoXLR "+(hardware.device_type || "")+" · firmware "+ver;
    $("deviceDot").classList.add("ok");
  }catch(ex){
    $("deviceDot").classList.remove("ok");
    $("deviceStatus").textContent="No readable GoXLR status";
    error(ex.message);
  }
});
refresh();

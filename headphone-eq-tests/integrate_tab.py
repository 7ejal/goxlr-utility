#!/usr/bin/env python3
"""Reproducible, pinned patch to the exact existing upstream Vue bundle.
Add fork controls to the device tabs. No replacement of upstream
device logic, and preserve existing tab indices/hotkeys. Refuse other bundles.
"""
import hashlib
from pathlib import Path
ROOT = Path(__file__).resolve().parents[1]
WEB = ROOT / "daemon/web-content"
ORIGINAL = WEB / "assets/index-BJM830Pw.js"
PATCHED = WEB / "assets/index-BJM830Pw-headphone-eq.js"
SHA = "14d9db29c6f05bdbe02663a3c00cf0c20ea6ac83e89b91900cc5136cb30fd954"
needle = 'v(d,{id:"system",name:e.$t("message.navigation.system")},{default:z(()=>[v(_,null,{default:z(()=>[v(w)]),_:1})]),_:1},8,["name"])'
addition = ',v(d,{id:"headphone-eq",name:"Headphones EQ"},{default:z(()=>[k("iframe",{src:"/headphone-eq/index.html?embedded=1",title:"Headphones EQ",style:"display:block;width:100%;height: min(1100px, 85vh);min-height:480px;border:0;background:#0c0d11;border-radius:8px;"})]),_:1})'
raw = ORIGINAL.read_bytes().replace(b'\r\n', b'\n')
assert hashlib.sha256(raw).hexdigest() == SHA, "Upstream bundle changed; review the integration against its source"
s = raw.decode()
assert s.count(needle)==1
mic = 'v(d,{id:"mic",name:e.$t("message.navigation.microphone")},{default:z(()=>[v(u)]),_:1},8,["name"])'
bandwidth = ',n.isDeviceMini()?Le("",!0):k("iframe",{src:"/mic-bandwidth/index.html?device="+encodeURIComponent(c.getActiveSerial()),title:"Microphone EQ bandwidth",style:"display:block;width:100%;height:350px;border:0;"})'
assert s.count(mic) == 1
patched = s.replace(needle, needle + addition).replace(mic, mic.replace('[v(u)]', '[v(u)' + bandwidth + ']'))
assert patched.replace(bandwidth, '').replace(addition, '') == s

PATCHED.write_bytes(patched.encode())
p = WEB / "index.html"
html = p.read_text()
import re
html = re.sub(r'\s*<!-- Fork-only navigation link; the upstream UI bundle is not modified\. -->.*?</a>', '',html,flags=re.S)
html = html.replace('/assets/index-BJM830Pw.js','/assets/index-BJM830Pw-headphone-eq.js')
assert html.count('/assets/index-BJM830Pw-headphone-eq.js') == 1
p.write_text(html)
print("PASS pinned upstream bundle with native tab and microphone controls")

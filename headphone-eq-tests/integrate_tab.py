#!/usr/bin/env python3
"""Reproducible, pinned patch to the exact existing upstream Vue bundle.
Append a standard Tab to the device tab list. No replacement of upstream
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
raw = ORIGINAL.read_bytes()
assert hashlib.sha256(raw).hexdigest() == SHA, "Upstream bundle changed; review the integration against its source"
s = raw.decode()
assert s.count(needle)==1
patched = s.replace(needle,needle+addition)
assert patched.replace(needle+addition,needle)==s
PATCHED.write_bytes(patched.encode())
p = WEB / "index.html"
html = p.read_text()
import re
html = re.sub(r'\s*<!-- Fork-only navigation link; the upstream UI bundle is not modified\. -->.*?</a>', '',html,flags=re.S)
html = html.replace('/assets/index-BJM830Pw.js','/assets/index-BJM830Pw-headphone-eq.js')
assert html.count('/assets/index-BJM830Pw-headphone-eq.js') == 1
p.write_text(html)
print("PASS pinned upstream bundle + single native tab insertion; original retained")

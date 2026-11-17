#!/usr/bin/env python3
"""Build fully self-contained EQ Lab single-file preview for offline browsers.
The generated page is an editor/exporter only; it cannot modify hardware DSP.
"""
from pathlib import Path
import re
ROOT=Path(__file__).resolve().parents[1]
source=ROOT/'daemon/web-content/headphone-eq'
html=(source/'index.html').read_text()
css=(source/'style.css').read_text()
core=(source/'eq-core.mjs').read_text()
app=(source/'app.mjs').read_text()
assert 'export const FS' in core and 'export function compile' in core
assert 'import {flat,validate,compile,apo,bank,preset,ENDPOINTS} from "./eq-core.mjs";' in app
core=core.replace('export const ','const ').replace('export function ','function ')
app=app.replace('import {flat,validate,compile,apo,bank,preset,ENDPOINTS} from "./eq-core.mjs";','')
# File:// URLs can restrict Storage. Still allow editing/export without persistence.
app=app.replace('catch{localStorage.removeItem(STORAGE);}',
                'catch{try{localStorage.removeItem(STORAGE);}catch{}}')
assert not re.search(r'^export ',core,re.M) and not re.search(r'^import ',app,re.M)
assert '</script>' not in core and '</script>' not in app
before='<link rel="stylesheet" href="/headphone-eq/style.css">'
assert html.count(before)==1
html=html.replace(before,'<style>\n'+css+'\n</style>')
after='<script type="module" src="/headphone-eq/app.mjs"></script>'
assert html.count(after)==1
html=html.replace(after,'<script>\n'+core+'\n'+app+'\n</script>')
html=html.replace('href="/"','href="https://github.com/7ejal/goxlr-utility"')
warning='<p style="padding:8px 14px;color:#ffbfbe;background:#321b20;border-radius:8px;font-size:12px">Standalone offline preview. EQ editor/export only; hardware EQ and live device control are disabled.</p>'
assert '</header>' in html
html=html.replace('</header>','</header>\n'+warning,1)
out=ROOT/'standalone/GoXLR-EQ-Lab-OFFLINE-PREVIEW.html'
out.parent.mkdir(parents=True,exist_ok=True)
out.write_text(html)
js=ROOT/'standalone/.preview-inline-script.tmp.js'
js.write_text(core+'\n'+app+'\n')
print('WROTE',out,'size',out.stat().st_size,'bytes')
print('JS syntax check file',js)

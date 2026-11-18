#!/usr/bin/env python3
"""Offline decoder of a single GoXLR Utility request/response payload.
No USB, sockets, device discovery, capture, or packet transmission.
"""
import argparse
import json
import re
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FEATURES = {
    0x807: "StockHandler_807", 0x80E: "StockHandler_80E",
    0x812: "StockHandler_812", 0x813: "StockHandler_813",
    0x815: "StockHandler_815",
}
def enum_body(path, name):
    return (ROOT / path).read_text().split("pub enum " + name + " {", 1)[1].split("}", 1)[0]

def numeric_keys(name):
    return {int(v,16):k for k,v in re.findall(r"(\w+)\s*=\s*(0x[0-9a-fA-F]+)",enum_body("types/src/lib.rs",name))}

def names():
    src = (ROOT / "usb/src/commands.rs").read_text()
    groups = dict(FEATURES)
    for label, n in re.findall(r"Command::(\w+)[^=\n]*=>[^\n]*?(0x[0-9a-fA-F]+)\s*<<\s*12",src):
        group = int(n,16)
        if group != 0x806 or group not in groups:
            groups[group] = label
    return groups

def decode(data, direction):
    if direction not in ("request","response"):
        raise ValueError("Direction must be request or response")
    if len(data)<16 or len(data)>1040:
        raise ValueError("Expected 16..1040 bytes")
    cmd,size,index=struct.unpack_from("<IHH",data)
    if len(data)!=16+size:
        raise ValueError("Declared body size does not match packet")
    group=cmd>>12
    sub=cmd&0xfff
    body=data[16:]
    name=names().get(group,"Unknown")
    if group==0:
        name={0:"ResetCommandIndex",1:"SupportsDCPCategory",2:"SystemFirmwareVersion"}.get(sub,"UnknownSystemCommand")
    if group==0x806 and 16<=sub<=23:
        name="SetSubChannelVolume"
    result={"direction":direction,"command":f"0x{cmd:08X}","action":name,
        "group":f"0x{group:03X}","selector":sub,"index":index,
        "body_size":size,"header_tail_hex":data[8:16].hex(),"body_hex":body.hex(),
        "decoded":{}}
    d=result["decoded"]
    if direction=="request":
        if group in (0x801,0x80b):
            if size%8:
                raise ValueError("Partial eight-byte effect/microphone parameter")
            keys=numeric_keys("EffectKey" if group==0x801 else "MicrophoneParamKey")
            d["parameters"]=[]
            for offset in range(0,size,8):
                k,v=struct.unpack_from("<Ii",body,offset)
                row={"key":f"0x{k:X}","name":keys.get(k,"Unknown"),"raw_value_hex":body[offset+4:offset+8].hex()}
                if group==0x801:row["signed_i32"]=v
                elif k in (1,2,3):row["gain_raw_u16"]=struct.unpack_from("<H",body,offset+6)[0]
                d["parameters"].append(row)
        elif group==0x818:
            if size!=1:raise ValueError("Monitored mix must be one byte")
            d["mix"]={0:"A",1:"B"}.get(body[0],"Unknown")
        elif group==0x817:
            if size not in (8,10):raise ValueError("Mix assignment expects 8 or 10 bytes")
            d["mix_a_selectors"]=list(body[:size//2]);d["mix_b_selectors"]=list(body[size//2:])
        elif group==0x804:
            if size not in (22,26):raise ValueError("Routing expects 22 or 26 bytes")
            labels=["Headphones L","Headphones R","Stream Mix 1 L","Stream Mix 1 R","Chat Mic L","Chat Mic R","Sampler L","Sampler R","Line Out L","Line Out R"]
            if size==26:labels+=["Stream Mix 2 L","Stream Mix 2 R"]
            d["outputs"]={label:body[2*i+1] for i,label in enumerate(labels)}
            d["hardtune_raw"]=body[-1]
        elif group==0x806:
            if size!=1:raise ValueError("Volume expects one byte")
            d["volume_raw"]=body[0]
        elif cmd==1:
            if size!=2:raise ValueError("Category support query expects two bytes")
            n=struct.unpack("<H",body)[0]
            d["category"]={1:"Peaks",2:"Router",3:"Mixer",4:"NVM"}.get(n,"Unknown")
    else:
        if group==0x800 and size==12:
            d["button_bits"]=hex(struct.unpack_from("<I",body)[0])
            d["encoders"]=dict(zip(("Pitch","Gender","Reverb","Echo"),struct.unpack_from("<4b",body,4)))
            d["faders"]=list(body[8:12])
        elif group==0x80c and size==2:
            d["mic_level_raw"]=struct.unpack("<H",body)[0]
        elif group==0x80f and sub==0 and size==24:
            f,build,unknown,fpga,dice_build,dice=struct.unpack("<6I",body)
            d.update(firmware=[f>>12,(f>>8)&15,f&255,build],unknown=unknown,fpga_count=fpga,dice=[(dice>>20)&15,(dice>>12)&255,dice&4095,dice_build])
    return result

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("direction",choices=["request","response"])
    p.add_argument("hex_payload",help="Hex bytes of the 16-byte GoXLR header and body (not USB capture metadata)")
    a=p.parse_args()
    try:print(json.dumps(decode(bytes.fromhex(a.hex_payload),a.direction),indent=2))
    except ValueError as e:p.error(str(e))
if __name__=="__main__":main()

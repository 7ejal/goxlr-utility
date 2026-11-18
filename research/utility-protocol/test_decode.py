import struct
import unittest
from decode import decode,names,numeric_keys
def packet(code,body=b""):
    return struct.pack("<IHH8x",code,len(body),42)+body
class Tests(unittest.TestCase):
    def test_source_maps(self):
        self.assertEqual(len(numeric_keys("EffectKey")),106)
        self.assertEqual(set(range(0x800,0x819))-{0x80d},set(names())-{4})
    def test_all_effect_keys(self):
        for key,name in numeric_keys("EffectKey").items():
            d=decode(packet(0x801000,struct.pack("<Ii",key,-5)),"request")["decoded"]["parameters"][0]
            self.assertEqual((d["name"],d["signed_i32"]),(name,-5))
    def test_route(self):
        for n in (22,26):
            b=bytearray(n);b[1]=32;b[-1]=16
            d=decode(packet(0x80400a,b),"request")["decoded"]
            self.assertEqual(d["outputs"]["Headphones L"],32)
            self.assertEqual(d["hardtune_raw"],16)
            self.assertEqual(len(d["outputs"]),10 if n==22 else 12)
    def test_monitor_and_volume(self):
        self.assertEqual(decode(packet(0x818000,b"\x01"),"request")["decoded"]["mix"],"B")
        self.assertEqual(decode(packet(0x806008,b"\x7f"),"request")["decoded"]["volume_raw"],127)
        self.assertEqual(decode(packet(0x806010,b"\x7f"),"request")["action"],"SetSubChannelVolume")
    def test_responses(self):
        d=decode(packet(0x80f000,struct.pack("<6I",0x1501,903,0,7,901,0x105001)),"response")["decoded"]
        self.assertEqual(d["firmware"],[1,5,1,903]);self.assertEqual(d["dice"],[1,5,1,901])
        self.assertEqual(decode(packet(0x80c000,b"\x34\x12"),"response")["decoded"]["mic_level_raw"],0x1234)
        self.assertEqual(decode(packet(0x800000,struct.pack("<I4b4B",1,-1,-2,3,4,5,6,7,8)),"response")["decoded"]["encoders"]["Pitch"],-1)
    def test_invalid(self):
        for b in (b"",packet(0x801000,b"x"),packet(0x804000,b"x"),packet(0x818000,b""),packet(0x801000)[:-1],packet(0x801000)+b"x"):
            with self.assertRaises(ValueError):decode(b,"request")
    def test_mic(self):
        for key in (1,2,3):
            d=decode(packet(0x80b000,struct.pack("<IHH",key,0,48)),"request")["decoded"]["parameters"][0]
            self.assertEqual(d["gain_raw_u16"],48)
if __name__=="__main__":unittest.main()

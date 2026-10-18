"""Offline format checks against actual LightBurn 2.1.04 golden exports."""
import collections
import json
import math
from pathlib import Path
import sys
import unittest
ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'tools'))
from rd_inspect import scramble,unscramble,pack7,unpack7,pack_relative,datagram,inspect,split_records

class CodecTests(unittest.TestCase):
    def test_scrambling_known_wire_responses(self):
        self.assertEqual(scramble(bytes.fromhex('cc cd cf ce 00')),bytes.fromhex('c6 46 48 c8 89'))
    def test_scrambling_all_byte_values(self):
        data=bytes(range(256))
        for magic in (0,0x88,0xff):
            encoded=scramble(data,magic)
            self.assertEqual(len(set(encoded)),256)
            self.assertEqual(unscramble(encoded,magic),data)
    def test_published_speed_and_relative_vectors(self):
        self.assertEqual(pack7(100000,5),bytes.fromhex('00 00 06 0d 20'))
        self.assertEqual(pack_relative(-100),bytes.fromhex('7f 1c'))
        self.assertEqual(unpack7(bytes.fromhex('7f 1c'),signed=True),-100)
    def test_relative_boundaries_exhaustively(self):
        for v in range(-8192,8192):
            self.assertEqual(unpack7(pack_relative(v),True),v)
        self.assertEqual(pack_relative(-8192),bytes.fromhex('40 00'))
        self.assertEqual(pack_relative(8191),bytes.fromhex('3f 7f'))
    def test_reject_range_errors(self):
        for v in (-8193,8192):
            with self.assertRaises(ValueError):pack_relative(v)
        for v in (-1,16384):
            with self.assertRaises(ValueError):pack7(v,2)
        with self.assertRaises(ValueError):unpack7(b'\x80')
    def test_udp_checksum_over_encoded_bytes(self):
        self.assertEqual(datagram(bytes.fromhex('c6 46 48')),bytes.fromhex('01 54 c6 46 48'))
        p=datagram(bytes([255])*1470)
        self.assertEqual(int.from_bytes(p[:2],'big'),(255*1470)&65535)
        with self.assertRaises(ValueError):datagram(bytes(1471))
    def test_malformed_record_prefix(self):
        with self.assertRaises(ValueError):list(split_records(b'\x01\xa8'))

class GoldenExportTests(unittest.TestCase):
    def load(self,name):return inspect((ROOT/'fixtures'/name).read_bytes())
    def test_line_geometry_and_parameters(self):
        r=self.load('rectangle-default.rd')
        self.assertEqual((r['bytes'],r['record_count']),(525,73))
        cut=[x for x in r['records'] if x.get('kind')=='cut']
        self.assertEqual([x['xy_mm'] for x in cut],[[801.687,463.024],[771.687,463.024],[771.687,448.024],[801.687,448.024]])
        self.assertIn(100,[x.get('speed_mm_s') for x in r['records']])
        self.assertEqual([x['power_raw14'] for x in r['records'] if 'power_raw14' in x],[3277]*4)
    def test_fill_geometry_matches_preview_cut_distance(self):
        r=self.load('rectangle-fill.rd')
        self.assertEqual((r['bytes'],r['record_count']),(2572,367))
        cut=[x for x in r['records'] if x.get('kind')=='cut']
        steps=[x for x in r['records'] if x.get('axis')=='Y']
        self.assertEqual(len(cut),150)
        self.assertEqual(len(steps),149)
        self.assertEqual({x['delta_um'] for x in steps},{-100})
        self.assertEqual(cut[0]['xy_mm'],[771.687,462.974])
        self.assertEqual(cut[-1]['xy_mm'],[801.687,448.074])
        positions=[x for x in r['records'] if 'xy_mm' in x]
        length=sum(math.dist(a['xy_mm'],b['xy_mm']) for a,b in zip(positions,positions[1:]) if b.get('kind')=='cut')
        self.assertAlmostEqual(length,4500)
    def test_grayscale_records_and_power_pairs(self):
        r=self.load('gray-ramp-grayscale.rd')
        self.assertEqual((r['bytes'],r['record_count']),(7318,2349))
        counts=collections.Counter(n for x in r['records'] for n in x['names'])
        self.assertEqual(counts['CUT_REL_X'],756)
        self.assertEqual(counts['MOVE_REL_Y'],13)
        mods=[x for x in r['records'] if 'modulation_raw14' in x]
        self.assertEqual(len(mods),1512)
        self.assertEqual(min(x['modulation_raw14'] for x in mods),0)
        self.assertEqual(max(x['modulation_raw14'] for x in mods),16383)
        for a,b in zip(mods[0::2],mods[1::2]):
            self.assertEqual(a['modulation_raw14'],b['modulation_raw14'])
        powers=[x['power_raw14'] for x in r['records'] if 'power_raw14' in x]
        self.assertEqual(powers,[164,9830,3277,3277])
    def test_checksums_for_all_real_exports(self):
        cases={'rectangle-default.rd':24999,'rectangle-fill.rd':142815,'gray-ramp-grayscale.rd':741544}
        for name,expected in cases.items():
            raw=unscramble((ROOT/'fixtures'/name).read_bytes())
            self.assertEqual(raw[-1],0xd7)
            self.assertEqual(raw[-8:-6],b'\xe5\x05')
            self.assertEqual(unpack7(raw[-6:-1]),expected)
            self.assertEqual(sum(raw[:-8])+0xd7,expected)
    def test_unknown_record_not_hidden(self):
        for name in ('rectangle-default.rd','rectangle-fill.rd','gray-ramp-grayscale.rd'):
            self.assertIn('f2 01 00',[x['hex'] for x in self.load(name)['records'] if not x['names']])
    def test_real_exports_byte_roundtrip(self):
        for p in (ROOT/'fixtures').glob('*.rd'):
            data=p.read_bytes();self.assertEqual(scramble(unscramble(data)),data)
    def test_wrong_magic_does_not_match_known_header(self):
        data=(ROOT/'fixtures/rectangle-default.rd').read_bytes()
        self.assertEqual(unscramble(data)[:4],bytes.fromhex('d8 10 e6 01'))
        self.assertNotEqual(unscramble(data,0x89)[:4],bytes.fromhex('d8 10 e6 01'))

if __name__=='__main__':unittest.main()

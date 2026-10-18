#!/usr/bin/env python3
"""Offline Ruida research codec. No network or device writes.

The command splitter is for 7-bit-payload streams; it is not a complete
firmware-independent grammar. Unknown records remain visible with raw bytes.
"""
import argparse
import json
from pathlib import Path


def swap_bits(value):
    return (value & 0x7e) | ((value & 1) << 7) | ((value >> 7) & 1)


def scramble(data, magic=0x88):
    return bytes(((swap_bits(v) ^ magic) + 1) & 255 for v in data)


def unscramble(data, magic=0x88):
    return bytes(swap_bits(((v - 1) & 255) ^ magic) for v in data)


def pack7(value, width):
    if not 0 <= value < (1 << (7 * width)):
        raise ValueError('unsigned value outside field')
    return bytes((value >> shift) & 127 for shift in range(7*(width-1), -1, -7))


def unpack7(data, signed=False):
    if any(v > 127 for v in data):
        raise ValueError('not a 7-bit payload')
    result = 0
    for v in data:
        result = (result << 7) | v
    bits = 7 * len(data)
    if signed and bits and result & (1 << (bits-1)):
        result -= 1 << bits
    return result


def pack_relative(um):
    if not -8192 <= um <= 8191:
        raise ValueError('relative coordinate outside signed14 range')
    return pack7(um & 0x3fff, 2)


def datagram(encoded_payload):
    if len(encoded_payload) > 1470:
        raise ValueError('payload exceeds conservative 1470 byte limit')
    return (sum(encoded_payload) & 65535).to_bytes(2, 'big') + encoded_payload


def split_records(raw):
    marks = [i for i,v in enumerate(raw) if v & 128]
    if raw and (not marks or marks[0] != 0):
        raise ValueError('stream has payload before first opcode')
    for start, end in zip(marks, marks[1:] + [len(raw)]):
        yield start, raw[start:end]


def inspect(data, magic=0x88):
    raw = unscramble(data, magic)
    registry_path = Path(__file__).resolve().parents[1]/'protocol/command-registry.json'
    registry = json.loads(registry_path.read_text())['commands'] if registry_path.exists() else []
    results=[]
    pos=[0,0]
    position_known=False
    for offset, record in split_records(raw):
        matches=[r for r in registry if record.startswith(bytes.fromhex(r['hex']))]
        longest=max((len(bytes.fromhex(r['hex'])) for r in matches),default=0)
        matches=[r for r in matches if len(bytes.fromhex(r['hex']))==longest]
        entry={'offset':offset,'hex':record.hex(' '),'names':[r['name'] for r in matches]}
        op=record[0]
        if op in (0x88,0xa8) and len(record)==11:
            # Positive coordinates only in the observed fixtures. Signed absolute
            # representation differs among implementations; don't generalize it.
            pos=[unpack7(record[1:6]),unpack7(record[6:11])]
            position_known=True
            entry.update(kind='cut' if op==0xa8 else 'travel',xy_mm=[v/1000 for v in pos])
        elif op in (0x89,0xa9) and len(record)==5:
            delta=[unpack7(record[1:3],True),unpack7(record[3:5],True)]
            pos=[p+d for p,d in zip(pos,delta)]
            entry.update(kind='cut' if op==0xa9 else 'travel',delta_um=delta)
            if position_known:entry['xy_mm']=[v/1000 for v in pos]
        elif op in (0x8a,0x8b,0xaa,0xab) and len(record)==3:
            axis=0 if op in (0x8a,0xaa) else 1
            delta=unpack7(record[1:3],True);pos[axis]+=delta
            entry.update(kind='cut' if op>=0xa0 else 'travel',axis='XY'[axis],delta_um=delta)
            if position_known:entry['xy_mm']=[v/1000 for v in pos]
        elif op in (0xc7, 0xc2) and len(record)==3:
            entry['modulation_raw14']=unpack7(record[1:])
        elif record.startswith(b'\xc9\x02') and len(record)==7:
            entry['speed_mm_s']=unpack7(record[2:])/1000
        elif record.startswith(b'\xc6') and len(record)==4 and record[1] in (1,2,0x21,0x22,5,6,7,8):
            value=unpack7(record[2:]);entry['power_raw14']=value;entry['power_pct_16383']=value*100/16383
        results.append(entry)
    return {'bytes':len(data),'magic':hex(magic),'record_count':len(results),'records':results}


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('file',type=Path)
    parser.add_argument('--magic',type=lambda s:int(s,0),default=0x88)
    parser.add_argument('--json',action='store_true')
    args=parser.parse_args()
    result=inspect(args.file.read_bytes(),args.magic)
    if args.json:print(json.dumps(result,indent=2))
    else:
        for r in result['records']:
            tail={k:v for k,v in r.items() if k not in ('offset','hex','names')}
            print(f"{r['offset']:06x}  {r['hex']:<48} {'/'.join(r['names']) or 'UNKNOWN'} {tail or ''}")

if __name__=='__main__':main()

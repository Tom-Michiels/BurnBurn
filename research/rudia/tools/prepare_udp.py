#!/usr/bin/env python3
"""Prepare Ruida UDP datagrams offline; no networking or execution envelope."""
import argparse
import hashlib
import json
from pathlib import Path


def packetize(data, chunk_size=1470):
    if not 1 <= chunk_size <= 1470:
        raise ValueError('chunk size must be between 1 and 1470')
    for offset in range(0, len(data), chunk_size):
        payload = data[offset:offset + chunk_size]
        checksum = sum(payload) & 0xffff
        yield offset, checksum.to_bytes(2, 'big') + payload


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('rd_file', type=Path)
    parser.add_argument('--chunk-size', type=int, default=1470)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    data = args.rd_file.read_bytes()
    if not data:
        parser.error('empty input file')
    if args.rd_file.resolve() == args.output.resolve():
        parser.error('output must differ from input')
    try:
        packets = [
            {'index': i, 'offset': offset, 'payload_bytes': len(packet) - 2,
             'checksum_hex': packet[:2].hex(), 'datagram_hex': packet.hex()}
            for i, (offset, packet) in enumerate(packetize(data, args.chunk_size))
        ]
    except ValueError as error:
        parser.error(str(error))
    result = {
        'mode': 'offline-only',
        'note': 'No storage envelope, network connection or execution performed.',
        'input': str(args.rd_file), 'sha256': hashlib.sha256(data).hexdigest(),
        'input_bytes': len(data), 'host_port': 40200, 'controller_port': 50200,
        'chunk_size': args.chunk_size, 'packets': packets,
    }
    args.output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(f'{len(data)} bytes -> {len(packets)} offline packets: {args.output}')


if __name__ == '__main__':
    main()

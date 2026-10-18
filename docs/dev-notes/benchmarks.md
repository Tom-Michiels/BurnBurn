# Benchmarks (Testing, fase C)

Ignored tests met vaste, in de test gegenereerde fixtures: `crates/burnburn-scenarios/tests/benchmarks.rs`. Ze meten
wandkloktijd en controleren één correctheidsinvariant; er zijn **geen harde tijdsgrenzen** (één machine, geen
baseline, geen statistiek). Een getal hier is een oriëntatie, geen contract.

```sh
cargo test -p burnburn-scenarios --release --test benchmarks -- --ignored --nocapture --test-threads=1
```

## Meting 18-10-2026

Machine: Apple M4 Max, 16 cores, 128 GB, macOS (Darwin 27.0), toolchain 1.99.1, `--release`, één thread per test.
Gedraaid op een scratch-kopie van de workspace (stand 01:59, toen alle crates compileerden; andere rollen bouwden
tegelijk). Drie runs achter elkaar; waarden in ms.

| Benchmark | Fixture | Invariant (geslaagd) | Run 1 | Run 2 | Run 3 |
|---|---|---|---|---|---|
| `bench_compile_100k_segments` — `cam::compile` | 1000 gesloten 100-hoeken (100 000 lijnsegmenten), één Line-operation | 100 000 cut-records na decode | 143,5 | 135,0 | 139,9 |
| ↳ `cam::preflight` | idem | geen fouten | 17,7 | 20,7 | 20,0 |
| ↳ `ruida::lower` + RD-encode | idem (1 111 220 RD-bytes) | — | 5,6 | 6,1 | 6,2 |
| ↳ `ruida::decode_rd_file` | idem | — | 19,6 | 22,2 | 22,6 |
| `bench_trace_2000_square` — `trace::trace` | 2000 × 2000 grijs, 10 × 10 ringen (R 80 px, r 40 px, geen AA), 0,05 mm/px | 100 componenten, 100 gaten | 76,5 | 76,7 | 83,5 |
| `bench_boolean_10k_segments` — `geometry::boolean` Union | twee 5000-hoeken (r 50, afstand 50) | oppervlak 12 637,036 vs analytisch 12 637,039 mm² (lensformule), 1 subpad | 2,4 | 1,2 | 1,3 |

Opmerkingen:
- De compile van 100 k segmenten (≈ 0,14 s) domineert de keten; preflight + lower + decode samen ≈ 45 ms.
- De tracebenchmark gebruikt een binair beeld zonder anti-aliasing; AA-randen en ruis zijn (nog) niet gemeten.
- De boolean-oracle is onafhankelijk (oppervlak van twee overlappende cirkels; het verschil 0,003 mm² past bij de
  inschrijvingsfout van een 5000-hoek, ≈ 0,0066 mm² per cirkel).

## Onbevestigd
- Alleen macOS arm64. Windows/x86_64 en debug-builds niet gemeten.
- Geen herhaalde metingen met spreiding/percentielen; geen vergelijking over tijd (geen regressiedrempel).

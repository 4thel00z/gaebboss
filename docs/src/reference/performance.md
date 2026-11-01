# Performance

Measured on 2026-11-01 on an aarch64 workstation (Cortex-X925, 20 cores,
single-threaded throughout), Rust 1.99, release profile. Reproduce with
`cargo bench -p gaebboss-core` and `cargo xtask corpus --dir <files>`.

## Stages, criterion

| benchmark | input | throughput |
|---|---|---|
| `lex/bvbs-x83-10mb` | the BVBS X83 repeated to 10 MB | 2.36 GiB/s |
| `lex/corpus-largest` | a 75 MB real X81 | 15.2 GiB/s |
| `parse/bvbs-x83-10mb` | full typed parse | 675 MiB/s |
| `parse/corpus-largest` | full typed parse | 2.59 GiB/s |
| `items/oz-and-kind` | Ordnungszahl and kind per item | 4.5 M items/s |
| `items/with-texts` | plus short and long text | 1.5 M items/s |
| `items/totals` | category and bill sums | 22 M items/s |
| `write/compact` | typed tree back to XML | 497 MiB/s |
| `write/pretty` | indented | 471 MiB/s |

The synthetic file is dense: small items with long texts and many
elements per byte. Real files carry more text per element, which is why
the 75 MB X81 reads four times faster per byte.

## Whole corpus, `cargo xtask corpus`

164 real-world files, 189.6 MB, DA XML 3.0 to 3.3:

| stage | throughput |
|---|---|
| tokenize | 2585 MB/s (1.12 M nodes) |
| typed parse | 866 MB/s, 0 errors |
| items with texts | 3797 MB/s (28 418 items, 15.4 MB of text) |
| write back | 421 MB/s (76.4 MB written) |

Warnings over the corpus: 800 unknown elements (vendor extensions such
as `Codetext` and `OZ`, and HTML placed where the schema has none) and 9
duplicated `Totals`, all kept.

## Why it is fast

- One pass over the bytes with `memchr` builds a flat tape of spans;
  nothing is copied unless an entity has to be decoded.
- Element names resolve through a generated length-switched match, no
  hashing.
- The typed model stores spans and fixed-point decimals, so building it
  allocates one `Vec` per repeated field and nothing per string.
- Readers and writers are generated per type as straight loops: no
  reflection, no per-value dispatch, no intermediate DOM.
- Texts are rendered from the tape on demand, so a caller that never
  asks for a long text never pays for it.

`perf` is not available in the environment this was built in; the
remaining profiling work is listed under limitations.

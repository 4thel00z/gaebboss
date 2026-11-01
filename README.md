# gaebboss

GAEB DA XML in pure Rust: read German construction tenders, bids and orders (X80 to X89, DA XML 3.0 to 3.3), write X83, X84 and X86 that the official schemas accept, and turn a tender into a priced bid. The typed model, readers, writers and per-phase rules are generated from the official GAEB XSDs; numbers are fixed-point; malformed files are read with warnings instead of exceptions and nothing is lost on the way through. One core behind the `gaebboss` CLI and the `gaebboss` Python package.

```bash
cargo install gaebboss-cli
pip install gaebboss
```

```bash
gaebboss info tender.X83
gaebboss items tender.X83 --tsv
gaebboss bid tender.X83 --prices prices.csv --bidder bidder.toml -o bid.X84
gaebboss validate bid.X84 --tender tender.X83
```

```python
from decimal import Decimal
import gaebboss

tender = gaebboss.Document("tender.X83")
bid = gaebboss.Bid.from_tender(tender, {"name1": "Bau GmbH", "street": "Weg 1", "pcode": "12345", "city": "Berlin"})
for item in tender.items():
    bid.price(item.id, Decimal("12.50"))
open("bid.X84", "wb").write(bid.finish().write())
```

## Proof

- Every official sample file (gaeb.de Musterdateien 3.1, BVBS Prüfdateien 3.3) and a 164-file real-world corpus read with zero errors and round-trip to byte-identical output.
- `make conformance` writes X81 to X84 files from the official fixtures and validates them against the official XSDs with lxml: all valid.
- The 75 MB real-world X81 parses and iterates in under 30 ms; see `docs/src/reference/performance.md`.
- Panics are denied by the workspace lints; every tolerated defect is a warning with a byte offset.

## Layout

| path | what |
|---|---|
| `crates/gaebboss-core` | the engine |
| `crates/gaebboss-cli` | the `gaebboss` binary |
| `crates/gaebboss-py`, `python/gaebboss` | the Python package (maturin) |
| `crates/gaebboss-testkit` | fixture builder and corpus discovery |
| `xtask` | `cargo xtask gen` (schemas to code), `coverage`, `corpus` |
| `standard/` | the official XSDs, vendored unmodified with checksums |
| `tests/fixtures` | official sample files and the failure-mode gallery |
| `docs/` | the mdBook |

`make help` lists the targets. Real-world files for the corpus tests are found through `GAEBBOSS_CORPUS`.

## License

MIT or Apache-2.0, at your option. The schema files under `standard/` remain the work of GAEB / DIN; see `standard/README.md`.

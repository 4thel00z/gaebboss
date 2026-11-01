# gaebboss

GAEB DA XML in pure Rust: read tenders, bids and orders (X80 to X89, DA XML 3.0 to 3.3), write schema-valid X83, X84 and X86, and turn a tender into a priced bid. The model is generated from the official GAEB schemas, numbers are fixed-point, nothing is lost on the way through, and malformed files are read with warnings instead of exceptions. One core behind the `gaebboss` CLI and the `gaebboss` Python package.

```bash
cargo install gaebboss-cli
pip install gaebboss
```

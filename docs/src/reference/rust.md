# Rust API

The crate is `gaebboss-core`; `cargo doc --open -p gaebboss-core` has the
full reference. The entry points:

| item | what it is |
|---|---|
| `Document::open`, `Document::parse`, `Document::parse_named` | read a file; the name decides the phase when nothing inside does |
| `Document::items`, `categories`, `markup_items` | navigation with `Oz`, `ItemKind`, texts and prices |
| `Document::entries` | categories, items and markup items together in document order, as `boq::Entry` |
| `Document::element_range` | where an element sits in `Document::bytes` |
| `Document::computed_totals`, `declared_total` | sums per Fachdokumentation 4.6.17 |
| `text::plain`, `text::markdown`, `text::complements` | projections of any text node |
| `validate::validate`, `validate::validate_against_tender` | findings |
| `Document::write`, `Document::write_with_report`, `WriteOptions` | XML out, per version and phase |
| `bid::Bid` | X83 to X84 |
| `json::to_json` | the document as JSON with string decimals |
| `decimal::Decimal` | fixed-point numbers, `item_total`, `total_after_discount` |
| `model::*` | the generated typed model, one struct per schema type |

Lower layers are public too: `bytes::normalize`, `lexer::lex` and `tape::Tape`
give the flat tape; `version::detect` the route; `read::Reader` and the
generated `generated::read` the typed readers.

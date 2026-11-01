# gaebboss

GAEB DA XML is how German construction tenders travel: the Leistungsverzeichnis
(bill of quantities) goes out as X83, bidders send their prices back as X84,
the order is placed as X86. gaebboss reads every version from 3.0 to 3.3,
writes X83, X84 and X86 that the official schemas accept, and turns a tender
into a priced bid without a single floating-point operation.

It is built from the standard, not from other parsers: the typed model, the
readers, the writers and the per-phase rules are generated from the official
XSDs that GAEB publishes. Where the Fachdokumentation prescribes a rule, such
as how totals round or which items count towards the sum, that rule is
implemented and tested against the official sample files.

## What you get

- `gaebboss-core`, a Rust crate with no C dependencies: tokenizer, typed
  model, navigation over categories and items, text projections, writers,
  validation and bid construction.
- `gaebboss`, the command line: `info`, `items`, `text`, `json`, `validate`,
  `roundtrip`, `bid`.
- `gaebboss`, the Python package: the same engine behind a small API that
  hands out `decimal.Decimal`, never floats.

## How it treats bad input

Real files are not clean. Vendor elements, HTML where the schema has none,
decimal commas, duplicated elements, truncated files and Latin-1 bytes under
a UTF-8 declaration all occur in the wild. gaebboss reads all of them,
reports each defect as a warning with its byte offset, keeps unknown
elements for writing back, and raises an error only when the input is not
GAEB DA XML at all.

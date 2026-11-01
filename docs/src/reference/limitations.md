# Limitations

- GAEB 90 (`.d8x`), GAEB 2000 (`.p8x`) and DA XML before 3.0 are detected
  and named, not parsed.
- Writing targets 3.2 and 3.3. A 3.0 or 3.1 document is written as 3.2.
- Phases outside X80 to X89 (trade X93 to X97, cost X50 to X52, quantity
  X31, Zeitvertrag) are read into the typed model and written back, but have
  no navigation layer, no validation rules and no tests beyond "reads
  without error".
- Converting a document to a phase that requires elements the source lacks
  yields an invalid file by construction; the write report names the missing
  elements, and `strict` turns them into an error.
- Validation checks decimal places for Qty, UP, IT and VAT; other facets
  (string lengths, patterns beyond RNoPart) are left to the XSD check in
  `make conformance`.
- Text projections and complements read the parsed source; edits made
  through `Bid` show up after the document is written and read again.
- `perf` was unavailable where this was built, so the profiling pass is
  still open; see performance.

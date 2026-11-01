# Failure modes

One defect per file, each a shape seen in the wild or implied by the
standard. `crates/gaebboss-core/tests/failure_modes.rs` pins how gaebboss
reads each one: never an error, always the warning or finding named here.

| file | defect | expected |
|---|---|---|
| `unclosed-tag.x83` | the file ends inside a long text | `UnclosedElement` warnings, the item still reads |
| `stray-end-tag.x83` | an end tag with no open element | `MismatchedEndTag` warning, siblings intact |
| `bad-decimal.x83` | `1,5` with a decimal comma | `BadDecimal` warning, quantity absent |
| `bad-enum.x83` | `Provis` and `LumpSumItem` with values outside the enumeration | `BadEnum` warnings, fields absent |
| `duplicate-element.x83` | two `Qty` on one item | `Cardinality` warning, first kept |
| `duplicate-id.x83` | two items share an `ID` | `DuplicateId` validation error |
| `vendor-elements.x83` | `Codetext` and `OZ` inside an item (vendor extensions) | `UnknownElement` warnings, kept in `extra`, written back verbatim |
| `latin1-bytes.x83` | declared UTF-8, bytes are ISO-8859-1 | `EncodingRepaired` warning, text reads `Größe` |
| `missing-breakdown.x83` | no `BoQBkdn` levels | parts pass through unpadded, `MissingRequired` for phase X83 |
| `oz-overflow.x83` | an item part of 13 characters under a 4-wide level | `OzTooLong` validation error |

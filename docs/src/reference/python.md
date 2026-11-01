# Python API

The authoritative signatures are the stubs shipped with the package,
`gaebboss/_gaebboss.pyi`. In short:

- `Document(path=None, *, data=None, name=None)`; properties `version`,
  `phase`, `warnings`, `project`, `boq`; methods `items(kind="all")`,
  `categories()`, `markup_items()`, `to_json()`, `validate()`,
  `validate_against(tender)`, `write(*, phase, version, pretty, strict)`,
  `write_report(*, phase, version, strict)`.
- `Item`: `id`, `oz`, `kind`, `alternative_group`, `alternative_serial`,
  `short_text`, `long_text`, `long_text_markdown`, `quantity`, `unit`,
  `unit_price`, `total`, `vat`, `counts_towards_total`, `not_offered`,
  `category_ozs`, `complements`.
- `Bid.from_tender(document, bidder)`; `price`, `price_components`,
  `not_offered`, `free_quantity`, `fill_complement`, `markup`,
  `discount_percent`, `vat_percent`, `finish()`.
- `GaebError` for every failure.

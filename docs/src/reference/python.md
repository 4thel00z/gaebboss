# Python API

The authoritative signatures are the stubs shipped with the package,
`gaebboss/_gaebboss.pyi`. In short:

- `Document(path=None, *, data=None, name=None)`; properties `version`,
  `phase`, `warnings`, `project`, `boq`; methods `items(kind="all")`,
  `categories()`, `markup_items()`, `entries()`, `award_add_texts()`,
  `gaeb_add_texts()`, `to_json()`,
  `validate(tender=None)`, `validate_against(tender)`,
  `write(*, phase, version, pretty, strict)`,
  `write_report(*, phase, version, strict)`.
- `entries()` returns `Category`, `Item`, `MarkupItem` and `Remark` objects in
  document order, each category before what it contains.
- `award_add_texts()` returns the award's `AddText` objects, which come before
  the bill; `gaeb_add_texts()` those of the file, which come after the award.
- `Item`: `id`, `oz`, `kind`, `alternative_group`, `alternative_serial`,
  `short_text`, `long_text`, `long_text_markdown`, `quantity`, `unit`,
  `unit_price`, `total`, `vat`, `counts_towards_total`, `not_offered`,
  `category_ozs`, `complements`.
- `Category`: `oz`, `label`, `depth`, `total`.
- `MarkupItem`: `id`, `oz`, `short_text`, `long_text`, `long_text_markdown`,
  `base`, `percent`, `total`, `category_ozs`.
- `Remark`: `id`, `short_text`, `long_text`, `long_text_markdown`,
  `category_ozs`.
- `AddText`: `short_text`, `long_text`, `long_text_markdown`.
- `Bid.from_tender(document, bidder)`; `price`, `price_components`,
  `not_offered`, `free_quantity`, `fill_complement`, `markup`,
  `discount_percent`, `vat_percent`, `finish()`.
- `GaebError` for every failure.

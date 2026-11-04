# Reading a bill

`Document::open` or `Document::parse` reads a file of any GAEB DA XML version
from 3.0 to 3.3. The version and phase come from the namespace first, then
from `GAEBInfo/Version` and `Award/DP`, then from the file extension; every
guess is a warning you can read in `doc.warnings`.

The typed tree is `doc.gaeb`: one struct per schema type, strings as spans
resolved with `doc.str(span)`, decimals as `Decimal`. Most callers never touch
it and use the navigation layer instead:

- `doc.items()` yields every item in document order with its `Oz` (the
  Ordnungszahl formed from the `BoQBkdn` levels), its kind, quantity, unit,
  prices and texts, plus the categories above it in `path`.
- `doc.categories()` yields the LV-Bereiche with label, depth and declared
  totals.
- `doc.markup_items()` yields Zuschlagspositionen with their base, percent,
  total and texts.
- `doc.entries()` yields categories, items and markup items together in
  document order, each category before what it contains.
- `doc.computed_totals()` sums items per Fachdokumentation 4.6.17 and puts
  the result next to what the file declares. A category passes its lump sum
  (`TotalLSUM`) or its sum less its discount up to its parent; an
  alternative category (`ALNBSerNo` other than 0) passes nothing.
  `ItemRef::computed_total()` is quantity times unit price less the item's
  `DiscountPcnt`, and none for a not-applicable item.

## Kinds of items

`ItemRef::kind()` tells a Standard item from a LumpSum, a Bedarfsposition
with or without Gesamtbetrag (`ProvisionalWithTotal`,
`ProvisionalWithoutTotal`), a Grundposition (`Base`) and its alternatives
(`Alternative { group, serial }`), hourly and key items, a free quantity the
bidder fills in, and a not-applicable item. `counts_towards_total()` applies
the standard's rule: alternatives, Bedarfspositionen ohne GB, not-applicable
and not-offered items stay out of the sums, whatever else they are.

## Ordnungszahl

`item.oz.dotted()` prints `001.002.0010A`: levels zero-padded to their
declared width (or space-padded per alignment for non-numeric levels),
joined by dots, the index appended directly; a skipped level shows as `_`.
`compact()` gives the fixed-width form, `trade()` the first level, `lot()`
the lot when the breakdown starts with one.

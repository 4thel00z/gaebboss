# From tender to bid

`Bid::from_tender(&tender, bidder)` re-reads the tender, relabels it X84 and
carries the bidder as `CTR`. Then:

- `price(id, up)` sets the unit price (three decimals) and the total from the
  quantity.
- `price_components(id, &[...])` sets up to six components that must add up
  to the unit price.
- `free_quantity(id, qty)` fills a quantity the tender left open (`QtyTBD`).
- `not_offered(id)` marks an item as not offered: no price is transmitted.
- `fill_complement(id, mark, text)` answers a bidder field in the long text.
- `markup(id, base, percent)` prices a Zuschlagsposition.
- `discount_percent`, `vat_percent` set the bill-level figures.

`finish()` refuses when an item is neither priced nor marked, computes the
total of every category and of the bill with the standard's rounding, writes
no total on items that do not count (an X84 no longer says why they would
not), and returns the X84 document. `doc.write(&doc.x84_options())` writes
it strictly: a missing required element is an error, not a report.

Tenders in 3.0 or 3.1 are answered in 3.2, the oldest version gaebboss
writes; 3.2 and 3.3 tenders are answered in their own version.

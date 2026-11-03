---
name: gaebboss
description: Use when reading, exploring, validating, converting or pricing GAEB DA XML files (German construction tenders, bids and orders: X80 to X89, typically .X83 tenders, .X84 bids, .X86 orders) with gaebboss, the pure-Rust GAEB engine with a CLI and Python bindings. Triggers include listing the items or Ordnungszahlen of a Leistungsverzeichnis, extracting long texts as plain text or Markdown, dumping a bill as JSON or querying it with jq, hexdumping an element, checking totals and phase rules, answering a tender with a priced X84 bid, converting between phases or versions, and browsing a bill interactively in the terminal.
---

# gaebboss

GAEB DA XML in pure Rust: read tenders, bids and orders in DA XML 3.0 to 3.3, validate them, write schema-valid X83, X84 and X86, and turn a tender into a priced bid. One core behind the `gaebboss` CLI and the `gaebboss` Python package. The typed model, readers, writers and phase rules are generated from the official XSDs; money and quantities are fixed-point decimals. The reader is lenient: broken encodings and tags are repaired, and every tolerated defect is reported as a warning with a byte offset.

## Install

```bash
pip install gaebboss           # abi3 wheels, CPython 3.12+, no toolchain needed
cargo install gaebboss-cli     # the `gaebboss` binary
```

Coding agents can install this skill with `gaebboss skill install` (writes it into ./.claude/skills, `-g` for ~/.claude/skills; `gaebboss skill show` prints it).

## CLI

```bash
gaebboss info      tender.X83                       # version, phase, project, bill, counts, totals, warnings
gaebboss items     tender.X83 --tsv                 # one row per item: OZ, id, kind, quantity, unit, prices, short text
gaebboss text      tender.X83 --oz 01.02.0010       # one long text; omit --oz for every item
gaebboss md        tender.X83                       # the bill as Markdown: category headings, item long texts
gaebboss json      tender.X83 --compact             # the whole document, decimals as strings
gaebboss q         tender.X83 '.items[] | select(.kind == "LumpSum") | .oz' -r
gaebboss hex       tender.X83 oz:01.02.0010         # hexdump one element (oz:OZ, id:ID, range:START-END)
gaebboss tui       tender.X83                       # interactive explorer
gaebboss validate  bid.X84 --tender tender.X83      # exit 1 on errors
gaebboss roundtrip tender.X83 -o copy.X83 --pretty  # --phase X84 --version 3.3 convert
gaebboss bid       tender.X83 --prices prices.csv --bidder bidder.toml -o bid.X84
```

Items are addressed by Ordnungszahl, dotted (`01.02.0010`) or compact (`01020010`), or by their `ID` attribute.

`prices.csv` holds `oz,up[,qty]` or `id,up[,qty]` rows, separated by `,`, `;` or tab, decimal commas accepted; a `-` price marks an item not offered and the optional third column is the quantity of a free-quantity item. `bidder.toml` carries `name1`, `street`, `pcode`, `city` and optionally `name2`, `contact`, `email`, `bidder_no`, `vat_percent`, `discount_percent`.

Exit codes: 0 on success; 1 when a file cannot be read or written, a jq program fails while running, or `validate` finds errors; 2 for an invalid jq program or bad arguments.

`hex` offsets index the document as parsed: UTF-8 without a byte order mark, Latin-1, CP1252 and UTF-16 inputs transcoded.

### Explorer keys (`gaebboss tui`)

| Key | Action |
|---|---|
| `j` `k` / arrows | move, or scroll the focused pane |
| `h` `l` | collapse / expand, `Enter` toggles, `Backspace` goes to the parent |
| `Tab` | focus tree, inspector, XML in turn |
| `/` then `n` `N` | search Ordnungszahl, ID, labels and long texts |
| `]` `[` | next / previous row with validation findings |
| `m` | long text as Markdown |
| `y` | yank: `o` OZ, `i` ID, `s` short text, `t` long text, `m` Markdown, `x` XML, `q` jq query, `c` shell command |
| `Alt`+arrows | resize panes |
| `q` | quit |

## JSON for jq

`gaebboss json` and `gaebboss q` see one object: `version`, `phase`, `project {name, label, currency}`, `boq {id, name, label, breakdown, totals}`, `categories[] {oz, label, depth, totals}`, `items[] {id, oz, kind, short_text, long_text, quantity, unit, unit_price, total, vat, counts_towards_total, not_offered, complements}`, `markup_items[] {id, oz, base, percent, total}` and `warnings[] {code, offset, message}`. Every decimal is a string.

```bash
gaebboss q tender.X83 '[.items[] | select(.counts_towards_total) | .total // "0" | tonumber] | add'
gaebboss q tender.X83 '.items[] | select(.long_text | test("Beton")) | .oz' -r
```

## Python

```python
from decimal import Decimal
import gaebboss

tender = gaebboss.Document("tender.X83")             # or Document(data=raw_bytes, name="tender.X83")
print(tender.version, tender.phase, tender.project.name, tender.boq.name)
for item in tender.items():                          # items(kind="counting") for what the total counts
    print(item.oz, item.kind, item.quantity, item.unit, item.short_text)
    print(item.long_text_markdown)

bid = gaebboss.Bid.from_tender(tender, {"name1": "Bau GmbH", "street": "Weg 1", "pcode": "12345", "city": "Berlin"})
for item in tender.items():
    bid.price(item.id, Decimal("12.50"))
bid.vat_percent(Decimal("19"))
x84 = bid.finish()                                   # raises on unpriced items
open("bid.X84", "wb").write(x84.write())
print(x84.validate_against(tender))
```

## Rust

```rust
use gaebboss_core::Document;
use gaebboss_core::boq::Entry;

let doc = Document::open(std::path::Path::new("tender.X83"))?;
for entry in doc.entries() {                          // categories, items and markup items in document order
    if let Entry::Item(item) = entry {
        println!("{} {:?} {}", item.oz, item.kind(), item.short_text());
    }
}
let totals = doc.computed_totals();                   // category and bill sums next to the declared ones
let findings = gaebboss_core::validate::validate(&doc);
```

## Gotchas

- Quantities and prices are decimals with their written scale (`5000.000`); compare them as decimals, never as floats.
- An item's `total` is what the file declares; `Qty x UP` rounded to cents is the computed one, and `validate` reports the difference.
- Alternatives, provisional items without total, not-applicable and not-offered items do not count towards the total (`counts_towards_total`).
- A 3.0 or 3.1 tender is answered in 3.2, the oldest version with phase schemas.

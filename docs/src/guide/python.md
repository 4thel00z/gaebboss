# Python

```bash
pip install gaebboss
```

`gaebboss.Document(path)` or `gaebboss.Document(data=bytes, name="x.X83")`
parses with the GIL released; `data` takes `bytes`, `bytearray`, `memoryview`
or any other buffer and copies it once. Everything it returns is a plain object:
`items()`, `categories()`, `markup_items()`, `entries()`, `boq`, `project`,
`warnings`, `validate(tender=None)`, `validate_against(tender)`, `to_json()`,
`write(...)` and `write_report(...)`. `entries()` returns categories, items and
markup items in document order, each category before what it contains, so a
reader can rebuild the tree in one pass. Every GAEB number is a `decimal.Decimal` built from its
exact text; passing a float to a bid raises `GaebError`.

```python
from decimal import Decimal
import gaebboss

tender = gaebboss.Document("tender.X83")
bid = gaebboss.Bid.from_tender(tender, {"name1": "Bau GmbH", "street": "Weg 1", "pcode": "12345", "city": "Berlin"})
for item in tender.items():
    if item.kind == "FreeQuantity":
        bid.free_quantity(item.id, Decimal("10.000"))
    bid.price(item.id, Decimal("12.50"))
bid.vat_percent(Decimal("19"))
doc = bid.finish()
assert [f for f in doc.validate(tender=tender) if f.severity == "Error"] == []
open("bid.X84", "wb").write(doc.write())
```

Documents are immutable and can be shared between threads; parsing sixteen
copies in a thread pool scales with cores. Reading a file, `items()`,
`validate()`, `validate_against()`, `write()`, `write_report()`, `to_json()`,
`Bid.from_tender()` and `Bid.finish()` all run with the GIL released, so
other Python threads keep running meanwhile. Item decimals become
`decimal.Decimal` when the attribute is read.

# Quickstart

```bash
cargo install gaebboss-cli
pip install gaebboss
```

Look at a tender:

```bash
gaebboss info tender.X83
gaebboss items tender.X83
gaebboss text tender.X83 --oz 01.02.0010 --markdown
```

Price it:

```bash
gaebboss items tender.X83 --tsv | cut -f1 > prices.csv   # then add ",<unit price>" per line
cat > bidder.toml <<'EOT'
name1 = "Bau GmbH"
street = "Weg 1"
pcode = "12345"
city = "Berlin"
vat_percent = "19"
EOT
gaebboss bid tender.X83 --prices prices.csv --bidder bidder.toml -o bid.X84
gaebboss validate bid.X84 --tender tender.X83
```

The same in Python:

```python
from decimal import Decimal
import gaebboss

tender = gaebboss.Document("tender.X83")
for item in tender.items():
    print(item.oz, item.quantity, item.unit, item.short_text)

bid = gaebboss.Bid.from_tender(tender, {"name1": "Bau GmbH", "street": "Weg 1", "pcode": "12345", "city": "Berlin"})
for item in tender.items():
    bid.price(item.id, Decimal("12.50"))
bid.vat_percent(Decimal("19"))
open("bid.X84", "wb").write(bid.finish().write())
```

And in Rust:

```rust
use gaebboss_core::Document;

let doc = Document::open(std::path::Path::new("tender.X83"))?;
for item in doc.items() {
    println!("{} {:?} {}", item.oz, item.quantity(), item.short_text());
}
```

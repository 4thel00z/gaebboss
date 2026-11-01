# Command line

```bash
gaebboss info FILE                      # version, phase, project, counts, total, warnings
gaebboss items FILE [--counting] [--tsv]
gaebboss text FILE --oz OZ [--markdown]
gaebboss json FILE [--compact]
gaebboss validate FILE [--tender X83]   # exit 1 on errors
gaebboss roundtrip FILE -o OUT [--phase X84] [--version 3.3] [--pretty] [--strict]
gaebboss bid X83 --prices prices.csv --bidder bidder.toml -o OUT.X84 [--pretty]
```

`prices.csv` has a header `oz,up` or `id,up`, one row per item, `-` for a
not-offered item and an optional third column with the quantity of a free
quantity item. `bidder.toml` holds `name1`, `street`, `pcode`, `city` and
optionally `name2`, `contact`, `email`, `bidder_no`, `vat_percent`,
`discount_percent`.

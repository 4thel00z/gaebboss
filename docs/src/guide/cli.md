# Command line

```bash
gaebboss info FILE                      # version, phase, project, counts, total, warnings
gaebboss items FILE [--counting] [--tsv]
gaebboss text FILE [--oz OZ] [--markdown]   # every item's long text unless --oz is given
gaebboss md FILE [--oz OZ]              # the bill as Markdown: category headings, item long texts
gaebboss json FILE [--compact]
gaebboss q FILE PROGRAM [-r]            # a jq program over the JSON
gaebboss hex FILE [oz:OZ | id:ID | range:START-END] [--annotate] [--width N]
gaebboss tui FILE                       # interactive explorer
gaebboss validate FILE [--tender X83]   # exit 1 on errors
gaebboss roundtrip FILE -o OUT [--phase X84] [--version 3.3] [--pretty] [--strict]
gaebboss bid X83 --prices prices.csv --bidder bidder.toml -o OUT.X84 [--pretty]
gaebboss skill install [-g] | show      # the bundled Claude Code skill
```

Items are named by Ordnungszahl, dotted (`01.02.0010`) or compact
(`01020010`), or by their `ID` attribute. `-o` is short for `--out`.

`prices.csv` has a header `oz,up` or `id,up`, one row per item, `-` for a
not-offered item and an optional third column with the quantity of a free
quantity item. `bidder.toml` holds `name1`, `street`, `pcode`, `city` and
optionally `name2`, `contact`, `email`, `bidder_no`, `vat_percent`,
`discount_percent`.

## Querying with jq

`gaebboss q` runs a jq program (the jaq engine) over the same JSON
`gaebboss json` prints. Decimals are strings, so convert them before
arithmetic:

```bash
gaebboss q tender.X83 '.items[] | select(.kind == "LumpSum") | .oz' -r
gaebboss q bid.X84 '[.items[] | select(.counts_towards_total) | .total // "0" | tonumber] | add'
```

## Hexdumps

`gaebboss hex` dumps the whole document or one element. Offsets index the
document as parsed: UTF-8 without a byte order mark, with Latin-1, CP1252
and UTF-16 inputs transcoded. `--annotate` prints a `── Item 01.0010 ──`
line wherever a category, item or markup item starts.

## The explorer

`gaebboss tui` opens the bill as a tree next to an inspector (fields, short
and long text, validation findings) and the selected element's XML. The
terminal opens at once; validation and the long texts are read in the
background.

| Key | Action |
|---|---|
| `j` `k`, arrows | move, or scroll the focused pane |
| `h` `l` | collapse or go to the parent / expand or go to the first child |
| `Enter`, `Space` | toggle; `Backspace` selects the parent |
| `g` `G`, `PageUp` `PageDown` | top, bottom, page |
| `Tab` | focus tree, inspector, XML in turn |
| `/`, then `n` `N` | search Ordnungszahl, ID, labels and long texts |
| `]` `[` | next / previous row with validation findings |
| `m` | long text as Markdown |
| `y` | yank: `o` OZ, `i` ID, `s` short text, `t` long text, `m` Markdown, `x` XML, `q` jq query, `c` shell command |
| `Alt`+arrows | resize the panes (`Ctrl`+arrows where `Alt` is taken) |
| `q`, `Esc` | quit |

Yanks go to the system clipboard, or through the OSC 52 escape sequence when
there is none (over SSH, for example).

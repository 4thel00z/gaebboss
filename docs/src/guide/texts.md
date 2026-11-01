# Texts and bidder fields

Long texts are formatted text: paragraphs, spans with styles, lists, tables
and images, with `TextComplement` fields inside. gaebboss renders them from
the tape in document order, so vendor deviations such as a list placed
directly under `DetailTxt` or a `<b>` inside a span still read.

- `item.short_text()` is the OutlineText as one line.
- `item.long_text()` is plain text: paragraphs on their own lines, list items
  bulleted, table cells tab-separated, fields inline. An empty bidder field
  reads `'....'`.
- `item.long_text_markdown()` turns bold and italic spans into Markdown
  emphasis and tables into GFM tables.
- `item.complements()` lists the fields with mark, kind (Owner or Bidder),
  caption, body, tail, the number behind `ComplBodyDec`/`ComplBodyInt`, and
  `value()` which strips the quote marks a body is written with.

The same functions work on any text-carrying node: `text::plain(&doc, node)`
takes a category label, a remark or an AddText as well.

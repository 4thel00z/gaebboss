# Writing files

`doc.write(&WriteOptions { version, phase, indent, prog_system, strict })`
writes the typed tree as XML in the target version's element order, applying
the target phase's profile: elements the phase does not carry are dropped
and reported in `WriteReport.dropped`; required elements that are missing
are reported in `missing`, or refused when `strict` is set.

Writing the version and phase a file came in reproduces its content: every
official fixture and every file of the real-world corpus round-trips to
byte-identical output on the second pass, with unknown vendor elements
copied verbatim.

Children that may interleave in the schema (texts and bidder fields, items
and remarks, paragraphs and lists) keep the order they were read in;
everything else follows the schema. Pretty printing never touches text-level
elements, so whitespace inside a paragraph is never changed.

`make conformance` writes X81 to X84 files from the official fixtures and
validates them against the official XSDs with lxml.

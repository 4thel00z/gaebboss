# Validation

`validate(&doc)` checks a document on its own:

- phase rules from the official schema profile: elements the phase does not
  carry, required elements that are missing (3.2 and 3.3 only);
- identifiers: duplicated `ID`, dangling `IDRef`;
- Ordnungszahlen: the `RNoPart` pattern, 14 characters at most, unique and
  ascending in the standard's sort order;
- decimals: Qty and UP with at most three places, IT and VAT with two;
- arithmetic: `IT = round2(Qty × UP)`, category totals against their items,
  the bill total against its categories;
- X84: no unit price on a not-offered item.

`validate_against_tender(&bid, &tender)` checks a bid against the tender it
answers: the same categories, the same items, quantities unchanged unless
the tender left them open. Items are matched by ID and, failing that, by
Ordnungszahl; a changed ID is a warning.

`validate_with_tender(&bid, &tender)` is what `gaebboss validate --tender`
runs: every check of `validate`, then `validate_against_tender`, with one
difference in the totals. An X84 cannot carry the markers for alternative
items and categories or for not-applicable items; only the tender has them.
Checked on its own, a bid with an alternative category sums that category
into its parent and reports a mismatch. With the tender, the sums leave out
what the tender marks, so validate a bid with its tender.

Findings carry a severity, a code, the Ordnungszahl where there is one and a
message. The CLI exits 1 when any finding is an error.

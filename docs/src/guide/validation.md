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

Findings carry a severity, a code, the Ordnungszahl where there is one and a
message. The CLI exits 1 when any finding is an error.

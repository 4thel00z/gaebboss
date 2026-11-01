#!/usr/bin/env bash
# Writes X83, X84 and X86 files with gaebboss from the official fixtures
# and checks every one against the official XSDs with lxml.
set -euo pipefail
cd "$(dirname "$0")/.."
out=target/conformance
rm -rf "$out" && mkdir -p "$out"
cargo build --release -p gaebboss-cli >/dev/null 2>&1
G=target/release/gaebboss
F=tests/fixtures/official
printf 'name1 = "Bau GmbH"\nstreet = "Weg 1"\npcode = "12345"\ncity = "Berlin"\nvat_percent = "19"\n' > "$out/bidder.toml"
for tender in "$F/3.3/bvbs_bauausfuehrung.x83" "$F/3.1/Musterdatei_3.1_2008-001.X83" "$F/3.1/Musterdatei_3.1_2008-004.X83"; do
  base=$(basename "$tender" | tr ' ' '_')
  "$G" items "$tender" --tsv | tail -n +2 | cut -f1 | awk 'BEGIN{print "oz,up"} {if (NR%9==5) print $0",-"; else print $0","NR".25"}' > "$out/$base.prices.csv"
  "$G" bid "$tender" --prices "$out/$base.prices.csv" --bidder "$out/bidder.toml" -o "$out/$base.bid.X84"
done
"$G" roundtrip "$F/3.3/bvbs_bauausfuehrung.x83" -o "$out/bvbs.rt.X83"
"$G" roundtrip "$F/3.3/bvbs_bauausfuehrung.x84" -o "$out/bvbs.rt.X84" --pretty
"$G" roundtrip "$F/3.3/bvbs_texterstellung.x81" -o "$out/bvbs.rt.X81"
"$G" roundtrip "$F/3.3/bvbs_texterstellung.x82" -o "$out/bvbs.rt.X82"
"$G" roundtrip "$F/3.1/Musterdatei_3.1_2008-001.X83" -o "$out/muster.rt.X83" --version 3.3
uv run -q --with lxml python -I scripts/xsd_check.py standard "$out"/*.X8?

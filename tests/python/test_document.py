from decimal import Decimal

import pytest

import gaebboss


def test_open_by_path_and_by_bytes(x84) -> None:
    doc = gaebboss.Document(x84)
    assert doc.version == "3.3"
    assert doc.phase == "X84"
    assert doc.project.name == "BVBS GAEB Muster"
    assert doc.boq.name == "BVBS GAEB Bauausf."
    assert doc.boq.total == Decimal("2000000.00")
    assert isinstance(doc.boq.total, Decimal)
    same = gaebboss.Document(data=x84.read_bytes(), name="anything.X84")
    assert same.phase == "X84"
    assert len(same.items()) == 27


def test_breakdown_levels(x84) -> None:
    doc = gaebboss.Document(x84)
    levels = doc.boq.breakdown
    assert [(l.type, l.length) for l in levels] == [("BoQLevel", 3), ("BoQLevel", 3), ("Item", 4), ("Index", 1)]


def test_warnings_are_typed(tmp_path) -> None:
    broken = tmp_path / "b.X83"
    broken.write_bytes(
        b'<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><Award><DP>83</DP><BoQ ID="b"><BoQBody><Itemlist><Item ID="i" RNoPart="1"><Qty>1,5</Qty></Item></Itemlist></BoQBody></BoQ></Award></GAEB>'
    )
    doc = gaebboss.Document(broken)
    assert any(w.code == "BadDecimal" for w in doc.warnings)
    assert doc.warnings[0].offset >= 0
    assert "decimal" in doc.warnings[0].message


def test_unreadable_input_raises_gaeb_error(tmp_path) -> None:
    with pytest.raises(gaebboss.GaebError):
        gaebboss.Document(data=b"hello world")
    with pytest.raises(gaebboss.GaebError):
        gaebboss.Document(tmp_path / "missing.X83")


def test_to_json_and_write(x84, tmp_path) -> None:
    doc = gaebboss.Document(x84)
    import json

    value = json.loads(doc.to_json())
    assert value["items"][0]["unit_price"] == "5000.000"
    out = doc.write(pretty=True)
    assert out.startswith(b'<?xml version="1.0" encoding="UTF-8"?>')
    assert b"\n  <GAEBInfo>" in out
    again = gaebboss.Document(data=out)
    assert again.boq.total == doc.boq.total
    report = doc.write_report(version="3.2")
    assert report.dropped == [] or all(isinstance(d.element, str) for d in report.dropped)


def test_validate(x83, x84) -> None:
    bid = gaebboss.Document(x84)
    tender = gaebboss.Document(x83)
    assert [f for f in bid.validate() if f.severity == "Error"] == []
    against = bid.validate_against(tender)
    assert all(f.severity != "Error" for f in against)
    findings = tender.validate()
    assert all(f.severity in ("Error", "Warning", "Info") for f in findings)
    assert all(isinstance(f.code, str) and isinstance(f.message, str) for f in findings)

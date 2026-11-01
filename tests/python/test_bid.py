from decimal import Decimal

import pytest

import gaebboss


def test_bid_from_tender_prices_and_writes_x84(x83, tmp_path) -> None:
    tender = gaebboss.Document(x83)
    bid = gaebboss.Bid.from_tender(tender, {"name1": "Bau GmbH", "street": "Weg 1", "pcode": "12345", "city": "Berlin"})
    for i, item in enumerate(tender.items()):
        if i % 5 == 4:
            bid.not_offered(item.id)
            continue
        if item.kind == "FreeQuantity":
            bid.free_quantity(item.id, Decimal("10.000"))
        bid.price(item.id, Decimal(i + 1) + Decimal("0.50"))
    for m in tender.markup_items():
        bid.markup(m.id, Decimal("1000.00"), Decimal("10"))
    bid.vat_percent(Decimal("19"))
    doc = bid.finish()
    assert doc.phase == "X84"
    assert doc.boq.total is not None
    out = doc.write()
    path = tmp_path / "bid.X84"
    path.write_bytes(out)
    again = gaebboss.Document(path)
    assert again.phase == "X84"
    assert [f for f in again.validate() if f.severity == "Error"] == []
    assert [f for f in again.validate_against(tender) if f.severity == "Error"] == []


def test_bid_errors_are_gaeb_errors(x83) -> None:
    tender = gaebboss.Document(x83)
    bid = gaebboss.Bid.from_tender(tender, {"name1": "B", "street": "s", "pcode": "1", "city": "c"})
    with pytest.raises(gaebboss.GaebError):
        bid.price("no-such-id", Decimal("1"))
    with pytest.raises(gaebboss.GaebError):
        bid.price(tender.items()[0].id, 1.5)
    with pytest.raises(gaebboss.GaebError):
        bid.finish()


def test_bid_rejects_a_bid_as_tender(x84) -> None:
    with pytest.raises(gaebboss.GaebError):
        gaebboss.Bid.from_tender(gaebboss.Document(x84), {"name1": "B", "street": "s", "pcode": "1", "city": "c"})

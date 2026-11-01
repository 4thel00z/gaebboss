from decimal import Decimal

import gaebboss


def test_items_carry_oz_prices_and_kind(x84) -> None:
    doc = gaebboss.Document(x84)
    items = doc.items()
    first = items[0]
    assert first.oz == "001.001.0010"
    assert first.id == "ID02BED3D8"
    assert first.kind == "Standard"
    assert first.unit_price == Decimal("5000.000")
    assert first.total == Decimal("5000.00")
    assert first.quantity is None
    assert first.counts_towards_total is True
    assert items[2].oz == "001.001.0010A"
    assert len(doc.items(kind="counting")) <= len(items)


def test_texts_and_complements(x83) -> None:
    doc = gaebboss.Document(x83)
    items = doc.items()
    assert items[0].short_text.startswith("Baustelleneinrichtung")
    assert "Baustelleneinrichtung" in items[0].long_text
    assert isinstance(items[0].long_text_markdown, str)
    with_fields = next(i for i in items if i.complements)
    field = with_fields.complements[0]
    assert field.kind in ("Owner", "Bidder")
    assert isinstance(field.mark, int)
    assert isinstance(field.value, str)


def test_kinds_from_tender(x83) -> None:
    doc = gaebboss.Document(x83)
    kinds = {i.kind for i in doc.items()}
    assert "Alternative" in kinds
    alt = next(i for i in doc.items() if i.kind == "Alternative")
    assert alt.alternative_group is not None
    assert alt.counts_towards_total is False
    assert alt.unit == "m2"


def test_categories_and_markup_items(x84) -> None:
    doc = gaebboss.Document(x84)
    categories = doc.categories()
    assert len(categories) == 12
    assert categories[0].oz == "001"
    assert categories[0].depth == 0
    assert categories[0].total == Decimal("1054734.03")
    markups = doc.markup_items()
    assert len(markups) == 1
    assert markups[0].total == Decimal("85000.00")
    assert markups[0].percent == Decimal("10.00")


def test_old_version_file(musterdatei_x83) -> None:
    doc = gaebboss.Document(musterdatei_x83)
    assert doc.version == "3.1"
    assert doc.phase == "X83"
    assert len(doc.items()) > 50

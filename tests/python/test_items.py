from decimal import Decimal

import gaebboss


BILL = (
    '<GAEB xmlns="http://www.gaeb.de/GAEB_DA_XML/DA83/3.3"><Award><DP>83</DP><BoQ ID="b"><BoQInfo>'
    "<BoQBkdn><Type>BoQLevel</Type><Length>2</Length><Num>Yes</Num></BoQBkdn>"
    "<BoQBkdn><Type>Item</Type><Length>4</Length><Num>Yes</Num></BoQBkdn></BoQInfo><BoQBody>"
    '<BoQCtgy ID="c1" RNoPart="01"><LblTx><p><span>Rohbau</span></p></LblTx><BoQBody><Itemlist>'
    '<Item ID="i" RNoPart="0010"><Qty>1.000</Qty><QU>m</QU></Item>'
    '<MarkupItem ID="m" RNoPart="0020"><Markup>5.00</Markup><Description><CompleteText>'
    "<DetailTxt><Text><p><span>Zuschlag auf Pos. 0010</span></p></Text></DetailTxt>"
    "<OutlineText><OutlTxt><TextOutlTxt><p><span>Wagnis</span></p></TextOutlTxt></OutlTxt></OutlineText>"
    "</CompleteText></Description></MarkupItem></Itemlist></BoQBody></BoQCtgy>"
    '<BoQCtgy ID="c2" RNoPart="02"><LblTx><p><span>Leer</span></p></LblTx></BoQCtgy>'
    "</BoQBody></BoQ></Award></GAEB>"
)

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

def test_entries_put_each_category_before_its_contents(x84) -> None:
    doc = gaebboss.Document(x84)
    entries = doc.entries()
    assert len(entries) == len(doc.categories()) + len(doc.items()) + len(doc.markup_items())
    ozs = [entry.oz for entry in entries]
    markup = ozs.index("002.001.0030")
    assert ozs[markup - 1 : markup + 2] == ["002.001.0020", "002.001.0030", "002.001.0040"]
    assert isinstance(entries[markup], gaebboss.MarkupItem)
    assert entries[markup].category_ozs == ["002", "002.001"]
    assert entries[markup].total == Decimal("85000.00")
    seen: set[str] = set()
    for entry in entries:
        if isinstance(entry, gaebboss.Category):
            seen.add(entry.oz)
            continue
        assert set(entry.category_ozs) <= seen


def test_entries_keep_empty_categories_and_markup_texts() -> None:
    doc = gaebboss.Document(data=BILL.encode(), name="bill.X83")
    entries = doc.entries()
    assert [(type(entry).__name__, entry.oz) for entry in entries] == [
        ("Category", "01"),
        ("Item", "01.0010"),
        ("MarkupItem", "01.0020"),
        ("Category", "02"),
    ]
    markup = entries[2]
    assert markup.short_text == "Wagnis"
    assert markup.long_text == "Zuschlag auf Pos. 0010"
    assert markup.category_ozs == ["01"]
    assert markup.percent == Decimal("5.00")
    assert markup.total is None
    assert entries[3].label == "Leer"


def test_old_version_file(musterdatei_x83) -> None:
    doc = gaebboss.Document(musterdatei_x83)
    assert doc.version == "3.1"
    assert doc.phase == "X83"
    assert len(doc.items()) > 50

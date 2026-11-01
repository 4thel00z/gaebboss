//! The generated typed model read from the official sample files.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gaebboss_core::Document;
use gaebboss_core::decimal::Decimal;
use gaebboss_core::warning::WarningCode;

fn fixture(rel: &str) -> Document {
    let path = format!(
        "{}/../../tests/fixtures/official/{rel}",
        env!("CARGO_MANIFEST_DIR")
    );
    Document::open(std::path::Path::new(&path)).unwrap()
}

fn d(text: &str) -> Decimal {
    Decimal::parse(text).unwrap()
}

#[test]
fn bvbs_x84_header() {
    let doc = fixture("3.3/bvbs_bauausfuehrung.x84");
    let info = doc.gaeb.gaeb_info.as_ref().unwrap();
    assert_eq!(doc.str(info.version.unwrap()), "3.3");
    assert_eq!(doc.str(info.prog_name.unwrap()), "BVBS");
    let award = doc.gaeb.award.as_ref().unwrap();
    assert_eq!(doc.str(award.dp.unwrap()), "84");
    let address = award.ctr.as_ref().unwrap().address.as_ref().unwrap();
    assert_eq!(doc.str(address.name1.unwrap()), "BVBS");
    assert_eq!(
        doc.str(doc.gaeb.prj_info.as_ref().unwrap().name_prj.unwrap()),
        "BVBS GAEB Muster"
    );
}

#[test]
fn bvbs_x84_first_item_prices() {
    let doc = fixture("3.3/bvbs_bauausfuehrung.x84");
    let boq = doc.gaeb.award.as_ref().unwrap().boq.as_ref().unwrap();
    assert_eq!(doc.str(boq.id.unwrap()), "ID02BE1BD8");
    let info = boq.boq_info.as_ref().unwrap();
    assert_eq!(info.boq_bkdn.len(), 4);
    assert_eq!(info.boq_bkdn[2].length, Some(4));
    assert_eq!(info.totals.as_ref().unwrap().total, Some(d("2000000.00")));
    let top = &boq.boq_body.as_ref().unwrap().boq_ctgy;
    assert_eq!(top.len(), 3);
    assert_eq!(doc.str(top[0].rno_part.unwrap()), "001");
    let inner = &top[0].boq_body.as_ref().unwrap().boq_ctgy[0];
    let items = &inner
        .boq_body
        .as_ref()
        .unwrap()
        .itemlist
        .as_ref()
        .unwrap()
        .item;
    assert_eq!(items.len(), 3);
    assert_eq!(doc.str(items[0].rno_part.unwrap()), "0010");
    assert_eq!(items[0].up, Some(d("5000.000")));
    assert_eq!(items[0].up.unwrap().scale(), 3);
    assert_eq!(items[0].it, Some(d("5000.00")));
    assert_eq!(doc.str(items[2].rno_index.unwrap()), "A");
    assert_eq!(inner.totals.as_ref().unwrap().total, Some(d("6100.00")));
}

#[test]
fn bvbs_x84_markup_item_and_bidder_complements() {
    let doc = fixture("3.3/bvbs_bauausfuehrung.x84");
    let boq = doc.gaeb.award.as_ref().unwrap().boq.as_ref().unwrap();
    let second = &boq.boq_body.as_ref().unwrap().boq_ctgy[1];
    let ctgy = &second.boq_body.as_ref().unwrap().boq_ctgy[1];
    let list = ctgy.boq_body.as_ref().unwrap().itemlist.as_ref().unwrap();
    assert_eq!(list.markup_item.len(), 1);
    assert_eq!(list.markup_item[0].it_markup, Some(d("850000.00")));
    assert_eq!(list.markup_item[0].markup, Some(d("10.00")));
    let third = &second.boq_body.as_ref().unwrap().boq_ctgy[2];
    let items = &third
        .boq_body
        .as_ref()
        .unwrap()
        .itemlist
        .as_ref()
        .unwrap()
        .item;
    let text = items[1]
        .description
        .as_ref()
        .unwrap()
        .complete_text
        .as_ref()
        .unwrap()
        .detail_txt
        .as_ref()
        .unwrap();
    assert_eq!(text.text_complement.len(), 2);
    assert_eq!(text.text_complement[0].mark_lbl, Some(60));
    assert_eq!(
        text.text_complement[0].kind,
        Some(gaebboss_core::model::AttKind::Bidder)
    );
    let body = text.text_complement[0].compl_body.as_ref().unwrap();
    assert_eq!(doc.str(body.span[0].text), "55");
}

#[test]
fn musterdatei_31_x83_reads() {
    let doc = fixture("3.1/Musterdatei_3.1_2008-001.X83");
    assert_eq!(doc.version(), gaebboss_core::version::Version::V31);
    assert_eq!(doc.phase(), Some(gaebboss_core::version::Phase::X83));
    let boq = doc.gaeb.award.as_ref().unwrap().boq.as_ref().unwrap();
    let info = boq.boq_info.as_ref().unwrap();
    assert!(!info.boq_bkdn.is_empty());
    let first = &boq.boq_body.as_ref().unwrap().boq_ctgy[0];
    let label = first.lbl_tx.as_ref().unwrap();
    assert!(!label.p.is_empty() || !label.span.is_empty());
    let item = doc
        .gaeb
        .award
        .as_ref()
        .unwrap()
        .boq
        .as_ref()
        .unwrap()
        .boq_body
        .as_ref()
        .unwrap()
        .boq_ctgy
        .iter()
        .find_map(|c| c.boq_body.as_ref()?.itemlist.as_ref()?.item.first());
    assert!(
        item.is_some()
            || first
                .boq_body
                .as_ref()
                .unwrap()
                .boq_ctgy
                .iter()
                .any(|c| c.boq_body.is_some())
    );
}

#[test]
fn unknown_child_goes_to_extra_with_warning() {
    let input = b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><GAEBInfo><Version>3.3</Version></GAEBInfo><Award><DP>83</DP><BoQ ID=\"b\"><BoQInfo><Name>x</Name></BoQInfo><BoQBody><Itemlist><Item ID=\"i1\" RNoPart=\"1\"><Qty>1.000</Qty><Foo>vendor</Foo><QU>m</QU></Item></Itemlist></BoQBody></BoQ></Award></GAEB>";
    let doc = Document::parse(input).unwrap();
    let item = &doc
        .gaeb
        .award
        .as_ref()
        .unwrap()
        .boq
        .as_ref()
        .unwrap()
        .boq_body
        .as_ref()
        .unwrap()
        .itemlist
        .as_ref()
        .unwrap()
        .item[0];
    assert_eq!(item.qty, Some(d("1.000")));
    assert_eq!(doc.str(item.qu.unwrap()), "m");
    assert_eq!(item.extra.len(), 1);
    assert_eq!(
        doc.str(doc.tape().nodes[item.extra[0] as usize].raw_name),
        "Foo"
    );
    assert!(
        doc.warnings
            .iter()
            .any(|w| w.code == WarningCode::UnknownElement)
    );
}

#[test]
fn bad_decimal_is_a_warning_not_an_error() {
    let input = b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><Award><DP>83</DP><BoQ ID=\"b\"><BoQBody><Itemlist><Item ID=\"i1\" RNoPart=\"1\"><Qty>1,5</Qty></Item></Itemlist></BoQBody></BoQ></Award></GAEB>";
    let doc = Document::parse(input).unwrap();
    let item = &doc
        .gaeb
        .award
        .as_ref()
        .unwrap()
        .boq
        .as_ref()
        .unwrap()
        .boq_body
        .as_ref()
        .unwrap()
        .itemlist
        .as_ref()
        .unwrap()
        .item[0];
    assert_eq!(item.qty, None);
    assert!(
        doc.warnings
            .iter()
            .any(|w| w.code == WarningCode::BadDecimal)
    );
}

#[test]
fn yes_no_and_enums_are_typed() {
    let input = b"<GAEB xmlns=\"http://www.gaeb.de/GAEB_DA_XML/DA83/3.3\"><Award><DP>83</DP><BoQ ID=\"b\"><BoQInfo><BoQBkdn><Type>BoQLevel</Type><Length>2</Length><Num>Yes</Num><Alignment>left</Alignment></BoQBkdn></BoQInfo><BoQBody><Itemlist><Item ID=\"i1\" RNoPart=\"1\"><Provis>WithoutTotal</Provis><LumpSumItem>Yes</LumpSumItem><QtyTBD>Yes</QtyTBD></Item></Itemlist></BoQBody></BoQ></Award></GAEB>";
    let doc = Document::parse(input).unwrap();
    let boq = doc.gaeb.award.as_ref().unwrap().boq.as_ref().unwrap();
    let bkdn = &boq.boq_info.as_ref().unwrap().boq_bkdn[0];
    assert_eq!(bkdn.ty, Some(gaebboss_core::model::BoQBkdnType::BoQLevel));
    assert_eq!(bkdn.num, Some(true));
    assert_eq!(bkdn.alignment, Some(gaebboss_core::model::LeftRight::Left));
    let item = &boq
        .boq_body
        .as_ref()
        .unwrap()
        .itemlist
        .as_ref()
        .unwrap()
        .item[0];
    assert_eq!(
        item.provis,
        Some(gaebboss_core::model::Provis::WithoutTotal)
    );
    assert_eq!(item.lump_sum_item, Some(true));
    assert_eq!(item.qty_tbd, Some(true));
}

#[test]
fn every_official_fixture_reads_without_error() {
    let root = format!(
        "{}/../../tests/fixtures/official",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut count = 0;
    for dir in ["3.1", "3.3"] {
        for entry in std::fs::read_dir(format!("{root}/{dir}"))
            .unwrap()
            .flatten()
        {
            let doc = Document::open(&entry.path())
                .unwrap_or_else(|e| panic!("{}: {e}", entry.path().display()));
            assert!(doc.gaeb.award.is_some(), "{}", entry.path().display());
            count += 1;
        }
    }
    assert_eq!(count, 12);
}

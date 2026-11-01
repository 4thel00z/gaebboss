//! Element and attribute names of GAEB DA XML, resolved to a dense id.

pub use crate::generated::names::Name;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_item_is_item() {
        assert_eq!(Name::lookup(b"Item"), Name::Item);
        assert_eq!(Name::lookup(b"BoQCtgy"), Name::BoQCtgy);
        assert_eq!(Name::lookup(b"span"), Name::Span);
    }

    #[test]
    fn lookup_unknown() {
        assert_eq!(Name::lookup(b"Foo"), Name::Unknown);
        assert_eq!(Name::lookup(b""), Name::Unknown);
        assert_eq!(Name::lookup(b"item"), Name::Unknown);
    }

    #[test]
    fn lookup_attr_rno_part() {
        assert_eq!(Name::lookup_attr(b"RNoPart"), Name::AttrRNoPart);
        assert_eq!(Name::lookup_attr(b"ID"), Name::AttrId);
        assert_eq!(Name::lookup_attr(b"Item"), Name::Unknown);
    }

    #[test]
    fn roundtrip_all_names() {
        for name in Name::ALL {
            if name == Name::Unknown {
                continue;
            }
            let found = match name.is_attr() {
                true => Name::lookup_attr(name.as_str().as_bytes()),
                false => Name::lookup(name.as_str().as_bytes()),
            };
            assert_eq!(found, name, "{}", name.as_str());
        }
    }

    #[test]
    fn element_count_at_least_465() {
        let count = Name::ALL.iter().count();
        assert!(count >= 465, "{count}");
        assert_eq!(count, Name::COUNT);
        assert_eq!(Name::Unknown as u16, 0);
    }
}

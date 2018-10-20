//! Exercise library API through proptest.

#[macro_use]
extern crate proptest;

extern crate conserve;
use conserve::*;

prop_compose! {
    fn arb_single_apath(dot: bool, n in "[^/\0]+") {
        "/" + if dot { "."} else { "" } + n
    }
}

proptest! {
    #[test]
    fn parse_bandid(ref s in r"b[[:digit:]]{1,9}(-[[:digit:]]{1,9})*") {
        let bid = BandId::from_string(s).unwrap();
        let bid2 = BandId::from_string(s).unwrap();
        assert_eq!(bid, bid2);
        let bs = bid.as_string();
        assert_eq!(BandId::from_string(bs).unwrap(), bid);

        assert!(bid.next_sibling() > bid);
    }

    #[test]
    fn apath_valid(ref a in "(/|(/[^/\0]+)+)") {
        if a.ends_with("/.") || a.ends_with("/..") || a.contains("/./") || a.contains("/../") {
            assert!(!Apath::is_valid(a));
        } else {
            assert!(Apath::is_valid(a));
        }
    }
}
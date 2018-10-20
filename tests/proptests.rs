//! Exercise library API through proptest.

#[macro_use]
extern crate proptest;

extern crate conserve;
use conserve::*;

prop_compose! {
    fn arb_single_apath()(dot: bool, n in "[^/.\0][^/\0]+") -> String {
        "/".to_owned() + if dot { "."} else { "" } + &n
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

    #[test]
    fn apath_simple_valid(ref a in arb_single_apath()) {
        assert!(Apath::is_valid(a));
    }

    #[test]
    fn apath_ordering(ref a in proptest::collection::vec(arb_single_apath(), 1..5),
        ref b in proptest::collection::vec(arb_single_apath(), 1..5)) {
        prop_assume!(a != b);
        let a = a.join("");
        let b = b.join("");
        let aa = Apath::from(a.as_str());
        let bb = Apath::from(b.as_str());
        assert!((aa > bb && bb < aa) || (bb > aa && aa < bb));
    }

    #[test]
    fn apath_long_valid(ref v in proptest::collection::vec(arb_single_apath(), 1..5)) {
        let a = v.join("");
        assert!(Apath::is_valid(&a), a);
    }
}
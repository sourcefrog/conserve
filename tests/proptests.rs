//! Exercise library API through proptest.

#[macro_use]
extern crate proptest;

extern crate conserve;

proptest! {
    #[test]
    fn parse_bandid(ref s in r"b[[:digit:]]{1,9}(-[[:digit:]]{1,9})*") {
        let bid = conserve::BandId::from_string(s).unwrap();
        let bid2 = conserve::BandId::from_string(s).unwrap();
        assert_eq!(bid, bid2);
        let bs = bid.as_string();
        assert_eq!(conserve::BandId::from_string(bs).unwrap(), bid);

        assert!(bid.next_sibling() > bid);
    }
}
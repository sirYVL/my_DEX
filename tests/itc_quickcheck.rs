// tests/itc_quickcheck.rs

use quickcheck::{quickcheck, Arbitrary, Gen};
use my_project::crdt::{ITCOrSet, ITCElement};
use my_project::itc::{IntervalTreeClock, IntervalNode};
use std::collections::HashSet;

impl Arbitrary for IntervalNode {
    fn arbitrary(g: &mut Gen) -> Self {
        let start = u64::arbitrary(g) % 10000;
        let end = start + (u64::arbitrary(g) % 50);
        Self { start, end }
    }
}

impl Arbitrary for IntervalTreeClock {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut intervals = Vec::new();
        let n = (u8::arbitrary(g) % 3) + 1;
        let mut rng = rand::rngs::StdRng::seed_from_u64(0xfeed);
        for _ in 0..n {
            // Erstelle ein IntervalNode
            let start = rng.gen_range(0..500);
            let end = start + rng.gen_range(0..100);
            intervals.push(IntervalNode { start, end });
        }
        IntervalTreeClock { intervals }
    }
}

impl Arbitrary for ITCElement {
    fn arbitrary(g: &mut Gen) -> Self {
        let id = format!("qcOrder_{}", String::arbitrary(g));
        let itc = IntervalTreeClock::arbitrary(g);
        Self { order_id: id, itc }
    }
}

impl Arbitrary for ITCOrSet {
    fn arbitrary(g: &mut Gen) -> Self {
        let mut a = HashSet::new();
        let mut r = HashSet::new();
        let n = (u8::arbitrary(g) % 4) + 1;
        for _ in 0..n {
            let elem = ITCElement::arbitrary(g);
            if bool::arbitrary(g) {
                a.insert(elem);
            } else {
                r.insert(elem);
            }
        }
        Self { adds: a, removes: r }
    }
}

#[test]
fn prop_merge_idempotent() {
    fn prop(a: ITCOrSet) -> bool {
        let mut x = a.clone();
        x.merge(&a);
        x == a
    }
    quickcheck(prop as fn(ITCOrSet) -> bool);
}

#[test]
fn prop_merge_commutative() {
    fn prop(a: ITCOrSet, b: ITCOrSet) -> bool {
        let mut ab = a.clone();
        ab.merge(&b);
        let mut ba = b.clone();
        ba.merge(&a);
        ab == ba
    }
    quickcheck(prop as fn(ITCOrSet, ITCOrSet) -> bool);
}

#[test]
fn prop_merge_associative() {
    fn prop(a: ITCOrSet, b: ITCOrSet, c: ITCOrSet) -> bool {
        let mut ab = a.clone();
        ab.merge(&b);
        ab.merge(&c);

        let mut bc = b.clone();
        bc.merge(&c);
        let mut abc2 = a.clone();
        abc2.merge(&bc);

        ab == abc2
    }
    quickcheck(prop as fn(ITCOrSet, ITCOrSet, ITCOrSet) -> bool);
}

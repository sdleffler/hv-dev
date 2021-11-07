use resources::*;

#[derive(Debug, PartialEq)]
struct One(usize);

#[derive(Debug, PartialEq)]
struct Two(usize);

impl Default for Two {
    fn default() -> Self {
        Self(2)
    }
}

#[test]
fn insert() {
    let mut resources = Resources::new();
    assert!(resources.insert(One(1)).is_none());
    assert!(resources.insert(Two(2)).is_none());
    assert_eq!(resources.insert(One(5)), Some(One(1)));
}

#[test]
fn contains() {
    let mut resources = Resources::new();
    resources.insert(One(1));

    assert!(resources.contains::<One>());
    assert!(!resources.contains::<Two>());

    resources.insert(Two(2));
    assert!(resources.contains::<Two>());
}

#[test]
fn multiple_borrow() {
    let mut resources = Resources::new();
    resources.insert(One(1));
    let resources = resources;

    let ref1 = resources.get::<One>().unwrap();
    let ref2 = resources.get::<One>().unwrap();

    assert_eq!(*ref1, *ref2)
}

#[test]
fn multiple_borrow_mutable() {
    let mut resources = Resources::new();
    resources.insert(One(1));
    let resources = resources;

    let ref1 = resources.get_mut::<One>();
    assert!(resources.get_mut::<One>().is_err());
    assert!(ref1.is_ok());
}

#[test]
fn multiple_borrow_mixed() {
    let mut resources = Resources::new();
    resources.insert(One(1));
    let resources = resources;

    {
        let ref1 = resources.get::<One>();
        assert!(resources.get_mut::<One>().is_err());
        assert!(ref1.is_ok());
    }

    {
        let ref1 = resources.get_mut::<One>();
        assert!(resources.get::<One>().is_err());
        assert!(ref1.is_ok());
    }
}

#[test]
fn orthogonal_borrow() {
    let mut resources = Resources::new();
    resources.insert(One(0));
    resources.insert(Two(0));
    let resources = resources;

    {
        let mut ref1 = resources.get_mut::<One>().unwrap();
        let mut ref2 = resources.get_mut::<Two>().unwrap();

        ref1.0 += 1;
        ref2.0 += 2;
    }

    let ref1 = resources.get::<One>().unwrap();
    let ref2 = resources.get::<Two>().unwrap();

    assert_eq!(ref1.0, 1);
    assert_eq!(ref2.0, 2);
}

#[test]
fn remove() {
    let mut resources = Resources::new();
    resources.insert(One(0));

    resources.get_mut::<One>().unwrap().0 = 1;

    assert_eq!(resources.remove::<One>().unwrap(), One(1));
    assert!(resources.remove::<One>().is_none());
}

#[test]
fn entry() {
    let mut resources = Resources::new();

    resources.insert(One(0));
    resources
        .entry::<One>()
        .and_modify(|ref1| ref1.0 += 1)
        .or_insert(One(5));

    resources
        .entry::<Two>()
        .and_modify(|ref2| ref2.0 = 5)
        .or_default();

    let resources = resources;

    let ref1 = resources.get::<One>().unwrap();
    let ref2 = resources.get::<Two>().unwrap();

    assert_eq!(ref1.0, 1);
    assert_eq!(ref2.0, 2);
}

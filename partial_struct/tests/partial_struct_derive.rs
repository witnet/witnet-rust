use partial_struct::PartialStruct;

#[test]
#[allow(clippy::redundant_clone)]
fn test_partial_derive() {
    #[derive(PartialStruct, Debug, Clone, PartialEq)]
    #[partial_struct(derive(Debug, Clone, PartialEq))]
    struct Obj;

    let p = PartialObj;
    let p_clone = p.clone();

    assert_eq!(p, p_clone);
}

#[test]
fn test_partial_derive_unnamed() {
    #[derive(PartialStruct)]
    #[partial_struct(derive(Default))]
    #[allow(dead_code)]
    struct Obj(u32);

    let p = PartialObj::default();

    assert_eq!(p.0, None);
}

#[test]
fn test_partial_derive_named() {
    #[derive(PartialStruct)]
    #[partial_struct(derive(Default))]
    #[allow(dead_code)]
    struct Obj {
        attr: u32,
    }

    let p = PartialObj::default();

    assert_eq!(p.attr, None);
}

#[test]
fn test_partial_derive_unit() {
    #[derive(PartialStruct)]
    #[allow(dead_code)]
    struct Obj();

    let _ = PartialObj();
}

#[test]
fn test_partial_attr_skip() {
    #[derive(PartialStruct)]
    #[partial_struct(derive(Default))]
    #[allow(dead_code)]
    struct Obj {
        a: u32,
        #[partial_struct(skip)]
        b: bool,
    }

    let p = PartialObj::default();

    assert_eq!(p.a, None);
    assert!(!p.b);
}

#[test]
fn test_partial_attr_partial() {
    #[derive(PartialStruct)]
    #[partial_struct(derive(Default))]
    #[allow(dead_code)]
    struct Obj {
        #[partial_struct(ty = "PartialAnotherObj")]
        f: AnotherObj,
    }

    #[derive(PartialStruct, Clone, Debug, PartialEq)]
    #[partial_struct(derive(Default, Clone, Debug, PartialEq))]
    struct AnotherObj;

    let p = PartialObj::default();

    assert_eq!(p.f, PartialAnotherObj::default());
}

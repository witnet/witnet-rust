use witnet_node::utils::mode_consensus;

#[test]
fn test_mode_consensus() {
    // The mode consensus function selects the most common item from a list
    let v = [1, 2, 3, 3, 3];
    let c = mode_consensus(v.iter(), 51);
    assert_eq!(c, Some(&3));

    // When there is only one element, that element is the mode
    let v = [3, 3, 3];
    let c = mode_consensus(v.iter(), 51);
    assert_eq!(c, Some(&3));

    let v = [3];
    let c = mode_consensus(v.iter(), 51);
    assert_eq!(c, Some(&3));

    // But when there is a tie, there is no consensus
    let v = [2, 2, 2, 3, 3, 3];
    let c = mode_consensus(v.iter(), 51);
    assert_eq!(c, None);

    // Similarly, when there are no elements, there is no consensus
    let v: Vec<i32> = vec![];
    let c = mode_consensus(v.iter(), 51);
    assert_eq!(c, None);

    let v = [1, 2, 3, 3, 3, 3, 3, 3];
    let c = mode_consensus(v.iter(), 70);
    assert_eq!(c, Some(&3));

    let v = [1, 2, 2, 3, 3, 3, 3, 3];
    let c = mode_consensus(v.iter(), 70);
    assert_eq!(c, None);

    let v = [
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
    ];
    let c = mode_consensus(v.iter(), 60);
    assert_eq!(c, Some(&Some(1)));

    let v = [
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
    ];
    let c = mode_consensus(v.iter(), 60);
    assert_eq!(c, None);

    let v = [
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(1),
        Some(0),
        Some(0),
        Some(0),
        Some(0),
        None,
    ];
    let c = mode_consensus(v.iter(), 60);
    assert_eq!(c, None);
}

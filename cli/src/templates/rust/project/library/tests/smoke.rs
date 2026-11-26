use {{crate_module}}::{LibraryItem, LibraryService, library_name};

#[test]
fn generated_library_stores_and_reads_items() {
    let mut service = LibraryService::new();
    service.upsert(LibraryItem::new("example", library_name()));

    assert_eq!(service.len(), 1);
    assert_eq!(
        service.get("example").expect("item should exist").value,
        library_name()
    );
}
